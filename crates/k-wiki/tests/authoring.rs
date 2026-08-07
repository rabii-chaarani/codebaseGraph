use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::authoring::{
    AuthoringConfig, AuthoringError, AuthoringService, AuthoringValidator, BundleRoot,
    ConformanceAuthoringValidator, CreateBundleRequest, CreatePageRequest, PageFrontmatter,
    PopulatePageRequest, RefreshEvent, RefreshNotifier, RefreshOperation, RepositoryRoot,
    ValidationTarget, ValidationTargetKind,
};
use yaml_serde::Value as YamlValue;

#[derive(Clone, Default)]
struct RecordingNotifier {
    events: Arc<Mutex<Vec<RefreshEvent>>>,
}

impl RecordingNotifier {
    fn events(&self) -> Vec<RefreshEvent> {
        self.events.lock().expect("events lock").clone()
    }
}

impl RefreshNotifier for RecordingNotifier {
    fn notify(&self, event: &RefreshEvent) {
        self.events.lock().expect("events lock").push(event.clone());
    }
}

#[derive(Clone, Default)]
struct GateValidator {
    reject_fragment: Option<String>,
}

impl AuthoringValidator for GateValidator {
    fn validate(&self, request: ValidationTarget<'_>) -> Result<(), AuthoringError> {
        if let Some(fragment) = self.reject_fragment.as_deref() {
            if request.content.contains(fragment) {
                return Err(AuthoringError::invalid_frontmatter(format!(
                    "validator rejected `{}`",
                    request.source_path
                )));
            }
        }
        Ok(())
    }
}

#[test]
fn production_authoring_validator_enforces_required_frontmatter() {
    let validator = ConformanceAuthoringValidator;
    let missing_type = validator
        .validate(ValidationTarget {
            kind: ValidationTargetKind::ConceptPage,
            bundle_id: "docs",
            source_path: "guides/start.md",
            content: "---\ntitle: Start\n---\n# Start\n",
        })
        .expect_err("concept type should be required");
    assert_eq!(missing_type.code(), "invalid_frontmatter");

    validator
        .validate(ValidationTarget {
            kind: ValidationTargetKind::ConceptPage,
            bundle_id: "docs",
            source_path: "guides/start.md",
            content: "---\ntype: guide\ntitle: Start\n---\n# Start\n",
        })
        .expect("valid concept");
}

#[test]
fn create_bundle_initializes_a_conformant_root_index_within_a_permitted_repository() {
    let root = unique_temp_dir("k-wiki-authoring-create-bundle");
    fs::create_dir_all(&root).expect("create repository root");
    let notifier = RecordingNotifier::default();
    let service = service_with(
        root.clone(),
        Vec::new(),
        GateValidator::default(),
        notifier.clone(),
    );

    let result = service
        .create_bundle(CreateBundleRequest {
            bundle_id: "docs".into(),
            repository_id: "repo".into(),
            bundle_path: "knowledge/team-wiki".into(),
            okf_version: "0.1".into(),
            title: Some("Team Wiki".into()),
            body_markdown: Some("Overview.\n".into()),
        })
        .expect("create bundle");

    assert_eq!(result.bundle_path, "knowledge/team-wiki");
    let index_path = root.join("knowledge/team-wiki/index.md");
    let content = fs::read_to_string(&index_path).expect("read root index");
    assert!(content.contains("okf_version:"));
    assert!(content.contains("0.1"));
    assert!(content.contains("title: Team Wiki"));
    assert!(content.ends_with("Overview.\n"));
    assert_eq!(
        notifier.events(),
        vec![RefreshEvent {
            operation: RefreshOperation::BundleCreated,
            bundle_id: "docs".into(),
            source_path: "index.md".into(),
        }]
    );
}

#[test]
fn create_bundle_fails_if_the_target_already_exists_without_modifying_it() {
    let root = unique_temp_dir("k-wiki-authoring-bundle-exists");
    let existing = root.join("knowledge/team-wiki");
    fs::create_dir_all(&existing).expect("create existing bundle");
    let service = service_with(
        root,
        Vec::new(),
        GateValidator::default(),
        RecordingNotifier::default(),
    );

    let error = service
        .create_bundle(CreateBundleRequest {
            bundle_id: "docs".into(),
            repository_id: "repo".into(),
            bundle_path: "knowledge/team-wiki".into(),
            okf_version: "0.1".into(),
            title: None,
            body_markdown: None,
        })
        .expect_err("bundle should already exist");

    assert_eq!(error.code(), "bundle_exists");
    assert!(!existing.join("index.md").exists());
}

