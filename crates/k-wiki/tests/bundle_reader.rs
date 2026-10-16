use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::bundle::{discover_bundles, load_bundle, BundleEntryKind};
use serde_json::json;

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn bundle_entry<'a>(
    bundle: &'a k_wiki::bundle::LoadedBundle,
    source_path: &str,
) -> &'a k_wiki::bundle::BundleEntry {
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
    path.push(format!("k_wiki_{prefix}_{}_{}", std::process::id(), unique));
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

#[test]
fn unterminated_frontmatter_is_invalid_even_when_yaml_is_valid() {
    let root = make_temp_dir("unterminated_frontmatter");
    for (name, content) in [
        ("valid-yaml.md", "---\ntype: note\ntitle: Unclosed\n"),
        ("opening-only.md", "---"),
        ("opening-lf.md", "---\n"),
        ("opening-crlf.md", "---\r\n"),
    ] {
        fs::write(root.join(name), content).expect("write unterminated document");
    }
    let bundle = load_bundle(&root).expect("consume malformed documents");
    for entry in &bundle.entries {
        assert!(entry.document.has_frontmatter);
        assert_eq!(
            entry.document.frontmatter_state,
            k_wiki::bundle::FrontmatterState::Invalid
        );
        assert!(
            bundle.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "invalid_frontmatter"
                    && diagnostic.source_path == entry.source_path
                    && diagnostic.line == Some(1)
                    && diagnostic.message.contains("closing delimiter")
            }),
            "missing delimiter diagnostic for {}",
            entry.source_path
        );
    }
    assert_eq!(bundle.entries.len(), 4);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn closed_frontmatter_accepts_lf_crlf_and_closing_delimiter_at_eof() {
    let root = make_temp_dir("closed_frontmatter");
    for (name, content, body, line) in [
        ("lf.md", "---\ntype: note\n---\nBody\n", "Body\n", 4),
        (
            "crlf.md",
            "---\r\ntype: note\r\n---\r\nBody\r\n",
            "Body\r\n",
            4,
        ),
        ("eof.md", "---\ntype: note\n---", "", 4),
    ] {
        fs::write(root.join(name), content).expect("write closed document");
        let bundle = load_bundle(&root).expect("load closed documents");
        let entry = bundle_entry(&bundle, name);
        assert_eq!(
            entry.frontmatter().unwrap().fields.type_name.as_deref(),
            Some("note")
        );
        assert_eq!(entry.document.body_markdown, body);
        assert_eq!(entry.document.body_start_line, line);
        assert!(bundle.diagnostics.is_empty(), "{:?}", bundle.diagnostics);
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn parser_diagnostics_exclude_source_values_and_keep_relative_locations() {
    let root = make_temp_dir("safe_parser_diagnostics");
    fs::write(
        root.join("page.md"),
        "---\ntype: note\nPRIVATE_SOURCE_MARKER: 1\nPRIVATE_SOURCE_MARKER: 2\n---\n",
    )
    .expect("write invalid YAML");
    fs::write(
        root.join("tagged.md"),
        "---\ntype: note\ncustom: !PRIVATE_SOURCE_MARKER hidden\n---\n",
    )
    .expect("write tagged extension");
    fs::write(root.join("bytes.md"), [0xff, 0xfe]).expect("write invalid UTF-8");
    let bundle = load_bundle(&root).expect("load malformed documents");
    assert_eq!(bundle.diagnostics.len(), 3);
    for diagnostic in &bundle.diagnostics {
        assert!(
            !diagnostic.message.contains("PRIVATE_SOURCE_MARKER"),
            "{:?}",
            diagnostic
        );
        assert!(!diagnostic.message.contains(root.to_str().unwrap()));
        assert!(!Path::new(&diagnostic.source_path).is_absolute());
        if diagnostic.source_path == "bytes.md" {
            assert_eq!(diagnostic.code, "invalid_utf8");
        } else {
            assert_eq!(diagnostic.code, "invalid_frontmatter");
            assert!(diagnostic.line.is_some());
            assert!(diagnostic.message.contains("YAML"));
        }
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

#[cfg(unix)]
#[test]
fn walk_errors_keep_relative_location_and_safe_io_category() {
    use std::os::unix::fs::PermissionsExt;

    struct RestorePermissions(PathBuf, fs::Permissions);
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            fs::set_permissions(&self.0, self.1.clone()).expect("restore directory permissions");
        }
    }

    let root = make_temp_dir("safe_walk_diagnostics");
    let blocked = root.join("blocked");
    fs::create_dir(&blocked).expect("create inaccessible directory");
    let restore = RestorePermissions(
        blocked.clone(),
        fs::metadata(&blocked).unwrap().permissions(),
    );
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000))
        .expect("deny directory access");
    // Elevated test processes can bypass Unix mode bits, so exercise the boundary only
    // when the platform actually denies this isolated fixture's directory read.
    if fs::read_dir(&blocked).is_err() {
        let bundle = load_bundle(&root).expect("load readable bundle root");
        let diagnostic = bundle
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "io_read_failed")
            .expect("walk error diagnostic");
        assert_eq!(diagnostic.source_path, "blocked");
        assert!(
            diagnostic.message.contains("permission denied"),
            "{:?}",
            diagnostic
        );
        assert!(
            !diagnostic.message.contains(root.to_str().unwrap()),
            "{:?}",
            diagnostic
        );
        assert!(!diagnostic.message.contains("os error"));
    }
    drop(restore);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn invalid_bundle_root_error_omits_absolute_host_path() {
    let root = make_temp_dir("PRIVATE_HOST_MARKER");
    let file = root.join("file.md");
    fs::write(&file, "body").expect("write non-directory root");
    let error = load_bundle(&file).expect_err("bundle root must be a directory");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("not a directory"));
    assert!(!error.to_string().contains("PRIVATE_HOST_MARKER"));
    fs::remove_dir_all(root).expect("remove fixture");
}
