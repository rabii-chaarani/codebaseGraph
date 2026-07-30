use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use okf_wiki::{
    diagnostic::Diagnostic,
    model::{Bundle, Concept, Link, LinkStatus},
    projection::{
        BundleDependencyIndex, BundlePublication, CacheStatus, FailurePoint, ProjectionStore,
        ProjectionStoreError, PublishRequest,
    },
};

#[test]
fn publish_generation_is_atomic_and_retains_last_valid_projection_on_failure() {
    let temp = TestDir::new();
    let store = ProjectionStore::new(temp.path());
    let graph_state = temp.path().join(".codebaseGraph");
    fs::create_dir_all(&graph_state).expect("create graph state");
    fs::write(graph_state.join("manifest.json"), "keep-me").expect("seed graph state");

    let first = publish_bundle(
        &store,
        "alpha",
        "concepts/alpha.md",
        "Alpha Title",
        None,
        None,
    )
    .expect("publish first generation");
    assert!(first.published);

    let failure = store.publish(PublishRequest {
        token: store.begin_generation(),
        generated_at: "2026-07-30T00:01:00Z".into(),
        okf_version: "0.1".into(),
        source_revision: Some("rev-2".into()),
        build_duration_ms: 8,
        bundles: vec![bundle_publication(
            "alpha",
            "concepts/alpha.md",
            "Changed Title",
            Some("linked"),
            Some(b"search-v2".to_vec()),
        )],
        diagnostics: vec![Diagnostic::warning(
            "projection_warning",
            "concepts/alpha.md",
            Some(1),
            "warning",
        )],
        inject_failure: Some(FailurePoint::BeforePointerSwap),
    });
    assert!(matches!(
        failure,
        Err(ProjectionStoreError::FailureInjected(
            FailurePoint::BeforePointerSwap
        ))
    ));

    let manifest = store
        .load_manifest()
        .expect("load manifest")
        .expect("published manifest");
    assert_eq!(manifest.generation, first.manifest.generation);
    assert_eq!(
        fs::read_to_string(graph_state.join("manifest.json")).expect("read graph manifest"),
        "keep-me"
    );
}

#[test]
fn unchanged_bundle_rebuild_is_a_cache_hit() {
    let temp = TestDir::new();
    let store = ProjectionStore::new(temp.path());

    let first = publish_bundle(
        &store,
        "alpha",
        "concepts/alpha.md",
        "Alpha Title",
        Some("linked"),
        Some(b"search".to_vec()),
    )
    .expect("publish first generation");
    let second = store
        .publish(PublishRequest {
            token: store.begin_generation(),
            generated_at: "2026-07-30T00:02:00Z".into(),
            okf_version: "0.1".into(),
            source_revision: Some("rev-1".into()),
            build_duration_ms: 5,
            bundles: vec![bundle_publication(
                "alpha",
                "concepts/alpha.md",
                "Alpha Title",
                Some("linked"),
                Some(b"search".to_vec()),
            )],
            diagnostics: Vec::new(),
            inject_failure: None,
        })
        .expect("publish second generation");

    assert_eq!(second.cache_status, CacheStatus::Hit);
    assert!(!second.published);
    assert_eq!(second.manifest.generation, first.manifest.generation);
    assert!(store
        .is_cache_hit(
            "alpha",
            second
                .manifest
                .content_hashes
                .get("alpha")
                .expect("alpha content hash"),
        )
        .expect("cache hit"));
}

#[test]
fn newest_generation_wins_when_competing_builds_finish_out_of_order() {
    let temp = TestDir::new();
    let store = ProjectionStore::new(temp.path());

    let stale_token = store.begin_generation();
    let newest_token = store.begin_generation();

    let newest = store
        .publish(PublishRequest {
            token: newest_token,
            generated_at: "2026-07-30T00:03:00Z".into(),
            okf_version: "0.1".into(),
            source_revision: Some("rev-2".into()),
            build_duration_ms: 4,
            bundles: vec![bundle_publication(
                "alpha",
                "concepts/alpha.md",
                "Newest Title",
                None,
                None,
            )],
            diagnostics: Vec::new(),
            inject_failure: None,
        })
        .expect("publish newest generation");
    assert!(newest.published);

    let stale = store.publish(PublishRequest {
        token: stale_token,
        generated_at: "2026-07-30T00:03:01Z".into(),
        okf_version: "0.1".into(),
        source_revision: Some("rev-1".into()),
        build_duration_ms: 9,
        bundles: vec![bundle_publication(
            "alpha",
            "concepts/alpha.md",
            "Old Title",
            None,
            None,
        )],
        diagnostics: Vec::new(),
        inject_failure: None,
    });

    assert!(matches!(
        stale,
        Err(ProjectionStoreError::StaleGeneration {
            attempted: 1,
            latest: 2
        })
    ));

    let manifest = store
        .load_manifest()
        .expect("load manifest")
        .expect("published manifest");
    assert_eq!(manifest.generation.sequence, 2);
    let projection = fs::read_to_string(
        temp.path()
            .join(".kWiki")
            .join("generations")
            .join(&manifest.generation.generation_id)
            .join("projections/alpha.json"),
    )
    .expect("read published projection");
    assert!(projection.contains("Newest Title"));
    assert!(!projection.contains("Old Title"));
}

fn publish_bundle(
    store: &ProjectionStore,
    bundle_id: &str,
    source_path: &str,
    title: &str,
    outbound_target: Option<&str>,
    search: Option<Vec<u8>>,
) -> Result<okf_wiki::projection::PublishOutcome, ProjectionStoreError> {
    store.publish(PublishRequest {
        token: store.begin_generation(),
        generated_at: "2026-07-30T00:00:00Z".into(),
        okf_version: "0.1".into(),
        source_revision: Some("rev-1".into()),
        build_duration_ms: 6,
        bundles: vec![bundle_publication(
            bundle_id,
            source_path,
            title,
            outbound_target,
            search,
        )],
        diagnostics: Vec::new(),
        inject_failure: None,
    })
}

fn bundle_publication(
    bundle_id: &str,
    source_path: &str,
    title: &str,
    outbound_target: Option<&str>,
    search: Option<Vec<u8>>,
) -> BundlePublication {
    let mut bundle = Bundle {
        id: bundle_id.into(),
        root_path: format!("bundles/{bundle_id}"),
        okf_version: "0.1".into(),
        title: format!("{bundle_id} bundle"),
        source_revision: Some("rev-1".into()),
        ..Bundle::default()
    };
    bundle.concepts.push(Concept {
        id: "alpha".into(),
        bundle_id: bundle_id.into(),
        source_path: source_path.into(),
        concept_type: "decision".into(),
        title: Some(title.into()),
        body_markdown: "body".into(),
        outbound_links: outbound_target
            .into_iter()
            .map(|target| Link {
                source_id: "alpha".into(),
                raw_href: target.into(),
                normalized_target_id: Some(target.into()),
                fragment: None,
                status: LinkStatus::Resolved,
                context: None,
            })
            .collect(),
        ..Concept::default()
    });
    let dependency_index = BundleDependencyIndex::from_bundle(&bundle);

    BundlePublication {
        bundle,
        dependency_index,
        search_artifact: search,
    }
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-wiki-projection-{unique:x}"));
        fs::create_dir_all(&path).expect("create temp directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
