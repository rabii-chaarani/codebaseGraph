use std::path::{Path, PathBuf};

use k_wiki::{
    bundle::load_bundle,
    conformance::{validate_bundle, ConformanceProfile},
};
use serde_json::json;

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn codes_for<'a>(
    report: &'a k_wiki::conformance::ConformanceReport,
    source_path: &str,
) -> Vec<(&'a str, Option<usize>)> {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.source_path == source_path)
        .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.line))
        .collect()
}

#[test]
fn resp_173_evaluates_normative_and_recommended_profiles_separately() {
    let bundle = load_bundle(fixture_root("comprehensive")).expect("load comprehensive bundle");

    let consume = validate_bundle(&bundle, ConformanceProfile::Consume);
    let conformant = validate_bundle(&bundle, ConformanceProfile::Conformant);
    let recommended = validate_bundle(&bundle, ConformanceProfile::Recommended);

    assert!(consume.accepted);
    assert!(conformant.accepted);
    assert!(!recommended.accepted);
    assert!(recommended.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "recommended_okf_version_missing" && diagnostic.source_path == "index.md"
    }));
    assert!(recommended.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "recommended_description_missing"
            && diagnostic.source_path == "guides/overview.md"
    }));
}

#[test]
fn resp_174_reports_errors_for_missing_or_invalid_concept_frontmatter() {
    let bundle = load_bundle(fixture_root("malformed")).expect("load malformed bundle");
    let report = validate_bundle(&bundle, ConformanceProfile::Conformant);

    assert!(!report.accepted);
    assert!(
        codes_for(&report, "invalid-frontmatter.md").contains(&("invalid_frontmatter", Some(3)))
    );
    assert!(codes_for(&report, "missing-type.md").contains(&("missing_type", Some(1))));
    assert!(codes_for(&report, "no-frontmatter.md").contains(&("missing_frontmatter", Some(1))));
    assert!(codes_for(&report, "tagged-extension.md").contains(&("invalid_frontmatter", Some(5))));
}

#[test]
fn resp_175_retains_unknown_types_and_extensions_in_consume_mode() {
    let bundle = load_bundle(fixture_root("comprehensive")).expect("load comprehensive bundle");
    let report = validate_bundle(&bundle, ConformanceProfile::Consume);
    let entry = bundle
        .entries
        .iter()
        .find(|entry| entry.source_path == "guides/overview.md")
        .expect("find overview concept");
    let frontmatter = entry.frontmatter().expect("parse overview frontmatter");

    assert!(report.accepted);
    assert_eq!(
        frontmatter.fields.type_name.as_deref(),
        Some("experimental_note")
    );
    assert_eq!(
        frontmatter.extensions.get("extra"),
        Some(&json!({"color": "blue", "features": ["navigation", "search"]}))
    );
    assert!(!report.diagnostics.iter().any(|diagnostic| {
        diagnostic.source_path == "guides/overview.md"
            && (diagnostic.message.contains("unknown")
                || diagnostic.message.contains("experimental_note"))
    }));
}

#[test]
fn resp_176_validates_reserved_file_structures() {
    let bundle = load_bundle(fixture_root("malformed")).expect("load malformed bundle");
    let report = validate_bundle(&bundle, ConformanceProfile::Conformant);

    assert!(codes_for(&report, "nested/index.md").contains(&("reserved_frontmatter", Some(2))));
    assert!(codes_for(&report, "log.md").contains(&("reserved_frontmatter", Some(2))));
    assert!(
        codes_for(&report, "stray-version.md").contains(&("invalid_reserved_metadata", Some(5)))
    );
}
