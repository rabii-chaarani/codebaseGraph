use crate::error::NativeError;
use crate::hash;
use crate::profiles::ProfileSet;
use crate::protocol::{ManifestDiff, NativeSyntaxMaterializationRequest, SourceSnapshot};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(crate) struct SourceScan {
    pub(crate) snapshots: BTreeMap<String, SourceSnapshot>,
    pub(crate) supported: BTreeMap<String, SourceSnapshot>,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) diff: ManifestDiff,
}

pub(crate) fn scan_source_state(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<SourceScan, NativeError> {
    let source_root = PathBuf::from(&request.source_root);
    let profiles = ProfileSet::new(&request.profiles);
    let excluded_parts = request
        .excluded_parts
        .iter()
        .map(|part| part.as_str())
        .collect::<BTreeSet<_>>();
    let mut snapshots = BTreeMap::new();
    let mut supported = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for entry in WalkDir::new(&source_root).sort_by_file_name() {
        let entry = entry.map_err(|error| NativeError::InvalidInput(error.to_string()))?;
        let path = entry.path();
        if path == source_root {
            continue;
        }
        if is_excluded(path, &source_root, &excluded_parts) {
            continue;
        }
        if !entry.file_type().is_file() && !path.is_file() {
            continue;
        }
        let relative_path = relative_path(path, &source_root)?;
        let language = profiles.language_for_path(path);
        let content_hash = if language.is_some() {
            hash::sha256_file(path)?
        } else {
            String::new()
        };
        if language.is_none() {
            diagnostics.push(format!("Skipped unsupported file: {relative_path}"));
        }
        let snapshot = SourceSnapshot {
            path: relative_path.clone(),
            absolute_path: path.to_string_lossy().to_string(),
            content_hash,
            language,
        };
        if snapshot.language.is_some() {
            supported.insert(relative_path.clone(), snapshot.clone());
        }
        snapshots.insert(relative_path, snapshot);
    }

    let diff = compute_diff(request, &supported);
    Ok(SourceScan {
        snapshots,
        supported,
        diagnostics,
        diff,
    })
}

fn is_excluded(path: &Path, source_root: &Path, excluded_parts: &BTreeSet<&str>) -> bool {
    path.strip_prefix(source_root)
        .ok()
        .map(|relative| {
            relative.components().any(|component| {
                excluded_parts.contains(component.as_os_str().to_string_lossy().as_ref())
            })
        })
        .unwrap_or(false)
}

fn relative_path(path: &Path, source_root: &Path) -> Result<String, NativeError> {
    Ok(path
        .strip_prefix(source_root)
        .map_err(|error| NativeError::InvalidInput(error.to_string()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn compute_diff(
    request: &NativeSyntaxMaterializationRequest,
    supported: &BTreeMap<String, SourceSnapshot>,
) -> ManifestDiff {
    let Some(previous) = &request.previous_manifest else {
        return ManifestDiff {
            added: supported.keys().cloned().collect(),
            modified: Vec::new(),
            unchanged: Vec::new(),
            deleted: Vec::new(),
            force_rebuild: true,
        };
    };
    let incompatible = previous.schema_version != request.manifest_schema_version
        || previous.ontology != request.ontology
        || previous.parser_version != request.parser_version;
    if request.mode == "full" || incompatible {
        return ManifestDiff {
            added: supported.keys().cloned().collect(),
            modified: Vec::new(),
            unchanged: Vec::new(),
            deleted: previous.files.keys().cloned().collect(),
            force_rebuild: true,
        };
    }

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut unchanged = Vec::new();
    for (path, snapshot) in supported {
        match previous.files.get(path) {
            None => added.push(path.clone()),
            Some(entry)
                if entry.content_hash != snapshot.content_hash
                    || entry.language != snapshot.language.clone().unwrap_or_default() =>
            {
                modified.push(path.clone())
            }
            Some(_) => unchanged.push(path.clone()),
        }
    }
    let deleted: Vec<String> = previous
        .files
        .keys()
        .filter(|path| !supported.contains_key(*path))
        .cloned()
        .collect();
    if request.atomic_rebuild && (!added.is_empty() || !modified.is_empty() || !deleted.is_empty())
    {
        return ManifestDiff {
            added: supported.keys().cloned().collect(),
            modified: Vec::new(),
            unchanged: Vec::new(),
            deleted: previous.files.keys().cloned().collect(),
            force_rebuild: true,
        };
    }

    ManifestDiff {
        added,
        modified,
        unchanged,
        deleted,
        force_rebuild: false,
    }
}
