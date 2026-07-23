use super::request::MaterializeOptions;
use crate::cli::{setup::GraphStatePaths, util::resolve_source_root};
use crate::protocol::{NativeManifest, NativeSyntaxMaterializationRequest};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::cli) fn read_manifest(path: &Path) -> Result<NativeManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read manifest {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse manifest {}: {error}", path.display()))
}

pub(in crate::cli) fn request_manifest_path(options: &MaterializeOptions) -> Option<PathBuf> {
    if options.native_request.is_some() {
        return options.manifest.clone();
    }
    let source_root =
        resolve_source_root(options.source_root.as_deref()).unwrap_or_else(|_| PathBuf::from("."));
    Some(
        options
            .manifest
            .clone()
            .unwrap_or_else(|| GraphStatePaths::derive(&source_root).manifest_path),
    )
}

pub(in crate::cli) fn read_request(
    path: &Path,
) -> Result<NativeSyntaxMaterializationRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read native request {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse native request {}: {error}", path.display()))
}

pub(in crate::cli) fn write_manifest(
    path: &Path,
    request: &NativeSyntaxMaterializationRequest,
    rebuilt_entries: &BTreeMap<String, crate::protocol::ManifestEntry>,
    diff: &crate::protocol::ManifestDiff,
) -> Result<(), String> {
    let mut files = if diff.force_rebuild {
        BTreeMap::new()
    } else {
        request
            .previous_manifest
            .as_ref()
            .map(|manifest| manifest.files.clone())
            .unwrap_or_default()
    };
    let removed: BTreeSet<String> = diff
        .deleted
        .iter()
        .chain(diff.rebuild_paths().iter())
        .cloned()
        .collect();
    files.retain(|path, _| !removed.contains(path));
    files.extend(
        rebuilt_entries
            .iter()
            .map(|(path, entry)| (path.clone(), entry.clone())),
    );

    let manifest = NativeManifest {
        schema_version: request.manifest_schema_version,
        ontology: request.ontology.clone(),
        parser_version: request.parser_version.clone(),
        files,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create manifest directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write manifest {}: {error}", path.display()))
}
