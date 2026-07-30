use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use okf_wiki::bundle::{discover_bundles, load_bundle, BundleEntryKind};
use serde_json::json;

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn bundle_entry<'a>(
    bundle: &'a okf_wiki::bundle::LoadedBundle,
    source_path: &str,
) -> &'a okf_wiki::bundle::BundleEntry {
    bundle
        .entries
        .iter()
        .find(|entry| entry.source_path == source_path)
        .unwrap_or_else(|| panic!("missing bundle entry: {source_path}"))
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    path.push(format!(
        "okf_wiki_{prefix}_{}_{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create destination directory");
    for entry in fs::read_dir(source).expect("read fixture directory") {
        let entry = entry.expect("read fixture entry");
        let file_type = entry.file_type().expect("read fixture file type");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

#[test]
fn resp_168_discovers_bundle_boundaries_and_metadata() {
    let bundles = discover_bundles([
        fixture_root("multi_bundle/alpha"),
        fixture_root("multi_bundle/beta"),
    ])
    .expect("discover bundle roots");

    let ids = bundles
        .iter()
        .map(|bundle| bundle.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["alpha", "beta"]);
    assert_eq!(bundles[0].okf_version.as_deref(), Some("0.1"));
    assert_eq!(bundles[1].okf_version.as_deref(), Some("0.1"));
    assert!(bundles[0]
        .entries
        .iter()
        .any(|entry| entry.source_path == "page.md"));
    assert!(bundles[1]
        .entries
        .iter()
        .any(|entry| entry.source_path == "page.md"));
}

#[test]
fn resp_169_parses_standard_fields_and_preserves_extensions() {
    let bundle = load_bundle(fixture_root("minimal")).expect("load minimal bundle");
    let entry = bundle_entry(&bundle, "decisions/adr-001.md");
    let frontmatter = entry.frontmatter().expect("parse concept frontmatter");

    assert_eq!(entry.kind, BundleEntryKind::Concept);
    assert_eq!(frontmatter.fields.type_name.as_deref(), Some("decision"));
    assert_eq!(
        frontmatter.fields.title.as_deref(),
        Some("Keep the parser small")
    );
    assert_eq!(
        frontmatter.fields.description.as_deref(),
        Some("We prefer a focused reader.")
    );
    assert_eq!(
        frontmatter.fields.resource.as_deref(),
        Some("https://example.test/adr-001")
    );
    assert_eq!(
        frontmatter.fields.timestamp.as_deref(),
        Some("2026-07-30T12:00:00Z")
    );
    assert_eq!(frontmatter.fields.tags, vec!["rust", "okf"]);
    assert_eq!(
        frontmatter.extensions.get("custom"),
        Some(&json!({"owner": "docs", "reviewers": ["alice", "bob"]}))
    );
    assert_eq!(frontmatter.extensions.get("score"), Some(&json!(7)));
}

#[test]
fn resp_170_classifies_reserved_files_without_promoting_them_to_concepts() {
    let bundle = load_bundle(fixture_root("comprehensive")).expect("load comprehensive bundle");
    let concept_paths = bundle
        .entries
        .iter()
        .filter(|entry| entry.kind == BundleEntryKind::Concept)
        .map(|entry| entry.source_path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        bundle_entry(&bundle, "guides/index.md").kind,
        BundleEntryKind::Index
    );
    assert_eq!(
        bundle_entry(&bundle, "guides/log.md").kind,
        BundleEntryKind::Log
    );
    assert_eq!(
        bundle_entry(&bundle, "guides/overview.md").kind,
        BundleEntryKind::Concept
    );
    assert_eq!(
        concept_paths,
        vec!["decisions/adr-002.md", "guides/overview.md"]
    );
}

#[test]
fn resp_171_records_root_okf_version_from_bundle_index() {
    let bundle = load_bundle(fixture_root("minimal")).expect("load minimal bundle");

    assert_eq!(bundle.okf_version.as_deref(), Some("0.1"));
    assert_eq!(bundle.title.as_deref(), Some("Minimal Bundle"));
    assert_eq!(
        bundle_entry(&bundle, "index.md").kind,
        BundleEntryKind::Index
    );
}

#[test]
fn resp_172_rejects_markdown_entries_that_escape_the_bundle_root() {
    #[cfg(not(unix))]
    {
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp_root = make_temp_dir("escape");
        let bundle_root = temp_root.join("bundle");
        copy_dir_all(&fixture_root("minimal"), &bundle_root);

        let outside_file = temp_root.join("outside.md");
        fs::write(
            &outside_file,
            "---\ntype: note\ntitle: Outside\n---\nThis file is outside the bundle.\n",
        )
        .expect("write outside file");
        symlink(&outside_file, bundle_root.join("escape.md")).expect("create symlink");

        let bundle = load_bundle(&bundle_root).expect("load copied bundle");
        assert!(!bundle
            .entries
            .iter()
            .any(|entry| entry.source_path == "escape.md"));
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "path_outside_bundle_root" && diagnostic.source_path == "escape.md"
        }));

        fs::remove_dir_all(temp_root).expect("remove temp fixture");
    }
}
