use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    api::{OkfWikiApi, WikiOperationRequest, WikiOperationResponse},
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, NoopRefreshNotifier, NoopValidator,
        RepositoryRoot,
    },
    memory::{
        MemoryKind, MemorySource, MemoryStatus, RecallMemoryRequest, RecordMemoryRequest,
        TransitionMemoryRequest,
    },
    service::LocalWikiService,
};

#[test]
fn record_memory_persists_candidate_with_structured_provenance() {
    let fixture = MemoryFixture::new("record");

    let recorded = fixture.record(record_request("build-contract", MemoryKind::Semantic));

    assert_eq!(recorded.status, MemoryStatus::Candidate);
    assert_eq!(recorded.owner, "repository-agent");
    assert_eq!(recorded.sources[0].kind, "repository");
    assert_eq!(recorded.sources[0].reference, "docs/build.md");
    let source = fs::read_to_string(fixture.bundle.join("memory/semantic/build-contract.md"))
        .expect("read recorded memory");
    assert!(source.contains("type: agent-memory"));
    assert!(source.contains("status: candidate"));
    assert!(source.contains("reference: docs/build.md"));
}

#[test]
fn recall_returns_only_valid_active_memory_in_requested_bundle() {
    let fixture = MemoryFixture::new("recall-active");
    fixture.record(record_request("build-contract", MemoryKind::Semantic));

    assert!(fixture.recall("release").is_empty());

    fixture.transition("build-contract", MemoryStatus::Active, None);
    let recalled = fixture.recall("release");

    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].memory_id, "build-contract");
    assert_eq!(recalled[0].status, MemoryStatus::Active);
    assert!(recalled[0].body_markdown.contains("release checks"));
}

#[test]
fn invalid_transition_rejects_without_changing_source() {
    let fixture = MemoryFixture::new("invalid-transition");
    fixture.record(record_request("build-contract", MemoryKind::Semantic));
    let path = fixture.bundle.join("memory/semantic/build-contract.md");
    let before = fs::read(&path).expect("read candidate memory");

    let error = fixture
        .api
        .execute_operation(&WikiOperationRequest::TransitionMemory(
            TransitionMemoryRequest {
                bundle_id: "docs".into(),
                memory_id: "build-contract".into(),
                to_status: MemoryStatus::Superseded,
                actor: "reviewer".into(),
                transitioned_at: "2026-08-07T10:00:00Z".into(),
                reason: "skip activation".into(),
                replacement_id: None,
            },
        ))
        .expect_err("candidate cannot transition directly to superseded");

    assert_eq!(error.code, "invalid_memory_transition");
    assert_eq!(fs::read(path).expect("read unchanged candidate"), before);
}

#[test]
fn default_recall_excludes_malformed_and_quarantined_memory() {
    let fixture = MemoryFixture::new("recall-safety");
    fixture.record(record_request("unsafe", MemoryKind::Procedural));
    fixture.transition("unsafe", MemoryStatus::Active, None);
    fixture.transition("unsafe", MemoryStatus::Quarantined, None);

    let malformed_dir = fixture.bundle.join("memory/semantic");
    fs::create_dir_all(&malformed_dir).expect("create malformed memory directory");
    fs::write(
        malformed_dir.join("malformed.md"),
        "---\ntype: agent-memory\ntitle: Malformed Memory\n---\nrelease checks\n",
    )
    .expect("write malformed memory");

    assert!(fixture.recall("release").is_empty());
}

#[test]
fn supersession_preserves_prior_record_for_audit() {
    let fixture = MemoryFixture::new("supersession");
    fixture.record(record_request("old-contract", MemoryKind::Semantic));
    fixture.transition("old-contract", MemoryStatus::Active, None);

    let mut replacement = record_request("new-contract", MemoryKind::Semantic);
    replacement.supersedes = vec!["old-contract".into()];
    replacement.body_markdown = "Use the new release checks.".into();
    fixture.record(replacement);
    fixture.transition("new-contract", MemoryStatus::Active, None);
    let superseded = fixture.transition(
        "old-contract",
        MemoryStatus::Superseded,
        Some("new-contract"),
    );

    assert_eq!(superseded.status, MemoryStatus::Superseded);
    assert_eq!(superseded.superseded_by.as_deref(), Some("new-contract"));
    assert!(fixture
        .bundle
        .join("memory/semantic/old-contract.md")
        .is_file());
    let recalled = fixture.recall("release checks");
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].memory_id, "new-contract");
}

