//! DC-S5 workload and independent public-response oracles.
//!
//! The schedule includes failure/maintenance boundaries that require real runtime
//! hooks. Merely constructing it is not evidence that those boundaries executed.
//! These helpers never create journals, ownership records, or retirement receipts.

use super::{observe_small_tree, FixtureRoot, ObservedEntry, ObservedKind};
use codebase_graph::api::{
    CodebaseGraphApi, ContextRequest, HealthRequest, MaterializationRequest, McpTransport,
    OperationRequest, OutputFormat, QueryRequest, RepoSelector, RepositoryLifecycleRequest,
    SearchRequest,
};
use codebase_graph::protocol::{NativeManifest, NativeSyntaxMaterializationResponse};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

pub const UPDATES: usize = 20;
pub const STABLE_PATH: &str = "src/stable.rs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureBoundary {
    ArtifactTemporaryWrite,
    ArtifactReplacement,
    Publication,
    Detachment,
    Deletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadStep {
    WriteRevision {
        revision: usize,
    },
    Publish {
        revision: usize,
    },
    FailAt {
        revision: usize,
        boundary: FailureBoundary,
    },
    UnchangedRefresh {
        revision: usize,
    },
    MaintenanceWithoutEdit {
        revision: usize,
    },
}

pub fn qualification_schedule() -> Vec<WorkloadStep> {
    let boundaries = [
        FailureBoundary::ArtifactTemporaryWrite,
        FailureBoundary::ArtifactReplacement,
        FailureBoundary::Publication,
        FailureBoundary::Detachment,
        FailureBoundary::Deletion,
    ];
    let mut steps = Vec::new();
    for revision in 1..=UPDATES {
        steps.push(WorkloadStep::WriteRevision { revision });
        let boundary = (revision % 4 == 0).then(|| boundaries[revision / 4 - 1]);
        // Producer interruptions happen while new source is still unpublished.
        // Replacement additionally needs a real producer replacement fixture.
        if let Some(
            boundary @ (FailureBoundary::ArtifactTemporaryWrite
            | FailureBoundary::ArtifactReplacement
            | FailureBoundary::Publication),
        ) = boundary
        {
            steps.push(WorkloadStep::FailAt { revision, boundary });
        }
        steps.push(WorkloadStep::Publish { revision });
        // Retirement interruptions need an already published successor and the
        // real collector's owned, eligible predecessor; a filename is no proof.
        if let Some(boundary @ (FailureBoundary::Detachment | FailureBoundary::Deletion)) = boundary
        {
            steps.push(WorkloadStep::FailAt { revision, boundary });
        }
        steps.push(WorkloadStep::UnchangedRefresh { revision });
        steps.push(WorkloadStep::MaintenanceWithoutEdit { revision });
    }
    steps
}

pub struct WorkloadPair {
    pub churn: FixtureRoot,
    pub control: FixtureRoot,
    // Equal repository basenames and equally shaped parent paths keep public
    // repo_name/source-unit search documents controlled between both arms.
    _churn_workspace: FixtureRoot,
    _control_workspace: FixtureRoot,
}

impl WorkloadPair {
    pub fn new() -> std::io::Result<Self> {
        let churn_workspace = FixtureRoot::new("workload")?;
        let control_workspace = FixtureRoot::new("workload")?;
        fn repository(workspace: &FixtureRoot) -> std::io::Result<FixtureRoot> {
            let path = workspace.path().join("repository");
            fs::create_dir(&path)?;
            Ok(FixtureRoot { path })
        }
        let churn = repository(&churn_workspace)?;
        let control = repository(&control_workspace)?;
        let pair = Self {
            churn,
            control,
            _churn_workspace: churn_workspace,
            _control_workspace: control_workspace,
        };
        pair.churn.write_revision(0)?;
        pair.control.write_revision(UPDATES)?;
        Ok(pair)
    }

