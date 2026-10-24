//! DC-S5 real collector qualification. No synthetic ownership or retirement receipts.
//! This module is compiled inside the product library to use its actual admission.
use crate::api::context::RepositoryIdentity;
use crate::api::{
    CodebaseGraphApi, ContextRequest, HealthRequest, MaterializationRequest, McpTransport,
    OperationRequest, OutputFormat, QueryRequest, RepoSelector, RepositoryLifecycleRequest,
    SearchRequest,
};
use crate::coordinator::ExecutionDispatcher;
use crate::protocol::{MaintenanceSummary, NativeManifest, NativeSyntaxMaterializationResponse};
use crate::storage::layout::ManagedLayout;
use crate::storage::maintenance::{start_controller, MAINTENANCE_MEMORY_BYTES};
use crate::storage::maintenance_budget::CleanupMemory;
use crate::storage::managed::ManagedStore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::SystemTime,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const STAGING_FIXTURE_FUNCTIONS: usize = 4;
struct FixtureRoot {
    path: PathBuf,
}
impl FixtureRoot {
    fn new() -> Self {
        for _ in 0..100 {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "cgraph-s5-collector-{}-{sequence:016x}",
                std::process::id()
            ));
            match fs::create_dir(&parent) {
                Ok(()) => {
                    let path = parent.join("repository");
                    fs::create_dir(&path).unwrap();
                    let path = fs::canonicalize(path).unwrap();
                    return Self { path };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("allocate fixture: {error}"),
            }
        }
        panic!("exclusive fixture allocation exhausted")
    }
    fn path(&self) -> &Path {
        &self.path
    }
    fn write_revision(&self, revision: usize) {
        fs::create_dir_all(self.path.join("src")).unwrap();
        if !self.path.join("src/stable.rs").exists() {
            fs::write(
                self.path.join("src/stable.rs"),
                "fn stable_helper() -> usize { 7 }\n",
            )
            .unwrap();
        }
        fs::write(self.path.join("src/lib.rs"), format!("mod stable;\npub use stable::stable_helper;\nfn revision_{revision}() -> usize {{ stable_helper() + {revision} }}\n")).unwrap();
    }
}
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retained failed storage qualification fixture: {}",
                self.path.display()
            );
            return;
        }
        let _ = fs::remove_dir_all(self.path.parent().unwrap());
    }
}

// These read oracles retain the same ranked fields and complete context as the
// external workload fixture. Opaque node IDs alone are omitted between roots.
fn selector(root: &FixtureRoot) -> RepoSelector {
    RepoSelector {
        repo_root: Some(root.path().to_path_buf()),
        ..RepoSelector::default()
    }
}

fn execute(request: OperationRequest) -> Result<Value, String> {
    CodebaseGraphApi::new()
        .execute_operation(&request)
        .map(|response| response.payload)
        .map_err(|error| format!("{}: {}", error.code, error.message))
}

/// Install through the public producer; do not synthesize a managed manifest or
/// ownership record. MCP/client and agent-hook installation are explicitly off.
fn install(root: &FixtureRoot) -> Result<Value, String> {
    execute(OperationRequest::Setup(RepositoryLifecycleRequest {
        repo: selector(root),
        action: "setup".into(),
        output_format: OutputFormat::Typed,
        dry_run: false,
        mcp_client: Some("none".into()),
        agent_hooks: "none".into(),
        mcp_config_path: None,
        instructions_target: Some("skip".into()),
        skip_mcp_config: true,
        mcp_transport: McpTransport::Stdio,
        mcp_daemon_port: None,
        mode: "full".into(),
        include_fts: true,
    }))
}

fn materialize(
    root: &FixtureRoot,
    mode: &str,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    materialize_selected(selector(root), mode)
}

fn materialize_selected(
    repo: RepoSelector,
    mode: &str,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    let payload = execute(OperationRequest::Materialize(MaterializationRequest {
        repo,
        native_request_path: None,
        source_root: None,
        mode: mode.into(),
        include_fts: true,
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
        max_parallelism: Some(1),
        progress: false,
        output_format: OutputFormat::Typed,
    }))?;
    serde_json::from_value(payload).map_err(|error| format!("materialization response: {error}"))
}

fn health(root: &FixtureRoot) -> Result<Value, String> {
    execute(OperationRequest::Health(HealthRequest {
        repo: selector(root),
        confirm_legacy_quiescence: false,
        refresh_status: None,
        output_format: OutputFormat::Typed,
    }))
}

#[derive(Debug, PartialEq, Eq)]
struct BehaviorSnapshot {
    function_rows: Vec<Value>,
    stable_search: Vec<Value>,
    revision_search: Vec<Value>,
    revision_context: Vec<Value>,
}

impl BehaviorSnapshot {
    fn observe(root: &FixtureRoot, revision: usize) -> Result<Self, String> {
        let query = execute(OperationRequest::Query(QueryRequest {
            repo: selector(root),
            statement: "MATCH (n:Function) RETURN n.label, n.path, n.line_start, n.line_end, n.summary ORDER BY n.path, n.line_start, n.label".into(),
            parameters: json!({}), limit: 100, output_format: OutputFormat::Typed,
        }))?;
        if query["truncated"] != false {
            return Err("function query is truncated or lacks completeness".into());
        }
        let function_rows = query["rows"]
            .as_array()
            .ok_or("missing function rows")?
            .clone();
        let revision_name = format!("revision_{revision}");
        let mut names = Vec::new();
        for row in &function_rows {
            let row = row.as_array().ok_or("function row is not an array")?;
            if row.len() != 5 {
                return Err("function projection columns changed".into());
            }
            names.push(row[0].as_str().ok_or("function name is not a string")?);
        }
        names.sort_unstable();
        let mut expected = vec![revision_name.as_str(), "stable_helper"];
        expected.sort_unstable();
        if names != expected {
            return Err(format!("unexpected reachable function names: {names:?}"));
        }
        let stable = search(root, "stable_helper")?;
        let current = search(root, &revision_name)?;
        let selected = current
            .iter()
            .find(|hit| hit["label"] == revision_name && hit["type"] == "Function")
            .ok_or("search did not find current revision function")?;
        if !stable
            .iter()
            .any(|hit| hit["label"] == "stable_helper" && hit["type"] == "Function")
        {
            return Err("search did not find unchanged helper".into());
        }
        let context = execute(OperationRequest::Context(ContextRequest {
            repo: selector(root),
            query: None,
            layer: "semantic".into(),
            profile: "definitions".into(),
            limit: 100,
            budget: 10000,
            context_limit: 100,
            max_depth: Some(1),
            detail: "slim".into(),
            node_id: Some(
                selected["id"]
                    .as_str()
                    .ok_or("search result has no node identity")?
                    .into(),
            ),
            node_type: Some("Function".into()),
            output_format: OutputFormat::Typed,
        }))?;
        let context = context["context"]
            .as_array()
            .ok_or("missing explicit-node context")?;
        if context.is_empty() {
            return Err("definitions context is unexpectedly empty".into());
        }
        let mut revision_context = context
            .iter()
            .map(project_context)
            .collect::<Result<Vec<_>, _>>()?;
        revision_context.sort_by_key(Value::to_string);
        Ok(Self {
            function_rows,
            stable_search: project_search(&stable)?,
            revision_search: project_search(&current)?,
            revision_context,
        })
    }
}

fn search(root: &FixtureRoot, query: &str) -> Result<Vec<Value>, String> {
    let result = execute(OperationRequest::Search(SearchRequest {
        repo: selector(root),
        query: query.into(),
        layer: "semantic".into(),
        profile: "definitions".into(),
        limit: 100,
        budget: 10000,
        context_limit: 2,
        max_depth: Some(1),
        detail: "slim".into(),
        output_format: OutputFormat::Typed,
    }))?;
    result["results"]
        .as_array()
        .cloned()
        .ok_or("missing search results".into())
}

/// Omit only repository-specific node IDs/evidence chains. Preserve ranked
/// order, scores, paths, source spans and content; context keeps multiplicity.
fn project_search(rows: &[Value]) -> Result<Vec<Value>, String> {
    rows.iter()
        .map(|row| {
            project(
                row,
                &["type", "label", "layer", "path", "rank_score"],
                &["span", "summary"],
            )
        })
        .collect()
}

fn project_context(row: &Value) -> Result<Value, String> {
    project(
        row,
        &["type", "label", "layer", "path", "direction", "relation"],
        &["span", "summary"],
    )
}

fn project(row: &Value, required: &[&str], optional: &[&str]) -> Result<Value, String> {
    let source = row.as_object().ok_or("response row is not an object")?;
    let mut projected = serde_json::Map::new();
    for key in required {
        let value = source
            .get(*key)
            .filter(|value| !value.is_null())
            .ok_or_else(|| format!("response row lacks {key}"))?;
        if (*key == "rank_score" && !value.is_number())
            || (*key != "rank_score" && !value.is_string())
        {
            return Err(format!("response row has invalid {key} type"));
        }
        projected.insert((*key).into(), value.clone());
    }
    for key in optional {
        if let Some(value) = source.get(*key) {
            if (*key == "summary" && !value.is_string()) || (*key == "span" && !value.is_object()) {
                return Err(format!("response row has invalid {key} type"));
            }
            projected.insert((*key).into(), value.clone());
        }
    }
    Ok(Value::Object(projected))
}

fn collector_attempt(root: &FixtureRoot) -> MaintenanceSummary {
    let summary = collector_attempt_observed(root);
    assert!(
        summary.errors.is_empty(),
        "collector reported error: {summary:?}"
    );
    summary
}

fn collector_attempt_observed(root: &FixtureRoot) -> MaintenanceSummary {
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let baseline = memory.used();
    let target = RepositoryIdentity::capture(&selector(root))
        .unwrap()
        .maintenance_target(&memory)
        .unwrap();
    let (mut runtime, client) = start_controller(
        target,
        memory.clone(),
        Arc::new(ExecutionDispatcher::default()),
        false,
    )
    .unwrap();
    client.wait_for_first_attempt();
    runtime.stop_and_join();
    let projection = client.projection();
    let summary = projection.summary.clone();
    if let Some(attempt) = &summary.last_attempt {
        if let Some(entries) = attempt.attempted_entries {
            assert!(entries <= 100, "mutation entry grant exceeded: {summary:?}");
        }
        if let Some(deleted) = attempt.deleted_entries {
            assert!(
                deleted <= 100,
                "mutation deletion count exceeded grant: {summary:?}"
            );
            if deleted > 0 {
                assert!(
                    attempt
                        .attempted_entries
                        .is_some_and(|entries| entries >= deleted),
                    "actual deletion lacks its charged mutation grant: {summary:?}"
                );
            }
        }
    }
    drop(projection);
    drop(client);
    drop(runtime);
    assert_eq!(
        memory.used(),
        baseline,
        "joined collector retained owned allocations"
    );
    assert!(memory.peak() <= memory.limit());
    summary
}

fn committed(root: &FixtureRoot) -> (PathBuf, String, NativeManifest) {
    let response = health(root).unwrap();
    assert_eq!(response["graph_readable"], true);
    // A count includes real closed producer roots retained for bounded GC.
    // It alone cannot classify live/recovery authority or verified idleness.
    let storage = fs::canonicalize(response["storage_root"].as_str().unwrap()).unwrap();
    assert!(storage.starts_with(fs::canonicalize(root.path()).unwrap()));
    let generation = response["active_generation"].as_str().unwrap().to_owned();
    let manifest_path = fs::canonicalize(response["manifest_path"].as_str().unwrap()).unwrap();
    assert!(manifest_path.starts_with(&storage));
    let manifest = serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    (storage, generation, manifest)
}

fn actual_generation_ids(storage: &Path) -> BTreeSet<String> {
    fs::read_dir(storage.join("generations"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(
                entry.file_type().unwrap().is_dir(),
                "foreign generation entry retained for investigation"
            );
            let name = entry.file_name().into_string().unwrap();
            name.strip_prefix("gen-")
                .expect("real managed generation directory prefix")
                .to_owned()
        })
        .collect()
}
fn cache_keys(storage: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for shard in fs::read_dir(storage.join("artifacts")).unwrap() {
        let shard = shard.unwrap();
        let name = shard.file_name().into_string().unwrap();
        let kind = shard.file_type().unwrap();
        if [
            ".use.lock",
            ".use-cohort.json",
            ".reference-epoch",
            ".reference-activity.lock",
            ".reference-epoch.lock",
            ".publication.binding",
            ".publication.data0",
            ".publication.data1",
            ".publication.selector",
            ".publication.stream",
            ".workspace-removal.data0",
            ".workspace-removal.data1",
            ".workspace-removal.selector",
            ".workspace-removal.binding",
        ]
        .contains(&name.as_str())
            && kind.is_file()
        {
            continue;
        }
        if name.len() != 2 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) || !kind.is_dir() {
            keys.insert(format!("!unclassified-root:{name}"));
            continue;
        }
        for entry in fs::read_dir(shard.path()).unwrap() {
            let entry = entry.unwrap();
            let key = entry.file_name().into_string().unwrap();
            let kind = entry.file_type().unwrap();
            if key.len() != 64
                || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !key.starts_with(&name)
                || !kind.is_dir()
            {
                keys.insert(format!("!unclassified-key:{name}/{key}"));
                continue;
            }
            let payload = fs::symlink_metadata(entry.path().join("partition.json"));
            if !payload.is_ok_and(|metadata| metadata.file_type().is_file()) {
                keys.insert(format!("!incomplete-key:{name}/{key}"));
            }
            for leaf in fs::read_dir(entry.path()).unwrap() {
                let leaf = leaf.unwrap();
                let name = leaf.file_name().into_string().unwrap();
                if !leaf.file_type().unwrap().is_file()
                    || !["partition.json", ".entry-origin"].contains(&name.as_str())
                {
                    keys.insert(format!("!unclassified-leaf:{key}/{name}"));
                }
            }
            if !active_origin_matches_payload(&entry.path(), &key) {
                keys.insert(format!("!unverified-origin:{key}"));
            }
            keys.insert(key);
        }
    }
    // Transient or unknown entries remain explicit non-key members, so they
    // prevent final convergence while allowing another real bounded attempt.
    keys
}

