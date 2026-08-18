use super::parallel::build_execution_plan;
use super::timing::elapsed_seconds;
use crate::artifact_store::{ArtifactExpectations, ArtifactStore};
use crate::error::{MemoryBudgetExceeded, NativeError};
use crate::partition_builder::GraphPartition;
use crate::protocol::{
    GraphSummary, ManifestEntry, NativeSyntaxMaterializationRequest,
    NativeSyntaxMaterializationResponse, ProgressEvent,
};
use crate::{scan, search_index, staging_writer};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

// Ladybug's bundled JSON reader does not consolidate a list or glob of JSON
// files, so production currently loads one complete table document at a time.
const COPY_FILE_CHUNK_BYTES: usize = usize::MAX;

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
    // A write invocation always assembles a complete candidate generation. Even a
    // source no-op must validate/reuse every artifact,
    // and produce a self-contained database that can be published atomically.
    fs::create_dir_all(&request.staging_dir)?;
    let artifact_store = ArtifactStore::new(request.resolved_artifact_root());
    let spill_chunk_bytes = usize::try_from(request.spill_chunk_mib)
        .ok()
        .and_then(|mib| mib.checked_mul(1024 * 1024))
        .ok_or_else(|| {
            NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
                "staging_spill_configuration",
                request.rust_memory_limit_bytes().unwrap_or(u64::MAX),
                u64::MAX,
                0,
            ))
        })?;
    let mut staging_accumulator = staging_writer::StagingAccumulator::with_limits(
        &request.staging_dir,
        spill_chunk_bytes,
        COPY_FILE_CHUNK_BYTES,
    )?;
    let materialization_paths = scan.supported.keys().cloned().collect::<Vec<_>>();
    let materialization_total = materialization_paths.len();

    let mut parse_diagnostics = Vec::new();
    let mut parse_seconds = 0.0;
    let mut graph_build_seconds = 0.0;
    let mut staging_seconds = 0.0;
    // Keep only the latest frame per phase. Long-running MCP builds stream their
    // progress separately; the materialization result must not retain a history
    // proportional to the number of source files.
    let mut progress_events = BTreeMap::new();
    let mut planned_entries = BTreeMap::new();
    let mut parsed_paths = BTreeSet::new();
    let mut artifacts_reused = 0usize;
    let mut artifacts_rebuilt = 0usize;

    let execution_stats = build_execution_plan(
        &scan,
        &materialization_paths,
        &artifact_store,
        |index, result| {
            parse_seconds += result.parse_seconds;
            graph_build_seconds += result.graph_build_seconds;
            parse_diagnostics.extend(result.diagnostics);
            if result.reused_artifact {
                artifacts_reused += 1;
            } else {
                artifacts_rebuilt += 1;
                parsed_paths.insert(result.partition.entry.path.clone());
            }
            if request.progress {
                progress_events.insert(
                    "parsed".to_string(),
                    ProgressEvent {
                        phase: "parsed".to_string(),
                        current: index + 1,
                        total: materialization_total,
                        path: Some(result.partition.entry.path.clone()),
                    },
                );
            }
            planned_entries.insert(
                result.partition.entry.path.clone(),
                result.partition.entry.compact(),
            );
            Ok(())
        },
    )?;

    phase_timings.insert("parse_seconds".to_string(), parse_seconds);
    phase_timings.insert("graph_build_seconds".to_string(), graph_build_seconds);

    let scan::SourceScan {
        input: request,
        profiles,
        snapshots,
        supported,
        mut diagnostics,
        diff,
    } = scan;
    diagnostics.extend(parse_diagnostics);
    let artifact_memory_limit = request.rust_memory_limit_bytes()?;
    let mut materialized_entries = BTreeMap::new();
    let mut rebuilt_entries = BTreeMap::new();
    for (index, path) in materialization_paths.iter().enumerate() {
        let partition = load_planned_partition(
            &supported,
            path,
            &planned_entries,
            &artifact_store,
            artifact_memory_limit,
        )?;
        let compact_entry = partition.entry.clone().compact();
        materialized_entries.insert(path.clone(), compact_entry.clone());
        if parsed_paths.contains(path) {
            rebuilt_entries.insert(path.clone(), compact_entry);
        }
        let staging_started = Instant::now();
        staging_accumulator.add_partition(&partition);
        staging_seconds += elapsed_seconds(staging_started);
        if request.progress {
            progress_events.insert(
                "staged".to_string(),
                ProgressEvent {
                    phase: "staged".to_string(),
                    current: index + 1,
                    total: materialization_total,
                    path: Some(partition.entry.path.clone()),
                },
            );
        }
    }

    let staging_started = Instant::now();
    let staging = staging_accumulator.finish()?;
    staging_seconds += elapsed_seconds(staging_started);
    phase_timings.insert("staging_seconds".to_string(), staging_seconds);

    let graph_summary = GraphSummary {
        node_count: staging.unique_node_count,
        edge_count: staging.unique_edge_count,
    };
    let graph_build_digest = request.graph_build_compatibility_digest()?;
    // The scan's supported-source index and planner maps are no longer needed.
    // Release them before entering native database loading; only compact result
    // data and the small COPY statement list cross that phase boundary.
    drop(supported);
    drop(profiles);
    drop(materialization_paths);
    drop(planned_entries);
    drop(parsed_paths);
    drop(artifact_store);

    let mut search_backend = None;
    let mut search_spill_bytes = 0_u64;
    let mut search_high_water_bytes = None;
    if request.include_fts {
        let search_started = Instant::now();
        let search = search_index::build(search_index::SearchIndexBuildRequest {
            db_path: Path::new(&request.db_path),
            staging_dir: Path::new(&request.staging_dir),
            chunk_bytes: spill_chunk_bytes,
        })?;
        search_backend = Some(search.metadata);
        search_spill_bytes = search.spill_bytes;
        search_high_water_bytes = Some(search.high_water_bytes);
        phase_timings.insert(
            "search_index_seconds".to_string(),
            elapsed_seconds(search_started),
        );
    }

    let graph_write_started = Instant::now();
    let database_metrics = staging_writer::write_graph_rows(
        &request,
        &staging.copy_statements,
        search_backend.is_some(),
    )?;
    phase_timings.insert(
        "database_write_seconds".to_string(),
        elapsed_seconds(graph_write_started),
    );

    let mut response = NativeSyntaxMaterializationResponse::from_parts(
        snapshots,
        diff,
        diagnostics,
        rebuilt_entries,
        materialized_entries,
        graph_summary,
        staging,
        phase_timings,
    );
    response.progress_events = progress_events.into_values().collect();
    response.artifacts_reused = artifacts_reused;
    response.artifacts_rebuilt = artifacts_rebuilt;
    response.graph_build_digest = Some(graph_build_digest);
    response.search_backend = search_backend;
    response.spill_bytes = response.spill_bytes.saturating_add(search_spill_bytes);
    response
        .phase_high_water_marks
        .insert("parse".to_string(), execution_stats.high_water_bytes);
    if let Some(high_water_bytes) = search_high_water_bytes {
        response
            .phase_high_water_marks
            .insert("search_index".to_string(), high_water_bytes);
    }
    if database_metrics.high_water_bytes > 0 {
        response.phase_high_water_marks.insert(
            "database_write".to_string(),
            database_metrics.high_water_bytes,
        );
    }
    response.database_written = true;
    Ok(response)
}

