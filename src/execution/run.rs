use super::parallel::build_partitions;
use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::protocol::{
    GraphSummary, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
    ProgressEvent,
};
use crate::{scan, semantic_enrichment, staging_writer};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub fn materialize_syntax_batch(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let scan = scan::scan_source_state(request)?;
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
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

    let mut staging_accumulator = staging_writer::StagingAccumulator::new(&request.staging_dir);
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

    for (index, result) in build_partitions(request, &scan, &rebuild_paths)?
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

    let semantic_stats = semantic_enrichment::enrich_partitions(&mut partitions, request)?;
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

pub fn materialize_syntax_batch_json(payload: &str) -> Result<String, NativeError> {
    let decode_started = Instant::now();
    let request: NativeSyntaxMaterializationRequest = serde_json::from_str(payload)?;
    let json_decode_seconds = elapsed_seconds(decode_started);
    let mut response = materialize_syntax_batch(&request)?;
    response.add_phase_timing("rust_json_decode_seconds", json_decode_seconds);
    Ok(serde_json::to_string(&response)?)
}
