use super::{
    types::{WatchFileSnapshot, WatchFileState},
    WatchEventFilter,
};
use crate::product_cli::materialize::default_excluded_parts;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(in crate::product_cli) fn watch_file_snapshot(
    filter: &WatchEventFilter,
) -> Result<WatchFileSnapshot, String> {
    let mut snapshot = BTreeMap::new();
    watch_file_snapshot_inner(filter, &filter.source_root, &mut snapshot)?;
    Ok(snapshot)
}

pub(in crate::product_cli) fn watch_file_snapshot_inner(
    filter: &WatchEventFilter,
    directory: &Path,
    snapshot: &mut WatchFileSnapshot,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read directory {}: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if filter.excluded_parts.contains(name) {
                continue;
            }
            watch_file_snapshot_inner(filter, &path, snapshot)?;
        } else if path.is_file() {
            let Some(relative_path) = filter.relevant_path(&path) else {
                continue;
            };
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified_nanos = metadata
                .modified()
                .ok()
                .and_then(|modified| {
                    modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_nanos())
                })
                .unwrap_or(0);
            snapshot.insert(
                relative_path,
                WatchFileState {
                    modified_nanos,
                    len: metadata.len(),
                },
            );
        }
    }
    Ok(())
}

pub(in crate::product_cli) fn watch_snapshot_diff(
    previous: &WatchFileSnapshot,
    current: &WatchFileSnapshot,
) -> BTreeSet<String> {
    let mut changed_paths = BTreeSet::new();
    for (path, state) in current {
        if previous.get(path) != Some(state) {
            changed_paths.insert(path.clone());
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            changed_paths.insert(path.clone());
        }
    }
    changed_paths
}

pub(in crate::product_cli) fn scan_source_snapshots(
    root: &Path,
) -> Vec<(String, Option<&'static str>)> {
    let mut snapshots = Vec::new();
    scan_source_snapshots_inner(root, root, &mut snapshots);
    snapshots.sort_by(|left, right| left.0.cmp(&right.0));
    snapshots
}

pub(in crate::product_cli) fn scan_source_snapshots_inner(
    root: &Path,
    directory: &Path,
    snapshots: &mut Vec<(String, Option<&'static str>)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if default_excluded_parts().iter().any(|part| part == name) {
            continue;
        }
        if path.is_dir() {
            scan_source_snapshots_inner(root, &path, snapshots);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            snapshots.push((relative.to_string(), language_for_path(&path)));
        }
    }
}

pub(in crate::product_cli) fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("py") => Some("python"),
        Some("rs") => Some("rust"),
        Some("go") => Some("go"),
        Some("c") | Some("h") => Some("c"),
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") | Some("hh") => Some("cpp"),
        Some("f") | Some("f90") | Some("f95") | Some("for") => Some("fortran"),
        _ => None,
    }
}
