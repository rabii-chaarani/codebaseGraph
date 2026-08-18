use super::timing::elapsed_seconds;
use crate::artifact_store::{ArtifactExpectations, ArtifactStore};
use crate::error::{MemoryBudgetExceeded, NativeError};
use crate::hash;
use crate::parser;
use crate::partition_builder;
use crate::protocol::{LanguageProfile, NativeSyntaxMaterializationRequest, SourceSnapshot};
use crate::scan;
use std::fs::File;
use std::io::Read;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Instant;

const PARSE_WORKING_BYTES_PER_SOURCE_BYTE: u64 = 8;
const ARTIFACT_STREAM_WORKING_BYTES: u64 = (2 * 64 * 1024) + (2 * 8 * 1024);
const NODE_BASE_BYTES: u64 = 512;
const EDGE_BASE_BYTES: u64 = 384;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ExecutionPlanStats {
    pub(super) high_water_bytes: u64,
}

pub(super) struct PartitionBuildResult {
    pub(super) partition: partition_builder::GraphPartition,
    pub(super) diagnostics: Vec<String>,
    pub(super) parse_seconds: f64,
    pub(super) graph_build_seconds: f64,
    pub(super) reused_artifact: bool,
    pub(super) accounted_bytes: u64,
}

pub(super) fn build_execution_plan(
    scan: &scan::SourceScan,
    rebuild_paths: &[String],
    artifact_store: &ArtifactStore,
    mut consume: impl FnMut(usize, PartitionBuildResult) -> Result<(), NativeError>,
) -> Result<ExecutionPlanStats, NativeError> {
    let request = &scan.input;
    let worker_count = resolved_worker_count(request, rebuild_paths.len());
    let mut stats = ExecutionPlanStats::default();
    if worker_count > 1 {
        let mut result_slots = Vec::new();
        result_slots.try_reserve(worker_count).map_err(|_| {
            memory_budget_error(
                "execution_plan",
                request.rust_memory_limit_bytes().unwrap_or(u64::MAX),
                request.rust_memory_limit_bytes().unwrap_or(u64::MAX),
            )
        })?;
        result_slots.resize_with(worker_count, || None);
        let (job_tx, job_rx) = mpsc::sync_channel::<usize>(worker_count);
        let (result_tx, result_rx) =
            mpsc::sync_channel::<(usize, Result<PartitionBuildResult, NativeError>)>(worker_count);
        let job_rx = Mutex::new(job_rx);
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let artifact_store = artifact_store.clone();
                let job_rx = &job_rx;
                let result_tx = result_tx.clone();
                handles.push(scope.spawn(move || {
                    loop {
                        let index = match job_rx
                            .lock()
                            .map_err(|_| {
                                NativeError::InvalidInput(
                                    "parallel parser job lock was poisoned".to_string(),
                                )
                            })?
                            .recv()
                        {
                            Ok(index) => index,
                            Err(_) => break,
                        };
                        let result = (
                            index,
                            build_partition_for_path_with_workers(
                                scan,
                                &rebuild_paths[index],
                                &artifact_store,
                                worker_count,
                            ),
                        );
                        if result_tx.send(result).is_err() {
                            break;
                        }
                    }
                    Ok::<_, NativeError>(())
                }));
            }
            drop(result_tx);

            let initially_dispatched = worker_count.min(rebuild_paths.len());
            for index in 0..initially_dispatched {
                job_tx.send(index).map_err(|_| {
                    NativeError::InvalidInput("parallel parser job queue closed".to_string())
                })?;
            }
            let mut next_dispatch = initially_dispatched;
            let mut next_consume = 0usize;
            while next_consume < rebuild_paths.len() {
                let (index, result) = result_rx.recv().map_err(|_| {
                    NativeError::InvalidInput("parallel parser result queue closed".to_string())
                })?;
                let slot = index % worker_count;
                if result_slots[slot].is_some() {
                    return Err(NativeError::InvalidInput(
                        "parallel parser result window overflowed".to_string(),
                    ));
                }
                result_slots[slot] = Some(result);

                while result_slots[next_consume % worker_count].is_some() {
                    let result = result_slots[next_consume % worker_count]
                        .take()
                        .expect("checked result slot should be populated")?;
                    stats.high_water_bytes = stats.high_water_bytes.max(
                        result
                            .accounted_bytes
                            .saturating_mul(worker_count as u64)
                            .min(request.rust_memory_limit_bytes().unwrap_or(u64::MAX)),
                    );
                    consume(next_consume, result)?;
                    next_consume += 1;
                    if next_dispatch < rebuild_paths.len() {
                        job_tx.send(next_dispatch).map_err(|_| {
                            NativeError::InvalidInput(
                                "parallel parser job queue closed".to_string(),
                            )
                        })?;
                        next_dispatch += 1;
                    }
                    if next_consume == rebuild_paths.len() {
                        break;
                    }
                }
            }
            drop(job_tx);
            for handle in handles {
                handle.join().map_err(|_| {
                    NativeError::InvalidInput("parallel parser panicked".to_string())
                })??;
            }
            Ok::<_, NativeError>(())
        })?;
        return Ok(stats);
    }

    for (index, path) in rebuild_paths.iter().enumerate() {
        let result = build_partition_for_path_with_workers(scan, path, artifact_store, 1)?;
        stats.high_water_bytes = stats.high_water_bytes.max(result.accounted_bytes);
        consume(index, result)?;
    }
    Ok(stats)
}

