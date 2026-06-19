use crate::cli::format::{block_value, value_array, value_str};
use crate::cli::setup::GraphStatePaths;
use crate::cli::watch::scan_source_snapshots;
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use serde_json::json;
use std::path::Path;

pub(in crate::cli) fn materialization_payload(
    response: &NativeSyntaxMaterializationResponse,
    mode: &str,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let rebuilt_paths = response.diff.rebuild_paths();
    let skipped_paths = response
        .snapshots
        .iter()
        .filter_map(|(path, snapshot)| {
            if snapshot.language.is_none() {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let ignored_paths = response
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.strip_prefix("Ignored file: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    json!({
        "mode": mode,
        "scanned": response.snapshots.len(),
        "rebuilt": rebuilt_paths.len(),
        "skipped": skipped_paths.len(),
        "ignored": ignored_paths.len(),
        "deleted": response.diff.deleted.len(),
        "diagnostics": response.diagnostics,
        "manifest_path": paths.manifest_path,
        "rebuilt_paths": rebuilt_paths,
        "skipped_paths": skipped_paths.clone(),
        "ignored_paths": ignored_paths,
        "deleted_paths": response.diff.deleted.clone(),
        "would_rebuild": response.diff.rebuild_paths(),
        "would_delete": response.diff.deleted,
        "would_skip": skipped_paths,
        "graph_summary": response.graph_summary,
        "node_rows": response.node_rows,
        "edge_rows": response.edge_rows,
        "connector_rows": response.connector_rows,
        "database_written": response.database_written,
        "progress_events": response.progress_events,
        "phase_timings": response.phase_timings,
    })
}

pub(in crate::cli) fn dry_run_materialization_payload(
    request: &NativeSyntaxMaterializationRequest,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let snapshots = scan_source_snapshots(Path::new(&request.source_root));
    let scanned = snapshots.len();
    let skipped_paths = snapshots
        .into_iter()
        .filter_map(|(path, language)| if language.is_none() { Some(path) } else { None })
        .collect::<Vec<_>>();
    json!({
        "mode": "dry_run",
        "scanned": scanned,
        "rebuilt": 0,
        "skipped": skipped_paths.len(),
        "deleted": 0,
        "diagnostics": [],
        "manifest_path": paths.manifest_path,
        "rebuilt_paths": [],
        "skipped_paths": skipped_paths,
        "deleted_paths": [],
        "graph_summary": {},
    })
}

pub(in crate::cli) fn serialize_plan_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "plan mode={} scanned={} rebuild={} delete={} skip={} ignored={}",
        block_value(value_str(payload, "mode")),
        payload
            .get("scanned")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("rebuilt")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("deleted")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("skipped")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("ignored")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    )];
    append_plan_path_lines(&mut lines, "rebuild", value_array(payload, "would_rebuild"));
    append_plan_path_lines(&mut lines, "delete", value_array(payload, "would_delete"));
    append_plan_path_lines(&mut lines, "skip", value_array(payload, "would_skip"));
    append_plan_path_lines(&mut lines, "ignore", value_array(payload, "ignored_paths"));
    format!("{}\n", lines.join("\n"))
}

pub(in crate::cli) fn append_plan_path_lines(
    lines: &mut Vec<String>,
    label: &str,
    paths: &[serde_json::Value],
) {
    for path in paths {
        if let Some(path) = path.as_str() {
            lines.push(format!("{label} {}", block_value(path)));
        }
    }
}
