use crate::error::NativeError;
use crate::hash;
use crate::profiles::ProfileSet;
use crate::protocol::{
    LanguageProfile, ManifestDiff, NativeSyntaxMaterializationRequest, SourceSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(crate) struct SourceScan {
    pub(crate) input: NativeSyntaxMaterializationRequest,
    pub(crate) profiles: Vec<LanguageProfile>,
    pub(crate) snapshots: BTreeMap<String, SourceSnapshot>,
    pub(crate) supported: BTreeMap<String, SourceSnapshot>,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) diff: ManifestDiff,
}

pub(crate) fn scan_sources(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<SourceScan, NativeError> {
    let source_root = PathBuf::from(&request.source_root);
    let profiles = ProfileSet::new(&request.profiles);
    let excluded_parts = request
        .excluded_parts
        .iter()
        .map(|part| part.as_str())
        .collect::<BTreeSet<_>>();
    let full_rebuild = requires_full_source_rebuild(request);
    let mut snapshots = BTreeMap::new();
    let mut supported = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for entry in WalkDir::new(&source_root).sort_by_file_name() {
        let entry = entry.map_err(|error| NativeError::InvalidInput(error.to_string()))?;
        let path = entry.path();
        if path == source_root {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative_path = relative_path(path, &source_root)?;
        if is_excluded(path, &source_root, &excluded_parts)
            || ignored_by_patterns(&relative_path, request)
        {
            diagnostics.push(format!("Ignored file: {relative_path}"));
            continue;
        }
        let language = profiles.language_for_path(path);
        let content_hash = if language.is_some() {
            hash::sha256_file(path)?
        } else {
            String::new()
        };
        if language.is_none() {
            diagnostics.push(format!("Skipped unsupported file: {relative_path}"));
        }
        let source = if language.is_some() {
            Some(fs::read_to_string(path)?)
        } else {
            None
        };
        let snapshot = SourceSnapshot {
            path: relative_path.clone(),
            absolute_path: path.to_string_lossy().to_string(),
            content_hash,
            language,
            source,
        };
        if snapshot.language.is_some() {
            supported.insert(relative_path.clone(), snapshot.clone());
        }
        snapshots.insert(relative_path, snapshot);
    }

    let diff = compute_diff(request, &supported, full_rebuild);
    let selected_languages = supported
        .values()
        .filter_map(|snapshot| snapshot.language.clone())
        .collect::<BTreeSet<_>>();
    Ok(SourceScan {
        input: request.clone(),
        profiles: profiles.selected_profiles(&selected_languages),
        snapshots,
        supported,
        diagnostics,
        diff,
    })
}

fn ignored_by_patterns(relative_path: &str, request: &NativeSyntaxMaterializationRequest) -> bool {
    if !request.include_patterns.is_empty()
        && !matches_any_pattern(relative_path, &request.include_patterns)
    {
        return true;
    }
    matches_any_pattern(relative_path, &request.ignore_patterns)
        || matches_any_pattern(relative_path, &request.exclude_patterns)
}

fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .any(|pattern| glob_matches(path, pattern))
}

fn glob_matches(path: &str, pattern: &str) -> bool {
    let pattern = normalize_relative_pattern(pattern);
    if pattern.ends_with('/') {
        return path.starts_with(pattern.trim_end_matches('/'));
    }
    if !pattern.contains('/') && wildcard_match(path.rsplit('/').next().unwrap_or(path), &pattern) {
        return true;
    }
    wildcard_match(path, &pattern)
}

fn normalize_relative_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

fn wildcard_match(text: &str, pattern: &str) -> bool {
    wildcard_match_bytes(text.as_bytes(), pattern.as_bytes())
}

fn wildcard_match_bytes(text: &[u8], pattern: &[u8]) -> bool {
    let (mut text_index, mut pattern_index) = (0_usize, 0_usize);
    let mut star_index = None;
    let mut match_index = 0_usize;
    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            text_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            match_index = text_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            text_index = match_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
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
    full_rebuild: bool,
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
    if request.mode == "full" || incompatible || full_rebuild {
        return ManifestDiff {
            added: supported.keys().cloned().collect(),
            modified: Vec::new(),
            unchanged: Vec::new(),
            deleted: previous
                .files
                .keys()
                .filter(|path| !supported.contains_key(*path))
                .cloned()
                .collect(),
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

    ManifestDiff {
        added,
        modified,
        unchanged,
        deleted,
        force_rebuild: false,
    }
}

fn requires_full_source_rebuild(request: &NativeSyntaxMaterializationRequest) -> bool {
    let Some(previous) = request.previous_manifest.as_ref() else {
        return true;
    };
    let current_digest = request.graph_build_compatibility_digest().ok();
    previous.graph_build_digest != current_digest
        || previous
            .files
            .values()
            .any(|entry| entry.artifact_key.as_deref().unwrap_or_default().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ManifestEntry, NativeManifest, OntologySchema};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn request(
        root: &Path,
        previous_manifest: Option<NativeManifest>,
    ) -> NativeSyntaxMaterializationRequest {
        NativeSyntaxMaterializationRequest {
            source_root: root.to_string_lossy().into_owned(),
            repository_label: "scan-test".to_string(),
            mode: "changed".to_string(),
            parser_version: "native-test".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: OntologySchema::default(),
            previous_manifest,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths: vec!["src/caller.rs".to_string()],
            artifact_root: root
                .join(".codebaseGraph/artifacts")
                .to_string_lossy()
                .into_owned(),
            db_path: root
                .join(".codebaseGraph/graph.ldb")
                .to_string_lossy()
                .into_owned(),
            include_fts: false,
            semantic_enrichment: false,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: root
                .join(".codebaseGraph/staging")
                .to_string_lossy()
                .into_owned(),
            atomic_rebuild: false,
            strict: true,
            parallel: false,
            progress: false,
        }
    }

    fn manifest_entry(path: &str, artifact_key: Option<&str>) -> ManifestEntry {
        ManifestEntry {
            path: path.to_string(),
            content_hash: "hash".to_string(),
            language: "rust".to_string(),
            partition_id: crate::hash::partition_id(path),
            artifact_key: artifact_key.map(str::to_string),
            node_ids: Vec::new(),
            edge_ids: Vec::new(),
            node_types: BTreeMap::new(),
            edge_types: BTreeMap::new(),
            materialized_at: "unix:0".to_string(),
        }
    }

    #[test]
    fn v1_manifest_forces_full_scan_even_with_candidate_paths() {
        let root = unique_temp_dir("codebase-graph-scan-upgrade");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/caller.rs"), "fn caller() { helper(); }\n").unwrap();
        fs::write(root.join("src/helper.rs"), "fn helper() {}\n").unwrap();

        let previous = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: None,
            files: BTreeMap::from([
                (
                    "src/caller.rs".to_string(),
                    manifest_entry("src/caller.rs", None),
                ),
                (
                    "src/helper.rs".to_string(),
                    manifest_entry("src/helper.rs", None),
                ),
            ]),
        };

        let scan = scan_sources(&request(&root, Some(previous))).unwrap();

        assert!(scan.diff.force_rebuild);
        assert_eq!(scan.supported.len(), 2);
        assert!(scan.diff.added.contains(&"src/helper.rs".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn v2_manifest_scans_all_supported_files_even_with_candidate_paths() {
        let root = unique_temp_dir("codebase-graph-scan-v2");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/caller.rs"), "fn caller() { helper(); }\n").unwrap();
        fs::write(root.join("src/helper.rs"), "fn helper() {}\n").unwrap();
        let compatible_digest = request(&root, None)
            .graph_build_compatibility_digest()
            .unwrap();

        let previous = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some(compatible_digest),
            files: BTreeMap::from([
                (
                    "src/caller.rs".to_string(),
                    manifest_entry(
                        "src/caller.rs",
                        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    ),
                ),
                (
                    "src/helper.rs".to_string(),
                    manifest_entry(
                        "src/helper.rs",
                        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                    ),
                ),
            ]),
        };

        let scan = scan_sources(&request(&root, Some(previous))).unwrap();

        assert!(!scan.diff.force_rebuild);
        assert_eq!(scan.supported.len(), 2);
        assert!(scan.supported.contains_key("src/caller.rs"));
        assert!(scan.supported.contains_key("src/helper.rs"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn digest_mismatch_forces_full_scan_but_atomic_rebuild_does_not() {
        let root = unique_temp_dir("codebase-graph-scan-digest");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/caller.rs"), "fn caller() { helper(); }\n").unwrap();
        fs::write(root.join("src/helper.rs"), "fn helper() {}\n").unwrap();

        let initial_request = request(&root, None);
        let digest = initial_request.graph_build_compatibility_digest().unwrap();
        let previous = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some("stale".to_string()),
            files: BTreeMap::from([
                (
                    "src/caller.rs".to_string(),
                    manifest_entry(
                        "src/caller.rs",
                        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    ),
                ),
                (
                    "src/helper.rs".to_string(),
                    manifest_entry(
                        "src/helper.rs",
                        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                    ),
                ),
            ]),
        };
        let mismatched_request = request(&root, Some(previous));
        let mismatched = scan_sources(&mismatched_request).unwrap();
        assert!(mismatched.diff.force_rebuild);
        assert_eq!(mismatched.supported.len(), 2);

        let mut atomic_request = request(&root, None);
        atomic_request.atomic_rebuild = true;
        let previous = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some(digest),
            files: BTreeMap::from([
                (
                    "src/caller.rs".to_string(),
                    manifest_entry(
                        "src/caller.rs",
                        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    ),
                ),
                (
                    "src/helper.rs".to_string(),
                    manifest_entry(
                        "src/helper.rs",
                        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                    ),
                ),
            ]),
        };
        atomic_request.previous_manifest = Some(previous);
        let atomic_rebuild = scan_sources(&atomic_request).unwrap();
        assert!(!atomic_rebuild.diff.force_rebuild);
        assert_eq!(atomic_rebuild.supported.len(), 2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn candidate_paths_do_not_hide_other_changed_or_deleted_files() {
        let root = unique_temp_dir("codebase-graph-scan-global-diff");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/caller.rs"), "fn caller() { helper(); }\n").unwrap();
        fs::write(root.join("src/helper.rs"), "fn helper() {}\n").unwrap();
        fs::write(root.join("src/deleted.rs"), "fn deleted() {}\n").unwrap();

        let previous = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some(
                request(&root, None)
                    .graph_build_compatibility_digest()
                    .unwrap(),
            ),
            files: BTreeMap::from([
                (
                    "src/caller.rs".to_string(),
                    manifest_entry(
                        "src/caller.rs",
                        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    ),
                ),
                (
                    "src/helper.rs".to_string(),
                    manifest_entry(
                        "src/helper.rs",
                        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                    ),
                ),
                (
                    "src/deleted.rs".to_string(),
                    manifest_entry(
                        "src/deleted.rs",
                        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
                    ),
                ),
            ]),
        };

        fs::write(root.join("src/helper.rs"), "fn helper() { caller(); }\n").unwrap();
        fs::remove_file(root.join("src/deleted.rs")).unwrap();

        let scan = scan_sources(&request(&root, Some(previous))).unwrap();

        assert!(scan.diff.modified.contains(&"src/helper.rs".to_string()));
        assert!(scan.diff.deleted.contains(&"src/deleted.rs".to_string()));
        assert!(scan.supported.contains_key("src/helper.rs"));

        let _ = fs::remove_dir_all(root);
    }
}
