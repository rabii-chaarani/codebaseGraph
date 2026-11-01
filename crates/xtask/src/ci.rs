use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

const METADATA_PATHS: [&str; 6] = [
    ".release-please-manifest.json",
    "Cargo.toml",
    "Cargo.lock",
    "CHANGELOG.md",
    "crates/k-wiki/Cargo.toml",
    "crates/xtask/Cargo.toml",
];
const MAX_DIFF_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_BLOB_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum MetadataDecision {
    Eligible {
        old_version: String,
        new_version: String,
    },
    Full {
        reason_code: &'static str,
    },
}

#[derive(Debug, Serialize)]
struct CiModeOutput<'a> {
    schema_version: u32,
    mode: &'static str,
    reason_code: &'static str,
    base_sha: &'a str,
    head_sha: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Default)]
struct LockPackage {
    name: Option<String>,
    version: Option<(String, Span)>,
    source_seen: bool,
}

pub(super) fn run_ci_mode(args: Vec<String>) -> Result<(), String> {
    let mut repo = None;
    let mut base_sha = None;
    let mut head_sha = None;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("ci-mode requires a value after {flag}"))?;
        let slot = match flag.as_str() {
            "--repo" => &mut repo,
            "--base-sha" => &mut base_sha,
            "--head-sha" => &mut head_sha,
            _ => return Err(format!("unknown ci-mode option: {flag}")),
        };
        if slot.replace(value).is_some() {
            return Err(format!("ci-mode option repeated: {flag}"));
        }
    }
    let repo = repo.ok_or_else(|| "ci-mode requires --repo <git-object-checkout>".to_string())?;
    let base_sha = base_sha.ok_or_else(|| "ci-mode requires --base-sha <full-sha>".to_string())?;
    let head_sha = head_sha.ok_or_else(|| "ci-mode requires --head-sha <full-sha>".to_string())?;
    if !is_full_sha(&base_sha) || !is_full_sha(&head_sha) {
        return Err("ci-mode requires full 40-character hexadecimal commit SHAs".to_string());
    }
    let decision = acquire_and_classify(Path::new(&repo), &base_sha, &head_sha);
    let (mode, reason_code) = match decision {
        MetadataDecision::Eligible { .. } => ("lightweight", "metadata-only-version-bump"),
        MetadataDecision::Full { reason_code } => ("full", reason_code),
    };
    let output = CiModeOutput {
        schema_version: 1,
        mode,
        reason_code,
        base_sha: &base_sha,
        head_sha: &head_sha,
    };
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, &output)
        .map_err(|error| format!("write ci-mode JSON: {error}"))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| format!("write ci-mode output: {error}"))
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn acquire_and_classify(repo: &Path, base_sha: &str, head_sha: &str) -> MetadataDecision {
    if !repo.is_dir()
        || verify_commit(repo, base_sha).is_err()
        || verify_commit(repo, head_sha).is_err()
    {
        return MetadataDecision::Full {
            reason_code: "git-acquisition-failed",
        };
    }
    match git(
        repo,
        ["merge-base", "--is-ancestor", base_sha, head_sha],
        4096,
    ) {
        Ok(output) if output.status.success() => {}
        Ok(output) if output.status.code() == Some(1) => {
            return MetadataDecision::Full {
                reason_code: "base-not-ancestor",
            }
        }
        _ => {
            return MetadataDecision::Full {
                reason_code: "git-acquisition-failed",
            }
        }
    }
    let output = match git(
        repo,
        [
            "diff-tree",
            "--raw",
            "--no-abbrev",
            "-r",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            base_sha,
            head_sha,
            "--",
        ],
        MAX_DIFF_OUTPUT,
    ) {
        Ok(output) if output.status.success() && output.stdout.len() <= MAX_DIFF_OUTPUT => output,
        _ => {
            return MetadataDecision::Full {
                reason_code: "git-acquisition-failed",
            }
        }
    };
    let entries = match parse_raw_diff(&output.stdout) {
        Ok(entries) => entries,
        Err(reason_code) => return MetadataDecision::Full { reason_code },
    };
    if entries
        .iter()
        .any(|entry| entry.old_mode != "100644" || entry.new_mode != "100644")
    {
        return MetadataDecision::Full {
            reason_code: "unsafe-file-mode",
        };
    }
    let expected: BTreeSet<&str> = METADATA_PATHS.into_iter().collect();
    let actual: BTreeSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    if actual != expected || entries.len() != METADATA_PATHS.len() {
        return MetadataDecision::Full {
            reason_code: "changed-path-set",
        };
    }
    let mut base = BTreeMap::new();
    let mut head = BTreeMap::new();
    for path in METADATA_PATHS {
        match (
            read_blob(repo, base_sha, path),
            read_blob(repo, head_sha, path),
        ) {
            (Ok(old), Ok(new)) => {
                base.insert(path.to_string(), old);
                head.insert(path.to_string(), new);
            }
            _ => {
                return MetadataDecision::Full {
                    reason_code: "git-acquisition-failed",
                }
            }
        }
    }
    classify_metadata_diff(&base, &head)
}

struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

struct RawDiffEntry {
    old_mode: String,
    new_mode: String,
    status: String,
    path: String,
}

fn parse_raw_diff(bytes: &[u8]) -> Result<Vec<RawDiffEntry>, &'static str> {
    if bytes.len() > MAX_DIFF_OUTPUT {
        return Err("git-acquisition-failed");
    }
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut entries = Vec::new();
    while let Some(record) = fields.next() {
        if record.is_empty() {
            if fields.peek().is_none() {
                break;
            }
            return Err("git-acquisition-failed");
        }
        let record = std::str::from_utf8(record).map_err(|_| "git-acquisition-failed")?;
        let fields_in_record: Vec<&str> = record.split_ascii_whitespace().collect();
        if fields_in_record.len() != 5
            || fields_in_record[0].len() != 7
            || !fields_in_record[0].starts_with(':')
            || !fields_in_record[0][1..]
                .bytes()
                .all(|byte| (b'0'..=b'7').contains(&byte))
            || !is_full_sha(fields_in_record[2])
            || !is_full_sha(fields_in_record[3])
        {
            return Err("git-acquisition-failed");
        }
        let raw_path = fields.next().ok_or("git-acquisition-failed")?;
        if raw_path.is_empty() {
            return Err("git-acquisition-failed");
        }
        let path = std::str::from_utf8(raw_path)
            .map_err(|_| "changed-path-set")?
            .to_string();
        let status = fields_in_record[4];
        if status != "M" {
            return Err(
                if matches!(status, "A" | "D" | "R" | "C" | "T" | "U" | "X" | "B") {
                    "changed-path-set"
                } else {
                    "git-acquisition-failed"
                },
            );
        }
        entries.push(RawDiffEntry {
            old_mode: fields_in_record[0].trim_start_matches(':').to_string(),
            new_mode: fields_in_record[1].to_string(),
            status: status.to_string(),
            path,
        });
    }
    if entries.iter().any(|entry| entry.status != "M") {
        return Err("changed-path-set");
    }
    Ok(entries)
}

fn verify_commit(repo: &Path, sha: &str) -> Result<(), ()> {
    let revision = format!("{sha}^{{commit}}");
    let output = git(
        repo,
        ["rev-parse", "--verify", "--end-of-options", &revision],
        256,
    )
    .map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    let resolved = String::from_utf8(output.stdout).map_err(|_| ())?;
    if resolved.trim().eq_ignore_ascii_case(sha) {
        Ok(())
    } else {
        Err(())
    }
}

fn read_blob(repo: &Path, commit: &str, path: &str) -> Result<Vec<u8>, ()> {
    let object = format!("{commit}:{path}");
    let output =
        git(repo, ["cat-file", "blob", &object], MAX_BLOB_SIZE as usize).map_err(|_| ())?;
    if !output.status.success() || output.stdout.len() as u64 > MAX_BLOB_SIZE {
        return Err(());
    }
    Ok(output.stdout)
}

fn git<const N: usize>(repo: &Path, args: [&str; N], max_stdout: usize) -> Result<GitOutput, ()> {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_INDEX_FILE",
        "GIT_PREFIX",
        "GIT_NAMESPACE",
        "GIT_CONFIG",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(name);
    }
    let mut child = command.spawn().map_err(|_| ())?;
    let stdout = child.stdout.take().ok_or(())?;
    let mut bounded = stdout.take(max_stdout as u64 + 1);
    let mut bytes = Vec::new();
    bounded.read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() > max_stdout {
        let _ = child.kill();
        let _ = child.wait();
        return Err(());
    }
    let status = child.wait().map_err(|_| ())?;
    Ok(GitOutput {
        status,
        stdout: bytes,
    })
}