fn resolved_worker_count(request: &NativeSyntaxMaterializationRequest, path_count: usize) -> usize {
    if request.parallel {
        request.max_parallelism.max(1).min(path_count)
    } else {
        path_count.min(1)
    }
}

fn build_partition_for_path_with_workers(
    scan: &scan::SourceScan,
    path: &str,
    artifact_store: &ArtifactStore,
    worker_count: usize,
) -> Result<PartitionBuildResult, NativeError> {
    let Some(snapshot) = scan.supported.get(path) else {
        return Err(NativeError::InvalidInput(format!(
            "missing scanned source for artifact rebuild: {path}"
        )));
    };
    let Some(language) = snapshot.language.as_deref() else {
        return Err(NativeError::InvalidInput(format!(
            "missing language for artifact rebuild: {path}"
        )));
    };
    let Some(profile) = scan
        .profiles
        .iter()
        .find(|profile| profile.language == language)
    else {
        return Err(NativeError::InvalidInput(format!(
            "missing language profile for artifact rebuild: {path}"
        )));
    };

    // A manifest schema upgrade requires one bounded rebuild so the artifact is
    // regenerated under the new schema. Other full-rebuild causes (for example
    // a compatibility-digest change or an explicit full build) may still reuse
    // a byte-compatible raw artifact.
    let previous_entry = scan
        .input
        .previous_manifest
        .as_ref()
        .filter(|manifest| manifest.schema_version == scan.input.manifest_schema_version)
        .and_then(|manifest| manifest.files.get(path));
    build_partition_for_snapshot(
        &scan.input,
        snapshot,
        profile,
        previous_entry,
        artifact_store,
        worker_count,
    )
}

fn build_partition_for_snapshot(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
    profile: &LanguageProfile,
    previous_entry: Option<&crate::protocol::ManifestEntry>,
    artifact_store: &ArtifactStore,
    worker_count: usize,
) -> Result<PartitionBuildResult, NativeError> {
    let per_worker_limit = request
        .rust_memory_limit_bytes()?
        .checked_div(worker_count.max(1) as u64)
        .unwrap_or(0);
    let artifact_key = ArtifactStore::key_for_request(request, snapshot, Some(profile))?;
    if previous_entry.and_then(|entry| entry.artifact_key.as_deref()) == Some(artifact_key.as_str())
    {
        if let Some(partition) = artifact_store.load_partition_with_budget(
            &artifact_key,
            &ArtifactExpectations {
                path: &snapshot.path,
                content_hash: &snapshot.content_hash,
                language: snapshot.language.as_deref().unwrap_or_default(),
            },
            per_worker_limit,
        )? {
            let accounted_bytes =
                ensure_partition_fits(&partition, snapshot.byte_len, per_worker_limit)?;
            return Ok(PartitionBuildResult {
                partition,
                diagnostics: Vec::new(),
                parse_seconds: 0.0,
                graph_build_seconds: 0.0,
                reused_artifact: true,
                accounted_bytes,
            });
        }
    }
    let snapshot = scan::spool_snapshot_for_build(request, snapshot)?;
    let parse_started = Instant::now();
    let mut parse_snapshot = snapshot.clone();
    parse_snapshot.source = Some(read_snapshotted_source(&snapshot, per_worker_limit)?);
    let parse = parser::parse_file(&parse_snapshot, profile)?;
    drop(parse_snapshot);
    let parse_seconds = elapsed_seconds(parse_started);
    let diagnostics = parse.diagnostics.clone();
    let graph_build_started = Instant::now();
    let mut partition = partition_builder::build_partition(request, &snapshot, parse)?;
    partition.set_artifact_key(artifact_key.clone());
    let accounted_bytes = ensure_partition_fits(&partition, snapshot.byte_len, per_worker_limit)?;
    let graph_build_seconds = elapsed_seconds(graph_build_started);
    let _ = artifact_store.store_partition(&artifact_key, &partition)?;
    Ok(PartitionBuildResult {
        partition,
        diagnostics,
        parse_seconds,
        graph_build_seconds,
        reused_artifact: false,
        accounted_bytes,
    })
}

