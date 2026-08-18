use codebase_graph::api::{
    CodebaseGraphApi, MaterializationRequest, OperationRequest, OutputFormat, QueryRequest,
    RepoSelector, SearchRequest,
};
use codebase_graph::protocol::NativeManifest;
use serde_json::json;
use std::fs::{self, File, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn managed_materialization_activates_generations_atomically() {
    let repo = temp_repo("managed_materialization_activates_generations_atomically");
    write_managed_config(&repo);
    write_source(&repo, "pub fn version() -> &'static str { \"v1\" }\n");

    let first = materialize_ok(&repo, None, None);
    assert_eq!(first["storage_format"], "managed_v2");
    let storage_root = repo.join(".codebaseGraph").join("storage");
    let first_generation = active_generation_id(&storage_root);
    let first_generation_root = generation_root(&storage_root, &first_generation);
    let lease = shared_generation_lease(&first_generation_root);

    write_source(&repo, "pub fn version() -> &'static str { \"v2\" }\n");
    let second = materialize_ok(&repo, None, None);
    let second_generation = second["active_generation"]
        .as_str()
        .expect("managed publish should expose active generation")
        .to_string();
    assert_ne!(first_generation, second_generation);
    assert_eq!(active_generation_id(&storage_root), second_generation);
    assert_eq!(second["cleanup_pending"], json!(true));
    assert!(first_generation_root.exists());

    drop(lease);

    write_source(&repo, "pub fn version() -> &'static str { \"v3\" }\n");
    let third = materialize_ok(&repo, None, None);
    assert_eq!(third["storage_format"], "managed_v2");
    assert_eq!(third["cleanup_pending"], json!(false));
    assert!(!first_generation_root.exists());

    let active_manifest = read_manifest(
        &generation_root(&storage_root, &active_generation_id(&storage_root)).join("manifest.json"),
    );
    assert_eq!(active_manifest.schema_version, 5);
    assert!(active_manifest.graph_build_digest.is_some());
}

#[cfg(unix)]
#[test]
fn managed_publish_failure_preserves_active_generation() {
    let repo = temp_repo("managed_publish_failure_preserves_active_generation");
    write_managed_config(&repo);
    write_source(&repo, "pub fn version() -> &'static str { \"stable\" }\n");

    materialize_ok(&repo, None, None);
    let storage_root = repo.join(".codebaseGraph").join("storage");
    let original_generation = active_generation_id(&storage_root);

    let original_permissions = fs::metadata(&storage_root)
        .expect("storage root metadata should exist")
        .permissions()
        .mode();
    fs::set_permissions(&storage_root, fs::Permissions::from_mode(0o555))
        .expect("storage root should become read-only");

    write_source(&repo, "pub fn version() -> &'static str { \"broken\" }\n");
    let error = materialize_err(&repo, None, None);

    fs::set_permissions(
        &storage_root,
        fs::Permissions::from_mode(original_permissions),
    )
    .expect("storage root permissions should be restored");

    assert_eq!(error.code, "materialization_failed");
    assert_eq!(active_generation_id(&storage_root), original_generation);
    assert!(generation_root(&storage_root, &original_generation).exists());
}

#[test]
fn direct_materialization_uses_custom_paths_and_cleans_shadow_files() {
    // Keep the longest staged connector path below Windows' classic MAX_PATH limit.
    let repo = temp_repo("direct_materialization");
    write_source(&repo, "pub fn direct_custom() -> bool { true }\n");

    let custom_root = repo.join("custom-output");
    fs::create_dir_all(&custom_root).expect("custom output root should exist");
    let db_path = custom_root.join("graph.ldb");
    let manifest_path = custom_root.join("manifest.json");

    let payload = materialize_ok(&repo, Some(db_path.clone()), Some(manifest_path.clone()));
    assert_eq!(payload["storage_format"], "direct");
    assert_eq!(payload["active_generation"], json!(null));
    assert_eq!(payload["cleanup_pending"], json!(false));
    assert_eq!(payload["pending_runs"], json!(0));

    let manifest = read_manifest(&manifest_path);
    assert_eq!(manifest.schema_version, 5);
    assert_eq!(
        manifest.graph_build_digest.as_deref(),
        payload["graph_build_digest"].as_str()
    );
    assert!(!manifest.files.is_empty());
    assert!(manifest.files.values().all(|entry| entry
        .artifact_key
        .as_deref()
        .is_some_and(|value| !value.is_empty())));

    let db_candidate = sibling_candidate_path(&db_path);
    let manifest_candidate = sibling_candidate_path(&manifest_path);
    assert!(!db_candidate.exists());
    assert!(!manifest_candidate.exists());
}

#[test]
fn direct_search_sidecars_publish_with_the_database_and_remain_queryable() {
    let repo = temp_repo("direct_search_sidecars");
    write_source(&repo, "pub fn searchable_sidecar() -> bool { true }\n");
    let output = repo.join("output");
    fs::create_dir_all(&output).unwrap();
    let db_path = output.join("graph.ldb");
    let manifest_path = output.join("manifest.json");
    let mut request =
        materialize_request(&repo, Some(db_path.clone()), Some(manifest_path.clone()));
    request.include_fts = true;
    let payload = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Materialize(request))
        .unwrap()
        .payload;
    assert_eq!(payload["search_backend"]["backend"], "disk_bm25_v1");

    let manifest = read_manifest(&manifest_path);
    let backend = manifest.search_backend.unwrap();
    for suffix in backend.files.keys() {
        assert!(PathBuf::from(format!("{}.{}", db_path.display(), suffix)).is_file());
        assert!(!PathBuf::from(format!(
            "{}.{}",
            sibling_candidate_path(&db_path).display(),
            suffix
        ))
        .exists());
    }

    let search = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Search(SearchRequest {
            repo: RepoSelector {
                repo_root: Some(repo.clone()),
                config_path: None,
                db_path: Some(db_path),
                manifest_path: Some(manifest_path),
            },
            query: "searchable_sidecar".to_string(),
            layer: "semantic".to_string(),
            profile: "brief".to_string(),
            limit: 3,
            budget: 0,
            context_limit: 0,
            max_depth: None,
            detail: "slim".to_string(),
            output_format: OutputFormat::Typed,
        }))
        .unwrap()
        .payload;
    assert!(search["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| result["label"] == "searchable_sidecar"));
}

#[test]
fn managed_materialization_survives_ten_updates_without_generation_leaks() {
    // Keep nested staging paths below the classic Windows MAX_PATH limit.
    let repo = temp_repo("managed_generation_churn");
    write_managed_config(&repo);
    let storage_root = repo.join(".codebaseGraph").join("storage");

    for index in 0..10 {
        write_source(
            &repo,
            &format!("pub fn churned() -> usize {{ {} }}\n", index + 1),
        );
        let payload = materialize_ok(&repo, None, None);
        assert_eq!(payload["storage_format"], "managed_v2");
    }

    let generations_root = storage_root.join("generations");
    let generation_count = fs::read_dir(&generations_root)
        .expect("generation root should exist")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                && entry.file_name().to_string_lossy().starts_with("gen-")
        })
        .count();
    assert_eq!(
        generation_count, 1,
        "expected exactly one active generation"
    );
    let pending_runs = fs::read_dir(storage_root.join("runs"))
        .expect("runs root should exist")
        .filter_map(Result::ok)
        .count();
    assert_eq!(pending_runs, 0, "expected no run workspaces at idle");

    let active_root = generation_root(&storage_root, &active_generation_id(&storage_root));
    let active_manifest = read_manifest(&active_root.join("manifest.json"));
    assert_eq!(active_manifest.schema_version, 5);
    assert!(active_manifest.graph_build_digest.is_some());
    assert!(active_manifest.files.values().all(|entry| entry
        .artifact_key
        .as_deref()
        .is_some_and(|value| !value.is_empty())));

    let query = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Query(QueryRequest {
            repo: RepoSelector {
                repo_root: Some(repo.clone()),
                ..RepoSelector::default()
            },
            statement: "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1".to_string(),
            parameters: json!({}),
            limit: 1,
            output_format: OutputFormat::Typed,
        }))
        .expect("final churned generation should remain queryable");
    assert!(query.payload["rows"][0][0].as_u64().unwrap() > 0);

    let clean_repo = temp_repo("managed_materialization_clean_control");
    write_managed_config(&clean_repo);
    write_source(&clean_repo, "pub fn churned() -> usize { 10 }\n");
    materialize_ok(&clean_repo, None, None);
    let clean_storage_root = clean_repo.join(".codebaseGraph").join("storage");
    let clean_active_root = generation_root(
        &clean_storage_root,
        &active_generation_id(&clean_storage_root),
    );
    let churned_size = physical_database_size(&active_root);
    let clean_size = physical_database_size(&clean_active_root);
    let allowance = (clean_size / 10).max(8 * 1024 * 1024);
    assert!(
        churned_size <= clean_size.saturating_add(allowance),
        "churned database used {churned_size} bytes; clean control used {clean_size} bytes"
    );
}