fn classify_metadata_diff(
    base: &BTreeMap<String, Vec<u8>>,
    head: &BTreeMap<String, Vec<u8>>,
) -> MetadataDecision {
    if base.len() != 6
        || head.len() != 6
        || METADATA_PATHS
            .iter()
            .any(|path| !base.contains_key(*path) || !head.contains_key(*path))
    {
        return MetadataDecision::Full {
            reason_code: "changed-path-set",
        };
    }
    let mut versions = Vec::new();
    for (path, target, expected_name) in [
        ("Cargo.toml", TomlTarget::Package, "codebase-graph"),
        ("crates/k-wiki/Cargo.toml", TomlTarget::Wiki, "k-wiki"),
        ("crates/xtask/Cargo.toml", TomlTarget::Package, "xtask"),
    ] {
        let old = recognize_toml(&base[path], target, expected_name);
        let new = recognize_toml(&head[path], target, expected_name);
        let (Ok(old), Ok(new)) = (old, new) else {
            return MetadataDecision::Full {
                reason_code: "metadata-not-eligible",
            };
        };
        let old_spans: Vec<Span> = old.iter().map(|item| item.1).collect();
        let new_spans: Vec<Span> = new.iter().map(|item| item.1).collect();
        let equal = masked_equal(&base[path], &old_spans, &head[path], &new_spans);
        if old.len() != new.len() || !equal {
            return MetadataDecision::Full {
                reason_code: "metadata-not-eligible",
            };
        }
        versions.extend(old.into_iter().zip(new).map(|(old, new)| (old.0, new.0)));
    }
    let old_manifest = recognize_manifest(&base[".release-please-manifest.json"]);
    let new_manifest = recognize_manifest(&head[".release-please-manifest.json"]);
    let (Ok(old_manifest), Ok(new_manifest)) = (old_manifest, new_manifest) else {
        return MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        };
    };
    if !masked_equal(
        &base[".release-please-manifest.json"],
        &[old_manifest.1],
        &head[".release-please-manifest.json"],
        &[new_manifest.1],
    ) {
        return MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        };
    }
    versions.push((old_manifest.0, new_manifest.0));
    let old_lock = recognize_lock(&base["Cargo.lock"]);
    let new_lock = recognize_lock(&head["Cargo.lock"]);
    let (Ok(old_lock), Ok(new_lock)) = (old_lock, new_lock) else {
        return MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        };
    };
    let old_spans: Vec<Span> = old_lock.iter().map(|item| item.1).collect();
    let new_spans: Vec<Span> = new_lock.iter().map(|item| item.1).collect();
    if !masked_equal(
        &base["Cargo.lock"],
        &old_spans,
        &head["Cargo.lock"],
        &new_spans,
    ) {
        return MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        };
    }
    versions.extend(
        old_lock
            .into_iter()
            .zip(new_lock)
            .map(|(old, new)| (old.0, new.0)),
    );
    let Some((old_version, new_version)) = versions.first() else {
        return MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        };
    };
    if versions
        .iter()
        .any(|(old, new)| old != old_version || new != new_version)
    {
        return MetadataDecision::Full {
            reason_code: "version-mismatch",
        };
    }
    let (Some(old_parts), Some(new_parts)) =
        (parse_version(old_version), parse_version(new_version))
    else {
        return MetadataDecision::Full {
            reason_code: "version-transition-invalid",
        };
    };
    if new_parts <= old_parts {
        return MetadataDecision::Full {
            reason_code: "version-transition-invalid",
        };
    }
    match changelog_transition(
        &base["CHANGELOG.md"],
        &head["CHANGELOG.md"],
        old_version,
        new_version,
    ) {
        Ok(()) => MetadataDecision::Eligible {
            old_version: old_version.clone(),
            new_version: new_version.clone(),
        },
        Err(()) => MetadataDecision::Full {
            reason_code: "metadata-not-eligible",
        },
    }
}

fn masked_equal(old: &[u8], old_spans: &[Span], new: &[u8], new_spans: &[Span]) -> bool {
    if old_spans.len() != new_spans.len() {
        return false;
    }
    fn masked(bytes: &[u8], spans: &[Span]) -> Option<Vec<u8>> {
        let mut spans = spans.to_vec();
        spans.sort_by_key(|span| span.start);
        let mut out = Vec::new();
        let mut cursor = 0;
        for span in spans {
            if span.start < cursor || span.end < span.start || span.end > bytes.len() {
                return None;
            }
            out.extend_from_slice(&bytes[cursor..span.start]);
            out.extend_from_slice(b"<VERSION>");
            cursor = span.end;
        }
        out.extend_from_slice(&bytes[cursor..]);
        Some(out)
    }
    masked(old, old_spans).is_some_and(|old| masked(new, new_spans).is_some_and(|new| old == new))
}

fn recognize_manifest(bytes: &[u8]) -> Result<(String, Span), ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    let prefix = "{\n  \".\": \"";
    let suffix = "\"\n}\n";
    let value = text
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .ok_or(())?;
    if value.is_empty() || value.contains(['\\', '"']) {
        return Err(());
    }
    let parsed: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| ())?;
    let object = parsed.as_object().ok_or(())?;
    if object.len() != 1 || object.get(".").and_then(serde_json::Value::as_str) != Some(value) {
        return Err(());
    }
    Ok((
        value.to_string(),
        Span {
            start: prefix.len(),
            end: prefix.len() + value.len(),
        },
    ))
}

#[derive(Clone, Copy)]
enum TomlTarget {
    Package,
    Wiki,
}