#[test]
fn create_page_creates_one_valid_concept_at_a_validated_bundle_relative_identity() {
    let root = seeded_bundle_root("k-wiki-authoring-create-page");
    let service = service_with_existing_bundle(&root, "docs");

    let created = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "guides/release-plan".into(),
            concept_type: "decision".into(),
            title: Some("Release Plan".into()),
            description: None,
            resource: None,
            tags: vec!["release".into()],
            timestamp: None,
            extensions: Default::default(),
            body_markdown: None,
        })
        .expect("create page");

    assert_eq!(created.source_path, "guides/release-plan.md");
    let content =
        fs::read_to_string(root.join("bundle/guides/release-plan.md")).expect("read page");
    assert!(content.contains("type: decision"));
    assert!(content.contains("title: Release Plan"));
    assert!(content.contains("- release"));
    assert!(content.contains("# Release Plan"));
}

#[test]
fn create_page_rejects_reserved_escape_and_absolute_targets() {
    let root = seeded_bundle_root("k-wiki-authoring-path-rejection");
    let service = service_with_existing_bundle(&root, "docs");

    let reserved = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/index.md".into(),
            concept_type: "note".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("ignored".into()),
        })
        .expect_err("reserved target must fail");
    assert_eq!(reserved.code(), "invalid_request");

    let traversal = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "../escape".into(),
            concept_type: "note".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("ignored".into()),
        })
        .expect_err("traversal target must fail");
    assert_eq!(traversal.code(), "path_outside_repository");

    let absolute = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: root.join("bundle/outside.md").to_string_lossy().to_string(),
            concept_type: "note".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("ignored".into()),
        })
        .expect_err("absolute target must fail");
    assert_eq!(absolute.code(), "path_outside_repository");
}

#[cfg(unix)]
#[test]
fn create_page_rejects_symlink_paths_that_escape_the_bundle_root() {
    use std::os::unix::fs::symlink;

    let root = seeded_bundle_root("k-wiki-authoring-symlink-rejection");
    let outside = unique_temp_dir("k-wiki-authoring-symlink-outside");
    fs::create_dir_all(&outside).expect("create outside directory");
    symlink(&outside, root.join("bundle/link")).expect("create symlink");
    let service = service_with_existing_bundle(&root, "docs");

    let error = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "link/escape".into(),
            concept_type: "note".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("ignored".into()),
        })
        .expect_err("escaping symlink must fail");

    assert_eq!(error.code(), "path_outside_repository");
}

#[test]
fn populate_page_writes_validated_frontmatter_and_markdown_atomically() {
    let root = seeded_bundle_root("k-wiki-authoring-populate");
    let notifier = RecordingNotifier::default();
    let service = service_with_existing_bundle_and_notifier(
        &root,
        "docs",
        GateValidator::default(),
        notifier.clone(),
    );

    service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-1".into(),
            concept_type: "decision".into(),
            title: Some("ADR 1".into()),
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("Initial body.\n".into()),
        })
        .expect("seed page");

    let result = service
        .populate_page(PopulatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-1".into(),
            frontmatter: PageFrontmatter {
                concept_type: "decision".into(),
                title: Some("ADR 1".into()),
                description: Some("Accepted decision".into()),
                resource: None,
                tags: vec!["architecture".into(), "accepted".into()],
                timestamp: Some("2026-07-30".into()),
                extensions: Default::default(),
            },
            body_markdown: "Updated body.\n".into(),
            expected_content_hash: None,
        })
        .expect("populate page");

    let content = fs::read_to_string(root.join("bundle/notes/adr-1.md")).expect("read page");
    assert_eq!(result.source_path, "notes/adr-1.md");
    assert!(content.contains("description: Accepted decision"));
    assert!(content.contains("timestamp: 2026-07-30"));
    assert!(content.contains("- accepted"));
    assert!(content.contains("Updated body.\n"));
    assert_eq!(
        notifier.events().last(),
        Some(&RefreshEvent {
            operation: RefreshOperation::PagePopulated,
            bundle_id: "docs".into(),
            source_path: "notes/adr-1.md".into(),
        })
    );
}

#[test]
fn populate_page_rejects_stale_writes_without_changing_source_content() {
    let root = seeded_bundle_root("k-wiki-authoring-write-conflict");
    let service = service_with_existing_bundle(&root, "docs");

    let created = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-2".into(),
            concept_type: "decision".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("Original.\n".into()),
        })
        .expect("seed page");

    let page_path = root.join("bundle/notes/adr-2.md");
    fs::write(
        &page_path,
        "---\ntype: decision\n---\nChanged by another writer.\n",
    )
    .expect("mutate source");

    let error = service
        .populate_page(PopulatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-2".into(),
            frontmatter: PageFrontmatter {
                concept_type: "decision".into(),
                title: None,
                description: None,
                resource: None,
                tags: Vec::new(),
                timestamp: None,
                extensions: Default::default(),
            },
            body_markdown: "New body.\n".into(),
            expected_content_hash: Some(created.content_hash),
        })
        .expect_err("stale writes must fail");

    assert_eq!(error.code(), "write_conflict");
    assert_eq!(
        fs::read_to_string(&page_path).expect("read unchanged source"),
        "---\ntype: decision\n---\nChanged by another writer.\n"
    );
}