/// Independent observation of the actual producer's fixed origin journal. This
/// verifies bytes and phase completion only; it never constructs authority.
fn active_origin_matches_payload(directory: &Path, key: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(directory.join(".entry-origin")) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() != 2048 {
        return false;
    }
    let Ok(bytes) = fs::read(directory.join(".entry-origin")) else {
        return false;
    };
    for (phase, frame) in bytes.chunks_exact(512).enumerate() {
        if &frame[..8] != b"CGORIG01"
            || frame[24] != phase as u8
            || frame[480..] != Sha256::digest(&frame[..480])[..]
        {
            return false;
        }
    }
    let last = &bytes[1536..];
    let observed_key = last[64..96]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if observed_key != key {
        return false;
    }
    let Ok(metadata) = fs::symlink_metadata(directory.join("partition.json")) else {
        return false;
    };
    if !metadata.file_type().is_file()
        || metadata.len() != u64::from_le_bytes(last[256..264].try_into().unwrap())
    {
        return false;
    }
    observe_file(&directory.join("partition.json")).digest == last[264..296]
}

fn referenced_keys(manifest: &NativeManifest) -> BTreeSet<String> {
    manifest
        .files
        .values()
        .map(|entry| {
            entry
                .artifact_key
                .clone()
                .expect("real producer artifact key")
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct FileObservation {
    digest: [u8; 32],
    modified: SystemTime,
    readonly: bool,
}
fn observe_file(path: &Path) -> FileObservation {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.file_type().is_file());
    let mut file = fs::File::open(path).unwrap();
    let mut buffer = [0u8; 8192];
    let mut digest = Sha256::new();
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    FileObservation {
        digest: digest.finalize().into(),
        modified: metadata.modified().unwrap(),
        readonly: metadata.permissions().readonly(),
    }
}

#[cfg(unix)]
fn observe_link(path: &Path) -> (PathBuf, u64, u64, SystemTime, u32) {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.file_type().is_symlink());
    (
        fs::read_link(path).unwrap(),
        metadata.dev(),
        metadata.ino(),
        metadata.modified().unwrap(),
        metadata.mode(),
    )
}

fn drain(root: &FixtureRoot) {
    let (storage, active, manifest) = committed(root);
    let expected = referenced_keys(&manifest);
    let receipts = ["runs", "phases"]
        .into_iter()
        .filter_map(|name| fs::read_dir(storage.join(name)).ok())
        .flatten()
        .map(|entry| {
            fs::read_dir(entry.unwrap().path().join(".workspace-origins"))
                .map(|entries| entries.count())
                .unwrap_or(0)
        })
        .sum::<usize>();
    // Each real receipt advances through fresh cursor, intent, detach and
    // removal grants. Size the fixture's bound to its actual work, while each
    // individual collector attempt still enforces the production grant limits.
    let attempts =
        128 + 16 * (receipts + cache_keys(&storage).len() + actual_generation_ids(&storage).len());
    let mut last = None;
    for _ in 0..attempts {
        last = Some(collector_attempt(root));
        if actual_generation_ids(&storage) == BTreeSet::from([active.clone()])
            && cache_keys(&storage) == expected
            && fs::read_dir(storage.join("runs")).unwrap().next().is_none()
            && fs::metadata(storage.join("artifacts/.publication.stream"))
                .is_ok_and(|metadata| metadata.len() == 0)
        {
            return;
        }
    }
    panic!(
        "actual collector did not converge in {attempts} attempts without source edit: {last:?}"
    );
}

#[test]
fn dc_s5_empty_terminal_workspace_removal_fits_actual_grants() {
    use crate::storage::maintenance_ownership::ReferenceDomain;
    use crate::storage::run_workspace::RunWorkspace;
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, active, _) = committed(&root);
    prepare_canonical_fixture_registry(&storage, &active);
    let layout = ManagedLayout::new(storage);
    let domain = ReferenceDomain::managed(&layout).unwrap();
    let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
    let terminal = run.retire_for_maintenance().unwrap();
    let mut last = None;
    for _ in 0..64 {
        last = Some(collector_attempt(&root));
        if !terminal.exists() {
            BehaviorSnapshot::observe(&root, 0).unwrap();
            return;
        }
    }
    panic!("fixed metadata removal cannot fit fresh actual grants: {last:?}");
}

#[test]
fn dc_s5_terminal_absent_barrier_rejects_appeared_entries_under_real_dispatcher() {
    use crate::storage::maintenance_admission::{prepare_target_binding, PreparedTargetBinding};
    use crate::storage::maintenance_budget::MemoryBox;
    use crate::storage::maintenance_controls::{
        prepare_terminal_workspace_boxed, with_terminal_workspace,
    };
    use crate::storage::maintenance_ownership::ReferenceDomain;
    use crate::storage::maintenance_scheduler::{
        with_claimed_legacy_confirmation, AttemptOutcome, TargetBinding,
    };
    use std::cell::RefCell;
    use std::sync::atomic::AtomicBool;

    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, _, _) = committed(&root);
    let terminal = fs::read_dir(storage.join("runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let barrier: PathBuf =
        serde_json::from_slice(&fs::read(terminal.join("cleanup-barrier.json")).unwrap()).unwrap();
    assert_eq!(barrier, storage.join("phases"));
    assert!(!barrier.exists());
    let sentinel = root.path().join("barrier-link-target");
    fs::write(&sentinel, b"foreign sentinel").unwrap();
    let sentinel_before = observe_file(&sentinel);
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let target = RepositoryIdentity::capture(&selector(&root))
        .unwrap()
        .maintenance_target(&memory)
        .unwrap()
        .target()
        .clone();
    for replacement in 0..if cfg!(unix) { 3 } else { 2 } {
        let request_target = target.clone();
        let prepare_target = target.clone();
        let prepare_terminal = terminal.clone();
        let domain = ReferenceDomain::managed(&ManagedLayout::new(&storage)).unwrap();
        let barrier = barrier.clone();
        let sentinel = sentinel.clone();
        let seen = Arc::new(AtomicBool::new(false));
        let seen_callback = seen.clone();
        let mut binding: Option<MemoryBox<PreparedTargetBinding>> = None;
        let snapshot = with_claimed_legacy_confirmation(
            Some(&request_target),
            Arc::new(ExecutionDispatcher::default()),
            move |memory, _, _| {
                let binding = binding.get_or_insert_with(|| {
                    MemoryBox::new(
                        prepare_target_binding(&prepare_target, memory).unwrap(),
                        memory,
                    )
                    .unwrap()
                });
                let prepared =
                    prepare_terminal_workspace_boxed(&prepare_terminal, &domain, memory).unwrap();
                Ok((TargetBinding(binding.digest()), RefCell::new(prepared)))
            },
            move |prepared, _, grant| {
                assert!(!seen_callback.swap(true, Ordering::AcqRel));
                grant.take_entry().unwrap();
                match replacement {
                    0 => fs::write(&barrier, b"foreign appeared file").unwrap(),
                    1 => fs::create_dir(&barrier).unwrap(),
                    #[cfg(unix)]
                    2 => std::os::unix::fs::symlink(&sentinel, &barrier).unwrap(),
                    _ => unreachable!(),
                }
                let mut effect_called = false;
                let result =
                    with_terminal_workspace(&mut prepared.borrow_mut(), grant, 0, &mut |_, _| {
                        effect_called = true;
                        Ok(())
                    });
                assert!(
                    result.is_err(),
                    "appeared barrier passed terminal admission"
                );
                assert!(!effect_called, "stale absence permitted an effect");
                if replacement == 1 {
                    fs::remove_dir(&barrier).unwrap();
                } else {
                    fs::remove_file(&barrier).unwrap();
                }
                Ok(AttemptOutcome::Complete)
            },
        );
        assert!(seen.load(Ordering::Acquire), "{snapshot:?}");
        assert_eq!(
            snapshot.last_attempt.unwrap().outcome,
            AttemptOutcome::Complete
        );
    }
    assert_eq!(observe_file(&sentinel), sentinel_before);
}

#[test]
fn dc_s5_twenty_updates_collect_without_source_edits_and_match_clean_control() {
    let churn = FixtureRoot::new();
    let control = FixtureRoot::new();
    churn.write_revision(0);
    install(&churn).unwrap();
    let (storage, _, first) = committed(&churn);
    let stable_key = first.files["src/stable.rs"].artifact_key.clone().unwrap();
    let stable_path = storage
        .join("artifacts")
        .join(&stable_key[..2])
        .join(&stable_key)
        .join("partition.json");
    let stable_before = observe_file(&stable_path);
    let foreign = storage.join("unknown-user-note");
    fs::write(
        &foreign,
        b"arbitrary foreign data, never a retirement proof",
    )
    .unwrap();
    let foreign_before = observe_file(&foreign);
    let external = FixtureRoot::new();
    let external_file = external.path().join("external-sentinel");
    fs::write(&external_file, b"independent external sentinel").unwrap();
    let external_before = observe_file(&external_file);
    #[cfg(unix)]
    let external_link = {
        let link = storage.join("unknown-user-link");
        std::os::unix::fs::symlink(&external_file, &link).unwrap();
        link
    };
    #[cfg(unix)]
    let external_link_before = observe_link(&external_link);
    for revision in 1..=20 {
        churn.write_revision(revision);
        let result = materialize(&churn, "full").unwrap();
        assert!(result.database_written);
        assert!(result.artifacts_reused >= 1);
        // Deliberately no subsequent source edit is used to unblock collection.
        drain(&churn);
        assert_eq!(observe_file(&stable_path), stable_before);
        assert_eq!(observe_file(&foreign), foreign_before);
        assert_eq!(observe_file(&external_file), external_before);
        #[cfg(unix)]
        assert_eq!(observe_link(&external_link), external_link_before);
        BehaviorSnapshot::observe(&churn, revision).unwrap();
        eprintln!("collector published/drained/reused revision {revision}");
    }
    let before = BehaviorSnapshot::observe(&churn, 20).unwrap();
    for _ in 0..20 {
        drain(&churn);
    }
    assert_eq!(BehaviorSnapshot::observe(&churn, 20).unwrap(), before);
    control.write_revision(20);
    install(&control).unwrap();
    drain(&control);
    assert_eq!(BehaviorSnapshot::observe(&control, 20).unwrap(), before);
}

#[test]
fn dc_s5_real_reader_lease_protects_predecessor_until_release() {
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, old_generation, _) = committed(&root);
    let store = ManagedStore::new(ManagedLayout::new(storage.clone()));
    let reader = store.open_read().unwrap();
    assert_eq!(reader.generation_id, old_generation);
    root.write_revision(1);
    materialize(&root, "full").unwrap();
    for _ in 0..4 {
        collector_attempt(&root);
    }
    assert!(
        actual_generation_ids(&storage).contains(&old_generation),
        "live reader's real predecessor was removed"
    );
    assert!(reader.db_path.is_file());
    BehaviorSnapshot::observe(&root, 1).unwrap();
    drop(reader);
    drain(&root);
    assert!(!actual_generation_ids(&storage).contains(&old_generation));
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

#[test]
fn dc_s5_publication_durability_failure_retains_real_evidence_and_recovers_without_edit() {
    use crate::storage::managed::inject_scoped_managed_sync_fault;
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, old, _) = committed(&root);
    // Isolate the new required-recovery journal from a prior completed run;
    // optional collection may legitimately consume that earlier terminal root.
    prepare_canonical_fixture_registry(&storage, &old);
    let layout = ManagedLayout::new(storage);
    let store = ManagedStore::new(layout.clone());
    root.write_revision(1);
    let fault = inject_scoped_managed_sync_fault(layout.storage_root()).unwrap();
    let error = materialize(&root, "full")
        .expect_err("mandatory publication sync failure must not be acknowledged as success");
    assert!(
        error.contains("mandatory managed sync"),
        "unexpected primary error: {error}"
    );
    // Publication reached pointer replacement before mandatory sync failed.
    // Reading a pointer is observation, never acknowledgment of durable success.
    let active = store.read_active_generation().unwrap().unwrap();
    assert_ne!(active.generation_id, old);
    let generation = layout.generation(&active.generation_id).unwrap();
    assert!(!generation.retired_path().exists());
    let mut journals = Vec::new();
    for entry in fs::read_dir(layout.runs_root()).unwrap() {
        let path = entry.unwrap().path().join("journal.json");
        if path.is_file() {
            let journal: crate::storage::run_workspace::RunJournal =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if journal.phase == crate::storage::run_workspace::RunPhase::Publishing
                && journal.candidate_generation_id.as_deref() == Some(active.generation_id.as_str())
            {
                journals.push(path);
            }
        }
    }
    assert!(
        !journals.is_empty(),
        "failed publication lost its real recovery journal"
    );
    let proof_paths = [
        layout.active_pointer_path(),
        generation.metadata_path(),
        generation.ready_path(),
    ];
    let proof = proof_paths
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    let journal_proof = journals
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    for _ in 0..2 {
        let error = store.cleanup_typed().unwrap_err();
        assert!(error.to_string().contains("mandatory managed sync"));
        assert_eq!(
            proof_paths
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            proof
        );
        assert_eq!(
            journals
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            journal_proof
        );
        assert!(!generation.retired_path().exists());
    }
    let recovering_runs = journals
        .iter()
        .filter_map(|path| {
            let journal: crate::storage::run_workspace::RunJournal =
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            (journal.phase == crate::storage::run_workspace::RunPhase::Publishing)
                .then(|| path.parent().unwrap().to_path_buf())
        })
        .collect::<Vec<_>>();
    assert!(!recovering_runs.is_empty());
    for run in &recovering_runs {
        assert!(
            run.join(".workspace.binding").is_file(),
            "must exercise actual registered producer recovery"
        );
    }
    drop(fault);
    // Exercise the production read path (perform_janitor=false), then the real
    // bounded collector. Required recovery publishes closure without traversal.
    let recovery = store.recover_for_read().unwrap();
    assert_eq!(recovery.recovery.publishing_recovered, 1);
    assert!(recovery.janitor.first_issue.is_none(), "{recovery:?}");
    let domain = crate::storage::maintenance_ownership::ReferenceDomain::managed(&layout).unwrap();
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    for run in &recovering_runs {
        assert!(run.exists(), "read recovery removed retained payload");
        crate::storage::maintenance_controls::prepare_terminal_workspace_boxed(
            run, &domain, &memory,
        )
        .unwrap();
    }
    assert_eq!(
        store
            .recover_for_read()
            .unwrap()
            .recovery
            .publishing_recovered,
        0,
        "owner-terminal recovery must not replay on subsequent read"
    );
    assert_eq!(
        store
            .read_active_generation()
            .unwrap()
            .unwrap()
            .generation_id,
        active.generation_id
    );
    assert_eq!(
        proof_paths
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>(),
        proof
    );
    BehaviorSnapshot::observe(&root, 1).unwrap();
    // The failed producer never reached NativeVerifiedGeneration sealing.
    // Required read recovery closes its run, but cannot invent successor Origin
    // or predecessor FINAL authority. Drain registered runs without requiring
    // unrelated generation/cache retirement that must remain protected here.
    let old_paths = layout.generation(&old).unwrap();
    let old_origin = fs::read(old_paths.root().join(".generation-origin")).unwrap();
    assert!(!old_paths.root().join(".generation-final").exists());
    assert!(!generation.root().join(".generation-origin").exists());
    let receipts = fs::read_dir(layout.runs_root())
        .unwrap()
        .map(|entry| {
            fs::read_dir(entry.unwrap().path().join(".workspace-origins"))
                .map(|origins| origins.count())
                .unwrap_or(0)
        })
        .sum::<usize>();
    let mut summary = collector_attempt(&root);
    for _ in 0..128 + 16 * receipts {
        if fs::read_dir(layout.runs_root()).unwrap().next().is_none() {
            break;
        }
        summary = collector_attempt(&root);
    }
    assert!(
        summary.pending,
        "unsealed generations were reported idle: {summary:?}"
    );
    assert!(!summary.observation_complete);
    assert!(
        summary.reasons.iter().any(|reason| matches!(
            reason.as_str(),
            "unknown_program_origin" | "legacy_quiescence_required"
        )),
        "unsealed generations lacked a concrete provenance or quiescence blocker: {summary:?}"
    );
    assert_eq!(
        fs::read(old_paths.root().join(".generation-origin")).unwrap(),
        old_origin
    );
    for run in recovering_runs {
        assert!(
            !run.exists(),
            "bounded collector retained resolved registered run"
        );
    }
    assert!(fs::read_dir(layout.runs_root()).unwrap().next().is_none());
    assert_eq!(
        actual_generation_ids(layout.storage_root()),
        BTreeSet::from([old.clone(), active.generation_id.clone()])
    );
    assert!(
        !old_paths.root().join(".generation-final").exists(),
        "read recovery or collector invented native-verified retirement authority"
    );
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

/// Genuine current-producer leaves for both negative provenance checks and the
/// later terminal-workspace test. This does not establish ownership of `path`.
fn emitted_partition(root: &FixtureRoot) -> crate::partition_builder::GraphPartition {
    use crate::partition_builder::GraphPartition;
    let (storage, _, manifest) = committed(root);
    let key = manifest.files["src/lib.rs"].artifact_key.as_ref().unwrap();
    let envelope: Value = serde_json::from_slice(
        &fs::read(
            storage
                .join("artifacts")
                .join(&key[..2])
                .join(key)
                .join("partition.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let partition: GraphPartition = serde_json::from_value(envelope["partition"].clone()).unwrap();
    partition.validate_raw_rows().unwrap();
    partition
}

fn emit_many_staging_leaves(root: &FixtureRoot, path: &Path) -> usize {
    emit_staging_partition(&emitted_partition(root), path)
}

fn emit_staging_partition(
    partition: &crate::partition_builder::GraphPartition,
    path: &Path,
) -> usize {
    use crate::staging_writer::StagingAccumulator;
    let mut accumulator =
        StagingAccumulator::with_limits(path.to_str().unwrap(), 256 * 1024, 1024).unwrap();
    accumulator.add_partition(partition);
    let staged = accumulator.finish().unwrap();
    assert_eq!(staged.unique_node_count, partition.nodes.len());
    assert_eq!(staged.unique_edge_count, partition.edges.len());
    staged.copy_calls
}

fn observe_staging_files(root: &Path) -> std::collections::BTreeMap<PathBuf, FileObservation> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, FileObservation>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink());
            if kind.is_dir() {
                visit(root, &entry.path(), files);
            } else {
                assert!(kind.is_file());
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    observe_file(&entry.path()),
                );
            }
        }
    }
    let mut files = std::collections::BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn dc_s5_genuine_multileaf_staging_without_terminal_authority_is_preserved() {
    let root = FixtureRoot::new();
    root.write_revision(0);
    let mut source = fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
    for number in 0..STAGING_FIXTURE_FUNCTIONS {
        source.push_str(&format!(
            "pub fn staged_{number}() -> usize {{ {number} }}\n"
        ));
    }
    fs::write(root.path().join("src/lib.rs"), source).unwrap();
    install(&root).unwrap();
    let (storage, _, _) = committed(&root);
    let staging = storage
        .parent()
        .unwrap()
        .join("native-staging")
        .join("unregistered-current-producer");
    fs::create_dir_all(&staging).unwrap();
    assert!(
        emit_many_staging_leaves(&root, &staging) > 100,
        "fixture did not exercise a multigrant leafset"
    );
    let before = observe_staging_files(&staging);
    assert!(before.len() > 100);
    // Valid current bytes and recognizable names do not create terminal-run
    // authority or assert old-helper quiescence. Each actual attempt must retain
    // this root; this is a negative test, not a >100-leaf deletion qualification.
    for _ in 0..4 {
        collector_attempt(&root);
        assert_eq!(observe_staging_files(&staging), before);
    }
    assert_eq!(health(&root).unwrap()["graph_readable"], true);
}

