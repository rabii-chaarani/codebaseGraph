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

const SOURCE_SNAPSHOT_ATTEMPTS: usize = 3;
const SOURCE_SNAPSHOT_DIRECTORY: &str = "source-snapshots";

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
    request.validate_resource_limits()?;
    validate_profile_grammar_versions(&request.profiles)?;
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
        let byte_len = if language.is_some() {
            fs::metadata(path)?.len()
        } else {
            0
        };
        let snapshot = SourceSnapshot {
            path: relative_path.clone(),
            absolute_path: path.to_string_lossy().to_string(),
            content_hash,
            language,
            byte_len,
            source: None,
        };
        if snapshot.language.is_some() {
            supported.insert(relative_path.clone(), snapshot.clone());
        }
        snapshots.insert(relative_path, snapshot);
    }

    let diff = compute_diff(request, &supported, full_rebuild);
    let rebuild_paths = diff.rebuild_paths().into_iter().collect::<BTreeSet<_>>();
    spool_supported_snapshots(request, &mut snapshots, &mut supported, &rebuild_paths)?;
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

fn spool_supported_snapshots(
    request: &NativeSyntaxMaterializationRequest,
    snapshots: &mut BTreeMap<String, SourceSnapshot>,
    supported: &mut BTreeMap<String, SourceSnapshot>,
    required_paths: &BTreeSet<String>,
) -> Result<(), NativeError> {
    if required_paths.is_empty() {
        return Ok(());
    }

    let snapshot_root = Path::new(&request.staging_dir).join(SOURCE_SNAPSHOT_DIRECTORY);
    fs::create_dir_all(&snapshot_root)?;
    for snapshot in supported.values_mut() {
        if !required_paths.contains(&snapshot.path) {
            continue;
        }
        let source_path = PathBuf::from(&snapshot.absolute_path);
        let (snapshot_path, byte_len) = spool_stable_source(
            &source_path,
            &snapshot_root,
            &snapshot.path,
            &snapshot.content_hash,
        )?;
        snapshot.absolute_path = snapshot_path.to_string_lossy().into_owned();
        snapshot.byte_len = byte_len;
        snapshots.insert(snapshot.path.clone(), snapshot.clone());
    }
    Ok(())
}

fn spool_stable_source(
    source_path: &Path,
    snapshot_root: &Path,
    relative_path: &str,
    expected_hash: &str,
) -> Result<(PathBuf, u64), NativeError> {
    let partition_id = hash::partition_id(relative_path);
    let shard = snapshot_root.join(&partition_id[..2]);
    fs::create_dir_all(&shard)?;
    let mut last_error = None;

    for attempt in 0..SOURCE_SNAPSHOT_ATTEMPTS {
        let temporary_path = shard.join(format!(
            ".{partition_id}.{}.{}.snapshot",
            std::process::id(),
            attempt
        ));
        match fs::copy(source_path, &temporary_path) {
            Ok(byte_len) => {
                let copied_hash = match hash::sha256_file(&temporary_path) {
                    Ok(value) => value,
                    Err(error) => {
                        last_error = Some(error.to_string());
                        let _ = fs::remove_file(&temporary_path);
                        continue;
                    }
                };
                let current_hash = match hash::sha256_file(source_path) {
                    Ok(value) => value,
                    Err(error) => {
                        last_error = Some(error.to_string());
                        let _ = fs::remove_file(&temporary_path);
                        continue;
                    }
                };
                if copied_hash != expected_hash || current_hash != expected_hash {
                    last_error =
                        Some("source changed after its scan metadata was captured".to_string());
                    let _ = fs::remove_file(&temporary_path);
                    continue;
                }

                let final_path = shard.join(format!("{partition_id}-{copied_hash}.snapshot"));
                if final_path.exists() {
                    let existing_hash = hash::sha256_file(&final_path)?;
                    if existing_hash == copied_hash {
                        let _ = fs::remove_file(&temporary_path);
                        return Ok((final_path, byte_len));
                    }
                    return Err(NativeError::InvalidInput(format!(
                        "source snapshot collision for {relative_path}"
                    )));
                }
                fs::rename(&temporary_path, &final_path)?;
                return Ok((final_path, byte_len));
            }
            Err(error) => {
                last_error = Some(error.to_string());
                let _ = fs::remove_file(&temporary_path);
            }
        }
    }

    Err(NativeError::InvalidInput(format!(
        "source remained unstable after {SOURCE_SNAPSHOT_ATTEMPTS} snapshot attempts: {relative_path}: {}",
        last_error.unwrap_or_else(|| "unknown snapshot failure".to_string())
    )))
}

pub(crate) fn spool_snapshot_for_build(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
) -> Result<SourceSnapshot, NativeError> {
    let snapshot_root = Path::new(&request.staging_dir).join(SOURCE_SNAPSHOT_DIRECTORY);
    let current_path = PathBuf::from(&snapshot.absolute_path);
    if current_path.starts_with(&snapshot_root) {
        return Ok(snapshot.clone());
    }
    fs::create_dir_all(&snapshot_root)?;
    let (snapshot_path, byte_len) = spool_stable_source(
        &current_path,
        &snapshot_root,
        &snapshot.path,
        &snapshot.content_hash,
    )?;
    let mut spooled = snapshot.clone();
    spooled.absolute_path = snapshot_path.to_string_lossy().into_owned();
    spooled.byte_len = byte_len;
    Ok(spooled)
}