#[test]
fn malformed_memory_is_advisory_outside_the_recommended_profile() {
    let fixture = MemoryFixture::new("recommended-validation");
    let memory_dir = fixture.bundle.join("memory/semantic");
    fs::create_dir_all(&memory_dir).expect("create memory directory");
    fs::write(
        memory_dir.join("malformed.md"),
        "---\ntype: agent-memory\ntitle: Malformed\ndescription: Missing metadata\n---\nBody.\n",
    )
    .expect("write malformed memory");
    let bundle = k_wiki::bundle::load_bundle(&fixture.bundle).expect("load bundle");

    let consume = k_wiki::conformance::validate_bundle(
        &bundle,
        k_wiki::conformance::ConformanceProfile::Consume,
    );
    let conformant = k_wiki::conformance::validate_bundle(
        &bundle,
        k_wiki::conformance::ConformanceProfile::Conformant,
    );
    let recommended = k_wiki::conformance::validate_bundle(
        &bundle,
        k_wiki::conformance::ConformanceProfile::Recommended,
    );

    assert!(consume.accepted);
    assert!(conformant.accepted);
    assert!(!recommended.accepted);
    assert!(recommended
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "recommended_agent_memory_invalid"));
}

struct MemoryFixture {
    _root: PathBuf,
    bundle: PathBuf,
    api: OkfWikiApi<LocalWikiService>,
}

impl MemoryFixture {
    fn new(label: &str) -> Self {
        let root = unique_temp_dir(label);
        let repository = root.join("repository");
        let bundle = repository.join("docs");
        fs::create_dir_all(&bundle).expect("create bundle");
        fs::write(
            bundle.join("index.md"),
            "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
        )
        .expect("write root index");
        let authoring = AuthoringService::new(
            AuthoringConfig {
                repositories: vec![RepositoryRoot {
                    id: "repo".into(),
                    root_path: repository,
                }],
                bundles: vec![BundleRoot {
                    id: "docs".into(),
                    repository_id: "repo".into(),
                    root_path: bundle.clone(),
                }],
            },
            NoopValidator,
            NoopRefreshNotifier,
        )
        .expect("configure authoring");
        let api = LocalWikiService::new(vec![bundle.clone()])
            .with_authoring(authoring)
            .into_api();
        Self {
            _root: root,
            bundle,
            api,
        }
    }

    fn record(&self, request: RecordMemoryRequest) -> k_wiki::memory::MemoryRecord {
        match self
            .api
            .execute_operation(&WikiOperationRequest::RecordMemory(request))
            .expect("record memory")
        {
            WikiOperationResponse::MemoryRecorded(record) => record,
            response => panic!("unexpected response: {response:?}"),
        }
    }

    fn transition(
        &self,
        memory_id: &str,
        to_status: MemoryStatus,
        replacement_id: Option<&str>,
    ) -> k_wiki::memory::MemoryRecord {
        match self
            .api
            .execute_operation(&WikiOperationRequest::TransitionMemory(
                TransitionMemoryRequest {
                    bundle_id: "docs".into(),
                    memory_id: memory_id.into(),
                    to_status,
                    actor: "reviewer".into(),
                    transitioned_at: "2026-08-07T10:00:00Z".into(),
                    reason: "reviewed against repository evidence".into(),
                    replacement_id: replacement_id.map(str::to_string),
                },
            ))
            .expect("transition memory")
        {
            WikiOperationResponse::MemoryTransitioned(record) => record,
            response => panic!("unexpected response: {response:?}"),
        }
    }

    fn recall(&self, text: &str) -> Vec<k_wiki::memory::MemoryRecallResult> {
        match self
            .api
            .execute_operation(&WikiOperationRequest::RecallMemory(RecallMemoryRequest {
                bundle_id: "docs".into(),
                text: text.into(),
                kinds: Vec::new(),
                limit: 20,
            }))
            .expect("recall memory")
        {
            WikiOperationResponse::MemoryRecalled(records) => records,
            response => panic!("unexpected response: {response:?}"),
        }
    }
}

fn record_request(memory_id: &str, kind: MemoryKind) -> RecordMemoryRequest {
    RecordMemoryRequest {
        bundle_id: "docs".into(),
        memory_id: memory_id.into(),
        kind,
        title: "Build Contract".into(),
        description: Some("Repository release procedure".into()),
        body_markdown: "Run the release checks before publishing.".into(),
        owner: "repository-agent".into(),
        created_at: "2026-08-07T09:00:00Z".into(),
        sources: vec![MemorySource {
            kind: "repository".into(),
            reference: "docs/build.md".into(),
            content_hash: Some("sha256:abc123".into()),
        }],
        tags: vec!["release".into()],
        review_after: Some("2026-09-07T09:00:00Z".into()),
        supersedes: Vec::new(),
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "k-wiki-memory-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp directory");
    path
}