#[test]
fn populate_page_preserves_unknown_frontmatter_fields_during_updates() {
    let root = seeded_bundle_root("k-wiki-authoring-preserve-extensions");
    let service = service_with_existing_bundle(&root, "docs");
    let page_path = root.join("bundle/notes/adr-3.md");
    write_file(
        &page_path,
        "---\ntype: decision\nowner: platform\nmetadata:\n  severity: high\n---\nInitial body.\n",
    );

    service
        .populate_page(PopulatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-3".into(),
            frontmatter: PageFrontmatter {
                concept_type: "decision".into(),
                title: Some("ADR 3".into()),
                description: None,
                resource: None,
                tags: vec!["platform".into()],
                timestamp: None,
                extensions: Default::default(),
            },
            body_markdown: "Updated body.\n".into(),
            expected_content_hash: None,
        })
        .expect("populate page");

    let content = fs::read_to_string(&page_path).expect("read updated page");
    assert!(content.contains("owner: platform"));
    assert!(content.contains("severity: high"));
    assert!(content.contains("title: ADR 3"));
    assert!(content.contains("Updated body.\n"));
}

#[test]
fn populate_page_validates_before_replacing_the_destination() {
    let root = seeded_bundle_root("k-wiki-authoring-validate-before-write");
    let service = service_with_existing_bundle_and_notifier(
        &root,
        "docs",
        GateValidator {
            reject_fragment: Some("forbidden".into()),
        },
        RecordingNotifier::default(),
    );

    let created = service
        .create_page(CreatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-4".into(),
            concept_type: "decision".into(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            extensions: Default::default(),
            body_markdown: Some("Original body.\n".into()),
        })
        .expect("seed page");

    let error = service
        .populate_page(PopulatePageRequest {
            bundle_id: "docs".into(),
            page_path: "notes/adr-4".into(),
            frontmatter: PageFrontmatter {
                concept_type: "decision".into(),
                title: None,
                description: None,
                resource: None,
                tags: Vec::new(),
                timestamp: None,
                extensions: [("custom".into(), YamlValue::String("forbidden".into()))]
                    .into_iter()
                    .collect(),
            },
            body_markdown: "forbidden\n".into(),
            expected_content_hash: Some(created.content_hash),
        })
        .expect_err("validator must reject");

    assert_eq!(error.code(), "invalid_frontmatter");
    assert_eq!(
        fs::read_to_string(root.join("bundle/notes/adr-4.md")).expect("read unchanged page"),
        "---\ntype: decision\n---\nOriginal body.\n"
    );
    let leaked_temps = fs::read_dir(root.join("bundle/notes"))
        .expect("read notes directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        .count();
    assert_eq!(leaked_temps, 0);
}

fn service_with_existing_bundle(
    root: &Path,
    bundle_id: &str,
) -> AuthoringService<GateValidator, RecordingNotifier> {
    service_with_existing_bundle_and_notifier(
        root,
        bundle_id,
        GateValidator::default(),
        RecordingNotifier::default(),
    )
}

fn service_with_existing_bundle_and_notifier(
    root: &Path,
    bundle_id: &str,
    validator: GateValidator,
    notifier: RecordingNotifier,
) -> AuthoringService<GateValidator, RecordingNotifier> {
    service_with(
        root.to_path_buf(),
        vec![BundleRoot {
            id: bundle_id.into(),
            repository_id: "repo".into(),
            root_path: root.join("bundle"),
        }],
        validator,
        notifier,
    )
}

fn service_with(
    root: PathBuf,
    bundles: Vec<BundleRoot>,
    validator: GateValidator,
    notifier: RecordingNotifier,
) -> AuthoringService<GateValidator, RecordingNotifier> {
    AuthoringService::new(
        AuthoringConfig {
            repositories: vec![RepositoryRoot {
                id: "repo".into(),
                root_path: root,
            }],
            bundles,
        },
        validator,
        notifier,
    )
    .expect("authoring service")
}

fn seeded_bundle_root(prefix: &str) -> PathBuf {
    let root = unique_temp_dir(prefix);
    let bundle_root = root.join("bundle");
    fs::create_dir_all(&bundle_root).expect("create bundle root");
    write_file(
        &bundle_root.join("index.md"),
        "---\nokf_version: 0.1\n---\nBundle home.\n",
    );
    root
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nonce}"))
}