fn read_snapshotted_source(
    snapshot: &SourceSnapshot,
    per_worker_limit: u64,
) -> Result<String, NativeError> {
    let accounted_bytes = snapshot
        .byte_len
        .saturating_mul(PARSE_WORKING_BYTES_PER_SOURCE_BYTE);
    if accounted_bytes > per_worker_limit {
        return Err(memory_budget_error(
            "source_parse",
            per_worker_limit,
            accounted_bytes,
        ));
    }
    let source_capacity = usize::try_from(snapshot.byte_len)
        .map_err(|_| memory_budget_error("source_parse", per_worker_limit, snapshot.byte_len))?;
    let mut source = String::new();
    source
        .try_reserve_exact(source_capacity)
        .map_err(|_| memory_budget_error("source_parse", per_worker_limit, snapshot.byte_len))?;
    File::open(&snapshot.absolute_path)?.read_to_string(&mut source)?;
    if source.len() as u64 != snapshot.byte_len
        || hash::sha256_file(std::path::Path::new(&snapshot.absolute_path))?
            != snapshot.content_hash
    {
        return Err(NativeError::InvalidInput(format!(
            "source snapshot changed before parsing: {}",
            snapshot.path
        )));
    }
    Ok(source)
}

fn ensure_partition_fits(
    partition: &partition_builder::GraphPartition,
    source_bytes: u64,
    per_worker_limit: u64,
) -> Result<u64, NativeError> {
    let source_working = source_bytes.saturating_mul(PARSE_WORKING_BYTES_PER_SOURCE_BYTE);
    let partition_bytes = estimate_partition_bytes(partition).unwrap_or(u64::MAX);
    let accounted_bytes = partition_working_bytes(source_working, partition_bytes);
    if accounted_bytes > per_worker_limit {
        return Err(memory_budget_error(
            "partition_build",
            per_worker_limit,
            accounted_bytes,
        ));
    }
    Ok(accounted_bytes)
}

fn partition_working_bytes(source_working: u64, partition_bytes: u64) -> u64 {
    source_working
        .checked_add(partition_bytes)
        .and_then(|bytes| bytes.checked_add(ARTIFACT_STREAM_WORKING_BYTES))
        .unwrap_or(u64::MAX)
}

fn estimate_partition_bytes(partition: &partition_builder::GraphPartition) -> Option<u64> {
    let mut bytes = (partition.nodes.len() as u64).checked_mul(NODE_BASE_BYTES)?;
    bytes = bytes.checked_add((partition.edges.len() as u64).checked_mul(EDGE_BASE_BYTES)?)?;
    for node in &partition.nodes {
        bytes = checked_add_strings(
            bytes,
            [
                &node.id,
                &node.table,
                &node.label,
                &node.kind,
                &node.language,
                &node.path,
                &node.qualified_name,
                &node.scope_id,
                &node.tree_sitter_node_type,
                &node.capture_name,
                &node.summary,
            ],
        )?;
        if let Some(grammar_version) = &node.grammar_version {
            bytes = bytes.checked_add(grammar_version.len() as u64)?;
        }
    }
    for edge in &partition.edges {
        bytes = checked_add_strings(
            bytes,
            [
                &edge.id,
                &edge.edge_type,
                &edge.source_id,
                &edge.target_id,
                &edge.kind,
            ],
        )?;
        if let Some(field_name) = &edge.field_name {
            bytes = bytes.checked_add(field_name.len() as u64)?;
        }
    }
    Some(bytes)
}

