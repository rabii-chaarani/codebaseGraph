use std::collections::BTreeSet;

/// Shared source-selection policy for scans and refresh watchers.
///
/// Pattern matching intentionally follows the historical materialization and
/// watcher behavior: `*` and `?` match path separators, patterns without a
/// separator match the final path component, and a trailing slash matches a
/// directory and all of its descendants. Generated output is filtered by
/// default, with an explicit include pattern serving as an opt-in.
pub(crate) struct SourceSelection<'a> {
    excluded_parts: &'a BTreeSet<String>,
    include_patterns: &'a [String],
    exclude_patterns: &'a [String],
    ignore_patterns: &'a [String],
}

impl<'a> SourceSelection<'a> {
    pub(crate) fn new(
        excluded_parts: &'a BTreeSet<String>,
        include_patterns: &'a [String],
        exclude_patterns: &'a [String],
        ignore_patterns: &'a [String],
    ) -> Self {
        Self {
            excluded_parts,
            include_patterns,
            exclude_patterns,
            ignore_patterns,
        }
    }

    /// Returns whether a relative path is an eligible source file.
    pub(crate) fn includes_file(&self, relative: &str) -> bool {
        let relative = normalize_relative_path(relative);
        if relative.is_empty() || self.has_hard_excluded_part(&relative) {
            return false;
        }

        let explicitly_included = matches_any_pattern(&relative, self.include_patterns);
        if !self.include_patterns.is_empty() && !explicitly_included {
            return false;
        }
        if is_generated_path(&relative) && !explicitly_included {
            return false;
        }
        !matches_any_pattern(&relative, self.ignore_patterns)
            && !matches_any_pattern(&relative, self.exclude_patterns)
    }

    /// Returns whether walking a relative directory can produce an eligible
    /// descendant. It only prunes when exclusion is proven, so a pattern that
    /// might select a descendant keeps the directory traversable.
    pub(crate) fn should_descend(&self, relative: &str) -> bool {
        let relative = normalize_relative_path(relative);
        if relative.is_empty() || self.has_hard_excluded_part(&relative) {
            return false;
        }

        if is_generated_directory(&relative)
            && (self.include_patterns.is_empty()
                || !self
                    .include_patterns
                    .iter()
                    .any(|pattern| pattern_may_match_descendant(&relative, pattern)))
        {
            return false;
        }

        if !self.include_patterns.is_empty()
            && !self
                .include_patterns
                .iter()
                .any(|pattern| pattern_may_match_descendant(&relative, pattern))
        {
            return false;
        }

        if patterns_prove_directory_excluded(&relative, self.ignore_patterns)
            || patterns_prove_directory_excluded(&relative, self.exclude_patterns)
        {
            return false;
        }

        true
    }

    fn has_hard_excluded_part(&self, relative: &str) -> bool {
        relative
            .split('/')
            .any(|part| self.excluded_parts.contains(part))
    }
}

const GENERATED_PARTS: &[&str] = &[".astro", ".kwiki", ".scryer"];

fn is_generated_path(relative: &str) -> bool {
    let parts = relative.split('/').collect::<Vec<_>>();
    parts.iter().enumerate().any(|(index, part)| {
        GENERATED_PARTS.contains(part) || (part.starts_with("dist-") && index + 1 < parts.len())
    })
}

fn is_generated_directory(relative: &str) -> bool {
    relative
        .split('/')
        .any(|part| GENERATED_PARTS.contains(&part) || part.starts_with("dist-"))
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
        let directory = pattern.trim_end_matches('/');
        return path == directory || path.starts_with(&format!("{directory}/"));
    }
    if !pattern.contains('/') && wildcard_match(path.rsplit('/').next().unwrap_or(path), &pattern) {
        return true;
    }
    wildcard_match(path, &pattern)
}

fn normalize_relative_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
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