#[test]
fn dc_s5_former_default_direct_is_retired_and_explicit_rebuild_creates_fresh_incarnation() {
    use crate::storage::layout::{DirectLayout, DIRECT_DB_SIDECAR_SUFFIXES};
    use crate::storage::maintenance_retirement::{observe_direct, DirectDisposition};
    let root = FixtureRoot::new();
    root.write_revision(0);
    materialize(&root, "full").unwrap();
    let direct_health = health(&root).unwrap();
    assert!(
        direct_health["storage_root"].is_null(),
        "fixture did not create the unconfigured default Direct destination"
    );
    let database = PathBuf::from(direct_health["database_path"].as_str().unwrap());
    let manifest = PathBuf::from(direct_health["manifest_path"].as_str().unwrap());
    let layout = DirectLayout::new(database.clone(), manifest.clone());
    let lock_identity = crate::storage::locks::path_identity(&layout.writer_lock_path()).unwrap();
    assert!(database.starts_with(root.path().join(".codebaseGraph")));
    assert_eq!(observe_direct(&layout).unwrap(), DirectDisposition::Active);
    install(&root).unwrap();
    let (storage, _, _) = committed(&root);
    assert_ne!(
        PathBuf::from(health(&root).unwrap()["database_path"].as_str().unwrap()),
        database
    );
    let mut retired = false;
    for _ in 0..256 {
        collector_attempt(&root);
        if observe_direct(&layout).unwrap() == DirectDisposition::Retired {
            retired = true;
            break;
        }
    }
    assert!(
        retired,
        "proven former default Direct bundle never retired without source edit"
    );
    assert!(!database.exists());
    assert!(!manifest.exists());
    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
        assert!(!PathBuf::from(format!("{}.{suffix}", database.display())).exists());
    }
    assert_eq!(
        crate::storage::locks::path_identity(&layout.writer_lock_path()).unwrap(),
        lock_identity
    );
    BehaviorSnapshot::observe(&root, 0).unwrap();
    let direct = RepoSelector {
        repo_root: Some(root.path().to_owned()),
        db_path: Some(database.clone()),
        manifest_path: Some(manifest),
        ..RepoSelector::default()
    };
    let error = execute(OperationRequest::Query(QueryRequest {
        repo: direct.clone(),
        statement: "MATCH (n:Function) RETURN count(n)".into(),
        parameters: json!({}),
        limit: 10,
        output_format: OutputFormat::Typed,
    }))
    .unwrap_err();
    assert!(
        error.contains("rebuild required"),
        "retired destination did not give explicit remediation: {error}"
    );
    materialize_selected(direct, "full").unwrap();
    assert_eq!(observe_direct(&layout).unwrap(), DirectDisposition::Active);
    assert!(database.is_file());
    assert!(storage.exists());
    BehaviorSnapshot::observe(&root, 0).unwrap();
}

#[test]
fn dc_s5_foreign_leaf_stays_at_its_original_path_when_owned_neighbor_is_unreferenced() {
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, _, manifest) = committed(&root);
    let key = manifest.files["src/lib.rs"].artifact_key.as_ref().unwrap();
    let directory = storage.join("artifacts").join(&key[..2]).join(key);
    let foreign = directory.join("foreign-user-note");
    fs::write(&foreign, b"not part of the artifact producer leafset").unwrap();
    let foreign_before = observe_file(&foreign);
    let foreign_identity = crate::storage::locks::path_identity(&foreign).unwrap();
    let directory_identity = crate::storage::locks::path_identity(&directory).unwrap();
    root.write_revision(1);
    materialize(&root, "full").unwrap();
    let (_, _, current) = committed(&root);
    assert!(!referenced_keys(&current).contains(key));
    let mut reported_unknown = false;
    for _ in 0..8 {
        let summary = collector_attempt(&root);
        reported_unknown |= summary.reasons.iter().any(|reason| {
            matches!(
                reason.as_str(),
                "unknown_entries_retained"
                    | "maintenance_candidate_unknown"
                    | "unknown_program_origin"
            )
        });
        assert_eq!(observe_file(&foreign), foreign_before);
        assert_eq!(
            crate::storage::locks::path_identity(&foreign).unwrap(),
            foreign_identity
        );
        assert_eq!(
            crate::storage::locks::path_identity(&directory).unwrap(),
            directory_identity
        );
    }
    assert!(
        reported_unknown,
        "foreign state was not reported unknown/incomplete"
    );
    // Exact-file retirement may dispose of the proven owned neighbor. The
    // unknown sibling and its public directory must stay at their identities;
    // this case cannot qualify a clean cache state.
    assert!(cache_keys(&storage).contains(&format!("!unclassified-leaf:{key}/foreign-user-note")));
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

#[test]
fn dc_s5_artifact_use_lease_defers_obsolete_key_until_release() {
    use crate::storage::maintenance_ownership::{ArtifactUseLease, ReferenceDomain};
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, _, manifest) = committed(&root);
    let key = manifest.files["src/lib.rs"].artifact_key.as_ref().unwrap();
    let directory = storage.join("artifacts").join(&key[..2]).join(key);
    let payload = directory.join("partition.json");
    let before = observe_file(&payload);
    let domain = ReferenceDomain::managed(&ManagedLayout::new(storage.clone())).unwrap();
    let held = ArtifactUseLease::acquire_in_domain(&domain).unwrap();
    root.write_revision(1);
    materialize(&root, "full").unwrap();
    let (_, _, successor) = committed(&root);
    assert!(!referenced_keys(&successor).contains(key));
    for _ in 0..4 {
        let summary = collector_attempt(&root);
        assert!(
            summary.pending,
            "live artifact use was not reported pending: {summary:?}"
        );
        assert!(
            summary.reasons.iter().any(|reason| matches!(
                reason.as_str(),
                "reference_in_use" | "maintenance_admission_busy"
            )),
            "live artifact use lacks a concrete deferral reason: {summary:?}"
        );
        assert_eq!(observe_file(&payload), before);
    }
    drop(held);
    drain(&root);
    assert!(!directory.exists());
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

