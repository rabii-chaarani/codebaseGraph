//! Deterministic OKF projection compilation.

use std::collections::{BTreeMap, BTreeSet};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::Value;

use crate::{
    diagnostic::Diagnostic,
    model::{
        Bundle, Citation, Concept, Directory, Heading, IndexSource, Link, LinkStatus, LogEntry,
        WikiProjection,
    },
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompileRequest {
    pub generated_at: String,
    pub source_revision: Option<String>,
    pub bundles: Vec<SourceBundle>,
}

impl CompileRequest {
    pub fn new(generated_at: impl Into<String>, source_revision: Option<String>) -> Self {
        Self {
            generated_at: generated_at.into(),
            source_revision,
            bundles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceBundle {
    pub id: String,
    pub root_path: String,
    pub okf_version: String,
    pub title: String,
    pub source_revision: Option<String>,
    pub documents: Vec<SourceDocument>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceDocument {
    pub path: String,
    pub kind: SourceDocumentKind,
    pub title: Option<String>,
    pub description: Option<String>,
    pub concept_type: Option<String>,
    pub resource: Option<String>,
    pub tags: Vec<String>,
    pub timestamp: Option<String>,
    pub extensions: BTreeMap<String, Value>,
    pub body_markdown: String,
}

impl SourceDocument {
    pub fn concept(
        path: impl Into<String>,
        concept_type: impl Into<String>,
        body_markdown: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            kind: SourceDocumentKind::Concept,
            concept_type: Some(concept_type.into()),
            body_markdown: body_markdown.into(),
            ..Self::default()
        }
    }

    pub fn index(path: impl Into<String>, body_markdown: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: SourceDocumentKind::Index,
            body_markdown: body_markdown.into(),
            ..Self::default()
        }
    }

    pub fn log(path: impl Into<String>, body_markdown: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: SourceDocumentKind::Log,
            body_markdown: body_markdown.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SourceDocumentKind {
    #[default]
    Concept,
    Index,
    Log,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteKind {
    Concept,
    Directory,
}

pub fn compile_projection(request: CompileRequest) -> WikiProjection {
    let mut projection = WikiProjection::new(request.generated_at, request.source_revision);
    projection.bundles = request.bundles.into_iter().map(compile_bundle).collect();
    projection.normalize();
    projection
}

pub fn compile_bundle(source: SourceBundle) -> Bundle {
    let SourceBundle {
        id,
        root_path,
        okf_version,
        title,
        source_revision,
        documents,
    } = source;
    let mut bundle = Bundle {
        id: id.clone(),
        root_path,
        okf_version,
        title: title.clone(),
        source_revision,
        ..Bundle::default()
    };
    let mut directories = BTreeMap::<String, DirectoryDraft>::new();
    let mut concept_links = BTreeMap::<String, Vec<ParsedLink>>::new();
    let mut concept_fragments = BTreeMap::<String, BTreeSet<String>>::new();
    let mut directory_fragments = BTreeMap::<String, BTreeSet<String>>::new();
    let mut logs = Vec::<LogSource>::new();

    directories.entry(String::new()).or_default();

    for document in documents {
        let path = normalize_source_path(&document.path);
        match document.kind {
            SourceDocumentKind::Concept => {
                let concept_id = concept_id_from_source_path(&path);
                let directory_path = parent_directory(&path);
                ensure_directory_chain(&directory_path, &mut directories);
                directories
                    .entry(directory_path)
                    .or_default()
                    .concept_ids
                    .insert(concept_id.clone());

                if bundle
                    .concepts
                    .iter()
                    .any(|concept| concept.id == concept_id)
                {
                    bundle.diagnostics.push(Diagnostic::warning(
                        "duplicate_concept_id",
                        path,
                        None,
                        format!("concept id `{concept_id}` is defined more than once"),
                    ));
                    continue;
                }

                let parsed = parse_markdown(&document.body_markdown);
                concept_fragments.insert(concept_id.clone(), parsed.fragments);
                concept_links.insert(concept_id.clone(), parsed.links);
                bundle.concepts.push(Concept {
                    id: concept_id,
                    bundle_id: id.clone(),
                    source_path: path,
                    concept_type: document
                        .concept_type
                        .unwrap_or_else(|| "concept".to_string()),
                    title: document.title,
                    description: document.description,
                    resource: document.resource,
                    tags: document.tags,
                    timestamp: document.timestamp,
                    extensions: document.extensions,
                    body_markdown: document.body_markdown,
                    headings: parsed.headings,
                    outbound_links: Vec::new(),
                    backlinks: Vec::new(),
                    citations: parsed.citations,
                });
            }
            SourceDocumentKind::Index => {
                let directory_path = reserved_document_directory(&path);
                ensure_directory_chain(&directory_path, &mut directories);
                let entry = directories.entry(directory_path).or_default();
                entry.index_source = IndexSource::Authored;
                entry.title = document.title;
                entry.description = document.description;
                entry.body_markdown = document.body_markdown;
            }
            SourceDocumentKind::Log => {
                let scope_path = reserved_document_directory(&path);
                ensure_directory_chain(&scope_path, &mut directories);
                logs.push(LogSource {
                    scope_path,
                    source_path: path,
                    body_markdown: document.body_markdown,
                });
            }
        }
    }

    add_directory_hierarchy(&mut directories);
    finalize_directories(&title, &bundle.concepts, &mut directories);
    build_directory_fragments(&directories, &mut directory_fragments);
    aggregate_logs(&mut bundle, &logs, &mut directories);
    resolve_bundle_links(
        &id,
        &mut bundle,
        &directories,
        &concept_links,
        &concept_fragments,
        &directory_fragments,
    );

    bundle.directories = directories
        .into_iter()
        .map(|(path, draft)| draft.into_directory(path))
        .collect();
    bundle.normalize();
    bundle
}

pub fn concept_route_id(bundle_id: &str, concept_id: &str) -> String {
    format!("bundle:{bundle_id}:concept:{concept_id}")
}

pub fn directory_route_id(bundle_id: &str, directory_path: &str) -> String {
    if directory_path.is_empty() {
        format!("bundle:{bundle_id}:directory:/")
    } else {
        format!("bundle:{bundle_id}:directory:{directory_path}")
    }
}

#[derive(Clone, Debug, Default)]
struct DirectoryDraft {
    title: Option<String>,
    description: Option<String>,
    index_source: IndexSource,
    body_markdown: String,
    child_directories: BTreeSet<String>,
    concept_ids: BTreeSet<String>,
    log_entries: Vec<LogEntry>,
}

impl DirectoryDraft {
    fn into_directory(self, path: String) -> Directory {
        Directory {
            path,
            title: self.title.unwrap_or_default(),
            description: self.description,
            index_source: self.index_source,
            body_markdown: self.body_markdown,
            child_directories: self.child_directories.into_iter().collect(),
            concept_ids: self.concept_ids.into_iter().collect(),
            log_entries: self.log_entries,
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedMarkdown {
    headings: Vec<Heading>,
    fragments: BTreeSet<String>,
    links: Vec<ParsedLink>,
    citations: Vec<Citation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedLink {
    raw_href: String,
    fragment: Option<String>,
    context: Option<String>,
    line: Option<usize>,
}

#[derive(Clone, Debug)]
struct LogSource {
    scope_path: String,
    source_path: String,
    body_markdown: String,
}

#[derive(Clone, Debug)]
enum ResolvedTarget {
    Concept {
        concept_id: String,
        route_id: String,
    },
    Directory {
        route_id: String,
    },
}

impl ResolvedTarget {
    fn route_id(&self) -> &str {
        match self {
            Self::Concept { route_id, .. } | Self::Directory { route_id } => route_id,
        }
    }
}

#[derive(Clone, Debug)]
enum LinkResolution {
    Resolved(ResolvedTarget),
    Broken {
        target: Option<ResolvedTarget>,
        message: String,
    },
    Rejected {
        message: String,
    },
    External,
}

#[derive(Clone, Debug)]
struct LinkCapture {
    raw_href: String,
    context: String,
    line: Option<usize>,
}

fn finalize_directories(
    bundle_title: &str,
    concepts: &[Concept],
    directories: &mut BTreeMap<String, DirectoryDraft>,
) {
    let concept_titles = concepts
        .iter()
        .map(|concept| (concept.id.as_str(), concept.display_title()))
        .collect::<BTreeMap<_, _>>();

    for (path, draft) in directories.iter_mut() {
        if draft.title.is_none() {
            draft.title = Some(default_directory_title(path, bundle_title));
        }

        if draft.index_source == IndexSource::Synthetic {
            draft.body_markdown = synthesize_directory_body(path, draft, &concept_titles);
        }
    }
}

fn build_directory_fragments(
    directories: &BTreeMap<String, DirectoryDraft>,
    fragments: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for (path, draft) in directories {
        let parsed = parse_markdown(&draft.body_markdown);
        fragments.insert(path.clone(), parsed.fragments);
    }
}

fn aggregate_logs(
    bundle: &mut Bundle,
    logs: &[LogSource],
    directories: &mut BTreeMap<String, DirectoryDraft>,
) {
    for log in logs {
        let (entries, diagnostics) = parse_log_entries(log);
        bundle.diagnostics.extend(diagnostics);
        for entry in entries {
            for scope in ancestor_directories(&log.scope_path) {
                if let Some(directory) = directories.get_mut(&scope) {
                    directory.log_entries.push(entry.clone());
                }
            }
        }
    }
}

fn resolve_bundle_links(
    bundle_id: &str,
    bundle: &mut Bundle,
    directories: &BTreeMap<String, DirectoryDraft>,
    parsed_links: &BTreeMap<String, Vec<ParsedLink>>,
    concept_fragments: &BTreeMap<String, BTreeSet<String>>,
    directory_fragments: &BTreeMap<String, BTreeSet<String>>,
) {
    let concept_ids = bundle
        .concepts
        .iter()
        .map(|concept| concept.id.clone())
        .collect::<BTreeSet<_>>();
    let directory_paths = directories.keys().cloned().collect::<BTreeSet<_>>();
    let mut backlinks = BTreeMap::<String, Vec<Link>>::new();
    let link_index = LinkIndex {
        bundle_id,
        concept_ids: &concept_ids,
        directory_paths: &directory_paths,
        concept_fragments,
        directory_fragments,
    };

    for concept in &mut bundle.concepts {
        let source_directory = parent_directory(&concept.source_path);
        let source_fragments = concept_fragments
            .get(&concept.id)
            .cloned()
            .unwrap_or_default();
        let concept_route = concept_route_id(bundle_id, &concept.id);

        for parsed_link in parsed_links.get(&concept.id).into_iter().flatten() {
            let resolution = resolve_link(
                &link_index,
                &concept.id,
                &source_directory,
                &source_fragments,
                parsed_link,
            );
            let (status, normalized_target_id, diagnostic) = match resolution {
                LinkResolution::Resolved(target) => {
                    if let ResolvedTarget::Concept {
                        concept_id: target_id,
                        route_id,
                    } = target.clone()
                    {
                        backlinks.entry(target_id).or_default().push(Link {
                            source_id: concept.id.clone(),
                            raw_href: parsed_link.raw_href.clone(),
                            normalized_target_id: Some(route_id.clone()),
                            fragment: parsed_link.fragment.clone(),
                            status: LinkStatus::Resolved,
                            context: parsed_link.context.clone(),
                        });
                    }
                    (
                        LinkStatus::Resolved,
                        Some(target.route_id().to_string()),
                        None,
                    )
                }
                LinkResolution::Broken { target, message } => (
                    LinkStatus::Broken,
                    target.map(|target| target.route_id().to_string()),
                    Some(Diagnostic::warning(
                        "broken_link",
                        concept.source_path.clone(),
                        parsed_link.line,
                        message,
                    )),
                ),
                LinkResolution::Rejected { message } => (
                    LinkStatus::Rejected,
                    None,
                    Some(Diagnostic::warning(
                        "rejected_link",
                        concept.source_path.clone(),
                        parsed_link.line,
                        message,
                    )),
                ),
                LinkResolution::External => (LinkStatus::External, None, None),
            };

            concept.outbound_links.push(Link {
                source_id: concept.id.clone(),
                raw_href: parsed_link.raw_href.clone(),
                normalized_target_id,
                fragment: parsed_link.fragment.clone(),
                status,
                context: parsed_link.context.clone(),
            });

            if let Some(diagnostic) = diagnostic {
                bundle.diagnostics.push(diagnostic);
            }
        }

        concept.outbound_links.sort();
        concept.outbound_links.dedup();
        if concept.id == concept_route {
            unreachable!("concept route ids are always namespaced");
        }
    }

    for concept in &mut bundle.concepts {
        concept.backlinks = backlinks.remove(&concept.id).unwrap_or_default();
        concept.backlinks.sort();
        concept.backlinks.dedup();
    }
}

struct LinkIndex<'a> {
    bundle_id: &'a str,
    concept_ids: &'a BTreeSet<String>,
    directory_paths: &'a BTreeSet<String>,
    concept_fragments: &'a BTreeMap<String, BTreeSet<String>>,
    directory_fragments: &'a BTreeMap<String, BTreeSet<String>>,
}

fn resolve_link(
    index: &LinkIndex<'_>,
    current_concept_id: &str,
    source_directory: &str,
    source_fragments: &BTreeSet<String>,
    link: &ParsedLink,
) -> LinkResolution {
    let href = link.raw_href.trim();
    if href.is_empty() {
        return LinkResolution::Rejected {
            message: "link target is empty".to_string(),
        };
    }

    if href.starts_with("//") {
        return LinkResolution::External;
    }

    if let Some(scheme) = link_scheme(href) {
        return if is_safe_external_scheme(scheme) {
            LinkResolution::External
        } else {
            LinkResolution::Rejected {
                message: format!("link `{href}` uses rejected scheme `{scheme}`"),
            }
        };
    }

    let (path_part, fragment) = split_fragment(href);
    let normalized_fragment = fragment
        .filter(|fragment| !fragment.is_empty())
        .map(slugify_fragment);
    let target = if path_part.is_empty() {
        ResolvedTarget::Concept {
            concept_id: current_concept_id.to_string(),
            route_id: concept_route_id(index.bundle_id, current_concept_id),
        }
    } else {
        match resolve_target_path(
            index.bundle_id,
            source_directory,
            &path_part,
            index.concept_ids,
            index.directory_paths,
        ) {
            Ok(target) => target,
            Err(ResolvePathError::Traversal) => {
                return LinkResolution::Rejected {
                    message: format!("link `{href}` escapes the bundle root"),
                };
            }
            Err(ResolvePathError::Broken { target, message }) => {
                return LinkResolution::Broken { target, message };
            }
        }
    };

    if let Some(fragment) = normalized_fragment.clone() {
        let found = match &target {
            ResolvedTarget::Concept { concept_id, .. } => index
                .concept_fragments
                .get(concept_id)
                .map(|fragments| fragments.contains(&fragment))
                .unwrap_or_else(|| source_fragments.contains(&fragment)),
            ResolvedTarget::Directory { route_id } => directory_path_from_route(route_id)
                .and_then(|path| index.directory_fragments.get(path))
                .map(|fragments| fragments.contains(&fragment))
                .unwrap_or(false),
        };

        if !found {
            return LinkResolution::Broken {
                target: Some(target),
                message: format!("link `{href}` points to missing fragment `{fragment}`"),
            };
        }
    }

    LinkResolution::Resolved(target)
}

fn resolve_target_path(
    bundle_id: &str,
    source_directory: &str,
    raw_path: &str,
    concept_ids: &BTreeSet<String>,
    directory_paths: &BTreeSet<String>,
) -> Result<ResolvedTarget, ResolvePathError> {
    let is_absolute = raw_path.starts_with('/');
    let has_trailing_slash = raw_path.ends_with('/');
    let base_directory = if is_absolute { "" } else { source_directory };
    let normalized = normalize_link_path(base_directory, raw_path)?;

    if normalized.is_empty() {
        return Ok(ResolvedTarget::Directory {
            route_id: directory_route_id(bundle_id, ""),
        });
    }

    if normalized.ends_with("/index.md") || normalized == "index.md" {
        let directory_path = reserved_document_directory(&normalized);
        return Ok(ResolvedTarget::Directory {
            route_id: directory_route_id(bundle_id, &directory_path),
        });
    }

    if normalized.ends_with("/log.md") || normalized == "log.md" {
        let directory_path = reserved_document_directory(&normalized);
        return Ok(ResolvedTarget::Directory {
            route_id: directory_route_id(bundle_id, &directory_path),
        });
    }

    if let Some(concept_id) = normalized.strip_suffix(".md") {
        return if concept_ids.contains(concept_id) {
            Ok(ResolvedTarget::Concept {
                concept_id: concept_id.to_string(),
                route_id: concept_route_id(bundle_id, concept_id),
            })
        } else {
            Err(ResolvePathError::Broken {
                target: Some(ResolvedTarget::Concept {
                    concept_id: concept_id.to_string(),
                    route_id: concept_route_id(bundle_id, concept_id),
                }),
                message: format!("link `{raw_path}` points to missing concept `{concept_id}`"),
            })
        };
    }

    if concept_ids.contains(&normalized) {
        return Ok(ResolvedTarget::Concept {
            concept_id: normalized.clone(),
            route_id: concept_route_id(bundle_id, &normalized),
        });
    }

    if directory_paths.contains(&normalized) || has_trailing_slash {
        return if directory_paths.contains(&normalized) {
            Ok(ResolvedTarget::Directory {
                route_id: directory_route_id(bundle_id, &normalized),
            })
        } else {
            Err(ResolvePathError::Broken {
                target: Some(ResolvedTarget::Directory {
                    route_id: directory_route_id(bundle_id, &normalized),
                }),
                message: format!("link `{raw_path}` points to missing directory `{normalized}`"),
            })
        };
    }

    Err(ResolvePathError::Broken {
        target: Some(ResolvedTarget::Concept {
            concept_id: normalized.clone(),
            route_id: concept_route_id(bundle_id, &normalized),
        }),
        message: format!("link `{raw_path}` points to missing target `{normalized}`"),
    })
}

#[derive(Clone, Debug)]
enum ResolvePathError {
    Traversal,
    Broken {
        target: Option<ResolvedTarget>,
        message: String,
    },
}

fn parse_markdown(body: &str) -> ParsedMarkdown {
    let mut headings = Vec::new();
    let mut heading_counts = BTreeMap::<String, usize>::new();
    let mut fragments = BTreeSet::<String>::new();
    let mut links = Vec::<ParsedLink>::new();
    let mut current_heading = Option::<(u8, String)>::None;
    let mut current_link = Option::<LinkCapture>::None;

    for (event, range) in Parser::new(body).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading = Some((heading_level(level), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = current_heading.take() {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        let slug = unique_slug(&text, &mut heading_counts);
                        fragments.insert(slug.clone());
                        headings.push(Heading {
                            level,
                            id: slug,
                            text,
                        });
                    }
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                current_link = Some(LinkCapture {
                    raw_href: dest_url.to_string(),
                    context: String::new(),
                    line: Some(line_number(body, range.start)),
                });
            }
            Event::End(TagEnd::Link) => {
                if let Some(link) = current_link.take() {
                    let raw_href = link.raw_href;
                    let fragment = split_fragment(&raw_href)
                        .1
                        .filter(|fragment| !fragment.is_empty())
                        .map(slugify_fragment);
                    links.push(ParsedLink {
                        raw_href,
                        fragment,
                        context: normalize_context(&link.context),
                        line: link.line,
                    });
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, current)) = current_heading.as_mut() {
                    current.push_str(text.as_ref());
                }
                if let Some(current) = current_link.as_mut() {
                    current.context.push_str(text.as_ref());
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, current)) = current_heading.as_mut() {
                    current.push(' ');
                }
                if let Some(current) = current_link.as_mut() {
                    current.context.push(' ');
                }
            }
            _ => {}
        }
    }

    let citations = extract_citations(body);
    ParsedMarkdown {
        headings,
        fragments,
        links,
        citations,
    }
}

fn extract_citations(body: &str) -> Vec<Citation> {
    let lines = body.lines().collect::<Vec<_>>();
    let mut citations = Vec::new();
    let mut in_citations = false;
    let mut citations_level = 0usize;
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        if let Some((level, heading)) = parse_heading_line(line.trim()) {
            if heading.eq_ignore_ascii_case("citations") {
                in_citations = true;
                citations_level = level;
                index += 1;
                continue;
            }
            if in_citations && level <= citations_level {
                break;
            }
        }

        if in_citations {
            if let Some((number, text, consumed)) = parse_list_item(&lines, index) {
                citations.push(Citation {
                    number,
                    href: extract_first_href(&text),
                    text,
                });
                index += consumed;
                continue;
            }
        }

        index += 1;
    }

    citations
}

fn parse_log_entries(log: &LogSource) -> (Vec<LogEntry>, Vec<Diagnostic>) {
    let lines = log.body_markdown.lines().collect::<Vec<_>>();
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let mut current_date = Option::<String>::None;
    let mut current_category = "update".to_string();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index].trim();
        if let Some((_, heading)) = parse_heading_line(line) {
            if is_iso_date(heading) {
                current_date = Some(heading.to_string());
                current_category = "update".to_string();
            } else if current_date.is_some() {
                current_category = heading.to_string();
            }
            index += 1;
            continue;
        }

        if let Some((_, text, consumed)) = parse_list_item(&lines, index) {
            if let Some(date) = current_date.clone() {
                entries.push(LogEntry {
                    scope_path: log.scope_path.clone(),
                    date,
                    category: current_category.clone(),
                    text: text.clone(),
                    links: extract_all_hrefs(&text),
                });
            } else {
                diagnostics.push(Diagnostic::warning(
                    "invalid_log_entry",
                    log.source_path.clone(),
                    Some(index + 1),
                    "log entries must appear under an ISO date heading",
                ));
            }
            index += consumed;
            continue;
        }

        index += 1;
    }

    (entries, diagnostics)
}

fn parse_list_item(lines: &[&str], start: usize) -> Option<(u32, String, usize)> {
    let line = lines.get(start)?.trim_end();
    let (number, content) = parse_list_marker(line.trim_start())?;
    let mut parts = vec![content.trim().to_string()];
    let mut consumed = 1usize;

    while let Some(next) = lines.get(start + consumed) {
        let trimmed = next.trim_end();
        if trimmed.trim().is_empty() {
            break;
        }
        if parse_heading_line(trimmed.trim()).is_some()
            || parse_list_marker(trimmed.trim_start()).is_some()
        {
            break;
        }
        if next.starts_with(' ') || next.starts_with('\t') {
            parts.push(trimmed.trim().to_string());
            consumed += 1;
            continue;
        }
        break;
    }

    Some((number, parts.join(" "), consumed))
}

fn parse_list_marker(line: &str) -> Option<(u32, &str)> {
    if let Some(rest) = line.strip_prefix("- ") {
        return Some((1, rest));
    }

    if let Some(rest) = line.strip_prefix("* ") {
        return Some((1, rest));
    }

    let marker = line
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if marker.is_empty() {
        return None;
    }

    let rest = line.get(marker.len()..)?;
    let rest = rest.strip_prefix(". ")?;
    Some((marker.parse().ok()?, rest))
}

fn parse_heading_line(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 {
        return None;
    }
    let text = line.get(hashes..)?.trim();
    if text.is_empty() {
        None
    } else {
        Some((hashes, text))
    }
}

fn extract_first_href(text: &str) -> Option<String> {
    extract_all_hrefs(text).into_iter().next()
}

fn extract_all_hrefs(text: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("](") {
        let candidate = &remaining[start + 2..];
        if let Some(end) = candidate.find(')') {
            hrefs.push(candidate[..end].to_string());
            remaining = &candidate[end + 1..];
        } else {
            break;
        }
    }
    hrefs
}

fn ensure_directory_chain(path: &str, directories: &mut BTreeMap<String, DirectoryDraft>) {
    for scope in ancestor_directories(path) {
        directories.entry(scope).or_default();
    }
}

fn add_directory_hierarchy(directories: &mut BTreeMap<String, DirectoryDraft>) {
    let paths = directories.keys().cloned().collect::<Vec<_>>();
    for path in paths {
        if path.is_empty() {
            continue;
        }
        let parent = parent_directory_from_directory(&path);
        if let Some(entry) = directories.get_mut(&parent) {
            entry.child_directories.insert(path.clone());
        }
    }
}

fn ancestor_directories(path: &str) -> Vec<String> {
    let mut scopes = Vec::new();
    let mut current = Some(path.to_string());
    while let Some(scope) = current.take() {
        scopes.push(scope.clone());
        if scope.is_empty() {
            break;
        }
        current = Some(parent_directory_from_directory(&scope));
    }
    scopes.reverse();
    scopes
}

fn synthesize_directory_body(
    path: &str,
    draft: &DirectoryDraft,
    concept_titles: &BTreeMap<&str, &str>,
) -> String {
    let title = draft.title.clone().unwrap_or_default();
    let mut body = format!("# {title}\n");

    if !draft.child_directories.is_empty() {
        body.push_str("\n## Directories\n");
        for child in &draft.child_directories {
            body.push_str("- `");
            body.push_str(child);
            body.push_str("`\n");
        }
    }

    if !draft.concept_ids.is_empty() {
        body.push_str("\n## Concepts\n");
        for concept_id in &draft.concept_ids {
            body.push_str("- `");
            body.push_str(concept_id);
            body.push('`');
            if let Some(title) = concept_titles.get(concept_id.as_str()) {
                if *title != concept_id {
                    body.push_str(" — ");
                    body.push_str(title);
                }
            }
            body.push('\n');
        }
    }

    if path.is_empty() && draft.child_directories.is_empty() && draft.concept_ids.is_empty() {
        body.push('\n');
    }

    body
}

fn default_directory_title(path: &str, bundle_title: &str) -> String {
    if path.is_empty() {
        return bundle_title.to_string();
    }

    path.split('/')
        .next_back()
        .unwrap_or(path)
        .split(['-', '_'])
        .filter(|segment| !segment.is_empty())
        .map(title_case_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut title = first.to_uppercase().collect::<String>();
    title.push_str(&chars.as_str().to_lowercase());
    title
}

fn normalize_source_path(path: &str) -> String {
    let mut parts = Vec::<String>::new();
    for part in path.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        parts.push(part.to_string());
    }
    parts.join("/")
}

fn concept_id_from_source_path(path: &str) -> String {
    path.strip_suffix(".md").unwrap_or(path).to_string()
}

fn reserved_document_directory(path: &str) -> String {
    let normalized = normalize_source_path(path);
    match normalized.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

fn parent_directory(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

fn parent_directory_from_directory(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

fn normalize_link_path(base_directory: &str, raw_path: &str) -> Result<String, ResolvePathError> {
    let cleaned = raw_path.replace('\\', "/");
    let absolute = cleaned.starts_with('/');
    let mut parts = if absolute || base_directory.is_empty() {
        Vec::<String>::new()
    } else {
        base_directory
            .split('/')
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect()
    };

    for part in cleaned.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.pop().is_none() {
                return Err(ResolvePathError::Traversal);
            }
            continue;
        }
        parts.push(part.to_string());
    }

    Ok(parts.join("/"))
}

fn unique_slug(text: &str, counts: &mut BTreeMap<String, usize>) -> String {
    let base = slugify_fragment(text);
    let count = counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", count)
    }
}

fn slugify_fragment(text: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in text.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn line_number(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn normalize_context(context: &str) -> Option<String> {
    let collapsed = context.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn split_fragment(href: &str) -> (String, Option<&str>) {
    match href.split_once('#') {
        Some((path, fragment)) => (path.to_string(), Some(fragment)),
        None => (href.to_string(), None),
    }
}

fn link_scheme(href: &str) -> Option<&str> {
    for (index, ch) in href.char_indices() {
        match ch {
            ':' => return Some(&href[..index]),
            '/' | '#' | '?' => return None,
            ch if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.') => {}
            _ => return None,
        }
    }
    None
}

fn is_safe_external_scheme(scheme: &str) -> bool {
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto" | "tel"
    )
}

fn is_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn directory_path_from_route(route_id: &str) -> Option<&str> {
    route_id
        .split_once(":directory:")
        .map(|(_, path)| if path == "/" { "" } else { path })
}