fn recognize_toml(
    bytes: &[u8],
    target: TomlTarget,
    expected_package_name: &str,
) -> Result<Vec<(String, Span)>, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    if text
        .as_bytes()
        .windows(3)
        .any(|window| window == [39, 39, 39])
        || text.contains("\"\"\"")
        || text.contains('\r')
    {
        return Err(());
    }
    let mut table = String::new();
    let mut package_tables = 0;
    let mut dependency_tables = 0;
    let mut package_name = None;
    let mut package_name_count = 0;
    let mut package_version = None;
    let mut dependency_version = None;
    let mut version_count = 0;
    let mut dep_count = 0;
    let mut offset = 0;
    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset += raw_line.len();
            continue;
        }
        if trimmed.starts_with('[') {
            let parsed = parse_table(trimmed).ok_or(())?;
            if parsed == "package" {
                package_tables += 1;
            }
            if parsed == "dependencies" {
                dependency_tables += 1;
            }
            if parsed == "dependencies.codebase-graph" {
                return Err(());
            }
            table = parsed;
            offset += raw_line.len();
            continue;
        }
        let Some((key, raw_value, value_offset)) = assignment(line, offset) else {
            offset += raw_line.len();
            continue;
        };
        if key == "name" && table == "package" {
            package_name_count += 1;
            package_name = Some(parse_quoted(raw_value, value_offset)?.0);
        }
        if key == "version" && table == "package" {
            version_count += 1;
            package_version = Some(parse_quoted(raw_value, value_offset)?);
        }
        if key == "codebase-graph" {
            if !matches!(target, TomlTarget::Wiki) || table != "dependencies" {
                return Err(());
            }
            dep_count += 1;
            dependency_version = Some(parse_dependency(raw_value, value_offset)?);
        }
        offset += raw_line.len();
    }
    if package_tables != 1
        || package_name_count != 1
        || package_name.as_deref() != Some(expected_package_name)
        || version_count != 1
    {
        return Err(());
    }
    let package_version = package_version.ok_or(())?;
    match target {
        TomlTarget::Package => Ok(vec![package_version]),
        TomlTarget::Wiki if dependency_tables == 1 && dep_count == 1 => {
            Ok(vec![package_version, dependency_version.ok_or(())?])
        }
        TomlTarget::Wiki => Err(()),
    }
}

fn parse_table(value: &str) -> Option<String> {
    if let Some(name) = value
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
    {
        if name.is_empty() || !name.bytes().all(is_bare_key) {
            return None;
        }
        return Some(name.to_string());
    }
    let name = value.strip_prefix('[')?.strip_suffix(']')?;
    if name.is_empty()
        || !name.bytes().all(|byte| {
            is_bare_key(byte) || matches!(byte, b'.' | b'\'' | b'(' | b')' | b'=' | b'\"')
        })
    {
        return None;
    }
    Some(name.to_string())
}

fn is_bare_key(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn assignment(line: &str, offset: usize) -> Option<(&str, &str, usize)> {
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || key.starts_with('#') {
        return None;
    }
    let equals = line.find('=')?;
    let value_leading = line[equals + 1..].len() - line[equals + 1..].trim_start().len();
    Some((
        key,
        line[equals + 1..].trim(),
        offset + equals + 1 + value_leading,
    ))
}

fn parse_quoted(value: &str, offset: usize) -> Result<(String, Span), ()> {
    if value.contains('#') {
        return Err(());
    }
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(())?;
    if inner.is_empty() || inner.contains(['\\', '"']) {
        return Err(());
    }
    Ok((
        inner.to_string(),
        Span {
            start: offset + 1,
            end: offset + 1 + inner.len(),
        },
    ))
}

fn parse_dependency(value: &str, offset: usize) -> Result<(String, Span), ()> {
    let prefix = "{ version = \"";
    let suffix = "\", path = \"../..\" }";
    let version = value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .ok_or(())?;
    if version.is_empty() {
        return Err(());
    }
    Ok((
        version.to_string(),
        Span {
            start: offset + prefix.len(),
            end: offset + prefix.len() + version.len(),
        },
    ))
}

fn recognize_lock(bytes: &[u8]) -> Result<Vec<(String, Span)>, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    if text
        .as_bytes()
        .windows(3)
        .any(|window| window == [39, 39, 39])
        || text.contains("\"\"\"")
        || text.contains('\r')
    {
        return Err(());
    }
    let mut packages = Vec::new();
    let mut current: Option<LockPackage> = None;
    let mut offset = 0;
    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            if let Some(package) = current.take() {
                packages.push(package);
            }
            current = Some(LockPackage::default());
            offset += raw_line.len();
            continue;
        }
        if trimmed.starts_with('[') {
            if parse_table(trimmed).is_none() {
                return Err(());
            }
            if let Some(package) = current.take() {
                packages.push(package);
            }
            offset += raw_line.len();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset += raw_line.len();
            continue;
        }
        if let Some(package) = current.as_mut() {
            if let Some((key, value, value_offset)) = assignment(line, offset) {
                match key {
                    "name" => {
                        if package.name.is_some() {
                            return Err(());
                        }
                        package.name = Some(parse_plain(value)?);
                    }
                    "version" => {
                        if package.version.is_some() {
                            return Err(());
                        }
                        package.version = Some(parse_quoted(value, value_offset)?);
                    }
                    "source" => {
                        if package.source_seen {
                            return Err(());
                        }
                        package.source_seen = true;
                    }
                    _ => {}
                }
            }
        }
        offset += raw_line.len();
    }
    if let Some(package) = current {
        packages.push(package);
    }
    let mut spans = Vec::new();
    for name in ["codebase-graph", "k-wiki", "xtask"] {
        let matched: Vec<&LockPackage> = packages
            .iter()
            .filter(|package| package.name.as_deref() == Some(name) && !package.source_seen)
            .collect();
        if matched.len() != 1 {
            return Err(());
        }
        spans.push(matched[0].version.clone().ok_or(())?);
    }
    Ok(spans)
}