#[cfg(unix)]
#[test]
fn dc_s5_permission_failure_is_reported_and_retry_without_edit_reclaims_owned_key() {
    use crate::storage::maintenance_ownership::{ArtifactUseLease, ReferenceDomain};
    use std::os::unix::fs::PermissionsExt;
    // A process with DAC override cannot exercise this permission boundary.
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "run permission qualification as an ordinary user"
    );
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, initial_active, manifest) = committed(&root);
    prepare_canonical_fixture_registry(&storage, &initial_active);
    let key = manifest.files["src/lib.rs"].artifact_key.as_ref().unwrap();
    let shard = storage.join("artifacts").join(&key[..2]);
    let directory = shard.join(key);
    let domain = ReferenceDomain::managed(&ManagedLayout::new(storage.clone())).unwrap();
    let held = ArtifactUseLease::acquire_in_domain(&domain).unwrap();
    root.write_revision(1);
    materialize(&root, "full").unwrap();
    let active = ManagedStore::new(ManagedLayout::new(storage.clone()))
        .read_active_generation()
        .unwrap()
        .unwrap()
        .generation_id;
    prepare_canonical_fixture_registry(&storage, &active);
    assert!(directory.exists());
    struct RestorePermissions {
        path: PathBuf,
        mode: u32,
    }
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode))
                .expect("restore only fixture-owned injected permissions");
        }
    }
    let restore = RestorePermissions {
        path: directory.clone(),
        mode: fs::metadata(&directory).unwrap().permissions().mode(),
    };
    // Block the payload's first detach, not only the final empty-directory
    // rmdir. A shard-only denial permits legitimate payload reclamation first.
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    let denial = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("permission-probe"));
    assert_eq!(
        denial.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied,
        "fixture must exercise actual OS denial rather than metadata inference"
    );
    drop(held);
    let payload_before = observe_file(&directory.join("partition.json"));
    let payload_identity =
        crate::storage::locks::path_identity(&directory.join("partition.json")).unwrap();
    let mut observed = false;
    let mut last = None;
    // Canonical work is isolated above; allow its real cooperative validation
    // and fair mutation turns, each retaining the production grant limits.
    for _ in 0..64 {
        let summary = collector_attempt_observed(&root);
        if summary
            .reasons
            .iter()
            .any(|reason| reason == "permission_denied")
        {
            assert!(summary.pending);
            observed = true;
            break;
        }
        last = Some(summary);
    }
    assert!(
        observed,
        "real denied mutation did not report permission_denied: {last:?}"
    );
    assert_eq!(
        observe_file(&directory.join("partition.json")),
        payload_before
    );
    assert_eq!(
        crate::storage::locks::path_identity(&directory.join("partition.json")).unwrap(),
        payload_identity,
        "permission denial replaced the owned payload incarnation"
    );
    BehaviorSnapshot::observe(&root, 1).unwrap();
    drop(restore);
    drain(&root);
    assert!(!directory.exists());
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

/// Canonical-only preparation. The public producer has returned successfully
/// and every remaining run must prove that same active candidate, a terminal
/// journal, unlocked ownership and an empty real phase registry. Established
/// compatibility cleanup here is fixture preparation, NOT terminal-run GC proof.
fn prepare_canonical_fixture_registry(storage: &Path, active: &str) {
    use crate::storage::run_workspace::{RunJournal, RunPhase, RunWorkspace};
    let layout = ManagedLayout::new(storage.to_owned());
    crate::storage::containment::ensure_registry_empty(&layout.phase_workspace_root()).unwrap();
    let runs = fs::read_dir(layout.runs_root())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    for run in &runs {
        let metadata = fs::symlink_metadata(run).unwrap();
        assert!(metadata.file_type().is_dir());
        let journal: RunJournal =
            serde_json::from_slice(&fs::read(run.join("journal.json")).unwrap()).unwrap();
        assert_eq!(
            run.file_name().unwrap().to_str().unwrap(),
            format!("run-{}", journal.run_id)
        );
        assert_eq!(journal.candidate_generation_id.as_deref(), Some(active));
        match journal.phase {
            RunPhase::Published => {
                assert_eq!(journal.active_generation_id.as_deref(), Some(active));
                assert!(journal.last_error.is_none());
            }
            RunPhase::CleanupPending => assert!(journal
                .last_error
                .as_deref()
                .is_some_and(|error| error.starts_with("cleanup pending:"))),
            phase => panic!("canonical fixture has unresolved run phase: {phase:?}"),
        }
        crate::storage::containment::ensure_registry_empty(&run.join("phases")).unwrap();
    }
    if !runs.is_empty() {
        let compatibility = RunWorkspace::cleanup_orphans(layout.runs_root()).unwrap();
        assert_eq!(compatibility.skipped_locked, 0);
        assert_eq!(compatibility.publishing_recovered, 0);
        assert_eq!(compatibility.deleted, runs.len());
        eprintln!("canonical-only fixture preparation: {} known completed run(s) removed by established compatibility cleanup; this does not qualify terminal GC", compatibility.deleted);
    }
    assert_eq!(fs::read_dir(layout.runs_root()).unwrap().count(), 0);
}

fn capture_receipt_diagnostics(root: &FixtureRoot, artifact_root: &Path, key: &str) -> PathBuf {
    use crate::storage::maintenance_ownership::{observe_reference_epoch, ReferenceDomain};
    let domain = ReferenceDomain::managed(&ManagedLayout::new(
        artifact_root.parent().unwrap().to_owned(),
    ));
    let epoch = match &domain {
        Ok(domain) => format!("{:?}", observe_reference_epoch(domain)),
        Err(error) => error.to_string(),
    };
    let names = [
        ".use.lock",
        ".use-cohort.json",
        ".reference-epoch",
        ".reference-activity.lock",
        ".reference-epoch.lock",
        ".publication.binding",
        ".publication.selector",
        ".publication.data0",
        ".publication.data1",
        ".publication.stream",
    ];
    let mut metadata = serde_json::Map::new();
    for name in names {
        let path = artifact_root.join(name);
        metadata.insert(name.into(), match fs::read(&path) {
            Ok(bytes) => json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes)),"prefix_hex":bytes.iter().take(32).map(|byte|format!("{byte:02x}")).collect::<String>()}),
            Err(error) => json!({"error":error.to_string()}),
        });
    }
    let receipt = artifact_root
        .join(&key[..2])
        .join(key)
        .join(".entry-origin");
    let receipt = match fs::read(receipt) {
        Ok(bytes) => {
            json!({"bytes":bytes.len(),"frames":bytes.chunks(512).map(|frame|json!({"bytes":frame.len(),"phase":frame.get(24),"checksum_valid":frame.len()==512 && frame[480..]==Sha256::digest(&frame[..480])[..]})).collect::<Vec<_>>()})
        }
        Err(error) => json!({"error":error.to_string()}),
    };
    let path = root
        .path()
        .parent()
        .unwrap()
        .join("qualification-receipt-diagnostics.json");
    fs::write(&path, serde_json::to_vec_pretty(&json!({"fixture_root":root.path(),"artifact_root":artifact_root,"key":key,"reference_epoch":epoch,"controls":metadata,"receipt":receipt})).unwrap()).unwrap();
    path
}

struct CanonicalFixture {
    root: Option<FixtureRoot>,
    artifact_root: PathBuf,
    key: String,
    directory: PathBuf,
    payload: FileObservation,
    origin: Vec<u8>,
    database: PathBuf,
    database_before: FileObservation,
    manifest: PathBuf,
    manifest_before: Vec<u8>,
    behavior: BehaviorSnapshot,
    completed: bool,
}
impl CanonicalFixture {
    fn new() -> Self {
        use crate::artifact_store::{
            artifact_key, ArtifactKeyInput, ArtifactStore, ArtifactWriteOutcome,
        };
        use crate::partition_builder::GraphPartition;
        let root = FixtureRoot::new();
        root.write_revision(0);
        install(&root).unwrap();
        let (storage, active, manifest_data) = committed(&root);
        assert_eq!(
            actual_generation_ids(&storage),
            BTreeSet::from([active.clone()])
        );
        prepare_canonical_fixture_registry(&storage, &active);
        let behavior = BehaviorSnapshot::observe(&root, 0).unwrap();
        let health = health(&root).unwrap();
        let database = fs::canonicalize(health["database_path"].as_str().unwrap()).unwrap();
        let manifest = fs::canonicalize(health["manifest_path"].as_str().unwrap()).unwrap();
        let database_before = observe_file(&database);
        let manifest_before = fs::read(&manifest).unwrap();
        let artifact_root = fs::canonicalize(storage.join("artifacts")).unwrap();
        let source_key = manifest_data.files["src/lib.rs"]
            .artifact_key
            .as_ref()
            .unwrap();
        let envelope: Value = serde_json::from_slice(
            &fs::read(
                artifact_root
                    .join(&source_key[..2])
                    .join(source_key)
                    .join("partition.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut partition: GraphPartition =
            serde_json::from_value(envelope["partition"].clone()).unwrap();
        // The real key builder and real artifact producer are used. A distinct
        // parser-version input makes an extra valid cache entry; no graph or
        // generation manifest is edited to introduce/remove a reference.
        let parser_version = format!(
            "{}-canonical-qualification-spare",
            manifest_data.parser_version
        );
        let ontology_schema = crate::protocol::OntologySchema::default();
        let key = artifact_key(&ArtifactKeyInput {
            source_root: root.path().to_str().unwrap(),
            repository_label: "repository",
            relative_path: &partition.entry.path,
            content_hash: &partition.entry.content_hash,
            language: &partition.entry.language,
            parser_version: &parser_version,
            profile: None,
            ontology: &manifest_data.ontology,
            ontology_schema: &ontology_schema,
        })
        .unwrap();
        assert!(!referenced_keys(&manifest_data).contains(&key));
        partition.set_artifact_key(key.clone());
        assert_eq!(
            ArtifactStore::new(artifact_root.clone())
                .store_partition(&key, &partition)
                .unwrap(),
            ArtifactWriteOutcome::Written
        );
        let directory = artifact_root.join(&key[..2]).join(&key);
        if !active_origin_matches_payload(&directory, &key) {
            let diagnostics = capture_receipt_diagnostics(&root, &artifact_root, &key);
            std::mem::forget(root);
            panic!(
                "normal producer did not emit a complete bound receipt; retained diagnostics: {}",
                diagnostics.display()
            );
        }
        let payload = observe_file(&directory.join("partition.json"));
        let origin = fs::read(directory.join(".entry-origin")).unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), manifest_before);
        Self {
            root: Some(root),
            artifact_root,
            key,
            directory,
            payload,
            origin,
            database,
            database_before,
            manifest,
            manifest_before,
            behavior,
            completed: false,
        }
    }
    fn root(&self) -> &FixtureRoot {
        self.root.as_ref().unwrap()
    }
    fn assert_active_unchanged_without_runtime_entry(&self) {
        assert_eq!(observe_file(&self.database), self.database_before);
        assert_eq!(fs::read(&self.manifest).unwrap(), self.manifest_before);
    }
    fn finish(&mut self) {
        let current: NativeManifest =
            serde_json::from_slice(&fs::read(&self.manifest).unwrap()).unwrap();
        assert!(!referenced_keys(&current).contains(&self.key));
        assert!(
            !self.directory.exists(),
            "actual unreferenced key directory survived completed retirement"
        );
        assert!(retired_payloads(&self.artifact_root).is_empty());
        self.assert_active_unchanged_without_runtime_entry();
        drain(self.root());
        assert_eq!(
            BehaviorSnapshot::observe(self.root(), 0).unwrap(),
            self.behavior
        );
        self.completed = true;
    }
}
impl Drop for CanonicalFixture {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(root) = self.root.take() {
                eprintln!(
                    "retained failed canonical qualification fixture: {}",
                    root.path().display()
                );
                std::mem::forget(root);
            }
        }
    }
}

fn retired_payloads(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if let Some(nonce) = name.strip_prefix(".retired-") {
            assert_eq!(nonce.len(), 64);
            assert!(nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
            assert!(entry.file_type().unwrap().is_file());
            paths.push(entry.path());
        }
    }
    paths.sort();
    paths
}

#[test]
fn dc_s5_canonical_only_real_producer_spare_key_is_collected_without_source_edit() {
    let mut fixture = CanonicalFixture::new();
    let mut charged_mutation = false;
    for _ in 0..128 {
        let summary = collector_attempt(fixture.root());
        charged_mutation |= summary
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.attempted_entries)
            .is_some_and(|entries| entries > 0);
        if !fixture.directory.exists() && retired_payloads(&fixture.artifact_root).is_empty() {
            break;
        }
    }
    assert!(
        charged_mutation,
        "actual canonical reclamation lacks a recorded mutation grant"
    );
    fixture.finish();
}