    /// Compare source bytes only: the control deliberately has a different
    /// creation history, independent storage, and independent timestamps.
    pub fn assert_equal_final_sources(&self) {
        for path in ["src/lib.rs", STABLE_PATH] {
            assert_eq!(
                fs::read(self.churn.path().join(path)).unwrap(),
                fs::read(self.control.path().join(path)).unwrap(),
                "final source differs at {path}"
            );
        }
    }
}

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
pub fn install(root: &FixtureRoot) -> Result<Value, String> {
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

pub fn materialize(
    root: &FixtureRoot,
    mode: &str,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    let payload = execute(OperationRequest::Materialize(MaterializationRequest {
        repo: selector(root),
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

/// Exercise the public refresh-intent path. Explicit Materialize operations
/// deliberately assemble a candidate even for unchanged source and are not a
/// substitute for this CLI watch-once opportunity.
pub fn unchanged_refresh(root: &FixtureRoot) -> Result<String, String> {
    let before = health(root)?;
    let generation = before["active_generation"]
        .as_str()
        .ok_or("missing active generation before refresh")?;
    let binary = std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")));
    let output = Command::new(binary)
        .current_dir(root.path())
        .args([
            "watch",
            "--once",
            "--mode",
            "changed",
            "--no-git",
            "--single-thread",
            "--max-parallelism",
            "1",
            "--repo-root",
        ])
        .arg(root.path())
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "watch-once failed: status={} stdout={} stderr={}",
            output.status,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let events = stdout
        .lines()
        .filter(|line| line.starts_with("watch "))
        .collect::<Vec<_>>();
    if events.len() != 1 {
        return Err(format!("expected one refresh event: {stdout}"));
    }
    let fields = events[0]
        .split_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect::<BTreeMap<_, _>>();
    for (field, expected) in [
        ("event", "refreshed"),
        ("event_count", "0"),
        ("changed_paths", "0"),
        ("rebuilt", "0"),
        ("deleted", "0"),
        ("skipped", "true"),
        ("database_written", "false"),
    ] {
        if fields.get(field) != Some(&expected) {
            return Err(format!(
                "unchanged refresh has unexpected {field}: {stdout}"
            ));
        }
    }
    let after = health(root)?;
    if after["active_generation"].as_str() != Some(generation) {
        return Err("unchanged refresh published another generation".into());
    }
    Ok(events[0].into())
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
pub struct StableArtifact {
    content_hash: String,
    artifact_key: String,
    observations: BTreeMap<PathBuf, ObservedEntry>,
}

impl StableArtifact {
    /// Observe only an actual published producer manifest and its referenced
    /// cache payload. This is a reuse oracle, never deletion authority.
    pub fn observe(root: &FixtureRoot) -> Result<Self, String> {
        let health = health(root)?;
        if health["graph_readable"] != true {
            return Err("fixture has no readable committed graph".into());
        }
        let path = health["manifest_path"]
            .as_str()
            .ok_or("missing manifest path")?;
        let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
        let fixture = fs::canonicalize(root.path()).map_err(|error| error.to_string())?;
        if !path.starts_with(&fixture) {
            return Err("published manifest escaped fixture".into());
        }
        let manifest: NativeManifest =
            serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        if manifest.files.len() != 2 || !manifest.files.contains_key("src/lib.rs") {
            return Err("producer manifest must contain exactly the two fixture sources".into());
        }
        let entry = manifest
            .files
            .get(STABLE_PATH)
            .ok_or("stable source absent from manifest")?;
        let key = entry
            .artifact_key
            .as_deref()
            .ok_or("stable source has no artifact key")?;
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("producer returned an invalid artifact key".into());
        }
        if entry.path != STABLE_PATH || entry.language != "rust" || entry.content_hash.is_empty() {
            return Err("stable source manifest identity is incomplete".into());
        }
        let storage = health["storage_root"]
            .as_str()
            .ok_or("fixture producer did not publish a managed storage root")?;
        let storage = fs::canonicalize(storage).map_err(|error| error.to_string())?;
        if !storage.starts_with(&fixture) {
            return Err("managed storage root escaped fixture".into());
        }
        let directory = storage.join("artifacts").join(&key[..2]).join(key);
        let physical = fs::canonicalize(&directory).map_err(|error| error.to_string())?;
        if !physical.starts_with(&storage) {
            return Err("referenced artifact escaped managed storage".into());
        }
        let observations = observe_small_tree(&directory).map_err(|error| error.to_string())?;
        if !observations
            .get(&PathBuf::from("partition.json"))
            .is_some_and(|entry| matches!(entry.kind, ObservedKind::File(_)))
            || observations
                .values()
                .any(|entry| matches!(entry.kind, ObservedKind::Symlink(_)))
        {
            return Err("referenced producer artifact is absent or contains a symlink".into());
        }
        Ok(Self {
            content_hash: entry.content_hash.clone(),
            artifact_key: key.into(),
            observations,
        })
    }

    pub fn assert_reused(&self, current: &Self, response: &NativeSyntaxMaterializationResponse) {
        assert!(
            response.database_written,
            "source update must actually publish"
        );
        assert!(
            response.artifacts_reused >= 1,
            "producer did not report any reused artifact"
        );
        assert_eq!(
            self, current,
            "unchanged source cache bytes/identity/metadata changed"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BehaviorSnapshot {
    function_rows: Vec<Value>,
    stable_search: Vec<Value>,
    revision_search: Vec<Value>,
    revision_context: Vec<Value>,
}

impl BehaviorSnapshot {
    pub fn observe(root: &FixtureRoot, revision: usize) -> Result<Self, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_covers_twenty_updates_and_each_required_interruption_boundary() {
        let schedule = qualification_schedule();
        let publications = schedule
            .iter()
            .filter_map(|step| match step {
                WorkloadStep::Publish { revision } => Some(*revision),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(publications, (1..=UPDATES).collect::<Vec<_>>());
        let failures = schedule
            .iter()
            .filter_map(|step| match step {
                WorkloadStep::FailAt { boundary, .. } => Some(*boundary),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            failures,
            vec![
                FailureBoundary::ArtifactTemporaryWrite,
                FailureBoundary::ArtifactReplacement,
                FailureBoundary::Publication,
                FailureBoundary::Detachment,
                FailureBoundary::Deletion,
            ]
        );
        for revision in 1..=UPDATES {
            let start = schedule
                .iter()
                .position(|step| *step == WorkloadStep::Publish { revision })
                .unwrap();
            let end = schedule
                .iter()
                .position(|step| *step == WorkloadStep::MaintenanceWithoutEdit { revision })
                .unwrap();
            let written = schedule
                .iter()
                .position(|step| *step == WorkloadStep::WriteRevision { revision })
                .unwrap();
            assert!(written < start);
            if let Some((failure, boundary)) =
                schedule
                    .iter()
                    .enumerate()
                    .find_map(|(index, step)| match step {
                        WorkloadStep::FailAt {
                            revision: found,
                            boundary,
                        } if *found == revision => Some((index, boundary)),
                        _ => None,
                    })
            {
                match boundary {
                    FailureBoundary::Detachment | FailureBoundary::Deletion => {
                        assert!(failure > start)
                    }
                    _ => assert!(written < failure && failure < start),
                }
            }

            assert!(schedule[start..end].contains(&WorkloadStep::UnchangedRefresh { revision }));
        }
    }

    #[test]
    fn independent_control_matches_twenty_source_updates_without_rewriting_stable_input() {
        let pair = WorkloadPair::new().unwrap();
        assert_eq!(
            pair.churn.path().file_name(),
            pair.control.path().file_name()
        );
        assert_ne!(pair.churn.path(), pair.control.path());
        let before = observe_small_tree(&pair.churn.path().join(STABLE_PATH)).unwrap();
        for revision in 1..=UPDATES {
            pair.churn.write_revision(revision).unwrap();
            assert_eq!(
                observe_small_tree(&pair.churn.path().join(STABLE_PATH)).unwrap(),
                before
            );
        }
        pair.assert_equal_final_sources();
        let source = fs::read_to_string(pair.churn.path().join("src/lib.rs")).unwrap();
        assert!(source.contains("revision_20()"));
        for revision in 0..UPDATES {
            assert!(!source.contains(&format!("revision_{revision}()")));
        }
    }

    #[test]
    fn search_oracle_omits_only_incarnation_ids_and_keeps_rank_spans_and_content() {
        let original = json!({"id":"churn-id", "type":"Function", "label":"revision_20",
            "layer":"semantic", "path":"src/lib.rs", "rank_score":1.0,
            "span":{"line_start":4,"line_end":4}, "summary":"stable_helper() + 20"});
        let mut other_incarnation = original.clone();
        other_incarnation["id"] = json!("clean-control-id");
        assert_eq!(
            project_search(&[original.clone()]).unwrap(),
            project_search(&[other_incarnation.clone()]).unwrap()
        );
        for (field, changed) in [
            ("rank_score", json!(0.2)),
            ("path", json!("foreign.rs")),
            ("span", json!({"line_start":5,"line_end":5})),
            ("summary", json!("wrong body")),
        ] {
            let mut changed_row = other_incarnation.clone();
            changed_row[field] = changed;
            assert_ne!(
                project_search(&[original.clone()]).unwrap(),
                project_search(&[changed_row]).unwrap()
            );
        }
    }

    #[test]
    fn oracle_rejects_missing_response_fields_and_preserves_context_multiplicity() {
        assert!(project_search(&[json!({"label":"revision_20"})]).is_err());
        let row = json!({"type":"Scope", "label":"scope", "layer":"semantic", "path":"src/lib.rs",
            "direction":"out", "relation":"HasScope", "id":"incarnation-id"});
        let projected = project_context(&row).unwrap();
        let one = vec![projected.clone()];
        let duplicate = vec![projected.clone(), projected];
        assert_ne!(one, duplicate);
        let mut changed_relation = row.clone();
        changed_relation["relation"] = json!("Defines");
        assert_ne!(
            project_context(&row).unwrap(),
            project_context(&changed_relation).unwrap()
        );
    }
}