fn load_planned_partition(
    supported: &BTreeMap<String, crate::protocol::SourceSnapshot>,
    path: &str,
    planned_entries: &BTreeMap<String, ManifestEntry>,
    artifact_store: &ArtifactStore,
    memory_limit_bytes: u64,
) -> Result<GraphPartition, NativeError> {
    let snapshot = supported.get(path).ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "missing scanned source metadata after execution planning: {path}"
        ))
    })?;
    let planned_entry = planned_entries.get(path).ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "missing planned manifest entry after artifact persistence: {path}"
        ))
    })?;
    let artifact_key = planned_entry.artifact_key.as_deref().ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "missing artifact key after execution planning: {path}"
        ))
    })?;
    artifact_store
        .load_partition_with_budget(
            artifact_key,
            &ArtifactExpectations {
                path,
                content_hash: &snapshot.content_hash,
                language: snapshot.language.as_deref().unwrap_or_default(),
            },
            memory_limit_bytes,
        )?
        .ok_or_else(|| {
            NativeError::InvalidInput(format!(
                "persisted artifact could not be reloaded after execution planning: {path}"
            ))
        })
}

#[cfg(test)]
fn unique_manifest_ids(
    entries: &BTreeMap<String, crate::protocol::ManifestEntry>,
    ids: impl Fn(&crate::protocol::ManifestEntry) -> &[String],
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
    use crate::protocol::{ManifestEntry, NativeManifest, OntologySchema};
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
                    node_count: 2,
                    edge_count: 1,
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
                    node_count: 2,
                    edge_count: 2,
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
    fn retired_semantic_settings_do_not_change_materialization() {
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
        assert_eq!(initial.graph_build_digest, semantic.graph_build_digest);
        assert_eq!(initial.edge_rows, semantic.edge_rows);
        assert_eq!(initial.connector_rows, semantic.connector_rows);
        assert!(!semantic.phase_high_water_marks.contains_key("semantic"));

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
            worker_memory_mib: 768,
            rust_memory_mib: 384,
            spill_chunk_mib: 32,
            max_parallelism: 2,
            progress: false,
        }
    }

    fn manifest_from_response(response: &NativeSyntaxMaterializationResponse) -> NativeManifest {
        NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "test".to_string(),
            graph_build_digest: response.graph_build_digest.clone(),
            search_backend: response.search_backend.clone(),
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