fn canonical_fault_resume(point: crate::storage::maintenance_candidate::CandidateFault) {
    use crate::storage::maintenance_candidate::{arm_candidate_fault, CandidateFault};
    let mut fixture = CanonicalFixture::new();
    let guard = arm_candidate_fault(&fixture.artifact_root, point).unwrap();
    let mut last = None;
    for _ in 0..128 {
        last = Some(collector_attempt_observed(fixture.root()));
        if guard.reached() {
            break;
        }
    }
    assert!(
        guard.reached(),
        "real transition hook was never reached: {last:?}"
    );
    assert!(
        last.as_ref()
            .unwrap()
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.attempted_entries)
            .is_some_and(|entries| entries > 0),
        "reached mutation fault lacks recorded charged grant: {last:?}"
    );
    fixture.assert_active_unchanged_without_runtime_entry();
    assert!(!fixture.directory.join("partition.json").exists());
    assert_eq!(
        fs::read(fixture.directory.join(".entry-origin")).unwrap(),
        fixture.origin
    );
    let private = retired_payloads(&fixture.artifact_root);
    match point {
        CandidateFault::AfterDetach => {
            assert_eq!(
                private.len(),
                1,
                "durable detach did not retain its actual private payload"
            );
            assert_eq!(observe_file(&private[0]).digest, fixture.payload.digest);
        }
        CandidateFault::AfterPayloadUnlink => assert!(
            private.is_empty(),
            "actual unlink boundary still has a private payload"
        ),
        CandidateFault::DuringPreparation => {
            unreachable!("preparation uses its dedicated cancellation fixture")
        }
    }
    let stream = fixture.artifact_root.join(".publication.stream");
    assert!(
        fs::metadata(&stream).unwrap().len() >= 512,
        "interruption lost the real retirement journal"
    );
    assert!(
        last.as_ref().unwrap().pending,
        "interrupted retirement was not reported pending"
    );
    drop(guard);
    // Each call constructs a fresh actual controller/admission pair; no source
    // writes, fake receipts or publication of another generation drive recovery.
    for _ in 0..128 {
        collector_attempt(fixture.root());
        if !fixture.directory.exists() && retired_payloads(&fixture.artifact_root).is_empty() {
            break;
        }
    }
    fixture.finish();
}

#[test]
fn dc_s5_canonical_only_durable_detach_interruption_resumes_without_source_edit() {
    canonical_fault_resume(crate::storage::maintenance_candidate::CandidateFault::AfterDetach);
}
#[test]
fn dc_s5_canonical_only_payload_unlink_interruption_resumes_without_source_edit() {
    canonical_fault_resume(
        crate::storage::maintenance_candidate::CandidateFault::AfterPayloadUnlink,
    );
}

fn all_function_rows(root: &FixtureRoot) -> Vec<Value> {
    let query = execute(OperationRequest::Query(QueryRequest {
        repo: selector(root), statement: "MATCH (n:Function) RETURN n.label, n.path, n.line_start, n.line_end, n.summary ORDER BY n.path, n.line_start, n.label".into(),
        parameters: json!({}), limit: 1000, output_format: OutputFormat::Typed,
    })).unwrap();
    assert_eq!(query["truncated"], false);
    let rows = query["rows"].as_array().unwrap().clone();
    let mut actual = rows
        .iter()
        .map(|row| row[0].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = (0..STAGING_FIXTURE_FUNCTIONS)
        .map(|number| format!("staged_{number}"))
        .collect::<Vec<_>>();
    expected.extend(["revision_0".into(), "stable_helper".into()]);
    expected.sort();
    assert_eq!(actual, expected);
    rows
}

fn staging_output_count(root: &Path) -> usize {
    let mut count = 0;
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            count += staging_output_count(&entry.path());
        } else {
            assert!(kind.is_file());
            count += 1;
        }
    }
    count
}

fn observed_directory_paths(root: &Path) -> std::collections::BTreeMap<(u64, u64), PathBuf> {
    fn visit(path: &Path, output: &mut std::collections::BTreeMap<(u64, u64), PathBuf>) {
        let identity = crate::storage::locks::path_identity(path)
            .unwrap()
            .fixed_words();
        assert!(
            output.insert(identity, path.to_owned()).is_none(),
            "directory identity alias in owned fixture"
        );
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink());
            if kind.is_dir() {
                visit(&entry.path(), output);
            }
        }
    }
    let mut output = std::collections::BTreeMap::new();
    visit(root, &mut output);
    output
}

#[test]
fn dc_s5_real_terminal_multigrant_outputs_resume_across_controller_restarts_without_edit() {
    use crate::storage::maintenance_origin::{
        decode_workspace_output_origin, WorkspaceOriginPhase, WORKSPACE_ORIGIN_FRAME_BYTES,
    };
    use crate::storage::maintenance_ownership::ReferenceDomain;
    use crate::storage::run_workspace::RunWorkspace;
    let root = FixtureRoot::new();
    root.write_revision(0);
    let mut source = fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
    for number in 0..STAGING_FIXTURE_FUNCTIONS {
        source.push_str(&format!(
            "pub fn staged_{number}() -> usize {{ {number} }}\n"
        ));
    }
    fs::write(root.path().join("src/lib.rs"), source).unwrap();
    install(&root).unwrap();
    let (storage, _, _) = committed(&root);
    let before = all_function_rows(&root);
    let partition = emitted_partition(&root);
    let layout = ManagedLayout::new(storage.clone());
    let domain = ReferenceDomain::managed(&layout).unwrap();
    let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
    let run_root = run.root().to_owned();
    let staging = run.staging_root();
    let copy_calls = emit_staging_partition(&partition, &staging);
    let payloads = observe_staging_files(&staging);
    assert!(
        copy_calls > 100 && payloads.len() > 100,
        "real producer did not create a multigrant fixture"
    );
    let registry = run_root.join(".workspace-origins");
    let registry_id = crate::storage::locks::path_identity(&registry).unwrap();
    let root_id = crate::storage::locks::path_identity(&run_root).unwrap();
    let directory_paths = observed_directory_paths(&run_root);
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let mut registered = BTreeSet::new();
    let mut registered_directories = BTreeSet::new();
    let mut already_removed = 0;
    for entry in fs::read_dir(&registry).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let bytes = fs::read(entry.path()).unwrap();
        assert!(bytes.len() >= WORKSPACE_ORIGIN_FRAME_BYTES);
        let receipt_id = crate::storage::locks::path_identity(&entry.path()).unwrap();
        let bound = decode_workspace_output_origin(
            &bytes[..WORKSPACE_ORIGIN_FRAME_BYTES],
            receipt_id,
            None,
            &memory,
        )
        .unwrap();
        assert_eq!(bound.binding.root, root_id);
        assert_eq!(bound.binding.registry, registry_id);
        let parent = directory_paths
            .get(&bound.binding.parent.fixed_words())
            .expect("real receipt parent disappeared before terminal event");
        let name =
            std::str::from_utf8(&bound.binding.name[..bound.binding.name_len as usize]).unwrap();
        let output_path = parent.join(name);
        if bound.fifo {
            // A FIFO has a genuine creation receipt, but no readable regular
            // payload or Complete event. Never open it for a digest here.
            assert!(!bound.directory);
            assert_eq!(bytes.len(), WORKSPACE_ORIGIN_FRAME_BYTES);
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                match fs::symlink_metadata(&output_path) {
                    Ok(metadata) => {
                        assert!(metadata.file_type().is_fifo());
                        assert_eq!(
                            crate::storage::locks::path_identity(&output_path).unwrap(),
                            bound.binding.output
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        // The real FIFO producer may already have consumed it.
                    }
                    Err(error) => panic!("observe actual FIFO output: {error}"),
                }
            }
            #[cfg(not(unix))]
            panic!("FIFO receipt on a platform without FIFO producer support");
        } else if bound.directory {
            assert_eq!(bytes.len(), WORKSPACE_ORIGIN_FRAME_BYTES);
            assert!(output_path.is_dir());
            assert_eq!(
                crate::storage::locks::path_identity(&output_path).unwrap(),
                bound.binding.output
            );
            assert!(registered_directories.insert(bound.binding.output.fixed_words()));
        } else {
            if output_path.exists() {
                assert_eq!(
                    bytes.len(),
                    2 * WORKSPACE_ORIGIN_FRAME_BYTES,
                    "surviving real file output lacks Complete event"
                );
                let relative = output_path
                    .strip_prefix(&staging)
                    .expect("actual staging output escaped staging tree")
                    .to_owned();
                let payload = payloads
                    .get(&relative)
                    .expect("receipt does not match a surviving staging output");
                let metadata = fs::metadata(&output_path).unwrap();
                let fact = decode_workspace_output_origin(
                    &bytes,
                    receipt_id,
                    Some((metadata.len(), payload.digest)),
                    &memory,
                )
                .unwrap();
                assert_eq!(fact.phase, WorkspaceOriginPhase::Complete);
                assert!(!fact.directory);
                assert_eq!(
                    fact.binding.output,
                    crate::storage::locks::path_identity(&output_path).unwrap()
                );
                assert!(registered.insert(relative));
            } else {
                // The actual spill producer can remove consumed input files.
                // Observe their completion framing/binding, without inventing
                // integrity evidence for bytes that are no longer present.
                if bytes.len() == 2 * WORKSPACE_ORIGIN_FRAME_BYTES {
                    let completion = &bytes[WORKSPACE_ORIGIN_FRAME_BYTES..];
                    assert_eq!(&completion[..8], b"CGWOUT01");
                    assert_eq!(completion[8], 1);
                    assert_eq!(completion[9..208], bytes[9..208]);
                    assert_eq!(completion[248..992], bytes[248..992]);
                    assert_eq!(completion[992..], Sha256::digest(&completion[..992])[..]);
                } else {
                    assert_eq!(bytes.len(), WORKSPACE_ORIGIN_FRAME_BYTES);
                    let file = fs::File::open(entry.path()).unwrap();
                    assert_eq!(
                        crate::storage::locks::created_file_nonce(&file).unwrap(),
                        Some(
                            crate::storage::maintenance_controls::workspace_receipt_nonce(
                                bound.binding.incarnation,
                                bound.binding.parent,
                                bound.binding.output,
                                bound.binding.operation
                            )
                        )
                    );
                }
                already_removed += 1;
            }
        }
    }
    assert_eq!(
        registered,
        payloads.keys().cloned().collect::<BTreeSet<_>>()
    );
    for (identity, path) in &directory_paths {
        if path.starts_with(&staging) && path != &staging {
            assert!(
                registered_directories.contains(identity),
                "actual spill directory has no genuine creation receipt: {}",
                path.display()
            );
        }
    }
    eprintln!("terminal fixture: {} surviving tracked payloads, {} actual directory creation receipts, {already_removed} receipts for producer-consumed inputs", registered.len(), registered_directories.len());
    drop(memory);
    let terminal = run.retire_for_maintenance().unwrap();
    assert_eq!(terminal, run_root);
    // No compatibility deletion is called for this test. Every observed removal
    // must come from the real bounded terminal lane and its durable continuation.
    let mut previous = payloads.len();
    let mut saw_partial_progress = false;
    let mut last = None;
    let attempt_limit = payloads
        .len()
        .checked_mul(32)
        .and_then(|count| count.checked_add(512))
        .expect("bounded fixture attempt count");
    for attempt in 0..attempt_limit {
        last = Some(collector_attempt(&root));
        let remaining = if staging.exists() {
            staging_output_count(&staging)
        } else {
            0
        };
        assert!(remaining <= previous, "collector recreated terminal output");
        if remaining < previous {
            assert!(
                last.as_ref()
                    .unwrap()
                    .last_attempt
                    .as_ref()
                    .and_then(|attempt| attempt.attempted_entries)
                    .is_some_and(|entries| entries > 0),
                "output disappeared without a recorded actual mutation grant"
            );
        }
        if remaining < previous && !saw_partial_progress {
            assert!(
                remaining > 0,
                "more than100 output leaves disappeared in the first mutation grant"
            );
            saw_partial_progress = true;
        }
        previous = remaining;
        if !terminal.exists() {
            break;
        }
        if attempt % 64 == 0 {
            eprintln!(
                "terminal continuation attempt {attempt}: {remaining}/{} producer outputs remain",
                payloads.len()
            );
        }
    }
    assert!(
        saw_partial_progress,
        "actual multigrant output progress was never observed: {last:?}"
    );
    assert!(
        !terminal.exists(),
        "real terminal root did not converge across controller restarts: {last:?}"
    );
    assert_eq!(all_function_rows(&root), before);
    drain(&root);
}