fn materialize_ok(
    repo_root: &Path,
    db_path: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
) -> serde_json::Value {
    CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Materialize(materialize_request(
            repo_root,
            db_path,
            manifest_path,
        )))
        .expect("materialization should succeed")
        .payload
}

fn materialize_err(
    repo_root: &Path,
    db_path: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
) -> codebase_graph::api::ApiError {
    CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Materialize(materialize_request(
            repo_root,
            db_path,
            manifest_path,
        )))
        .expect_err("materialization should fail")
}

fn materialize_request(
    repo_root: &Path,
    db_path: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
) -> MaterializationRequest {
    MaterializationRequest {
        repo: RepoSelector {
            repo_root: Some(repo_root.to_path_buf()),
            config_path: None,
            db_path,
            manifest_path,
        },
        native_request_path: None,
        source_root: None,
        mode: "full".to_string(),
        include_fts: false,
        semantic_enrichment: false,
        semantic_provider_mode: "local_only".to_string(),
        use_git: false,
        git_diff: false,
        git_base: None,
        include_patterns: Vec::new(),
        exclude_patterns: Vec::new(),
        candidate_paths: Vec::new(),
        parallel: false,
        worker_memory_mib: None,
        rust_memory_mib: None,
        spill_chunk_mib: None,
        max_parallelism: None,
        progress: false,
        output_format: OutputFormat::Typed,
    }
}

