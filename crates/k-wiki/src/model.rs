use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{diagnostic::Diagnostic, WIKI_SCHEMA_VERSION};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WikiProjection {
    pub schema_version: u32,
    pub generated_at: String,
    pub source_revision: Option<String>,
    pub bundles: Vec<Bundle>,
}

impl WikiProjection {
    pub fn new(generated_at: impl Into<String>, source_revision: Option<String>) -> Self {
        Self {
            schema_version: WIKI_SCHEMA_VERSION,
            generated_at: generated_at.into(),
            source_revision,
            bundles: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        self.bundles.sort_by(|left, right| left.id.cmp(&right.id));
        for bundle in &mut self.bundles {
            bundle.normalize();
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Bundle {
    pub id: String,
    pub root_path: String,
    pub okf_version: String,
    pub title: String,
    pub source_revision: Option<String>,
    pub directories: Vec<Directory>,
    pub concepts: Vec<Concept>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Bundle {
    pub fn normalize(&mut self) {
        self.directories
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.concepts.sort_by(|left, right| left.id.cmp(&right.id));
        self.diagnostics.sort_by(|left, right| {
            (
                &left.source_path,
                left.line,
                &left.code,
                &left.severity,
                &left.message,
            )
                .cmp(&(
                    &right.source_path,
                    right.line,
                    &right.code,
                    &right.severity,
                    &right.message,
                ))
        });
        for directory in &mut self.directories {
            directory.normalize();
        }
        for concept in &mut self.concepts {
            concept.normalize();
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Directory {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    pub index_source: IndexSource,
    pub body_markdown: String,
    pub child_directories: Vec<String>,
    pub concept_ids: Vec<String>,
    pub log_entries: Vec<LogEntry>,
}

impl Directory {
    pub fn normalize(&mut self) {
        self.child_directories.sort();
        self.child_directories.dedup();
        self.concept_ids.sort();
        self.concept_ids.dedup();
        self.log_entries.sort_by(|left, right| {
            (&right.date, &left.scope_path, &left.category, &left.text).cmp(&(
                &left.date,
                &right.scope_path,
                &right.category,
                &right.text,
            ))
        });
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexSource {
    Authored,
    #[default]
    Synthetic,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Concept {
    pub id: String,
    pub bundle_id: String,
    pub source_path: String,
    #[serde(rename = "type")]
    pub concept_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resource: Option<String>,
    pub tags: Vec<String>,
    pub timestamp: Option<String>,
    pub extensions: BTreeMap<String, Value>,
    pub body_markdown: String,
    pub headings: Vec<Heading>,
    pub outbound_links: Vec<Link>,
    pub backlinks: Vec<Link>,
    pub citations: Vec<Citation>,
}

impl Concept {
    pub fn normalize(&mut self) {
        self.tags.sort();
        self.tags.dedup();
        self.headings
            .sort_by(|left, right| (&left.id, left.level).cmp(&(&right.id, right.level)));
        self.outbound_links.sort();
        self.outbound_links.dedup();
        self.backlinks.sort();
        self.backlinks.dedup();
        self.citations.sort_by_key(|citation| citation.number);
    }

    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Link {
    pub source_id: String,
    pub raw_href: String,
    pub normalized_target_id: Option<String>,
    pub fragment: Option<String>,
    pub status: LinkStatus,
    pub context: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    Resolved,
    Broken,
    External,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Citation {
    pub number: u32,
    pub text: String,
    pub href: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LogEntry {
    pub scope_path: String,
    pub date: String,
    pub category: String,
    pub text: String,
    pub links: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_projection_round_trips_deterministically() {
        let mut projection = WikiProjection::new("fixed", Some("abc123".into()));
        projection.bundles.push(Bundle {
            id: "zeta".into(),
            concepts: vec![Concept {
                id: "concept".into(),
                bundle_id: "zeta".into(),
                source_path: "concept.md".into(),
                concept_type: "decision".into(),
                tags: vec!["rust".into(), "architecture".into(), "rust".into()],
                ..Concept::default()
            }],
            ..Bundle::default()
        });
        projection.normalize();

        let first = serde_json::to_vec_pretty(&projection).expect("serialize projection");
        let decoded: WikiProjection =
            serde_json::from_slice(&first).expect("deserialize projection");
        let second = serde_json::to_vec_pretty(&decoded).expect("serialize projection again");

        assert_eq!(first, second);
        assert_eq!(
            decoded.bundles[0].concepts[0].tags,
            ["architecture", "rust"]
        );
    }

    #[test]
    fn diagnostic_serialization_never_requires_absolute_paths() {
        let diagnostic = Diagnostic::error(
            "invalid_frontmatter",
            "concepts/example.md",
            Some(3),
            "frontmatter is invalid",
        );
        let json = serde_json::to_string(&diagnostic).expect("serialize diagnostic");

        assert!(json.contains("concepts/example.md"));
        assert!(!json.contains(std::path::MAIN_SEPARATOR_STR.repeat(2).as_str()));
    }
}
