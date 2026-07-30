//! OKF conformance profiles and validation.

use crate::{
    bundle::{BundleEntryKind, FrontmatterState, LoadedBundle},
    diagnostic::{Diagnostic, DiagnosticSeverity},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceProfile {
    Consume,
    Conformant,
    Recommended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceReport {
    pub profile: ConformanceProfile,
    pub accepted: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl ConformanceReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    }
}

pub fn validate_bundle(bundle: &LoadedBundle, profile: ConformanceProfile) -> ConformanceReport {
    let mut diagnostics = bundle.diagnostics.clone();

    for entry in &bundle.entries {
        match entry.kind {
            BundleEntryKind::Concept => validate_concept(entry, &mut diagnostics),
            BundleEntryKind::Index => validate_index(entry, &mut diagnostics),
            BundleEntryKind::Log => validate_log(entry, &mut diagnostics),
        }
    }

    apply_recommended_guidance(bundle, &mut diagnostics);
    diagnostics.sort_by(diagnostic_order);

    let accepted = match profile {
        ConformanceProfile::Consume => true,
        ConformanceProfile::Conformant => diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error),
        ConformanceProfile::Recommended => diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == DiagnosticSeverity::Info),
    };

    ConformanceReport {
        profile,
        accepted,
        diagnostics,
    }
}

fn validate_concept(entry: &crate::bundle::BundleEntry, diagnostics: &mut Vec<Diagnostic>) {
    if !entry.document.has_frontmatter {
        diagnostics.push(Diagnostic::error(
            "missing_frontmatter",
            entry.source_path.clone(),
            Some(1),
            "concept documents require YAML frontmatter",
        ));
        return;
    }

    let Some(frontmatter) = entry.frontmatter() else {
        return;
    };

    if frontmatter.fields.type_name.is_none() {
        diagnostics.push(Diagnostic::error(
            "missing_type",
            entry.source_path.clone(),
            frontmatter.line_for("type").or(Some(1)),
            "concept frontmatter requires a non-empty 'type' field",
        ));
    }

    if frontmatter.fields.okf_version.is_some() {
        diagnostics.push(Diagnostic::error(
            "invalid_reserved_metadata",
            entry.source_path.clone(),
            frontmatter.line_for("okf_version"),
            "only the bundle-root index.md may declare 'okf_version'",
        ));
    }
}

fn validate_index(entry: &crate::bundle::BundleEntry, diagnostics: &mut Vec<Diagnostic>) {
    if entry.is_root_index() {
        return;
    }

    if entry.document.has_frontmatter {
        diagnostics.push(Diagnostic::error(
            "reserved_frontmatter",
            entry.source_path.clone(),
            entry
                .frontmatter()
                .and_then(|frontmatter| frontmatter.first_key_line()),
            "nested index.md files must not declare frontmatter",
        ));
    }
}

fn validate_log(entry: &crate::bundle::BundleEntry, diagnostics: &mut Vec<Diagnostic>) {
    if entry.document.has_frontmatter {
        diagnostics.push(Diagnostic::error(
            "reserved_frontmatter",
            entry.source_path.clone(),
            entry
                .frontmatter()
                .and_then(|frontmatter| frontmatter.first_key_line()),
            "log.md files must not declare frontmatter",
        ));
    }
}

fn apply_recommended_guidance(bundle: &LoadedBundle, diagnostics: &mut Vec<Diagnostic>) {
    for entry in &bundle.entries {
        match entry.kind {
            BundleEntryKind::Concept => {
                let Some(frontmatter) = entry.frontmatter() else {
                    continue;
                };
                if frontmatter.fields.title.is_none() {
                    diagnostics.push(Diagnostic::warning(
                        "recommended_title_missing",
                        entry.source_path.clone(),
                        frontmatter.line_for("type").or(Some(1)),
                        "recommended profile expects concept documents to declare a title",
                    ));
                }
                if frontmatter.fields.description.is_none() {
                    diagnostics.push(Diagnostic::warning(
                        "recommended_description_missing",
                        entry.source_path.clone(),
                        frontmatter.line_for("type").or(Some(1)),
                        "recommended profile expects concept documents to declare a description",
                    ));
                }
            }
            BundleEntryKind::Index if entry.is_root_index() => {
                let okf_version = match &entry.document.frontmatter_state {
                    FrontmatterState::Parsed(frontmatter) => frontmatter.fields.okf_version.clone(),
                    FrontmatterState::Absent | FrontmatterState::Invalid => None,
                };
                if okf_version.is_none() {
                    diagnostics.push(Diagnostic::warning(
                        "recommended_okf_version_missing",
                        entry.source_path.clone(),
                        Some(1),
                        "recommended profile expects bundle-root index.md to declare okf_version",
                    ));
                }
            }
            BundleEntryKind::Index | BundleEntryKind::Log => {}
        }
    }

    if bundle.entries.iter().all(|entry| !entry.is_root_index()) {
        diagnostics.push(Diagnostic::warning(
            "recommended_okf_version_missing",
            "index.md",
            Some(1),
            "recommended profile expects bundle-root index.md to declare okf_version",
        ));
    }
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