fn write_managed_config(repo_root: &Path) {
    let state_root = repo_root.join(".codebaseGraph");
    fs::create_dir_all(&state_root).expect("state root should exist");
    fs::write(
        state_root.join("config.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 2,
            "repo_root": repo_root,
        }))
        .expect("config should serialize"),
    )
    .expect("config should be written");
}

fn write_source(repo_root: &Path, source: &str) {
    let src_dir = repo_root.join("src");
    fs::create_dir_all(&src_dir).expect("src dir should exist");
    fs::write(src_dir.join("lib.rs"), source).expect("source should be written");
}

fn read_manifest(path: &Path) -> NativeManifest {
    serde_json::from_slice(
        &fs::read(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn active_generation_id(storage_root: &Path) -> String {
    let payload: serde_json::Value = serde_json::from_slice(
        &fs::read(storage_root.join("active.json")).expect("managed active pointer should exist"),
    )
    .expect("managed active pointer should parse");
    payload["generation_id"]
        .as_str()
        .expect("active pointer should contain a generation id")
        .to_string()
}

fn generation_root(storage_root: &Path, generation_id: &str) -> PathBuf {
    storage_root
        .join("generations")
        .join(format!("gen-{generation_id}"))
}

fn sibling_candidate_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("graph");
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{file_name}.candidate"))
}

fn shared_generation_lease(generation_root: &Path) -> File {
    let lease_path = generation_root.join("lease.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lease_path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", lease_path.display()));
    fs2::FileExt::lock_shared(&file)
        .unwrap_or_else(|error| panic!("failed to lock {}: {error}", lease_path.display()));
    file
}

fn physical_database_size(generation_root: &Path) -> u64 {
    [
        "graph.ldb",
        "graph.ldb.wal",
        "graph.ldb.tmp",
        "graph.ldb.lock",
    ]
    .into_iter()
    .map(|name| generation_root.join(name))
    .filter_map(|path| fs::metadata(path).ok())
    .map(|metadata| allocated_size(&metadata))
    .sum()
}

#[cfg(unix)]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn temp_repo(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "codebase_graph_generation_storage_{name}_{}_{}",
        std::process::id(),
        sequence
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap_or_else(|error| {
            panic!(
                "failed to remove stale temp dir {}: {error}",
                root.display()
            )
        });
    }
    fs::create_dir_all(&root)
        .unwrap_or_else(|error| panic!("failed to create temp dir {}: {error}", root.display()));
    root
}
