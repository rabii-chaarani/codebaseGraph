//! OKF bundle discovery and document parsing.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::Value as JsonValue;
use walkdir::WalkDir;
use yaml_serde::Value as YamlValue;

use crate::diagnostic::Diagnostic;

const ROOT_INDEX_PATH: &str = "index.md";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedBundle {
    pub id: String,
    pub root_path: PathBuf,
    pub title: Option<String>,
    pub okf_version: Option<String>,
    pub entries: Vec<BundleEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundleEntry {
    pub source_path: String,
    pub kind: BundleEntryKind,
    pub document: BundleDocument,
}

impl BundleEntry {
    pub fn frontmatter(&self) -> Option<&DocumentFrontmatter> {
        match &self.document.frontmatter_state {
            FrontmatterState::Parsed(frontmatter) => Some(frontmatter),
            FrontmatterState::Absent | FrontmatterState::Invalid => None,
        }
    }

    pub fn is_root_index(&self) -> bool {
        self.kind == BundleEntryKind::Index && self.source_path == ROOT_INDEX_PATH
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleEntryKind {
    Concept,
    Index,
    Log,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundleDocument {
    pub has_frontmatter: bool,
    pub frontmatter_state: FrontmatterState,
    pub body_markdown: String,
    pub body_start_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrontmatterState {
    Absent,
    Invalid,
    Parsed(Box<DocumentFrontmatter>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentFrontmatter {
    pub fields: StandardFrontmatter,
    pub extensions: BTreeMap<String, JsonValue>,
    pub key_lines: BTreeMap<String, usize>,
}

impl DocumentFrontmatter {
    pub fn line_for(&self, key: &str) -> Option<usize> {
        self.key_lines.get(key).copied()
    }

    pub fn first_key_line(&self) -> Option<usize> {
        self.key_lines.values().copied().min()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StandardFrontmatter {
    pub type_name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resource: Option<String>,
    pub tags: Vec<String>,
    pub timestamp: Option<String>,
    pub okf_version: Option<String>,
}

pub fn discover_bundles<I, P>(roots: I) -> io::Result<Vec<LoadedBundle>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut bundles = roots
        .into_iter()
        .map(load_bundle)
        .collect::<io::Result<Vec<_>>>()?;
    bundles.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(bundles)
}

pub fn load_bundle(root: impl AsRef<Path>) -> io::Result<LoadedBundle> {
    let root = root.as_ref();
    let canonical_root = fs::canonicalize(root)?;
    if !canonical_root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "bundle root is not a directory: {}",
                canonical_root.display()
            ),
        ));
    }

    let mut bundle = LoadedBundle {
        id: canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bundle")
            .to_owned(),
        root_path: canonical_root.clone(),
        title: None,
        okf_version: None,
        entries: Vec::new(),
        diagnostics: Vec::new(),
    };

    for entry in WalkDir::new(&canonical_root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let path = error
                    .path()
                    .and_then(|path| path.strip_prefix(&canonical_root).ok())
                    .map(path_to_source_path)
                    .unwrap_or_else(|| ROOT_INDEX_PATH.to_owned());
                bundle.diagnostics.push(Diagnostic::error(
                    "io_read_failed",
                    path,
                    None,
                    error.to_string(),
                ));
                continue;
            }
        };

        if entry.path() == canonical_root {
            continue;
        }

        if entry.file_type().is_dir() {
            continue;
        }

        let source_path = match entry.path().strip_prefix(&canonical_root) {
            Ok(relative) => path_to_source_path(relative),
            Err(_) => continue,
        };

        if entry.file_type().is_symlink() {
            match fs::canonicalize(entry.path()) {
                Ok(target) if !target.starts_with(&canonical_root) => {
                    bundle.diagnostics.push(Diagnostic::error(
                        "path_outside_bundle_root",
                        source_path,
                        None,
                        "symlink target escapes the bundle root",
                    ));
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    bundle.diagnostics.push(Diagnostic::error(
                        "io_read_failed",
                        source_path,
                        None,
                        error.to_string(),
                    ));
                    continue;
                }
            }
        }

        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let canonical_path = match fs::canonicalize(entry.path()) {
            Ok(path) => path,
            Err(error) => {
                bundle.diagnostics.push(Diagnostic::error(
                    "io_read_failed",
                    source_path,
                    None,
                    error.to_string(),
                ));
                continue;
            }
        };
        if !canonical_path.starts_with(&canonical_root) {
            bundle.diagnostics.push(Diagnostic::error(
                "path_outside_bundle_root",
                source_path,
                None,
                "markdown file escapes the bundle root",
            ));
            continue;
        }

        let bytes = match fs::read(entry.path()) {
            Ok(bytes) => bytes,
            Err(error) => {
                bundle.diagnostics.push(Diagnostic::error(
                    "io_read_failed",
                    source_path,
                    None,
                    error.to_string(),
                ));
                continue;
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                bundle.diagnostics.push(Diagnostic::error(
                    "invalid_utf8",
                    source_path,
                    None,
                    "markdown files must be valid UTF-8",
                ));
                continue;
            }
        };

        let entry = parse_markdown_entry(&source_path, &content, &mut bundle.diagnostics);
        if entry.is_root_index() {
            if let Some(frontmatter) = entry.frontmatter() {
                bundle.okf_version = frontmatter.fields.okf_version.clone();
                bundle.title = frontmatter.fields.title.clone();
            }
        }
        bundle.entries.push(entry);
    }

    bundle
        .entries
        .sort_by(|left, right| left.source_path.cmp(&right.source_path));
    bundle.diagnostics.sort_by(diagnostic_order);
    if bundle.title.is_none() {
        bundle.title = Some(bundle.id.clone());
    }

    Ok(bundle)
}

fn parse_markdown_entry(
    source_path: &str,
    content: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> BundleEntry {
    let kind = classify_entry(source_path);
    let split = split_frontmatter(content);
    let mut document = BundleDocument {
        has_frontmatter: split.frontmatter.is_some(),
        frontmatter_state: FrontmatterState::Absent,
        body_markdown: split.body.to_owned(),
        body_start_line: split.body_start_line,
    };

    if let Some(frontmatter_source) = split.frontmatter {
        let key_lines = frontmatter_key_lines(frontmatter_source, split.frontmatter_start_line);
        match yaml_serde::from_str::<YamlValue>(frontmatter_source) {
            Ok(YamlValue::Mapping(mapping)) => {
                let frontmatter =
                    parse_frontmatter_mapping(source_path, mapping, key_lines, diagnostics);
                document.frontmatter_state = FrontmatterState::Parsed(Box::new(frontmatter));
            }
            Ok(_) => {
                diagnostics.push(Diagnostic::error(
                    "invalid_frontmatter",
                    source_path,
                    Some(split.frontmatter_start_line),
                    "frontmatter must be a YAML mapping",
                ));
                document.frontmatter_state = FrontmatterState::Invalid;
            }
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "invalid_frontmatter",
                    source_path,
                    error.location().map(|location| {
                        let relative_line = location
                            .line()
                            .min(frontmatter_source.lines().count().max(1));
                        split.frontmatter_start_line + relative_line - 1
                    }),
                    error.to_string(),
                ));
                document.frontmatter_state = FrontmatterState::Invalid;
            }
        }
    }

    BundleEntry {
        source_path: source_path.to_owned(),
        kind,
        document,
    }
}

