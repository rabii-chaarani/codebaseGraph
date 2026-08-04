//! Concept-aware deterministic search.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Citation, Concept, Heading, WikiProjection};

pub const EXACT_CONCEPT_ID_BOOST: u32 = 1_000;
pub const EXACT_TITLE_BOOST: u32 = 980;
pub const PREFIX_CONCEPT_ID_BOOST: u32 = 760;
pub const PREFIX_TITLE_BOOST: u32 = 740;
pub const TITLE_TERM_BOOST: u32 = 520;
pub const TYPE_TERM_BOOST: u32 = 460;
pub const TAG_TERM_BOOST: u32 = 430;
pub const DESCRIPTION_TERM_BOOST: u32 = 260;
pub const HEADING_TERM_BOOST: u32 = 230;
pub const EXTENSION_TERM_BOOST: u32 = 210;
pub const CITATION_TERM_BOOST: u32 = 180;
pub const BODY_TERM_BOOST: u32 = 140;
pub const MAX_SNIPPET_LEN: usize = 160;
pub const SNIPPET_RADIUS: usize = 48;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchIndex {
    pub documents: Vec<SearchDocument>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchDocument {
    pub bundle_id: String,
    pub concept_id: String,
    pub concept_type: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub source_path: String,
    pub headings: Vec<Heading>,
    pub body_markdown: String,
    pub citations: Vec<Citation>,
    pub scalar_extensions: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchQuery {
    pub text: String,
    pub bundle: Option<String>,
    pub concept_type: Option<String>,
    pub tags: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchResult {
    pub bundle_id: String,
    pub concept_id: String,
    pub concept_type: String,
    pub title: String,
    pub score: u32,
    pub matched_fields: Vec<String>,
    pub snippet: Option<String>,
}

impl SearchIndex {
    pub fn build(projection: &WikiProjection) -> Self {
        let mut documents = projection
            .bundles
            .iter()
            .flat_map(|bundle| {
                bundle.concepts.iter().map(|concept| SearchDocument {
                    bundle_id: bundle.id.clone(),
                    concept_id: concept.id.clone(),
                    concept_type: concept.concept_type.clone(),
                    title: concept.display_title().to_string(),
                    description: concept.description.clone(),
                    tags: concept.tags.clone(),
                    source_path: concept.source_path.clone(),
                    headings: concept.headings.clone(),
                    body_markdown: concept.body_markdown.clone(),
                    citations: concept.citations.clone(),
                    scalar_extensions: scalar_extension_terms(concept),
                })
            })
            .collect::<Vec<_>>();

        documents.sort_by(|left, right| {
            (&left.bundle_id, &left.concept_id, &left.source_path).cmp(&(
                &right.bundle_id,
                &right.concept_id,
                &right.source_path,
            ))
        });

        Self { documents }
    }

    pub fn search(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let normalized_query = normalize_text(&query.text);
        if normalized_query.is_empty() {
            return Vec::new();
        }

        let filters = SearchFilters::from_query(query);
        let tokens = tokenize(&query.text);
        let limit = if query.limit == 0 { 20 } else { query.limit };

        let mut results = self
            .documents
            .iter()
            .filter(|document| filters.matches(document))
            .filter_map(|document| score_document(document, &normalized_query, &tokens))
            .collect::<Vec<_>>();

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.bundle_id.cmp(&right.bundle_id))
                .then_with(|| left.concept_id.cmp(&right.concept_id))
                .then_with(|| left.title.cmp(&right.title))
        });
        results.truncate(limit);
        results
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: 20,
            ..Self::default()
        }
    }
}

#[derive(Default)]
struct SearchFilters {
    bundle: Option<String>,
    concept_type: Option<String>,
    tags: BTreeSet<String>,
}

impl SearchFilters {
    fn from_query(query: &SearchQuery) -> Self {
        Self {
            bundle: query.bundle.as_deref().map(normalize_text),
            concept_type: query.concept_type.as_deref().map(normalize_text),
            tags: query.tags.iter().map(|tag| normalize_text(tag)).collect(),
        }
    }

    fn matches(&self, document: &SearchDocument) -> bool {
        if let Some(bundle) = &self.bundle {
            if normalize_text(&document.bundle_id) != *bundle {
                return false;
            }
        }

        if let Some(concept_type) = &self.concept_type {
            if normalize_text(&document.concept_type) != *concept_type {
                return false;
            }
        }

        if self.tags.is_empty() {
            return true;
        }

        let tags = document
            .tags
            .iter()
            .map(|tag| normalize_text(tag))
            .collect::<BTreeSet<_>>();
        self.tags.iter().all(|tag| tags.contains(tag))
    }
}

