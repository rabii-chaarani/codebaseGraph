use super::parallel::build_execution_plan;
use super::timing::elapsed_seconds;
use crate::artifact_store::ArtifactStore;
use crate::error::NativeError;
use crate::protocol::{
    GraphSummary, ManifestEntry, NativeSyntaxMaterializationRequest,
    NativeSyntaxMaterializationResponse, ProgressEvent,
};
use crate::{scan, semantic_enrichment, staging_writer};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
    // A write invocation always assembles a complete candidate generation. Even a
    // source no-op must validate/reuse every artifact, rerun global enrichment,
    // and produce a self-contained database that can be published atomically.
    fs::create_dir_all(&request.staging_dir)?;
    let artifact_store = ArtifactStore::new(request.resolved_artifact_root());
    let mut staging_accumulator = staging_writer::StagingAccumulator::new(&request.staging_dir);
    let materialization_paths = scan.supported.keys().cloned().collect::<Vec<_>>();
    let materialization_total = materialization_paths.len();

    let mut diagnostics = scan.diagnostics.clone();
    let mut parse_seconds = 0.0;
    let mut graph_build_seconds = 0.0;
    let mut staging_seconds = 0.0;
    let mut progress_events = Vec::new();
    let mut partitions = Vec::new();
    let mut parsed_paths = BTreeSet::new();
    let mut artifacts_reused = 0usize;
    let mut artifacts_rebuilt = 0usize;

    for (index, result) in build_execution_plan(&scan, &materialization_paths, &artifact_store)?
        .into_iter()
        .enumerate()
    {
        parse_seconds += result.parse_seconds;
        graph_build_seconds += result.graph_build_seconds;
        diagnostics.extend(result.diagnostics);
        if result.reused_artifact {
            artifacts_reused += 1;
        } else {
            artifacts_rebuilt += 1;
            parsed_paths.insert(result.partition.entry.path.clone());
        }
        if request.progress {
            progress_events.push(ProgressEvent {
                phase: "parsed".to_string(),
                current: index + 1,
                total: materialization_total,
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

    let materialized_entries = partitions
        .iter()
        .map(|partition| (partition.entry.path.clone(), partition.entry.clone()))
        .collect::<BTreeMap<_, _>>();
    let rebuilt_entries = parsed_paths
        .iter()
        .filter_map(|path| {
            materialized_entries
                .get(path)
                .cloned()
                .map(|entry| (path.clone(), entry))
        })
        .collect::<BTreeMap<_, _>>();

    let materialized_total = partitions.len();
    for (index, partition) in partitions.into_iter().enumerate() {
        let staging_started = Instant::now();
        staging_accumulator.add_partition(&partition);
        staging_seconds += elapsed_seconds(staging_started);
        if request.progress {
            progress_events.push(ProgressEvent {
                phase: "staged".to_string(),
                current: index + 1,
                total: materialized_total,
                path: Some(partition.entry.path.clone()),
            });
        }
    }

    let staging_started = Instant::now();
    let staging = staging_accumulator.finish()?;
    staging_seconds += elapsed_seconds(staging_started);
    phase_timings.insert("staging_seconds".to_string(), staging_seconds);

    let graph_summary = GraphSummary {
        node_count: unique_manifest_ids(&materialized_entries, |entry| &entry.node_ids),
        edge_count: unique_manifest_ids(&materialized_entries, |entry| &entry.edge_ids),
    };
    let graph_build_digest = request.graph_build_compatibility_digest()?;

    let mut response = NativeSyntaxMaterializationResponse::from_parts(
        scan.snapshots,
        diff,
        diagnostics,
        rebuilt_entries,
        materialized_entries,
        graph_summary,
        staging,
        phase_timings,
    );
    response.progress_events = progress_events;
    response.artifacts_reused = artifacts_reused;
    response.artifacts_rebuilt = artifacts_rebuilt;
    response.graph_build_digest = Some(graph_build_digest);

    let graph_write_started = Instant::now();
    staging_writer::write_graph_rows(request, &response).map_err(NativeError::InvalidInput)?;
    response.phase_timings.insert(
        "database_write_seconds".to_string(),
        elapsed_seconds(graph_write_started),
    );
    response.database_written = true;
    Ok(response)
}

fn unique_manifest_ids(
    entries: &BTreeMap<String, ManifestEntry>,
    ids: impl Fn(&ManifestEntry) -> &[String],
) -> usize {
    entries
        .values()
        .flat_map(|entry| ids(entry).iter().cloned())
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{NativeManifest, OntologySchema};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn scanned_payload_completes_enrichment_and_graph_write_after_source_removal() {
        let root = unique_temp_dir("codebase-graph-scanned-pipeline");
        let source_root = root.join("repository");
        let state_root = root.join("state");
        fs::create_dir_all(source_root.join("src")).expect("source directory should be created");
        fs::create_dir_all(&state_root).expect("state directory should be created");
        fs::write(
            source_root.join("src/lib.rs"),
            "pub fn scanned_pipeline() -> bool { true }\n",
        )
        .expect("source file should be written");
        let request = request(&source_root, &state_root, None, Vec::new(), true);
        let mut timings = BTreeMap::new();
        let scan = crate::scan::scan_sources(&request).expect("scan should succeed");
        timings.insert("scan_seconds".to_string(), 0.0);

        fs::remove_dir_all(&source_root).expect("source repository should be removed");

        let response = execute_scanned_materialization(scan, timings)
            .expect("scanned payload should complete the pipeline");
        assert!(response.database_written);
        assert_eq!(response.rebuilt_entries.len(), 1);
        assert_eq!(response.materialized_entries.len(), 1);
        assert!(Path::new(&request.db_path).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unchanged_partitions_reuse_artifacts_without_reparsing() {
        let root = unique_temp_dir("codebase-graph-artifact-reuse");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            true,
        ))
        .unwrap();
        let previous = manifest_from_response(&initial);

        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool {\n    helper()\n}\n",
        )
        .unwrap();
        let second_state = root.join("state-second");
        fs::create_dir_all(&second_state).unwrap();
        let response = execute_materialization_pipeline(&request(
            &source_root,
            &second_state,
            Some(previous),
            vec!["src/caller.rs".to_string()],
            true,
        ))
        .unwrap();

        assert_eq!(response.artifacts_reused, 1);
        assert_eq!(response.artifacts_rebuilt, 1);
        assert!(response.materialized_entries.contains_key("src/helper.rs"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_or_corrupt_unchanged_artifacts_rebuild_from_scanned_sources() {
        let root = unique_temp_dir("codebase-graph-artifact-rebuild");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            true,
        ))
        .unwrap();
        let previous = manifest_from_response(&initial);
        let artifact_root = root.join("artifacts");
        let helper_key = previous.files["src/helper.rs"]
            .artifact_key
            .as_ref()
            .unwrap()
            .clone();
        let helper_dir = artifact_root.join(&helper_key[..2]).join(&helper_key);
        fs::remove_dir_all(&helper_dir).unwrap();

        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool {\n    helper()\n}\n",
        )
        .unwrap();
        let rebuild_state = root.join("state-rebuild");
        fs::create_dir_all(&rebuild_state).unwrap();
        let response = execute_materialization_pipeline(&request(
            &source_root,
            &rebuild_state,
            Some(previous.clone()),
            Vec::new(),
            true,
        ))
        .unwrap();
        assert!(response.artifacts_rebuilt >= 2);

        let corrupt_key = response.materialized_entries["src/helper.rs"]
            .artifact_key
            .as_ref()
            .unwrap()
            .clone();
        let corrupt_path = artifact_root
            .join(&corrupt_key[..2])
            .join(&corrupt_key)
            .join("partition.json");
        fs::write(&corrupt_path, "{not-json").unwrap();
        let corrupt_previous = manifest_from_response(&response);

        let corrupt_state = root.join("state-corrupt");
        fs::create_dir_all(&corrupt_state).unwrap();
        let corrupt_response = execute_materialization_pipeline(&request(
            &source_root,
            &corrupt_state,
            Some(corrupt_previous),
            Vec::new(),
            true,
        ))
        .unwrap();
        assert!(corrupt_response.artifacts_rebuilt >= 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cached_and_clean_builds_stage_identical_rows() {
        let root = unique_temp_dir("codebase-graph-artifact-determinism");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            true,
        ))
        .unwrap();
        let previous = manifest_from_response(&initial);

        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool {\n    helper()\n}\n",
        )
        .unwrap();
        let cached_state = root.join("state-cached");
        fs::create_dir_all(&cached_state).unwrap();
        let cached = execute_materialization_pipeline(&request(
            &source_root,
            &cached_state,
            Some(previous),
            vec!["src/caller.rs".to_string()],
            true,
        ))
        .unwrap();

        let clean_state = root.join("state-clean");
        fs::create_dir_all(&clean_state).unwrap();
        let clean = execute_materialization_pipeline(&request(
            &source_root,
            &clean_state,
            None,
            Vec::new(),
            true,
        ))
        .unwrap();

        assert_eq!(
            copy_statement_shapes(&cached.copy_statements),
            copy_statement_shapes(&clean.copy_statements)
        );
        assert_eq!(cached.materialized_entries, clean.materialized_entries);
        assert_eq!(cached.graph_build_digest, clean.graph_build_digest);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deleted_files_are_excluded_from_materialized_entries() {
        let root = unique_temp_dir("codebase-graph-artifact-delete");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            true,
        ))
        .unwrap();
        let previous = manifest_from_response(&initial);

        fs::remove_file(source_root.join("src/helper.rs")).unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { true }\n",
        )
        .unwrap();
        let delete_state = root.join("state-delete");
        fs::create_dir_all(&delete_state).unwrap();
        let response = execute_materialization_pipeline(&request(
            &source_root,
            &delete_state,
            Some(previous),
            Vec::new(),
            true,
        ))
        .unwrap();

        assert!(!response.materialized_entries.contains_key("src/helper.rs"));
        assert!(response.diff.deleted.contains(&"src/helper.rs".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_summary_counts_unique_ids_once() {
        let entries = BTreeMap::from([
            (
                "src/a.rs".to_string(),
                ManifestEntry {
                    path: "src/a.rs".to_string(),
                    content_hash: "a".to_string(),
                    language: "rust".to_string(),
                    partition_id: "a".to_string(),
                    artifact_key: None,
                    node_ids: vec!["node:shared".to_string(), "node:a".to_string()],
                    edge_ids: vec!["edge:shared".to_string()],
                    node_types: BTreeMap::new(),
                    edge_types: BTreeMap::new(),
                    materialized_at: "unix:0".to_string(),
                },
            ),
            (
                "src/b.rs".to_string(),
                ManifestEntry {
                    path: "src/b.rs".to_string(),
                    content_hash: "b".to_string(),
                    language: "rust".to_string(),
                    partition_id: "b".to_string(),
                    artifact_key: None,
                    node_ids: vec!["node:shared".to_string(), "node:b".to_string()],
                    edge_ids: vec!["edge:shared".to_string(), "edge:b".to_string()],
                    node_types: BTreeMap::new(),
                    edge_types: BTreeMap::new(),
                    materialized_at: "unix:0".to_string(),
                },
            ),
        ]);

        assert_eq!(unique_manifest_ids(&entries, |entry| &entry.node_ids), 3);
        assert_eq!(unique_manifest_ids(&entries, |entry| &entry.edge_ids), 2);
    }

    #[test]
    fn digest_only_build_changes_can_reuse_all_raw_artifacts() {
        let root = unique_temp_dir("codebase-graph-artifact-digest-reuse");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            false,
        ))
        .unwrap();
        let mut previous = manifest_from_response(&initial);

        let second_state = root.join("state-second");
        fs::create_dir_all(&second_state).unwrap();
        let mut second_request = request(
            &source_root,
            &second_state,
            Some(previous.clone()),
            Vec::new(),
            false,
        );
        second_request.include_fts = true;
        let response = execute_materialization_pipeline(&second_request).unwrap();

        assert_eq!(response.artifacts_reused, 2);
        assert_eq!(response.artifacts_rebuilt, 0);

        previous.graph_build_digest = response.graph_build_digest.clone();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn semantic_enrichment_changes_reuse_raw_artifacts_and_rerun_global_enrichment() {
        let root = unique_temp_dir("codebase-graph-artifact-semantic-reuse");
        let source_root = root.join("repository");
        let initial_state = root.join("state-initial");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::create_dir_all(&initial_state).unwrap();
        fs::write(
            source_root.join("src/helper.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            source_root.join("src/caller.rs"),
            "pub fn caller() -> bool { helper() }\n",
        )
        .unwrap();

        let initial = execute_materialization_pipeline(&request(
            &source_root,
            &initial_state,
            None,
            Vec::new(),
            false,
        ))
        .unwrap();
        let previous = manifest_from_response(&initial);

        let semantic_state = root.join("state-semantic");
        fs::create_dir_all(&semantic_state).unwrap();
        let semantic = execute_materialization_pipeline(&request(
            &source_root,
            &semantic_state,
            Some(previous),
            Vec::new(),
            true,
        ))
        .unwrap();

        assert_eq!(semantic.artifacts_reused, 2);
        assert_eq!(semantic.artifacts_rebuilt, 0);
        assert_ne!(initial.graph_build_digest, semantic.graph_build_digest);
        assert!(semantic.edge_rows > initial.edge_rows);
        assert!(semantic.connector_rows > initial.connector_rows);

        let _ = fs::remove_dir_all(root);
    }

    fn request(
        source_root: &Path,
        state_root: &Path,
        previous_manifest: Option<NativeManifest>,
        candidate_paths: Vec<String>,
        semantic_enrichment: bool,
    ) -> NativeSyntaxMaterializationRequest {
        NativeSyntaxMaterializationRequest {
            source_root: source_root.to_string_lossy().into_owned(),
            repository_label: "execution-test".to_string(),
            mode: "changed".to_string(),
            parser_version: "test".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: OntologySchema::default(),
            previous_manifest,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths,
            artifact_root: state_root
                .parent()
                .unwrap_or(state_root)
                .join("artifacts")
                .to_string_lossy()
                .into_owned(),
            db_path: state_root.join("graph.ldb").to_string_lossy().into_owned(),
            include_fts: false,
            semantic_enrichment,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: state_root.join("staging").to_string_lossy().into_owned(),
            atomic_rebuild: false,
            strict: true,
            parallel: false,
            progress: false,
        }
    }

    fn manifest_from_response(response: &NativeSyntaxMaterializationResponse) -> NativeManifest {
        NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "test".to_string(),
            graph_build_digest: response.graph_build_digest.clone(),
            files: response.materialized_entries.clone(),
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nonce}"))
    }

    fn copy_statement_shapes(statements: &[String]) -> Vec<String> {
        statements
            .iter()
            .map(|statement| {
                statement
                    .split(" FROM ")
                    .next()
                    .unwrap_or(statement)
                    .to_string()
            })
            .collect()
    }
}