fn parse_frontmatter_mapping(
    source_path: &str,
    mapping: yaml_serde::Mapping,
    key_lines: BTreeMap<String, usize>,
    diagnostics: &mut Vec<Diagnostic>,
) -> DocumentFrontmatter {
    let mut fields = StandardFrontmatter::default();
    let mut extensions = BTreeMap::new();

    for (raw_key, raw_value) in mapping {
        let Some(key) = raw_key.as_str().map(str::to_owned) else {
            diagnostics.push(Diagnostic::error(
                "invalid_frontmatter",
                source_path,
                Some(1),
                "frontmatter keys must be strings",
            ));
            continue;
        };
        let line = key_lines.get(&key).copied();

        match key.as_str() {
            "type" => {
                fields.type_name =
                    parse_optional_string(source_path, line, "type", &raw_value, diagnostics, true);
            }
            "title" => {
                fields.title = parse_optional_string(
                    source_path,
                    line,
                    "title",
                    &raw_value,
                    diagnostics,
                    false,
                );
            }
            "description" => {
                fields.description = parse_optional_string(
                    source_path,
                    line,
                    "description",
                    &raw_value,
                    diagnostics,
                    false,
                );
            }
            "resource" => {
                fields.resource = parse_optional_string(
                    source_path,
                    line,
                    "resource",
                    &raw_value,
                    diagnostics,
                    false,
                );
            }
            "timestamp" => {
                fields.timestamp = parse_optional_string(
                    source_path,
                    line,
                    "timestamp",
                    &raw_value,
                    diagnostics,
                    false,
                );
            }
            "okf_version" => {
                fields.okf_version = parse_optional_string(
                    source_path,
                    line,
                    "okf_version",
                    &raw_value,
                    diagnostics,
                    false,
                );
            }
            "tags" => {
                fields.tags = parse_tags(source_path, line, &raw_value, diagnostics);
            }
            _ => {
                if let Some(json_value) =
                    yaml_value_to_json(source_path, line, &raw_value, diagnostics)
                {
                    extensions.insert(key, json_value);
                }
            }
        }
    }

    DocumentFrontmatter {
        fields,
        extensions,
        key_lines,
    }
}