fn validate_profile_grammar_versions(profiles: &[LanguageProfile]) -> Result<(), NativeError> {
    for profile in profiles {
        if profile.grammar_version.trim().is_empty() {
            return Err(NativeError::InvalidInput(format!(
                "language profile {} requires grammar_version",
                profile.language
            )));
        }
    }
    Ok(())
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
    use crate::protocol::{
        LanguageProfile, ManifestEntry, NativeManifest, OntologySchema,
        MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
    };
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
            worker_memory_mib: 768,
            rust_memory_mib: 384,
            spill_chunk_mib: 32,
            max_parallelism: 2,
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
            node_count: 0,
            edge_count: 0,
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
            search_backend: None,
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
    fn prior_manifest_schema_forces_a_full_rebuild() {
        let root = unique_temp_dir("codebase-graph-scan-schema-upgrade");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/caller.rs"), "fn caller() {}\n").unwrap();
        let mut request = request(&root, None);
        request.manifest_schema_version = MATERIALIZATION_MANIFEST_SCHEMA_VERSION;
        let digest = request.graph_build_compatibility_digest().unwrap();
        request.previous_manifest = Some(NativeManifest {
            schema_version: MATERIALIZATION_MANIFEST_SCHEMA_VERSION - 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some(digest),
            search_backend: None,
            files: BTreeMap::from([(
                "src/caller.rs".to_string(),
                manifest_entry(
                    "src/caller.rs",
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                ),
            )]),
        });

        let scan = scan_sources(&request).unwrap();

        assert!(scan.diff.force_rebuild);
        assert_eq!(scan.diff.added, vec!["src/caller.rs"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn custom_profiles_require_a_non_empty_grammar_version() {
        let root = unique_temp_dir("codebase-graph-scan-profile-version");
        let mut request = request(&root, None);
        request.profiles = vec![LanguageProfile {
            language: "custom".to_string(),
            suffixes: vec![".custom".to_string()],
            grammar_package: "custom_grammar".to_string(),
            grammar_version: String::new(),
            root_node_types: Vec::new(),
            capture_mappings: Vec::new(),
        }];

        let error = match scan_sources(&request) {
            Ok(_) => panic!("empty grammar versions must be rejected"),
            Err(error) => error,
        };

        assert!(error
            .to_string()
            .contains("language profile custom requires grammar_version"));
    }

    #[test]
    fn scan_spools_only_paths_that_require_rebuilding() {
        let root = unique_temp_dir("codebase-graph-scan-required-snapshots");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/changed.rs"), "fn changed() {}\n").unwrap();
        fs::write(root.join("src/reused.rs"), "fn reused() {}\n").unwrap();

        let initial_request = request(&root, None);
        let initial = scan_sources(&initial_request).unwrap();
        let digest = initial_request.graph_build_compatibility_digest().unwrap();
        let previous = NativeManifest {
            schema_version: initial_request.manifest_schema_version,
            ontology: initial_request.ontology.clone(),
            parser_version: initial_request.parser_version.clone(),
            graph_build_digest: Some(digest),
            search_backend: None,
            files: initial
                .supported
                .iter()
                .map(|(path, snapshot)| {
                    let mut entry = manifest_entry(path, Some(&"a".repeat(64)));
                    entry.content_hash = snapshot.content_hash.clone();
                    entry.language = snapshot.language.clone().unwrap();
                    (path.clone(), entry)
                })
                .collect(),
        };
        fs::write(root.join("src/changed.rs"), "fn changed() { reused(); }\n").unwrap();

        let scan = scan_sources(&request(&root, Some(previous))).unwrap();
        let changed = &scan.supported["src/changed.rs"];
        let reused = &scan.supported["src/reused.rs"];

        assert!(Path::new(&changed.absolute_path)
            .components()
            .any(|component| component.as_os_str() == SOURCE_SNAPSHOT_DIRECTORY));
        assert_eq!(
            PathBuf::from(&reused.absolute_path),
            root.join("src/reused.rs")
        );
        assert!(scan
            .supported
            .values()
            .all(|snapshot| snapshot.source.is_none()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_copy_rejects_content_that_no_longer_matches_scan_metadata() {
        let root = unique_temp_dir("codebase-graph-scan-unstable-snapshot");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.rs");
        let snapshots = root.join("snapshots");
        fs::write(&source, "fn current() {}\n").unwrap();

        let error =
            spool_stable_source(&source, &snapshots, "source.rs", "stale-hash").unwrap_err();

        assert!(error.to_string().contains("remained unstable after 3"));
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
            search_backend: None,
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
            search_backend: None,
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
            search_backend: None,
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
            search_backend: None,
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