fn checked_add_strings<const N: usize>(mut bytes: u64, strings: [&String; N]) -> Option<u64> {
    for value in strings {
        bytes = bytes.checked_add(value.len() as u64)?;
    }
    Some(bytes)
}

fn memory_budget_error(phase: &str, limit_bytes: u64, accounted_bytes: u64) -> NativeError {
    NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
        phase,
        limit_bytes,
        accounted_bytes,
        0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_store::ArtifactStore;
    use crate::protocol::{
        NativeManifest, NativeSyntaxMaterializationRequest, OntologySchema,
        MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn execution_plan_uses_scanned_source_after_repository_is_removed() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codebase-graph-scanned-source-{nonce}"));
        let source_root = root.join("repository");
        let state_root = root.join("state");
        fs::create_dir_all(source_root.join("src")).expect("source directory should be created");
        fs::create_dir_all(&state_root).expect("state directory should be created");
        fs::write(
            source_root.join("src/lib.rs"),
            "pub fn scanned_source() -> bool { true }\n",
        )
        .expect("source file should be written");

        let request = NativeSyntaxMaterializationRequest {
            source_root: source_root.to_string_lossy().into_owned(),
            repository_label: "scanned-source".to_string(),
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
            artifact_root: state_root.join("artifacts").to_string_lossy().into_owned(),
            db_path: state_root.join("graph").to_string_lossy().into_owned(),
            include_fts: false,
            semantic_enrichment: false,
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
        };
        let scan = crate::scan::scan_sources(&request).expect("scan should succeed");
        let rebuild_paths = scan.diff.rebuild_paths();
        let artifact_store = ArtifactStore::new(request.resolved_artifact_root());

        fs::remove_dir_all(&source_root).expect("source repository should be removed");

        let mut plan = Vec::new();
        build_execution_plan(&scan, &rebuild_paths, &artifact_store, |_, result| {
            plan.push(result);
            Ok(())
        })
        .expect("planning should use the scanned source payload");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].partition.entry.path, "src/lib.rs");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallelism_is_capped_by_the_configured_worker_pool() {
        let mut request: NativeSyntaxMaterializationRequest = serde_json::from_str(
            r#"{
                "source_root":".","repository_label":"repo","mode":"full",
                "parser_version":"test","manifest_schema_version":1,
                "ontology":"code_ontology_v1","profiles":[],"excluded_parts":[],
                "artifact_root":"artifacts","db_path":"graph","include_fts":false,
                "staging_dir":"staging","parallel":true,"max_parallelism":2
            }"#,
        )
        .unwrap();

        assert_eq!(resolved_worker_count(&request, 100), 2);
        assert_eq!(resolved_worker_count(&request, 1), 1);
        assert_eq!(resolved_worker_count(&request, 0), 0);
        request.parallel = false;
        assert_eq!(resolved_worker_count(&request, 100), 1);
    }

    #[test]
    fn bounded_worker_pool_delivers_results_in_path_order() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codebase-graph-worker-order-{nonce}"));
        let source_root = root.join("repository");
        let state_root = root.join("state");
        fs::create_dir_all(source_root.join("src")).unwrap();
        for name in ["d", "b", "a", "c"] {
            fs::write(
                source_root.join(format!("src/{name}.rs")),
                format!("pub fn {name}() {{}}\n"),
            )
            .unwrap();
        }
        let request: NativeSyntaxMaterializationRequest =
            serde_json::from_value(serde_json::json!({
                "source_root": source_root,
                "repository_label": "worker-order",
                "mode": "full",
                "parser_version": "test",
                "manifest_schema_version": 1,
                "ontology": "code_ontology_v1",
                "profiles": [],
                "excluded_parts": [],
                "artifact_root": state_root.join("artifacts"),
                "db_path": state_root.join("graph"),
                "include_fts": false,
                "staging_dir": state_root.join("staging"),
                "parallel": true,
                "max_parallelism": 2
            }))
            .unwrap();
        let scan = crate::scan::scan_sources(&request).unwrap();
        let paths = scan.diff.rebuild_paths();
        let artifact_store = ArtifactStore::new(request.resolved_artifact_root());
        let mut delivered = Vec::new();

        let stats = build_execution_plan(&scan, &paths, &artifact_store, |index, result| {
            assert_eq!(index, delivered.len());
            delivered.push(result.partition.entry.path);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            delivered,
            vec!["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"]
        );
        assert!(stats.high_water_bytes > 0);
        assert!(stats.high_water_bytes <= request.rust_memory_limit_bytes().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prior_manifest_schema_forces_rebuild_even_when_artifact_key_matches() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codebase-graph-v4-rebuild-{nonce}"));
        let source_root = root.join("repository");
        let state_root = root.join("state");
        fs::create_dir_all(source_root.join("src")).unwrap();
        fs::write(source_root.join("src/lib.rs"), "pub fn versioned() {}\n").unwrap();
        let mut request: NativeSyntaxMaterializationRequest =
            serde_json::from_value(serde_json::json!({
                "source_root": source_root,
                "repository_label": "v4-rebuild",
                "mode": "changed",
                "parser_version": "test",
                "manifest_schema_version": MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
                "ontology": "code_ontology_v1",
                "profiles": [],
                "excluded_parts": [],
                "artifact_root": state_root.join("artifacts"),
                "db_path": state_root.join("graph"),
                "include_fts": false,
                "staging_dir": state_root.join("first-staging"),
                "parallel": false
            }))
            .unwrap();
        let initial_scan = crate::scan::scan_sources(&request).unwrap();
        let initial_paths = initial_scan.diff.rebuild_paths();
        let artifact_store = ArtifactStore::new(request.resolved_artifact_root());
        let mut stored_entry = None;
        build_execution_plan(
            &initial_scan,
            &initial_paths,
            &artifact_store,
            |_, result| {
                assert!(!result.reused_artifact);
                stored_entry = Some(result.partition.entry);
                Ok(())
            },
        )
        .unwrap();
        let stored_entry = stored_entry.unwrap();
        request.previous_manifest = Some(NativeManifest {
            schema_version: MATERIALIZATION_MANIFEST_SCHEMA_VERSION - 1,
            ontology: request.ontology.clone(),
            parser_version: request.parser_version.clone(),
            graph_build_digest: Some(request.graph_build_compatibility_digest().unwrap()),
            search_backend: None,
            files: BTreeMap::from([("src/lib.rs".to_string(), stored_entry)]),
        });
        request.staging_dir = state_root
            .join("upgrade-staging")
            .to_string_lossy()
            .into_owned();

        let upgrade_scan = crate::scan::scan_sources(&request).unwrap();
        assert!(upgrade_scan.diff.force_rebuild);
        let mut reused = None;
        build_execution_plan(
            &upgrade_scan,
            &upgrade_scan.diff.rebuild_paths(),
            &artifact_store,
            |_, result| {
                reused = Some(result.reused_artifact);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(reused, Some(false));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_source_returns_structured_budget_failure() {
        let snapshot = SourceSnapshot {
            path: "src/huge.rs".to_string(),
            absolute_path: "missing".to_string(),
            content_hash: "hash".to_string(),
            language: Some("rust".to_string()),
            byte_len: 2 * 1024 * 1024,
            source: None,
        };

        let error = read_snapshotted_source(&snapshot, 1024 * 1024).unwrap_err();
        let NativeError::MemoryBudgetExceeded(error) = error else {
            panic!("oversized source should return a memory budget error");
        };
        assert_eq!(error.phase, "source_parse");
        assert_eq!(error.limit_bytes, 1024 * 1024);
        assert!(error.accounted_bytes > error.limit_bytes);
        assert_eq!(error.observed_rss_bytes, 0);
    }

    #[test]
    fn reference_sized_partition_fits_the_default_per_worker_budget() {
        let reference_partition_bytes = 77 * 1024 * 1024;
        let per_worker_budget_bytes = 192 * 1024 * 1024;

        assert!(partition_working_bytes(0, reference_partition_bytes) <= per_worker_budget_bytes);
    }
}