fn parse_optional_string(
    source_path: &str,
    line: Option<usize>,
    key: &str,
    value: &YamlValue,
    diagnostics: &mut Vec<Diagnostic>,
    reject_empty: bool,
) -> Option<String> {
    match value.as_str().map(str::trim) {
        Some(text) if !text.is_empty() => Some(text.to_owned()),
        Some(_) if reject_empty => {
            diagnostics.push(Diagnostic::error(
                "invalid_frontmatter",
                source_path,
                line,
                format!("field '{key}' must not be empty"),
            ));
            None
        }
        Some(_) => None,
        None => {
            diagnostics.push(Diagnostic::error(
                "invalid_frontmatter",
                source_path,
                line,
                format!("field '{key}' must be a string"),
            ));
            None
        }
    }
}

fn parse_tags(
    source_path: &str,
    line: Option<usize>,
    value: &YamlValue,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    if let Some(text) = value.as_str() {
        return vec![text.trim().to_owned()];
    }

    let Some(sequence) = value.as_sequence() else {
        diagnostics.push(Diagnostic::error(
            "invalid_frontmatter",
            source_path,
            line,
            "field 'tags' must be a string or sequence of strings",
        ));
        return Vec::new();
    };

    let mut tags = Vec::new();
    for item in sequence {
        match item.as_str().map(str::trim) {
            Some(tag) if !tag.is_empty() => tags.push(tag.to_owned()),
            _ => diagnostics.push(Diagnostic::error(
                "invalid_frontmatter",
                source_path,
                line,
                "field 'tags' must contain only non-empty strings",
            )),
        }
    }
    tags
}

