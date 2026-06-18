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
    let candidate_paths = request
        .candidate_paths
        .iter()
        .map(|path| normalize_relative_pattern(path))
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
        if !entry.file_type().is_file() {
            continue;
        }
        let relative_path = relative_path(path, &source_root)?;
        if !candidate_paths.is_empty() && !candidate_paths.contains(&relative_path) {
            continue;
        }
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

    let diff = compute_diff(request, &supported, &candidate_paths);
    Ok(SourceScan {
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
    candidate_paths: &BTreeSet<String>,
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
            deleted: previous
                .files
                .keys()
                .filter(|path| candidate_paths.is_empty() || candidate_paths.contains(*path))
                .cloned()
                .collect(),
            force_rebuild: request.mode == "full" || candidate_paths.is_empty(),
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
        .filter(|path| {
            !supported.contains_key(*path)
                && (candidate_paths.is_empty() || candidate_paths.contains(*path))
        })
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
