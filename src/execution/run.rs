use super::parallel::build_execution_plan;
use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::protocol::{
    GraphSummary, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
    ProgressEvent,
};
use crate::{scan, semantic_enrichment, staging_writer};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub fn execute_materialization_pipeline(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let scan = scan::scan_sources(request)?;
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
    execute_scanned_materialization(scan, phase_timings)
}

fn execute_scanned_materialization(
    scan: scan::SourceScan,
    mut phase_timings: BTreeMap<String, f64>,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let request = &scan.input;
    let diff = scan.diff.clone();
    if diff.rebuild_paths().is_empty() && diff.deleted.is_empty() {
        return Ok(NativeSyntaxMaterializationResponse::skipped(
            scan.snapshots,
            diff,
            scan.diagnostics,
            Vec::new(),
            phase_timings,
        ));
    }

    let staging_run = staging_writer::StagingRunDirectory::create(&request.staging_dir)?;
    let mut staging_accumulator =
        staging_writer::StagingAccumulator::new(&staging_run.path().to_string_lossy());
    let mut rebuilt_entries = BTreeMap::new();
    let mut node_ids = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();
    let mut diagnostics = scan.diagnostics.clone();
    let mut parse_seconds = 0.0;
    let mut graph_build_seconds = 0.0;
    let mut staging_seconds = 0.0;
    let mut progress_events = Vec::new();
    let rebuild_paths = diff.rebuild_paths();
    let rebuild_total = rebuild_paths.len();
    let (retained_nodes, retained_edges) = retained_manifest_ids(request, &diff, &rebuild_paths);
    let mut partitions = Vec::new();

    for (index, result) in build_execution_plan(&scan, &rebuild_paths)?
        .into_iter()
        .enumerate()
    {
        parse_seconds += result.parse_seconds;
        graph_build_seconds += result.graph_build_seconds;
        diagnostics.extend(result.diagnostics);
        if request.progress {
            progress_events.push(ProgressEvent {
                phase: "parsed".to_string(),
                current: index + 1,
                total: rebuild_total,
                path: Some(result.partition.entry.path.clone()),
            });
        }
        partitions.push(result.partition);
    }
    partitions.sort_by(|left, right| left.entry.path.cmp(&right.entry.path));
    phase_timings.insert("parse_seconds".to_string(), parse_seconds);
    phase_timings.insert("graph_build_seconds".to_string(), graph_build_seconds);

    let semantic_stats = semantic_enrichment::enrich_semantics(&mut partitions, request)?;
    for (phase, seconds) in semantic_stats.phase_timings {
        phase_timings.insert(phase, seconds);
    }

    for partition in partitions {
        for node_id in &partition.entry.node_ids {
            node_ids.insert(node_id.clone());
        }
        for edge_id in &partition.entry.edge_ids {
            edge_ids.insert(edge_id.clone());
        }
        let staging_started = Instant::now();
        staging_accumulator.add_partition_filtered(&partition, &retained_nodes, &retained_edges);
        staging_seconds += elapsed_seconds(staging_started);
        if request.progress {
            progress_events.push(ProgressEvent {
                phase: "staged".to_string(),
                current: rebuilt_entries.len() + 1,
                total: rebuild_total,
                path: Some(partition.entry.path.clone()),
            });
        }
        let entry_path = partition.entry.path.clone();
        rebuilt_entries.insert(entry_path, partition.entry);
    }

    let staging_started = Instant::now();
    let staging = staging_accumulator.finish()?;
    staging_seconds += elapsed_seconds(staging_started);
    phase_timings.insert("staging_seconds".to_string(), staging_seconds);
    let graph_summary = GraphSummary {
        node_count: node_ids.len(),
        edge_count: edge_ids.len(),
    };
    let mut response = NativeSyntaxMaterializationResponse::from_parts(
        scan.snapshots,
        diff,
        diagnostics,
        rebuilt_entries,
        graph_summary,
        staging,
        phase_timings,
    );
    response.progress_events = progress_events;
    let graph_write_started = Instant::now();
    staging_writer::write_graph_rows(request, &response).map_err(NativeError::InvalidInput)?;
    staging_run.cleanup();
    response.phase_timings.insert(
        "database_write_seconds".to_string(),
        elapsed_seconds(graph_write_started),
    );
    response.database_written = true;
    Ok(response)
}

fn retained_manifest_ids(
    request: &NativeSyntaxMaterializationRequest,
    diff: &crate::protocol::ManifestDiff,
    rebuild_paths: &[String],
) -> (BTreeSet<String>, BTreeSet<String>) {
    if diff.force_rebuild {
        return (BTreeSet::new(), BTreeSet::new());
    }
    let Some(previous) = request.previous_manifest.as_ref() else {
        return (BTreeSet::new(), BTreeSet::new());
    };
    let touched = diff
        .deleted
        .iter()
        .chain(rebuild_paths.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut retained_nodes = BTreeSet::new();
    let mut retained_edges = BTreeSet::new();
    for (path, entry) in &previous.files {
        if touched.contains(path) {
            continue;
        }
        retained_nodes.extend(entry.node_ids.iter().cloned());
        retained_edges.extend(entry.edge_ids.iter().cloned());
    }
    (retained_nodes, retained_edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{NativeSyntaxMaterializationRequest, OntologySchema};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn scanned_payload_completes_enrichment_and_graph_write_after_source_removal() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codebase-graph-scanned-pipeline-{nonce}"));
        let source_root = root.join("repository");
        let state_root = root.join("state");
        fs::create_dir_all(source_root.join("src")).expect("source directory should be created");
        fs::create_dir_all(&state_root).expect("state directory should be created");
        fs::write(
            source_root.join("src/lib.rs"),
            "pub fn scanned_pipeline() -> bool { true }\n",
        )
        .expect("source file should be written");
        let db_path = state_root.join("graph.ldb");
        let request = NativeSyntaxMaterializationRequest {
            source_root: source_root.to_string_lossy().into_owned(),
            repository_label: "scanned-pipeline".to_string(),
            mode: "full".to_string(),
            parser_version: "test".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: OntologySchema::default(),
            previous_manifest: None,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            db_path: db_path.to_string_lossy().into_owned(),
            include_fts: false,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: state_root.join("staging").to_string_lossy().into_owned(),
            atomic_rebuild: false,
            strict: true,
            parallel: false,
            progress: false,
        };
        let mut timings = BTreeMap::new();
        let scan = crate::scan::scan_sources(&request).expect("scan should succeed");
        timings.insert("scan_seconds".to_string(), 0.0);

        fs::remove_dir_all(&source_root).expect("source repository should be removed");

        let response = execute_scanned_materialization(scan, timings)
            .expect("scanned payload should complete the pipeline");
        assert!(response.database_written);
        assert_eq!(response.rebuilt_entries.len(), 1);
        assert!(db_path.exists());
        assert_eq!(
            fs::read_dir(state_root.join("staging"))
                .expect("staging root should remain readable")
                .count(),
            0,
            "successful materialization should remove its isolated staging directory"
        );
        let _ = fs::remove_dir_all(root);
    }
}