fn yaml_value_to_json(
    source_path: &str,
    line: Option<usize>,
    value: &YamlValue,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<JsonValue> {
    match value {
        YamlValue::Null => Some(JsonValue::Null),
        YamlValue::Bool(value) => Some(JsonValue::Bool(*value)),
        YamlValue::Number(value) => serde_json::to_value(value).ok(),
        YamlValue::String(value) => Some(JsonValue::String(value.clone())),
        YamlValue::Sequence(values) => {
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(yaml_value_to_json(source_path, line, value, diagnostics)?);
            }
            Some(JsonValue::Array(items))
        }
        YamlValue::Mapping(values) => {
            let mut object = serde_json::Map::with_capacity(values.len());
            for (key, value) in values {
                let Some(key) = key.as_str() else {
                    diagnostics.push(Diagnostic::error(
                        "invalid_frontmatter",
                        source_path,
                        line,
                        "extension mappings must use string keys",
                    ));
                    return None;
                };
                object.insert(
                    key.to_owned(),
                    yaml_value_to_json(source_path, line, value, diagnostics)?,
                );
            }
            Some(JsonValue::Object(object))
        }
        YamlValue::Tagged(_) => {
            diagnostics.push(Diagnostic::error(
                "invalid_frontmatter",
                source_path,
                line,
                "frontmatter YAML tags are not supported",
            ));
            None
        }
    }
}

fn classify_entry(source_path: &str) -> BundleEntryKind {
    if source_path.ends_with("/index.md") || source_path == ROOT_INDEX_PATH {
        BundleEntryKind::Index
    } else if source_path.ends_with("/log.md") || source_path == "log.md" {
        BundleEntryKind::Log
    } else {
        BundleEntryKind::Concept
    }
}

fn frontmatter_key_lines(frontmatter: &str, start_line: usize) -> BTreeMap<String, usize> {
    let mut key_lines = BTreeMap::new();
    for (index, raw_line) in frontmatter.lines().enumerate() {
        let trimmed = raw_line.trim_end_matches('\r');
        if trimmed.trim().is_empty()
            || trimmed.starts_with('#')
            || raw_line.starts_with(' ')
            || raw_line.starts_with('\t')
            || raw_line.starts_with('-')
        {
            continue;
        }
        let Some((key, _)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"').trim_matches('\'');
        if !key.is_empty() {
            key_lines
                .entry(key.to_owned())
                .or_insert(start_line + index);
        }
    }
    key_lines
}

fn path_to_source_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn diagnostic_order(left: &Diagnostic, right: &Diagnostic) -> std::cmp::Ordering {
    (
        &left.source_path,
        left.line,
        &left.code,
        &left.message,
        &left.severity,
    )
        .cmp(&(
            &right.source_path,
            right.line,
            &right.code,
            &right.message,
            &right.severity,
        ))
}

struct SplitFrontmatter<'a> {
    frontmatter: Option<&'a str>,
    frontmatter_start_line: usize,
    body: &'a str,
    body_start_line: usize,
}

fn split_frontmatter(content: &str) -> SplitFrontmatter<'_> {
    let Some(first_line_end) = content.find('\n').map(|index| index + 1) else {
        return SplitFrontmatter {
            frontmatter: None,
            frontmatter_start_line: 1,
            body: content,
            body_start_line: 1,
        };
    };
    if content[..first_line_end]
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        != "---"
    {
        return SplitFrontmatter {
            frontmatter: None,
            frontmatter_start_line: 1,
            body: content,
            body_start_line: 1,
        };
    }

    let mut cursor = first_line_end;
    let mut line_number = 2;
    while cursor < content.len() {
        let next_break = content[cursor..]
            .find('\n')
            .map(|offset| cursor + offset + 1)
            .unwrap_or(content.len());
        let line = content[cursor..next_break]
            .trim_end_matches('\n')
            .trim_end_matches('\r');
        if line == "---" {
            return SplitFrontmatter {
                frontmatter: Some(&content[first_line_end..cursor]),
                frontmatter_start_line: 2,
                body: &content[next_break..],
                body_start_line: line_number + 1,
            };
        }
        cursor = next_break;
        line_number += 1;
    }

    SplitFrontmatter {
        frontmatter: Some(&content[first_line_end..]),
        frontmatter_start_line: 2,
        body: "",
        body_start_line: line_number,
    }
}