fn pattern_may_match_descendant(directory: &str, pattern: &str) -> bool {
    let pattern = normalize_relative_pattern(pattern);
    if pattern.is_empty() || pattern.starts_with('#') {
        return false;
    }
    if pattern.ends_with('/') {
        let base = pattern.trim_end_matches('/');
        return base == directory
            || base.starts_with(&format!("{directory}/"))
            || directory.starts_with(&format!("{base}/"));
    }

    // A basename pattern can match a descendant's basename at any depth.
    if !pattern.contains('/') {
        return true;
    }

    let wildcard_index = pattern.find(['*', '?']);
    let literal_prefix = wildcard_index
        .map(|index| &pattern[..index])
        .unwrap_or(&pattern);
    if literal_prefix.is_empty() {
        return true;
    }
    let prefix = literal_prefix.trim_end_matches('/');
    if wildcard_index.is_some() && directory.starts_with(prefix) {
        return true;
    }
    prefix == directory
        || prefix.starts_with(&format!("{directory}/"))
        || directory.starts_with(&format!("{prefix}/"))
}

fn patterns_prove_directory_excluded(directory: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .any(|pattern| {
            if pattern.is_empty() || pattern.starts_with('#') {
                return false;
            }
            let normalized = normalize_relative_pattern(pattern);
            if normalized.ends_with('/') {
                let base = normalized.trim_end_matches('/');
                return directory == base;
            }
            if let Some(base) = normalized.strip_suffix("/**") {
                return directory == base;
            }
            false
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection<'a>(
        excluded_parts: &'a BTreeSet<String>,
        includes: &'a [String],
        excludes: &'a [String],
        ignores: &'a [String],
    ) -> SourceSelection<'a> {
        SourceSelection::new(excluded_parts, includes, excludes, ignores)
    }

    #[test]
    fn preserves_basename_and_separator_pattern_semantics() {
        let excluded = BTreeSet::new();
        let includes = vec!["*.rs".to_string()];
        let empty = Vec::new();
        let selected = selection(&excluded, &includes, &empty, &empty);
        assert!(selected.includes_file("src/main.rs"));
        assert!(!selected.includes_file("src/main.txt"));

        let includes = vec!["src/".to_string()];
        let selected = selection(&excluded, &includes, &empty, &empty);
        assert!(selected.includes_file("src/main.rs"));
        assert!(!selected.includes_file("src-other/main.rs"));
    }

    #[test]
    fn generated_paths_require_explicit_inclusion_and_preserve_descendants() {
        let excluded = BTreeSet::new();
        let empty = Vec::new();
        let selected = selection(&excluded, &empty, &empty, &empty);
        assert!(!selected.includes_file(".kwiki/site/index.html"));
        assert!(!selected.should_descend(".kwiki"));
        assert!(!selected.includes_file("dist-build/output.rs"));

        let includes = vec![".kwiki/site/*.html".to_string()];
        let selected = selection(&excluded, &includes, &empty, &empty);
        assert!(selected.should_descend(".kwiki"));
        assert!(selected.should_descend(".kwiki/site"));
        assert!(selected.includes_file(".kwiki/site/index.html"));
    }

    #[test]
    fn wildcard_prefix_keeps_a_directory_that_can_match_across_separators() {
        let excluded = BTreeSet::new();
        let includes = vec!["src/foo*.rs".to_string()];
        let empty = Vec::new();
        let selected = selection(&excluded, &includes, &empty, &empty);

        assert!(selected.should_descend("src/foobar"));
        assert!(selected.includes_file("src/foobar/item.rs"));
    }

    #[test]
    fn dist_prefix_filters_generated_directories_but_not_source_filenames() {
        let excluded = BTreeSet::new();
        let empty = Vec::new();
        let selected = selection(&excluded, &empty, &empty, &empty);

        assert!(!selected.should_descend("dist-build"));
        assert!(!selected.includes_file("dist-build/output.rs"));
        assert!(selected.includes_file("src/dist-helper.rs"));
    }

    #[test]
    fn hard_exclusions_cannot_be_overridden() {
        let excluded = BTreeSet::from(["target".to_string()]);
        let includes = vec!["target/**/*.rs".to_string()];
        let empty = Vec::new();
        let selected = selection(&excluded, &includes, &empty, &empty);
        assert!(!selected.should_descend("target"));
        assert!(!selected.includes_file("target/debug/build.rs"));
    }

    #[test]
    fn excludes_prune_only_when_the_directory_boundary_is_proven() {
        let excluded = BTreeSet::new();
        let empty = Vec::new();
        let excludes = vec!["generated/".to_string()];
        let selected = selection(&excluded, &empty, &excludes, &empty);
        assert!(!selected.should_descend("generated"));
        assert!(selected.should_descend("generated-other"));
    }
}