#[cfg(unix)]
#[test]
fn dc_s5_real_containment_fifo_retires_with_pin_receipt_and_terminal_root() {
    use crate::process_lifecycle::{OutputMode, PreparedChild, SpawnSpec};
    use crate::storage::maintenance_origin::{
        decode_workspace_output_origin, WORKSPACE_ORIGIN_FRAME_BYTES,
    };
    use crate::storage::maintenance_ownership::ReferenceDomain;
    use crate::storage::run_workspace::RunWorkspace;
    use std::os::unix::fs::FileTypeExt;

    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, _, _) = committed(&root);
    let before = BehaviorSnapshot::observe(&root, 0).unwrap();
    let layout = ManagedLayout::new(storage);
    let domain = ReferenceDomain::managed(&layout).unwrap();
    let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
    let run_root = run.root().to_owned();
    let executable = std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .expect("real containment qualification requires the captured shipping executable");
    let mut child = PreparedChild::prepare(SpawnSpec {
        program: PathBuf::from("/bin/sh"),
        guardian_executable: Some(executable),
        args: vec!["-c".into(), "exit 0".into()],
        workspace: run_root.clone(),
        stderr_path: run_root.join("containment.stderr"),
        stdout: OutputMode::Null,
        memory_limit_bytes: None,
        memory_phase: None,
        parent_rss_pid: None,
    })
    .unwrap();
    let fifo = child.descriptor().lifecycle_dir.join("writers.fifo");
    assert!(fs::symlink_metadata(&fifo).unwrap().file_type().is_fifo());
    let fifo_identity = crate::storage::locks::path_identity(&fifo).unwrap();
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let registry = run_root.join(".workspace-origins");
    let mut bound_fifo = None;
    for entry in fs::read_dir(&registry).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().as_encoded_bytes().starts_with(b"fifo-") {
            continue;
        }
        let bytes = fs::read(entry.path()).unwrap();
        if bytes.len() < WORKSPACE_ORIGIN_FRAME_BYTES {
            continue;
        }
        let receipt_identity = crate::storage::locks::path_identity(&entry.path()).unwrap();
        let fact = decode_workspace_output_origin(
            &bytes[..WORKSPACE_ORIGIN_FRAME_BYTES],
            receipt_identity,
            None,
            &memory,
        )
        .unwrap();
        if fact.fifo && fact.binding.output == fifo_identity {
            assert_eq!(bytes.len(), WORKSPACE_ORIGIN_FRAME_BYTES);
            let pin = registry.join(
                std::str::from_utf8(
                    &crate::storage::maintenance_controls::workspace_fifo_pin_name(&fact.binding),
                )
                .unwrap(),
            );
            assert!(fs::symlink_metadata(&pin).unwrap().file_type().is_fifo());
            assert_eq!(
                crate::storage::locks::path_identity(&pin).unwrap(),
                fifo_identity
            );
            assert!(bound_fifo.replace((entry.path(), pin)).is_none());
        }
    }
    let (receipt, pin) = bound_fifo.expect("real containment producer lacks its bound FIFO origin");
    drop(memory);
    child.start().unwrap();
    let outcome = child.wait().unwrap();
    assert!(outcome.success && outcome.containment_empty, "{outcome:?}");
    drop(child);
    assert_eq!(run.retire_for_maintenance().unwrap(), run_root);
    let mut charged_mutation = false;
    let mut last = None;
    for _ in 0..4096 {
        let summary = collector_attempt(&root);
        charged_mutation |= summary
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.attempted_entries)
            .is_some_and(|entries| entries > 0);
        last = Some(summary);
        if !run_root.exists() {
            break;
        }
    }
    assert!(
        charged_mutation,
        "actual FIFO retirement lacks a mutation grant"
    );
    assert!(!fifo.exists(), "public FIFO survived: {last:?}");
    assert!(!pin.exists(), "private FIFO pin survived: {last:?}");
    assert!(!receipt.exists(), "FIFO receipt survived: {last:?}");
    assert!(!run_root.exists(), "terminal root survived: {last:?}");
    assert_eq!(BehaviorSnapshot::observe(&root, 0).unwrap(), before);
}

#[test]
fn dc_s5_real_producer_prepared_temporary_is_collected_without_source_edit() {
    use crate::artifact_store::{leave_prepared_temporary_for_test, ArtifactStore};
    let mut fixture = CanonicalFixture::new();
    let mut partition = emitted_partition(fixture.root());
    partition.set_artifact_key(fixture.key.clone());
    leave_prepared_temporary_for_test(
        &ArtifactStore::new(fixture.artifact_root.clone()),
        &fixture.key,
        &partition,
    )
    .unwrap();
    let prefix = format!(".tmp-{}-", fixture.key);
    let temporaries = fs::read_dir(&fixture.artifact_root)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_name().to_str().unwrap().starts_with(&prefix))
        .collect::<Vec<_>>();
    assert_eq!(
        temporaries.len(),
        1,
        "real prepared producer hook did not leave exactly its temporary"
    );
    let temporary = temporaries[0].path();
    assert!(temporaries[0].file_type().unwrap().is_dir());
    let receipt = fs::read(temporary.join(".entry-origin")).unwrap();
    assert_eq!(
        receipt.len(),
        3 * 512,
        "prepared interruption lacks real Complete origin"
    );
    for (phase, frame) in receipt.chunks_exact(512).enumerate() {
        assert_eq!(&frame[..8], b"CGORIG01");
        assert_eq!(frame[24], phase as u8);
        assert_eq!(frame[480..], Sha256::digest(&frame[..480])[..]);
    }
    let payload = observe_file(&temporary.join("partition.json"));
    assert_eq!(payload.digest, receipt[2 * 512 + 264..2 * 512 + 296]);
    let mut charged_mutation = false;
    for _ in 0..256 {
        let summary = collector_attempt(fixture.root());
        charged_mutation |= summary
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.attempted_entries)
            .is_some_and(|entries| entries > 0);
        if !temporary.exists()
            && !fixture.directory.exists()
            && retired_payloads(&fixture.artifact_root).is_empty()
        {
            break;
        }
    }
    assert!(charged_mutation);
    assert!(
        !temporary.exists(),
        "real prepared temporary survived no-edit collection"
    );
    fixture.finish();
}

struct ReferencedBackupFixture {
    root: FixtureRoot,
    canonical: PathBuf,
    backup: PathBuf,
    payload: FileObservation,
    payload_identity: crate::storage::locks::FileIdentity,
    behavior: BehaviorSnapshot,
}
impl ReferencedBackupFixture {
    fn new() -> Self {
        let root = FixtureRoot::new();
        root.write_revision(0);
        install(&root).unwrap();
        let (storage, active, manifest) = committed(&root);
        // Initial fixture preparation uses the established terminal-run
        // recovery path; this case qualifies backup restoration. Dedicated
        // terminal/churn cases qualify automatic workspace convergence.
        prepare_canonical_fixture_registry(&storage, &active);
        let behavior = BehaviorSnapshot::observe(&root, 0).unwrap();
        let key = referenced_keys(&manifest).into_iter().next().unwrap();
        let shard = storage.join("artifacts").join(&key[..2]);
        let canonical = shard.join(&key);
        let backup = shard.join(format!(".replaced-{key}-{}", u128::MAX));
        let payload = observe_file(&canonical.join("partition.json"));
        let payload_identity =
            crate::storage::locks::path_identity(&canonical.join("partition.json")).unwrap();
        // Represent the historical replacement interruption using actual
        // producer bytes and inode. No ownership or association fact is minted.
        fs::rename(&canonical, &backup).unwrap();
        assert!(!canonical.exists());
        assert_eq!(observe_file(&backup.join("partition.json")), payload);
        Self {
            root,
            canonical,
            backup,
            payload,
            payload_identity,
            behavior,
        }
    }
}

fn backup_control_stage(root: &FixtureRoot) -> Option<u8> {
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let target = RepositoryIdentity::capture(&selector(root))
        .unwrap()
        .maintenance_target(&memory)
        .unwrap();
    crate::storage::maintenance_backup::backup_control_stage_for_test(target.target(), &memory)
        .unwrap()
}

#[test]
fn dc_s5_real_referenced_historical_backup_restores_missing_canonical_without_edit() {
    let fixture = ReferencedBackupFixture::new();
    let mut last = None;
    let mut charged_mutation = false;
    let mut saw_restored_publication = false;
    let mut completed = false;
    for _ in 0..4096 {
        let summary = collector_attempt(&fixture.root);
        charged_mutation |= summary
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.attempted_entries)
            .is_some_and(|entries| entries > 0);
        last = Some(summary);
        let stage = backup_control_stage(&fixture.root);
        saw_restored_publication |= stage == Some(4);
        if saw_restored_publication && stage == Some(0) {
            completed = true;
            break;
        }
    }
    assert!(charged_mutation, "restore lacks actual mutation grant");
    assert!(
        completed,
        "real restore publication did not complete: {last:?}"
    );
    assert!(
        fixture.canonical.join("partition.json").exists(),
        "referenced backup did not restore: {last:?}"
    );
    assert_eq!(
        observe_file(&fixture.canonical.join("partition.json")),
        fixture.payload
    );
    assert_eq!(
        crate::storage::locks::path_identity(&fixture.canonical.join("partition.json")).unwrap(),
        fixture.payload_identity
    );
    assert_eq!(
        BehaviorSnapshot::observe(&fixture.root, 0).unwrap(),
        fixture.behavior
    );
}

#[test]
fn dc_s5_real_referenced_historical_backup_preserves_foreign_canonical() {
    let fixture = ReferencedBackupFixture::new();
    fs::create_dir(&fixture.canonical).unwrap();
    let foreign = fixture.canonical.join("foreign-note");
    fs::write(
        &foreign,
        b"foreign canonical incarnation must not be adopted",
    )
    .unwrap();
    let foreign_before = observe_file(&foreign);
    let foreign_identity = crate::storage::locks::path_identity(&foreign).unwrap();
    for _ in 0..64 {
        collector_attempt(&fixture.root);
        assert_eq!(observe_file(&foreign), foreign_before);
        assert_eq!(
            crate::storage::locks::path_identity(&foreign).unwrap(),
            foreign_identity
        );
        assert_eq!(
            observe_file(&fixture.backup.join("partition.json")),
            fixture.payload
        );
        assert!(!fixture.canonical.join("partition.json").exists());
    }
    assert_eq!(
        BehaviorSnapshot::observe(&fixture.root, 0).unwrap(),
        fixture.behavior
    );
}

