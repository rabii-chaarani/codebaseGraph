use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};
use yaml_serde::Value as YamlValue;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub trait AuthoringValidator {
    fn validate(&self, request: ValidationTarget<'_>) -> Result<(), AuthoringError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationTargetKind {
    BundleIndex,
    ConceptPage,
}

#[derive(Clone, Copy, Debug)]
pub struct ValidationTarget<'a> {
    pub kind: ValidationTargetKind,
    pub bundle_id: &'a str,
    pub source_path: &'a str,
    pub content: &'a str,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopValidator;

impl AuthoringValidator for NoopValidator {
    fn validate(&self, _request: ValidationTarget<'_>) -> Result<(), AuthoringError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ConformanceAuthoringValidator;

impl AuthoringValidator for ConformanceAuthoringValidator {
    fn validate(&self, request: ValidationTarget<'_>) -> Result<(), AuthoringError> {
        let parsed = parse_document(request.content)?;
        match request.kind {
            ValidationTargetKind::BundleIndex => {
                required_frontmatter_string(&parsed, "okf_version", request.source_path)?;
            }
            ValidationTargetKind::ConceptPage => {
                required_frontmatter_string(&parsed, "type", request.source_path)?;
                if parsed.frontmatter.contains_key("okf_version") {
                    return Err(AuthoringError::invalid_frontmatter(format!(
                        "{} must not declare okf_version",
                        request.source_path
                    )));
                }
            }
        }
        Ok(())
    }
}

pub trait RefreshNotifier {
    fn notify(&self, event: &RefreshEvent);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshOperation {
    BundleCreated,
    PageCreated,
    PagePopulated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshEvent {
    pub operation: RefreshOperation,
    pub bundle_id: String,
    pub source_path: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopRefreshNotifier;

impl RefreshNotifier for NoopRefreshNotifier {
    fn notify(&self, _event: &RefreshEvent) {}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoringError {
    code: &'static str,
    message: String,
}

impl AuthoringError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn bundle_not_found(bundle_id: &str) -> Self {
        Self::new(
            "bundle_not_found",
            format!("bundle `{bundle_id}` is not configured"),
        )
    }

    pub fn bundle_exists(path: &str) -> Self {
        Self::new("bundle_exists", format!("bundle `{path}` already exists"))
    }

    pub fn concept_not_found(path: &str) -> Self {
        Self::new(
            "concept_not_found",
            format!("concept `{path}` does not exist"),
        )
    }

    pub fn concept_exists(path: &str) -> Self {
        Self::new("concept_exists", format!("concept `{path}` already exists"))
    }

    pub fn path_outside_repository(path: &str) -> Self {
        Self::new(
            "path_outside_repository",
            format!("path `{path}` is outside the configured authoring roots"),
        )
    }

    pub fn invalid_frontmatter(message: impl Into<String>) -> Self {
        Self::new("invalid_frontmatter", message)
    }

    pub fn write_conflict(path: &str) -> Self {
        Self::new(
            "write_conflict",
            format!("concept `{path}` changed after it was read"),
        )
    }

    pub fn io(action: &str, path: &str, error: io::Error) -> Self {
        Self::new(
            "invalid_request",
            format!("{action} failed for `{path}`: {error}"),
        )
    }
}

impl fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AuthoringError {}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedDocument {
    pub frontmatter: BTreeMap<String, YamlValue>,
    pub body_markdown: String,
}

pub fn parse_document(content: &str) -> Result<ParsedDocument, AuthoringError> {
    let opening_delimiter_len = if content.starts_with("---\r\n") {
        5
    } else if content.starts_with("---\n") {
        4
    } else {
        0
    };

    if opening_delimiter_len == 0 {
        return Ok(ParsedDocument {
            frontmatter: BTreeMap::new(),
            body_markdown: content.to_string(),
        });
    }

    let bytes = content.as_bytes();
    let mut cursor = opening_delimiter_len;
    let mut closing = None;
    while cursor < bytes.len() {
        let line_end = content[cursor..]
            .find('\n')
            .map(|offset| cursor + offset)
            .unwrap_or(bytes.len());
        let line = content[cursor..line_end].trim_end_matches('\r');
        if line == "---" {
            closing = Some((cursor, line_end));
            break;
        }
        cursor = line_end.saturating_add(1);
    }

    let Some((frontmatter_end, closing_line_end)) = closing else {
        return Err(AuthoringError::invalid_frontmatter(
            "frontmatter is missing a closing delimiter",
        ));
    };

    let yaml = &content[opening_delimiter_len..frontmatter_end];
    let frontmatter = if yaml.trim().is_empty() {
        BTreeMap::new()
    } else {
        yaml_serde::from_str::<BTreeMap<String, YamlValue>>(yaml).map_err(|error| {
            AuthoringError::invalid_frontmatter(format!("frontmatter is invalid: {error}"))
        })?
    };

    let body_start = if closing_line_end < bytes.len() {
        closing_line_end + 1
    } else {
        closing_line_end
    };

    Ok(ParsedDocument {
        frontmatter,
        body_markdown: content[body_start..].to_string(),
    })
}

fn required_frontmatter_string(
    document: &ParsedDocument,
    field: &str,
    source_path: &str,
) -> Result<(), AuthoringError> {
    if document
        .frontmatter
        .get(field)
        .and_then(YamlValue::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(());
    }
    Err(AuthoringError::invalid_frontmatter(format!(
        "{source_path} requires a non-empty `{field}` field"
    )))
}

pub fn render_document(
    frontmatter: &BTreeMap<String, YamlValue>,
    body_markdown: &str,
) -> Result<String, AuthoringError> {
    if frontmatter.is_empty() {
        return Ok(body_markdown.to_string());
    }

    let yaml = yaml_serde::to_string(frontmatter)
        .map_err(|error| AuthoringError::invalid_frontmatter(error.to_string()))?;

    Ok(format!("---\n{yaml}---\n{body_markdown}"))
}

pub fn write_atomically(path: &Path, content: &str) -> Result<String, AuthoringError> {
    let parent = path.parent().ok_or_else(|| {
        AuthoringError::invalid_request("destination path must have a parent directory")
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        AuthoringError::io("create parent directory", &display_path(path), error)
    })?;

    let temp_path = unique_temp_path(path);
    let mut file = File::create(&temp_path)
        .map_err(|error| AuthoringError::io("create temporary file", &display_path(path), error))?;
    file.write_all(content.as_bytes())
        .map_err(|error| AuthoringError::io("write temporary file", &display_path(path), error))?;
    file.flush()
        .map_err(|error| AuthoringError::io("flush temporary file", &display_path(path), error))?;
    file.sync_all()
        .map_err(|error| AuthoringError::io("sync temporary file", &display_path(path), error))?;
    drop(file);

    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(AuthoringError::io(
            "replace destination",
            &display_path(path),
            error,
        ));
    }

    sync_directory(parent)
        .map_err(|error| AuthoringError::io("sync parent directory", &display_path(path), error))?;

    Ok(content_hash(content.as_bytes()))
}

pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.tmp-{nonce}-{counter}"))
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::{iter, os::windows::ffi::OsStrExt};

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are encoded as owned, NUL-terminated UTF-16 buffers
    // that remain alive for the duration of the synchronous Win32 call.
    let replaced = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> io::Result<()> {
    let directory = File::open(path)?;
    directory.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> io::Result<()> {
    // MOVEFILE_WRITE_THROUGH above provides the publication durability guarantee.
    Ok(())
}