fn parse_plain(value: &str) -> Result<String, ()> {
    if value.contains('#') {
        return Err(());
    }
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(())?;
    if inner.is_empty() || inner.contains(['\\', '"']) {
        return Err(());
    }
    Ok(inner.to_string())
}

fn changelog_transition(
    base: &[u8],
    head: &[u8],
    old_version: &str,
    new_version: &str,
) -> Result<(), ()> {
    let base = std::str::from_utf8(base).map_err(|_| ())?;
    let head = std::str::from_utf8(head).map_err(|_| ())?;
    let base_headers = headers(base);
    let head_headers = headers(head);
    if base_headers.is_empty() || head_headers.len() < 2 {
        return Err(());
    }
    let (base_start, _) = base_headers[0];
    let (head_start, _) = head_headers[0];
    let new_heading = &head[head_start..head_headers[0].1];
    let old_heading = &base[base_start..base_headers[0].1];
    if base[..base_start] != head[..head_start]
        || heading_version(new_heading)? != new_version
        || heading_version(old_heading)? != old_version
        || heading_comparison(new_heading)? != format!("v{old_version}...v{new_version}")
    {
        return Err(());
    }
    let previous_start = head_headers[1].0;
    if base[base_start..] != head[previous_start..] {
        return Err(());
    }
    if head[head_headers[0].1..previous_start].trim().is_empty() {
        return Err(());
    }
    if head_headers[1..]
        .iter()
        .any(|(start, end)| header_title_version(&head[*start..*end]) == Some(new_version))
    {
        return Err(());
    }
    Ok(())
}

fn headers(text: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.strip_suffix('\n').unwrap_or(line).starts_with("## [") {
            result.push((offset, offset + line.len()));
        }
        offset += line.len();
    }
    result
}

fn header_title_version(heading: &str) -> Option<&str> {
    let line = heading.lines().next()?;
    line.strip_prefix("## [")?
        .split_once(']')
        .map(|(version, _)| version)
}

fn heading_version(heading: &str) -> Result<&str, ()> {
    let line = heading.lines().next().ok_or(())?;
    let (version, suffix) = line
        .strip_prefix("## [")
        .and_then(|value| value.split_once("]("))
        .ok_or(())?;
    let (url, date) = suffix.split_once(") (").ok_or(())?;
    if !url.starts_with("https://github.com/rabii-chaarani/codebaseGraph/compare/")
        || !valid_date(date.strip_suffix(')').ok_or(())?)
    {
        return Err(());
    }
    parse_version(version).ok_or(())?;
    Ok(version)
}

