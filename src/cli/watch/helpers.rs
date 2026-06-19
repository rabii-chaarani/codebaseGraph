use std::time::Duration;

pub(in crate::cli) fn watch_max_wait(debounce_ms: u64) -> Duration {
    Duration::from_secs(5).max(Duration::from_millis(debounce_ms.saturating_mul(10)))
}

pub(in crate::cli) fn watch_matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .any(|pattern| watch_glob_matches(path, pattern))
}

pub(in crate::cli) fn watch_glob_matches(path: &str, pattern: &str) -> bool {
    let pattern = watch_normalize_pattern(pattern);
    if pattern.ends_with('/') {
        return path.starts_with(pattern.trim_end_matches('/'));
    }
    if !pattern.contains('/')
        && watch_wildcard_match(path.rsplit('/').next().unwrap_or(path), &pattern)
    {
        return true;
    }
    watch_wildcard_match(path, &pattern)
}

pub(in crate::cli) fn watch_normalize_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

pub(in crate::cli) fn watch_wildcard_match(text: &str, pattern: &str) -> bool {
    let (mut text_index, mut pattern_index) = (0_usize, 0_usize);
    let mut star_index = None;
    let mut match_index = 0_usize;
    let text = text.as_bytes();
    let pattern = pattern.as_bytes();
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