fn score_document(
    document: &SearchDocument,
    normalized_query: &str,
    tokens: &[String],
) -> Option<SearchResult> {
    let concept_id = normalize_text(&document.concept_id);
    let title = normalize_text(&document.title);
    let concept_type = normalize_text(&document.concept_type);
    let description = document
        .description
        .as_deref()
        .map(normalize_text)
        .unwrap_or_default();
    let body = normalize_text(&document.body_markdown);
    let headings = document
        .headings
        .iter()
        .map(|heading| normalize_text(&heading.text))
        .collect::<Vec<_>>();
    let citations = document
        .citations
        .iter()
        .map(citation_text)
        .map(|text| normalize_text(&text))
        .collect::<Vec<_>>();
    let extensions = document
        .scalar_extensions
        .iter()
        .map(|value| normalize_text(value))
        .collect::<Vec<_>>();

    let mut score = 0;
    let mut matched_fields = BTreeSet::new();

    if concept_id == normalized_query {
        score += EXACT_CONCEPT_ID_BOOST;
        matched_fields.insert("concept_id".to_string());
    }
    if title == normalized_query {
        score += EXACT_TITLE_BOOST;
        matched_fields.insert("title".to_string());
    }
    if concept_id.starts_with(normalized_query) && concept_id != normalized_query {
        score += PREFIX_CONCEPT_ID_BOOST;
        matched_fields.insert("concept_id".to_string());
    }
    if title.starts_with(normalized_query) && title != normalized_query {
        score += PREFIX_TITLE_BOOST;
        matched_fields.insert("title".to_string());
    }

    for token in tokens {
        if title.contains(token) {
            score += TITLE_TERM_BOOST;
            matched_fields.insert("title".to_string());
        }
        if concept_type.contains(token) {
            score += TYPE_TERM_BOOST;
            matched_fields.insert("type".to_string());
        }
        if document
            .tags
            .iter()
            .any(|tag| normalize_text(tag).contains(token))
        {
            score += TAG_TERM_BOOST;
            matched_fields.insert("tag".to_string());
        }
        if !description.is_empty() && description.contains(token) {
            score += DESCRIPTION_TERM_BOOST;
            matched_fields.insert("description".to_string());
        }
        if headings.iter().any(|heading| heading.contains(token)) {
            score += HEADING_TERM_BOOST;
            matched_fields.insert("heading".to_string());
        }
        if extensions.iter().any(|value| value.contains(token)) {
            score += EXTENSION_TERM_BOOST;
            matched_fields.insert("extension".to_string());
        }
        if citations.iter().any(|citation| citation.contains(token)) {
            score += CITATION_TERM_BOOST;
            matched_fields.insert("citation".to_string());
        }
        if body.contains(token) {
            score += BODY_TERM_BOOST;
            matched_fields.insert("body".to_string());
        }
    }

    if score == 0 {
        return None;
    }

    let snippet = build_snippet(document, normalized_query, tokens);
    Some(SearchResult {
        bundle_id: document.bundle_id.clone(),
        concept_id: document.concept_id.clone(),
        concept_type: document.concept_type.clone(),
        title: document.title.clone(),
        score,
        matched_fields: matched_fields.into_iter().collect(),
        snippet,
    })
}

fn build_snippet(
    document: &SearchDocument,
    normalized_query: &str,
    tokens: &[String],
) -> Option<String> {
    let fields = snippet_candidates(document);

    fields.into_iter().find_map(|value| {
        highlight_match(&value, normalized_query, tokens).map(|snippet| truncate_snippet(&snippet))
    })
}

fn snippet_candidates(document: &SearchDocument) -> Vec<String> {
    let mut fields = vec![document.title.clone()];
    if let Some(description) = &document.description {
        fields.push(description.clone());
    }
    fields.extend(document.headings.iter().map(|heading| heading.text.clone()));
    fields.extend(document.scalar_extensions.iter().cloned());
    fields.extend(document.citations.iter().map(citation_text));
    fields.push(document.body_markdown.clone());
    fields
}

fn highlight_match(value: &str, normalized_query: &str, tokens: &[String]) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let mut matches = Vec::new();
    if let Some(position) = lower.find(normalized_query) {
        matches.push((position, normalized_query.len()));
    }
    for token in tokens {
        if let Some(position) = lower.find(token) {
            matches.push((position, token.len()));
        }
    }

    let (position, length) = matches.into_iter().min_by_key(|(position, _)| *position)?;
    let start = position.saturating_sub(SNIPPET_RADIUS);
    let end = value.len().min(position + length + SNIPPET_RADIUS);

    let prefix = escape_html(&value[start..position]);
    let matched = escape_html(&value[position..position + length]);
    let suffix = escape_html(&value[position + length..end]);

    Some(format!("{prefix}<mark>{matched}</mark>{suffix}"))
}

fn truncate_snippet(snippet: &str) -> String {
    let mut truncated = String::new();
    for character in snippet.chars().take(MAX_SNIPPET_LEN) {
        truncated.push(character);
    }
    truncated
}

fn tokenize(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(normalize_text)
        .filter(|token| seen.insert(token.clone()))
        .collect()
}

fn normalize_text(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}

fn scalar_extension_terms(concept: &Concept) -> Vec<String> {
    let mut values = Vec::new();
    for (key, value) in &concept.extensions {
        match value {
            Value::String(text) => values.push(format!("{key} {text}")),
            Value::Number(number) => values.push(format!("{key} {number}")),
            Value::Bool(value) => values.push(format!("{key} {value}")),
            Value::Array(array) => {
                for value in array {
                    match value {
                        Value::String(text) => values.push(format!("{key} {text}")),
                        Value::Number(number) => values.push(format!("{key} {number}")),
                        Value::Bool(value) => values.push(format!("{key} {value}")),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    values.sort();
    values
}

fn citation_text(citation: &Citation) -> String {
    match &citation.href {
        Some(href) => format!("{} {}", citation.text, href),
        None => citation.text.clone(),
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