fn heading_comparison(heading: &str) -> Result<&str, ()> {
    let line = heading.lines().next().ok_or(())?;
    let (_, suffix) = line
        .strip_prefix("## [")
        .and_then(|value| value.split_once("]("))
        .ok_or(())?;
    let (url, _) = suffix.split_once(") (").ok_or(())?;
    url.strip_prefix("https://github.com/rabii-chaarani/codebaseGraph/compare/")
        .ok_or(())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u32>().ok();
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let parse = |part: &str| {
        if part.is_empty()
            || part.len() > 1 && part.starts_with('0')
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        part.parse::<u64>().ok()
    };
    let result = (
        parse(parts.next()?)?,
        parse(parts.next()?)?,
        parse(parts.next()?)?,
    );
    if parts.next().is_some() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{acquire_and_classify, classify_metadata_diff, MetadataDecision};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "cod108-ci-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            git(&root, ["init", "--quiet", "-b", "main"]);
            Self { root }
        }
        fn commit(&self, files: &BTreeMap<String, Vec<u8>>, message: &str) -> String {
            for (path, contents) in files {
                let path = self.root.join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, contents).unwrap();
            }
            git(&self.root, ["add", "--all"]);
            git(
                &self.root,
                [
                    "-c",
                    "user.name=COD-108 tests",
                    "-c",
                    "user.email=cod108@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    message,
                ],
            );
            String::from_utf8(git_output(&self.root, ["rev-parse", "HEAD"]).stdout)
                .unwrap()
                .trim()
                .to_string()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn git<const N: usize>(root: &Path, args: [&str; N]) {
        assert!(git_output(root, args).status.success());
    }
    fn git_output<const N: usize>(root: &Path, args: [&str; N]) -> Output {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap()
    }

    fn release_log(version: &str, previous: &str, suffix: &str) -> String {
        format!("# Changelog\n\n## [{version}](https://github.com/rabii-chaarani/codebaseGraph/compare/v{previous}...v{version}) (2026-11-01)\n\n### Bug Fixes\n\n* generated release metadata\n\n{suffix}")
    }
    fn files(version: &str, changelog: String) -> BTreeMap<String, Vec<u8>> {
        let root = format!("{{\n  \".\": \"{version}\"\n}}\n");
        let cargo = format!("[package]\nname = \"codebase-graph\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[workspace]\nmembers = [\".\", \"crates/k-wiki\", \"crates/xtask\"]\n");
        let wiki = format!("[package]\nname = \"k-wiki\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[dependencies]\ncodebase-graph = {{ version = \"{version}\", path = \"../..\" }}\nserde = \"1\"\n");
        let xtask =
            format!("[package]\nname = \"xtask\"\nversion = \"{version}\"\npublish = false\n");
        let lock = format!("# lock fixture\nversion = 4\n\n[[package]]\nname = \"codebase-graph\"\nversion = \"{version}\"\ndependencies = [\"k-wiki\", \"serde\"]\n\n[[package]]\nname = \"k-wiki\"\nversion = \"{version}\"\ndependencies = [\"codebase-graph\"]\n\n[[package]]\nname = \"xtask\"\nversion = \"{version}\"\ndependencies = [\"serde\"]\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://example.invalid\"\n");
        [
            (".release-please-manifest.json", root.into_bytes()),
            ("Cargo.toml", cargo.into_bytes()),
            ("Cargo.lock", lock.into_bytes()),
            ("CHANGELOG.md", changelog.into_bytes()),
            ("crates/k-wiki/Cargo.toml", wiki.into_bytes()),
            ("crates/xtask/Cargo.toml", xtask.into_bytes()),
        ]
        .into_iter()
        .map(|(path, bytes)| (path.to_string(), bytes))
        .collect()
    }
    fn base_files() -> BTreeMap<String, Vec<u8>> {
        files(
            "4.0.5",
            release_log(
                "4.0.5",
                "4.0.4",
                "## [4.0.4](https://github.com/rabii-chaarani/codebaseGraph/compare/v4.0.3...v4.0.4) (2026-10-30)\n\nold release notes\n",
            ),
        )
    }
    fn bumped(base: &BTreeMap<String, Vec<u8>>, old: &str, new: &str) -> BTreeMap<String, Vec<u8>> {
        let mut head = base.clone();
        for path in [
            ".release-please-manifest.json",
            "Cargo.toml",
            "Cargo.lock",
            "crates/k-wiki/Cargo.toml",
            "crates/xtask/Cargo.toml",
        ] {
            head.insert(
                path.to_string(),
                String::from_utf8(base[path].clone())
                    .unwrap()
                    .replace(old, new)
                    .into_bytes(),
            );
        }
        let suffix = String::from_utf8(base["CHANGELOG.md"].clone())
            .unwrap()
            .strip_prefix("# Changelog\n\n")
            .unwrap()
            .to_string();
        head.insert(
            "CHANGELOG.md".into(),
            release_log(new, old, &suffix).into_bytes(),
        );
        head
    }

    #[test]
    fn pr165_shaped_complete_metadata_transition_is_lightweight() {
        let base = base_files();
        let head = bumped(&base, "4.0.5", "4.0.6");
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Eligible {
                old_version: "4.0.5".into(),
                new_version: "4.0.6".into()
            }
        );
    }
    #[test]
    fn pr167_shaped_complete_metadata_transition_is_lightweight() {
        let base = base_files();
        let head = bumped(&base, "4.0.5", "5.0.0");
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Eligible {
                old_version: "4.0.5".into(),
                new_version: "5.0.0".into()
            }
        );
    }
    #[test]
    fn metadata_classifier_rejects_partial_mismatch_external_lock_and_history_edits() {
        let base = base_files();
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.remove("crates/xtask/Cargo.toml");
        assert!(matches!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full { .. }
        ));
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.insert(
            "crates/k-wiki/Cargo.toml".into(),
            String::from_utf8(head["crates/k-wiki/Cargo.toml"].clone())
                .unwrap()
                .replace("version = \"4.0.6\", path", "version = \"4.0.7\", path")
                .into_bytes(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full {
                reason_code: "version-mismatch"
            }
        );
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.insert(
            "Cargo.lock".into(),
            String::from_utf8(head["Cargo.lock"].clone())
                .unwrap()
                .replace("1.0.0", "1.0.1")
                .into_bytes(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full {
                reason_code: "metadata-not-eligible"
            }
        );
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.insert(
            "CHANGELOG.md".into(),
            String::from_utf8(head["CHANGELOG.md"].clone())
                .unwrap()
                .replace("old release notes", "edited release notes")
                .into_bytes(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full {
                reason_code: "metadata-not-eligible"
            }
        );
    }
    #[test]
    fn metadata_classifier_rejects_duplicate_and_decoy_contexts() {
        let base = base_files();
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.insert(
            "Cargo.toml".into(),
            String::from_utf8(head["Cargo.toml"].clone())
                .unwrap()
                .replace(
                    "version = \"4.0.6\"",
                    "# version = \"4.0.6\"\nversion = \"4.0.6\"",
                )
                .into_bytes(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full {
                reason_code: "metadata-not-eligible"
            }
        );
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.insert(
            ".release-please-manifest.json".into(),
            b"{\n  \".\": \"4.0.6\",\n  \".\": \"4.0.6\"\n}\n".to_vec(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full {
                reason_code: "metadata-not-eligible"
            }
        );
    }
    #[test]
    fn metadata_recognizers_reject_multiline_comment_context_and_duplicate_workspace_records() {
        let base = base_files();
        for replacement in [
            "version = \"4.0.6\" # trailing comment",
            concat!("version = ", "\"\"\"", "\n4.0.6\n", "\"\"\""),
            "version = \"4.0.6\"\n[package]\nversion = \"4.0.6\"",
        ] {
            let mut head = bumped(&base, "4.0.5", "4.0.6");
            head.insert(
                "Cargo.toml".into(),
                String::from_utf8(head["Cargo.toml"].clone())
                    .unwrap()
                    .replacen("version = \"4.0.6\"", replacement, 1)
                    .into_bytes(),
            );
            assert!(matches!(
                classify_metadata_diff(&base, &head),
                MetadataDecision::Full { .. }
            ));
        }

        let mut head = bumped(&base, "4.0.5", "4.0.6");
        let lock = String::from_utf8(head["Cargo.lock"].clone()).unwrap();
        head.insert(
            "Cargo.lock".into(),
            lock.replace(
                "[[package]]\nname = \"xtask\"",
                "[[package]]\nname = \"xtask\"\nversion = \"4.0.6\"\n\n[[package]]\nname = \"xtask\"",
            )
            .into_bytes(),
        );
        assert!(matches!(
            classify_metadata_diff(&base, &head),
            MetadataDecision::Full { .. }
        ));
    }

    #[test]
    fn classifier_requires_strictly_increasing_versions_and_new_changelog_section() {
        let base = base_files();
        let mut equal = bumped(&base, "4.0.5", "4.0.5");
        assert_eq!(
            classify_metadata_diff(&base, &equal),
            MetadataDecision::Full {
                reason_code: "version-transition-invalid"
            }
        );
        equal = bumped(&base, "4.0.5", "4.0.6");
        let changelog = String::from_utf8(equal["CHANGELOG.md"].clone()).unwrap();
        equal.insert(
            "CHANGELOG.md".into(),
            changelog.replace("## [4.0.4]", "## [4.0.6]").into_bytes(),
        );
        assert_eq!(
            classify_metadata_diff(&base, &equal),
            MetadataDecision::Full {
                reason_code: "metadata-not-eligible"
            }
        );
    }

    #[test]
    fn immutable_git_diff_requires_ancestor_and_exact_metadata_paths() {
        let fixture = Fixture::new();
        let base = base_files();
        let base_sha = fixture.commit(&base, "base");
        let head = bumped(&base, "4.0.5", "4.0.6");
        let head_sha = fixture.commit(&head, "metadata release");
        assert_eq!(
            acquire_and_classify(&fixture.root, &base_sha, &head_sha),
            MetadataDecision::Eligible {
                old_version: "4.0.5".into(),
                new_version: "4.0.6".into()
            }
        );
        assert_eq!(
            acquire_and_classify(&fixture.root, &head_sha, &base_sha),
            MetadataDecision::Full {
                reason_code: "base-not-ancestor"
            }
        );
        let mut unrelated = head.clone();
        unrelated.insert("src/lib.rs".into(), b"code change\n".to_vec());
        let unrelated_sha = fixture.commit(&unrelated, "unrelated code");
        assert_eq!(
            acquire_and_classify(&fixture.root, &head_sha, &unrelated_sha),
            MetadataDecision::Full {
                reason_code: "changed-path-set"
            }
        );
    }
    #[test]
    fn git_path_names_are_nul_delimited_and_extra_paths_force_full() {
        let fixture = Fixture::new();
        let base = base_files();
        let base_sha = fixture.commit(&base, "base");
        let head = bumped(&base, "4.0.5", "4.0.6");
        let head_sha = fixture.commit(&head, "release");
        let odd = fixture.root.join("unrelated name\nwith newline.txt");
        fs::write(&odd, "not metadata\n").unwrap();
        git(&fixture.root, ["add", "--all"]);
        git(
            &fixture.root,
            [
                "-c",
                "user.name=COD-108 tests",
                "-c",
                "user.email=cod108@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "newline path",
            ],
        );
        let with_extra = String::from_utf8(git_output(&fixture.root, ["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            acquire_and_classify(&fixture.root, &head_sha, &with_extra),
            MetadataDecision::Full {
                reason_code: "changed-path-set"
            }
        );
        assert_ne!(base_sha, head_sha);
    }

    #[test]
    fn git_acquisition_failures_select_full_and_same_sha_is_full() {
        let fixture = Fixture::new();
        let files = base_files();
        let sha = fixture.commit(&files, "base");
        assert_eq!(
            acquire_and_classify(&fixture.root, &sha, &sha),
            MetadataDecision::Full {
                reason_code: "changed-path-set"
            }
        );
        assert_eq!(
            acquire_and_classify(&fixture.root, &"0".repeat(40), &sha),
            MetadataDecision::Full {
                reason_code: "git-acquisition-failed"
            }
        );
        assert_eq!(
            acquire_and_classify(&fixture.root.join("missing"), &sha, &sha),
            MetadataDecision::Full {
                reason_code: "git-acquisition-failed"
            }
        );
    }

    #[test]
    fn raw_diff_parser_accepts_newline_names_and_rejects_unknown_or_truncated_records() {
        use super::parse_raw_diff;

        let valid = b":100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 M\0a\nfile.txt\0";
        let parsed = parse_raw_diff(valid).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, "a\nfile.txt");
        assert!(matches!(parse_raw_diff(b":100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 Q\0name\0"), Err("git-acquisition-failed")));
        assert!(matches!(parse_raw_diff(b":100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 M\0"), Err("git-acquisition-failed")));
        assert!(matches!(parse_raw_diff(b":100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 M\0path\0trailing"), Err("git-acquisition-failed")));
    }

    #[cfg(unix)]
    #[test]
    fn immutable_git_diff_rejects_executable_metadata_modes() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = Fixture::new();
        let base = base_files();
        let base_sha = fixture.commit(&base, "base");
        let head = bumped(&base, "4.0.5", "4.0.6");
        let _head_sha = fixture.commit(&head, "metadata version update");
        let changelog = fixture.root.join("CHANGELOG.md");
        let mut permissions = fs::metadata(&changelog).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&changelog, permissions).unwrap();
        git(&fixture.root, ["add", "CHANGELOG.md"]);
        git(
            &fixture.root,
            ["update-index", "--chmod=+x", "CHANGELOG.md"],
        );
        git(
            &fixture.root,
            [
                "-c",
                "user.name=COD-108 tests",
                "-c",
                "user.email=cod108@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "executable metadata",
            ],
        );
        let executable_sha =
            String::from_utf8(git_output(&fixture.root, ["rev-parse", "HEAD"]).stdout)
                .unwrap()
                .trim()
                .to_string();
        assert_eq!(
            acquire_and_classify(&fixture.root, &base_sha, &executable_sha),
            MetadataDecision::Full {
                reason_code: "unsafe-file-mode"
            }
        );
        assert_ne!(base_sha, _head_sha);
    }

    #[test]
    fn immutable_git_diff_rejects_submodule_mode() {
        let fixture = Fixture::new();
        let base = base_files();
        let base_sha = fixture.commit(&base, "base");
        let head = bumped(&base, "4.0.5", "4.0.6");
        let _head_sha = fixture.commit(&head, "metadata version update");
        git(
            &fixture.root,
            [
                "update-index",
                "--add",
                "--cacheinfo",
                "160000",
                &base_sha,
                "CHANGELOG.md",
            ],
        );
        git(
            &fixture.root,
            [
                "-c",
                "user.name=COD-108 tests",
                "-c",
                "user.email=cod108@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "submodule metadata",
            ],
        );
        let submodule_sha =
            String::from_utf8(git_output(&fixture.root, ["rev-parse", "HEAD"]).stdout)
                .unwrap()
                .trim()
                .to_string();
        assert!(matches!(
            acquire_and_classify(&fixture.root, &base_sha, &submodule_sha),
            MetadataDecision::Full { .. }
        ));
    }

    #[test]
    fn immutable_git_diff_rejects_added_deleted_and_symlink_paths() {
        let fixture = Fixture::new();
        let base = base_files();
        let base_sha = fixture.commit(&base, "base");
        let mut head = bumped(&base, "4.0.5", "4.0.6");
        head.remove("crates/xtask/Cargo.toml");
        let head_sha = fixture.commit(&head, "delete metadata file");
        assert_eq!(
            acquire_and_classify(&fixture.root, &base_sha, &head_sha),
            MetadataDecision::Full {
                reason_code: "changed-path-set"
            }
        );
        #[cfg(unix)]
        {
            let mut symlinked = bumped(&base, "4.0.5", "4.0.6");
            symlinked.remove("CHANGELOG.md");
            let path = fixture.root.join("CHANGELOG.md");
            let _ = fs::remove_file(&path);
            std::os::unix::fs::symlink("Cargo.toml", &path).unwrap();
            git(&fixture.root, ["add", "--all"]);
            git(
                &fixture.root,
                [
                    "-c",
                    "user.name=COD-108 tests",
                    "-c",
                    "user.email=cod108@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "symlink changelog",
                ],
            );
            let sha = String::from_utf8(git_output(&fixture.root, ["rev-parse", "HEAD"]).stdout)
                .unwrap()
                .trim()
                .to_string();
            assert_eq!(
                acquire_and_classify(&fixture.root, &base_sha, &sha),
                MetadataDecision::Full {
                    reason_code: "changed-path-set"
                }
            );
            let _ = symlinked;
        }
    }
}