fn referenced_backup_fault_resume(point: crate::storage::maintenance_backup::BackupFault) {
    use crate::storage::maintenance_backup::{arm_backup_fault, BackupFault};
    let fixture = ReferencedBackupFixture::new();
    let artifact_root = fixture.backup.parent().unwrap().parent().unwrap();
    let guard = arm_backup_fault(
        crate::storage::locks::path_identity(artifact_root).unwrap(),
        point,
    );
    let mut last = None;
    for _ in 0..4096 {
        last = Some(collector_attempt_observed(&fixture.root));
        if guard.reached() {
            break;
        }
    }
    assert!(
        guard.reached(),
        "real backup effect cut was not reached: {last:?}"
    );
    assert!(fixture.canonical.join(".restore-origin").is_file());
    let created = crate::storage::locks::path_identity(&fixture.canonical).unwrap();
    match point {
        BackupFault::AfterCreatorSync => {
            assert!(!fixture.canonical.join("partition.json").exists());
            assert_eq!(
                observe_file(&fixture.backup.join("partition.json")),
                fixture.payload
            );
        }
        BackupFault::AfterPayloadRestore => {
            assert!(!fixture.backup.join("partition.json").exists());
            assert_eq!(
                observe_file(&fixture.canonical.join("partition.json")),
                fixture.payload
            );
        }
    }
    drop(guard);
    let mut saw_restored_publication = false;
    let mut completed = false;
    for _ in 0..4096 {
        last = Some(collector_attempt(&fixture.root));
        let stage = backup_control_stage(&fixture.root);
        saw_restored_publication |= stage == Some(4);
        if saw_restored_publication && stage == Some(0) {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "interrupted restore publication did not complete: {last:?}"
    );
    assert!(
        fixture.canonical.join("partition.json").exists(),
        "backup cut did not recover: {last:?}"
    );
    assert_eq!(
        crate::storage::locks::path_identity(&fixture.canonical).unwrap(),
        created
    );
    assert_eq!(
        observe_file(&fixture.canonical.join("partition.json")),
        fixture.payload
    );
    assert_eq!(
        crate::storage::locks::path_identity(&fixture.canonical.join("partition.json")).unwrap(),
        fixture.payload_identity
    );
    assert_eq!(
        BehaviorSnapshot::observe(&fixture.root, 0).unwrap(),
        fixture.behavior
    );
}

#[test]
fn dc_s5_real_referenced_backup_creator_sync_interruption_resumes_without_edit() {
    referenced_backup_fault_resume(
        crate::storage::maintenance_backup::BackupFault::AfterCreatorSync,
    );
}

#[test]
fn dc_s5_real_referenced_backup_payload_restore_interruption_resumes_without_edit() {
    referenced_backup_fault_resume(
        crate::storage::maintenance_backup::BackupFault::AfterPayloadRestore,
    );
}

fn wait_for_real_preparation(guard: &crate::storage::maintenance_candidate::CandidateFaultGuard) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !guard.reached() {
        assert!(
            std::time::Instant::now() < deadline,
            "actual advisory preparation never reached its root-scoped hook"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Exercise the actual stdio parser, session and tool transport, which connects
/// to the already held real coordinator owner. This does not construct a second
/// maintenance runtime or a substitute dispatcher.
fn query_through_same_owner_mcp(root: &FixtureRoot, revision: usize) {
    let path = root.path().to_owned();
    let (sender, receiver) = std::sync::mpsc::channel();
    let join = std::thread::spawn(move || {
        let result =
            std::panic::catch_unwind(|| query_through_same_owner_mcp_path(&path, revision));
        let _ = sender.send(result.is_ok());
    });
    let completed = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect(
            "actual same-owner MCP query did not progress while advisory preparation was paused",
        );
    join.join().unwrap();
    assert!(completed, "actual MCP protocol assertions failed");
}

fn query_through_same_owner_mcp_path(root: &Path, revision: usize) {
    use crate::adapters::mcp::{serve_mcp_stdio, McpServeOptions};
    let storage = root.join(".codebaseGraph/storage");
    let coordinator_before = fs::read(storage.join("coordinator.json")).unwrap();
    let options = McpServeOptions::parse(
        &[
            "--repo-root".into(),
            root.to_str().unwrap().into(),
            "--refresh-policy".into(),
            "off".into(),
        ],
        "DC-S5 real stdio transport fixture",
    )
    .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"dc-s5-preparation","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"graph_query","arguments":{"statement":"MATCH (n:Function) RETURN n.label ORDER BY n.label","limit":100,"include_structured_content":true}}}),
    ];
    let input = messages
        .iter()
        .map(|message| format!("{message}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    serve_mcp_stdio(
        &options,
        std::io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .unwrap();
    let replies = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let response = replies
        .iter()
        .find(|response| response["id"] == 2)
        .expect("actual MCP query response missing");
    assert_eq!(
        response["result"]["isError"], false,
        "actual MCP query stalled/failed during advisory preparation: {response}"
    );
    let query = &response["result"]["structuredContent"];
    assert_eq!(query["truncated"], false);
    let actual = query["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[0].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mut expected = vec![format!("revision_{revision}"), "stable_helper".into()];
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        fs::read(storage.join("coordinator.json")).unwrap(),
        coordinator_before,
        "MCP used a replacement owner rather than the held dispatcher"
    );
}

fn coordinator_health(client: &crate::coordinator::CoordinatorClient, root: &FixtureRoot) -> Value {
    client
        .execute_invocation_with_context(
            "health",
            &crate::api::OperationInvocation {
                repo: selector(root),
                arguments: json!({}),
                output_format: OutputFormat::Typed,
            },
            crate::api::ExecutionContext::with_timeout(
                std::time::Instant::now(),
                std::time::Duration::from_secs(5),
            ),
        )
        .unwrap()
        .payload
}

fn create_genuine_future_cache(root: &FixtureRoot, storage: &Path) -> (String, String) {
    use crate::api::{build_materialization_request_for_test as build_request, MaterializeOptions};
    use crate::artifact_store::{ArtifactStore, ArtifactWriteOutcome};
    let request = build_request(&MaterializeOptions {
        source_root: Some(root.path().to_owned()),
        storage_root: Some(storage.to_owned()),
        config: Some(root.path().join(".codebaseGraph/config.json")),
        mode: "full".into(),
        use_git: false,
        parallel: false,
        max_parallelism: Some(1),
        ..MaterializeOptions::default()
    })
    .unwrap();
    let source = "mod stable;\npub use stable::stable_helper;\npub fn revision_1() -> usize { stable_helper() + 1 }\n".to_owned();
    let snapshot = crate::protocol::SourceSnapshot {
        path: "src/lib.rs".into(),
        absolute_path: root.path().join("src/lib.rs").to_str().unwrap().into(),
        content_hash: format!("{:x}", Sha256::digest(source.as_bytes())),
        language: Some("rust".into()),
        byte_len: source.len() as u64,
        source: Some(source.clone()),
    };
    let profiles = crate::profiles::ProfileSet::new(&request.profiles);
    let profile = profiles.profile_for_language("rust").unwrap();
    let parse = crate::parser::parse_file(&snapshot, profile).unwrap();
    let mut partition =
        crate::partition_builder::build_partition(&request, &snapshot, parse).unwrap();
    let key = ArtifactStore::key_for_request(&request, &snapshot, Some(profile)).unwrap();
    partition.set_artifact_key(key.clone());
    assert_eq!(
        ArtifactStore::new(storage.join("artifacts"))
            .store_partition(&key, &partition)
            .unwrap(),
        ArtifactWriteOutcome::Written
    );
    assert!(active_origin_matches_payload(
        &storage.join("artifacts").join(&key[..2]).join(&key),
        &key
    ));
    (source, key)
}

#[test]
fn dc_s5_real_mcp_and_writer_progress_during_preparation_and_changed_epoch_blocks_stale_effect() {
    use crate::coordinator::{CoordinatorApiConfig, CoordinatorClient};
    use crate::storage::maintenance_candidate::arm_preparation_pause;
    use crate::storage::maintenance_ownership::{
        observe_reference_epoch, EpochObservation, ReferenceDomain,
    };
    let root = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, active_before, initial_manifest) = committed(&root);
    prepare_canonical_fixture_registry(&storage, &active_before);
    let artifact_root = storage.join("artifacts");
    let (future_source, future_key) = create_genuine_future_cache(&root, &storage);
    let future_directory = artifact_root.join(&future_key[..2]).join(&future_key);
    let future_before = observe_file(&future_directory.join("partition.json"));
    let future_origin = fs::read(future_directory.join(".entry-origin")).unwrap();
    let mut expected_cache = referenced_keys(&initial_manifest);
    expected_cache.insert(future_key.clone());
    assert_eq!(
        cache_keys(&storage),
        expected_cache,
        "future cache must be the only genuine unreferenced candidate"
    );
    assert!(!referenced_keys(&initial_manifest).contains(&future_key));
    let layout = ManagedLayout::new(storage.clone());
    let domain = ReferenceDomain::managed(&layout).unwrap();
    let before_epoch = match observe_reference_epoch(&domain).unwrap() {
        EpochObservation::Verified(epoch) => epoch.counter(),
        other => panic!("future-cache fixture lacks genuine covered E: {other:?}"),
    };
    let guard = arm_preparation_pause(&artifact_root).unwrap();
    let refresh = crate::api::RefreshServiceConfig {
        backend: crate::api::context::GraphRefreshBackend::Poll,
        max_parallelism: 1,
        reconcile_interval: std::time::Duration::from_millis(250),
        explicit_overrides: crate::api::RefreshConfigOverrides {
            policy: true,
            backend: true,
            max_parallelism: true,
            reconcile_interval: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let owner = CoordinatorClient::connect_or_start(CoordinatorApiConfig::new(
        selector(&root),
        Some(refresh),
    ))
    .unwrap();
    wait_for_real_preparation(&guard);
    let preparing = coordinator_health(&owner, &root);
    assert_eq!(preparing["maintenance"]["running"], true);
    assert!(
        preparing["maintenance"]["last_attempt"]["finished_at_unix_ms"].is_null(),
        "hook is not pausing the current actual attempt"
    );
    let started = preparing["maintenance"]["last_attempt"]["started_at_unix_ms"]
        .as_u64()
        .unwrap();
    query_through_same_owner_mcp(&root, 0);
    fs::write(root.path().join("src/lib.rs"), future_source).unwrap();
    // The held owner's actual refresh service performs the native write.
    // Observe its committed manifest while the owner's cleanup stays paused.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        let active: crate::storage::managed::ActiveGeneration =
            serde_json::from_slice(&fs::read(storage.join("active.json")).unwrap()).unwrap();
        if active.generation_id != active_before {
            let generation = layout.generation(&active.generation_id).unwrap();
            let published: NativeManifest =
                serde_json::from_slice(&fs::read(generation.manifest_path()).unwrap()).unwrap();
            if published.files["src/lib.rs"].artifact_key.as_deref() == Some(future_key.as_str()) {
                break;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual owner refresh writer stalled during advisory preparation: {}",
            coordinator_health(&owner, &root)
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    query_through_same_owner_mcp(&root, 1);
    // A committed pointer/manifest is observable before the refresh writer
    // releases source activity. Poll only that real busy state, within the same
    // 45-second writer-progress deadline; uncovered/corrupt epochs still fail.
    let after_epoch = loop {
        match observe_reference_epoch(&domain).unwrap() {
            EpochObservation::Verified(epoch) => break epoch.counter(),
            EpochObservation::Pending(reason) if reason == "reference_activity_in_use" => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "writer did not release source activity within its progress deadline"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            other => panic!("writer did not leave genuine covered E: {other:?}"),
        }
    };
    assert!(after_epoch > before_epoch);
    assert_eq!(
        observe_file(&future_directory.join("partition.json")),
        future_before
    );
    drop(guard);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let health = coordinator_health(&owner, &root);
        let attempt = &health["maintenance"]["last_attempt"];
        if attempt["finished_at_unix_ms"].is_number()
            || attempt["started_at_unix_ms"]
                .as_u64()
                .is_some_and(|value| value > started)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "resumed actual preparation did not finish/dispose: {health}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    owner.drain_owned_operation();
    drop(owner);
    assert_eq!(
        observe_file(&future_directory.join("partition.json")),
        future_before,
        "stale pre-writer advisory references removed the newly active cache"
    );
    assert_eq!(
        fs::read(future_directory.join(".entry-origin")).unwrap(),
        future_origin
    );
    let store = ManagedStore::new(ManagedLayout::new(storage));
    let active = store.read_active_generation().unwrap().unwrap();
    let manifest: NativeManifest = serde_json::from_slice(
        &fs::read(
            store
                .layout()
                .generation(active.generation_id)
                .unwrap()
                .manifest_path(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(referenced_keys(&manifest).contains(&future_key));
    BehaviorSnapshot::observe(&root, 1).unwrap();
}

#[test]
fn dc_s5_real_owner_cancel_joins_paused_preparation_before_fixture_reuse() {
    use crate::coordinator::{CoordinatorApiConfig, CoordinatorClient};
    use crate::storage::maintenance_candidate::arm_preparation_pause;
    let mut fixture = CanonicalFixture::new();
    let guard = arm_preparation_pause(&fixture.artifact_root).unwrap();
    let owner = CoordinatorClient::connect_or_start(CoordinatorApiConfig::new(
        selector(fixture.root()),
        None,
    ))
    .unwrap();
    wait_for_real_preparation(&guard);
    query_through_same_owner_mcp(fixture.root(), 0);
    let (sender, receiver) = std::sync::mpsc::channel();
    let join = std::thread::spawn(move || {
        owner.drain_owned_operation();
        drop(owner);
        sender.send(()).unwrap();
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect(
        "real owner did not cancel and join advisory preparation while its pause remained armed",
    );
    join.join().unwrap();
    assert!(
        guard.cancelled(),
        "real cancellation token was not observed by the actual paused preparation"
    );
    fixture.assert_active_unchanged_without_runtime_entry();
    assert_eq!(
        observe_file(&fixture.directory.join("partition.json")),
        fixture.payload
    );
    assert_eq!(
        fs::read(fixture.directory.join(".entry-origin")).unwrap(),
        fixture.origin
    );
    assert!(retired_payloads(&fixture.artifact_root).is_empty());
    drop(guard);
    for _ in 0..128 {
        collector_attempt(fixture.root());
        if !fixture.directory.exists() && retired_payloads(&fixture.artifact_root).is_empty() {
            break;
        }
    }
    fixture.finish();
}

fn assert_root_migration_accepted(root: &FixtureRoot) {
    use crate::storage::maintenance_legacy_bootstrap as legacy;
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let target = RepositoryIdentity::capture(&selector(root))
        .unwrap()
        .maintenance_target(&memory)
        .unwrap();
    let binding =
        crate::storage::maintenance_admission::prepare_target_binding(target.target(), &memory)
            .unwrap();
    let (storage, active, _) = committed(root);
    let generation = ManagedLayout::new(&storage).generation(&active).unwrap();
    let mut scope = legacy::prepare_legacy_scope(
        target.target(),
        &binding,
        legacy::LegacyScope::ManagedAdministration {
            storage_root: &storage,
            generation_root: generation.root(),
        },
        false,
        &memory,
    )
    .unwrap();
    assert!(
        scope.has_accepted_job(&memory).unwrap(),
        "single confirmation did not durably accept the exact physical root migration"
    );
}

fn public_maintenance(root: &FixtureRoot, confirm_legacy_quiescence: bool) -> Value {
    execute(OperationRequest::Health(HealthRequest {
        repo: selector(root),
        confirm_legacy_quiescence,
        refresh_status: None,
        output_format: OutputFormat::Typed,
    }))
    .unwrap()
}

fn copy_actual_legacy_orphan(
    root: &FixtureRoot,
) -> crate::storage::maintenance_legacy_staging::CopiedOrphanStagingFixture {
    root.write_revision(0);
    install(root).unwrap();
    // Copy immediately after the actual public producer, before extra health
    // entries can advance modern terminal GC. Only real public COPY/JSON files
    // are copied; the helper creates no DB, compact manifest or modern receipt.
    let storage = root.path().join(".codebaseGraph/storage");
    let copied = crate::storage::maintenance_legacy_staging::copy_public_orphan_staging_fixture(
        root.path(),
        &storage,
    )
    .unwrap();
    assert!(!copied.payloads.is_empty());
    assert!(!copied.directory.join("manifest.json").exists());
    assert!(!copied.directory.join(".workspace.binding").exists());
    assert!(!copied.directory.join(".entry-origin").exists());
    for payload in &copied.payloads {
        assert!(payload.starts_with(&copied.directory));
        assert!(payload
            .extension()
            .is_some_and(|extension| extension == "json" || extension == "csv"));
        assert!(fs::symlink_metadata(payload).unwrap().file_type().is_file());
    }
    copied
}

fn assert_original_payloads_unchanged(paths: &[PathBuf], before: &[FileObservation]) {
    for (path, observation) in paths.iter().zip(before) {
        assert_eq!(
            &observe_file(path),
            observation,
            "unconfirmed/invalid legacy payload changed: {}",
            path.display()
        );
    }
}

fn legacy_regular_files_without_following_links(
    root: &Path,
) -> std::collections::BTreeMap<PathBuf, FileObservation> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, FileObservation>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                visit(root, &entry.path(), files);
            } else if kind.is_file() {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    observe_file(&entry.path()),
                );
            }
            // Symlinks/FIFOs are never opened for this regular-payload duplicate
            // observation. The independently captured foreign link stays exact.
        }
    }
    let mut files = std::collections::BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn copy_old_native_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        // Copy native payloads and historical metadata, never modern cleanup
        // provenance. Every copied byte still comes from the actual producer.
        if name.as_encoded_bytes().starts_with(b".") {
            continue;
        }
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        let target = destination.join(name);
        if kind.is_dir() {
            copy_old_native_tree(&entry.path(), &target);
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn assert_copied_native_payloads_unchanged(
    root: &Path,
    before: &std::collections::BTreeMap<PathBuf, FileObservation>,
) {
    for (relative, observation) in before {
        assert_eq!(&observe_file(&root.join(relative)), observation);
    }
}

#[test]
fn dc_s5_existing_old_current_managed_store_requires_one_confirmation_then_retires_old_generation()
{
    let root = FixtureRoot::new();
    let archive = FixtureRoot::new();
    root.write_revision(0);
    install(&root).unwrap();
    let (storage, old_id, old_manifest) = committed(&root);
    let layout = ManagedLayout::new(storage.clone());
    let old_name = format!("gen-{old_id}");
    let saved_generation = archive.path().join("historical-generation");
    copy_old_native_tree(
        layout.generation(&old_id).unwrap().root(),
        &saved_generation,
    );
    let saved_cache = archive.path().join("historical-artifacts");
    for key in referenced_keys(&old_manifest) {
        let target = saved_cache.join(&key[..2]).join(&key);
        fs::create_dir_all(&target).unwrap();
        fs::copy(
            storage
                .join("artifacts")
                .join(&key[..2])
                .join(&key)
                .join("partition.json"),
            target.join("partition.json"),
        )
        .unwrap();
    }
    root.write_revision(1);
    materialize(&root, "changed").unwrap();
    let (_, active, active_manifest) = committed(&root);
    assert_ne!(active, old_id);
    let behavior_before = BehaviorSnapshot::observe(&root, 1).unwrap();
    let state = root.path().join(".codebaseGraph");
    let saved_modern = archive.path().join("modern-state");
    fs::rename(&state, &saved_modern).unwrap();
    fs::create_dir(&state).unwrap();
    fs::copy(saved_modern.join("config.json"), state.join("config.json")).unwrap();
    fs::create_dir_all(storage.join("generations")).unwrap();
    for directory in ["runs", "phases", "artifacts"] {
        fs::create_dir(storage.join(directory)).unwrap();
    }
    fs::copy(
        saved_modern.join("storage/active.json"),
        storage.join("active.json"),
    )
    .unwrap();
    let historical = storage.join("generations").join(old_name);
    copy_old_native_tree(&saved_generation, &historical);
    let active_name = format!("gen-{active}");
    let active_root = storage.join("generations").join(&active_name);
    copy_old_native_tree(
        &saved_modern.join("storage/generations").join(active_name),
        &active_root,
    );
    copy_old_native_tree(&saved_cache, &storage.join("artifacts"));
    for key in referenced_keys(&active_manifest) {
        let target = storage.join("artifacts").join(&key[..2]).join(&key);
        fs::create_dir_all(&target).unwrap();
        fs::copy(
            saved_modern
                .join("storage/artifacts")
                .join(&key[..2])
                .join(&key)
                .join("partition.json"),
            target.join("partition.json"),
        )
        .unwrap();
    }
    for path in [
        storage.join("writer.maintenance-cohort.json"),
        storage.join("artifacts/.reference-epoch"),
        storage.join("artifacts/.use-cohort.json"),
        historical.join(".generation-origin"),
        active_root.join(".generation-origin"),
    ] {
        assert!(
            !path.exists(),
            "old-current fixture accidentally copied modern authority: {}",
            path.display()
        );
    }
    let active_before = legacy_regular_files_without_following_links(&active_root);
    let historical_before = legacy_regular_files_without_following_links(&historical);
    let cache_before = referenced_keys(&active_manifest)
        .into_iter()
        .map(|key| {
            let path = storage
                .join("artifacts")
                .join(&key[..2])
                .join(key)
                .join("partition.json");
            let observed = observe_file(&path);
            (path, observed)
        })
        .collect::<Vec<_>>();
    let active_pointer = fs::read(storage.join("active.json")).unwrap();
    for _ in 0..32 {
        public_maintenance(&root, false);
        assert_eq!(
            legacy_regular_files_without_following_links(&historical),
            historical_before
        );
        assert_copied_native_payloads_unchanged(&active_root, &active_before);
        for (path, observed) in &cache_before {
            assert_eq!(&observe_file(path), observed);
        }
        assert_eq!(
            fs::read(storage.join("active.json")).unwrap(),
            active_pointer
        );
        assert!(!storage.join("artifacts/.reference-epoch").exists());
    }
    let accepted = public_maintenance(&root, true);
    let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).unwrap();
    let target = RepositoryIdentity::capture(&selector(&root))
        .unwrap()
        .maintenance_target(&memory)
        .unwrap();
    let binding =
        crate::storage::maintenance_admission::prepare_target_binding(target.target(), &memory)
            .unwrap();
    let mut scope = crate::storage::maintenance_legacy_bootstrap::prepare_legacy_scope(
        target.target(),
        &binding,
        crate::storage::maintenance_legacy_bootstrap::LegacyScope::ManagedAdministration {
            storage_root: &storage,
            generation_root: &active_root,
        },
        true,
        &memory,
    )
    .unwrap();
    assert!(scope.has_accepted_job(&memory).unwrap(), "single quiescence confirmation was not durably accepted for the actual old current destination: {accepted}");
    drop(scope);
    drop(binding);
    drop(target);
    drop(memory);
    let mut last = accepted;
    for _ in 0..4096 {
        last = public_maintenance(&root, false);
        assert_copied_native_payloads_unchanged(&active_root, &active_before);
        for (path, observed) in &cache_before {
            assert_eq!(&observe_file(path), observed);
        }
        assert_eq!(
            fs::read(storage.join("active.json")).unwrap(),
            active_pointer
        );
        if !historical.exists() {
            break;
        }
    }
    assert!(
        !historical.exists(),
        "confirmed historical generation did not retire through default-false restarts: {last}"
    );
    assert_eq!(
        BehaviorSnapshot::observe(&root, 1).unwrap(),
        behavior_before
    );
}

#[test]
fn dc_s5_real_copied_orphan_staging_requires_confirmation_then_retires_without_source_edit() {
    let root = FixtureRoot::new();
    let copied = copy_actual_legacy_orphan(&root);
    let before = BehaviorSnapshot::observe(&root, 0).unwrap();
    let payload_before = copied
        .payloads
        .iter()
        .map(|path| observe_file(path))
        .collect::<Vec<_>>();
    let foreign = copied.directory.join("foreign-user-note");
    fs::write(
        &foreign,
        b"foreign historical sibling must stay at this physical public path",
    )
    .unwrap();
    let foreign_before = observe_file(&foreign);
    let foreign_identity = crate::storage::locks::path_identity(&foreign).unwrap();
    let directory_identity = crate::storage::locks::path_identity(&copied.directory).unwrap();
    let external = FixtureRoot::new();
    let external_file = external.path().join("legacy-external-sentinel");
    fs::write(&external_file, b"independent legacy fixture symlink target").unwrap();
    let external_before = observe_file(&external_file);
    #[cfg(unix)]
    let link = {
        let path = copied.directory.join("foreign-external-link");
        std::os::unix::fs::symlink(&external_file, &path).unwrap();
        path
    };
    #[cfg(unix)]
    let link_before = observe_link(&link);
    let mut explained_confirmation = false;
    let storage = root.path().join(".codebaseGraph/storage");
    let idle_limit = copied
        .payloads
        .len()
        .checked_mul(32)
        .and_then(|count| count.checked_add(512))
        .unwrap();
    let mut modern_idle = false;
    for attempt in 0..idle_limit {
        let health = public_maintenance(&root, false);
        explained_confirmation |= health["maintenance"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "legacy_quiescence_required");
        assert_original_payloads_unchanged(&copied.payloads, &payload_before);
        let no_runs = fs::read_dir(storage.join("runs")).unwrap().next().is_none();
        let no_private = retired_payloads(&storage.join("artifacts")).is_empty();
        if no_runs && no_private && attempt >= 32 {
            modern_idle = true;
            break;
        }
    }
    assert!(modern_idle, "real producer's modern work never became idle before the single administrative confirmation");
    assert!(
        explained_confirmation,
        "real unconfirmed legacy staging lacks its required quiescence reason"
    );
    let attempt_limit = copied
        .payloads
        .len()
        .checked_mul(32)
        .and_then(|count| count.checked_add(512))
        .unwrap();
    let accepted_health = public_maintenance(&root, true);
    assert_root_migration_accepted(&root);
    let mut accepted_job = None;
    let mut version = 0;
    let mut last = accepted_health;
    for _ in 0..attempt_limit {
        // ONE confirmation above accepts this real bound administrative job.
        // Every later public entry restarts with the default false input.
        last = public_maintenance(&root, false);
        let observed =
            crate::storage::maintenance_legacy_staging::observe_accepted_staging_job(root.path())
                .unwrap();
        if let Some(observed) = observed {
            assert_eq!(observed.member, directory_identity);
            assert_ne!(observed.job, [0; 32]);
            if let Some(accepted) = accepted_job {
                assert_eq!(
                    observed.job, accepted,
                    "default-false restart invented/replaced the accepted member job"
                );
            } else {
                accepted_job = Some(observed.job);
            }
            assert!(observed.version >= version);
            version = observed.version;
        } else if accepted_job.is_some() && copied.payloads.iter().any(|path| path.exists()) {
            panic!("accepted member job was lost before its original payloads retired: {last}");
        } else {
            assert_root_migration_accepted(&root);
        }
        assert_eq!(observe_file(&foreign), foreign_before);
        assert_eq!(
            crate::storage::locks::path_identity(&foreign).unwrap(),
            foreign_identity
        );
        assert_eq!(
            crate::storage::locks::path_identity(&copied.directory).unwrap(),
            directory_identity
        );
        assert_eq!(observe_file(&external_file), external_before);
        #[cfg(unix)]
        assert_eq!(observe_link(&link), link_before);
        if copied.payloads.iter().all(|path| !path.exists()) {
            break;
        }
    }
    assert!(
        copied.payloads.iter().all(|path| !path.exists()),
        "real confirmed R4 payloads failed to converge across public controller restarts: {last}"
    );
    // No renamed duplicate of a known copied payload may remain in this legacy
    // root. Unknown foreign state stays and prevents a full clean-state claim.
    let remaining = legacy_regular_files_without_following_links(&copied.directory);
    for (path, file) in &remaining {
        if path == Path::new("foreign-user-note") {
            continue;
        }
        assert!(
            !payload_before.iter().any(|old| old.digest == file.digest),
            "retired legacy payload remains under another private name: {}",
            path.display()
        );
    }
    assert_eq!(BehaviorSnapshot::observe(&root, 0).unwrap(), before);
}

#[test]
fn dc_s5_incomplete_copied_orphan_staging_is_preserved_even_with_confirmation() {
    let root = FixtureRoot::new();
    let copied = copy_actual_legacy_orphan(&root);
    let before = BehaviorSnapshot::observe(&root, 0).unwrap();
    let invalid = copied
        .payloads
        .iter()
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .unwrap();
    // This changes only an exclusively owned synthetic legacy artifact, never
    // repository source. The actual association validator must reject it.
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(invalid)
        .unwrap()
        .write_all(b"trailing-invalid-json")
        .unwrap();
    let payload_before = copied
        .payloads
        .iter()
        .map(|path| observe_file(path))
        .collect::<Vec<_>>();
    let mut reported = false;
    // One genuine confirmation, followed by ordinary default-false restarts.
    // Repeated confirmations add a full cooldown without testing a new rule.
    for attempt in 0..=16 {
        let health = public_maintenance(&root, attempt == 0);
        reported |= health["maintenance"]["pending"] == true
            || health["maintenance"]["observation_complete"] == false;
        assert_original_payloads_unchanged(&copied.payloads, &payload_before);
    }
    assert!(
        reported,
        "incomplete legacy input was presented as complete idle state"
    );
    assert_eq!(BehaviorSnapshot::observe(&root, 0).unwrap(), before);
}
