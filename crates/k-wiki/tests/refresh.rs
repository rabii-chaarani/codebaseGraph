use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    diagnostic::Diagnostic,
    model::{Bundle, Concept, Link, LinkStatus},
    projection::BundleDependencyIndex,
    refresh::{
        ConsumerRefreshStatus, CoordinatedRefresh, GraphRefreshConsumer, RefreshCompletion,
        RefreshCoordinator, RefreshFailure, RepositoryChange, RepositoryEvent, WatchedBundle,
        WikiRefreshConsumer,
    },
};

#[test]
fn coalesces_duplicate_paths_and_produces_deterministic_batches() {
    let bundle = sample_bundle();
    let dependencies = BTreeMap::from([(
        "alpha".to_string(),
        BundleDependencyIndex::from_bundle(&bundle),
    )]);
    let mut coordinator = RefreshCoordinator::new();

    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "./concepts/alpha.md".into(),
    });
    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "concepts/alpha.md".into(),
    });
    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "guides/index.md".into(),
    });

    let lease = coordinator
        .start_next(&dependencies)
        .expect("refresh batch");

    assert_eq!(lease.sequence, 1);
    assert_eq!(lease.bundles.len(), 1);
    assert_eq!(
        lease.bundles[0].changed_paths,
        vec![
            "concepts/alpha.md".to_string(),
            "guides/index.md".to_string()
        ]
    );
    assert!(lease.bundles[0]
        .invalidation
        .concept_pages
        .contains("alpha"));
    assert!(lease.bundles[0]
        .invalidation
        .directory_pages
        .contains("guides"));
}

#[test]
fn concept_index_and_log_changes_invalidate_expected_artifacts() {
    let dependencies = BundleDependencyIndex::from_bundle(&sample_bundle());
    let invalidation =
        dependencies.invalidate_paths(["concepts/alpha.md", "guides/index.md", "guides/log.md"]);

    assert!(invalidation.concept_pages.contains("alpha"));
    assert!(invalidation.search_documents.contains("alpha"));
    assert!(invalidation.backlink_pages.contains("linked"));
    assert!(invalidation
        .outbound_edges
        .iter()
        .any(|edge| edge.source_id == "alpha" && edge.target_id == "linked"));
    assert!(invalidation.directory_pages.contains(""));
    assert!(invalidation.directory_pages.contains("guides"));
    assert!(invalidation.history_pages.contains("guides"));
    assert!(invalidation.aggregate_history);
}

#[test]
fn failed_refresh_records_diagnostics_until_a_successful_retry() {
    let dependencies = BTreeMap::from([(
        "alpha".to_string(),
        BundleDependencyIndex::from_bundle(&sample_bundle()),
    )]);
    let mut coordinator = RefreshCoordinator::new();
    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "concepts/alpha.md".into(),
    });

    let lease = coordinator
        .start_next(&dependencies)
        .expect("refresh batch");
    assert!(coordinator.complete(
        lease.sequence,
        RefreshCompletion::Failure {
            diagnostics: vec![Diagnostic::error(
                "projection_failed",
                "concepts/alpha.md",
                Some(3),
                "compile failed",
            )]
        }
    ));
    assert_eq!(coordinator.last_failure().len(), 1);

    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "concepts/alpha.md".into(),
    });
    let retry = coordinator.start_next(&dependencies).expect("retry batch");
    assert!(coordinator.complete(retry.sequence, RefreshCompletion::Success));
    assert!(coordinator.last_failure().is_empty());
}

#[test]
fn active_wiki_refresh_prevents_stale_publication_until_completion() {
    let dependencies = BTreeMap::from([(
        "alpha".to_string(),
        BundleDependencyIndex::from_bundle(&sample_bundle()),
    )]);
    let mut coordinator = RefreshCoordinator::new();
    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "concepts/alpha.md".into(),
    });
    let first = coordinator.start_next(&dependencies).expect("first lease");

    coordinator.enqueue(RepositoryChange {
        bundle_id: "alpha".into(),
        path: "concepts/linked.md".into(),
    });
    assert!(coordinator.start_next(&dependencies).is_none());
    assert!(!coordinator.complete(first.sequence + 1, RefreshCompletion::Success));
    assert!(coordinator.complete(first.sequence, RefreshCompletion::Success));

    let second = coordinator.start_next(&dependencies).expect("second lease");
    assert_eq!(second.sequence, first.sequence + 1);
    assert_eq!(second.bundles[0].changed_paths, vec!["concepts/linked.md"]);
}

#[test]
fn one_debounced_burst_dispatches_each_consumer_once() {
    let mut coordinator = coordinated_refresh();
    let mut graph = GraphSpy::default();
    let mut wiki = WikiSpy::default();

    coordinator.enqueue(
        RepositoryEvent::Modified {
            path: "bundles/alpha/concepts/alpha.md".into(),
        },
        Duration::from_millis(0),
    );
    coordinator.enqueue(
        RepositoryEvent::Modified {
            path: "./bundles/alpha/concepts/alpha.md".into(),
        },
        Duration::from_millis(10),
    );
    coordinator.enqueue(
        RepositoryEvent::Modified {
            path: "src/lib.rs".into(),
        },
        Duration::from_millis(20),
    );

    assert!(coordinator
        .flush_if_ready(Duration::from_millis(69), &mut graph, &mut wiki)
        .is_none());
    let report = coordinator
        .flush_if_ready(Duration::from_millis(70), &mut graph, &mut wiki)
        .expect("settled batch");

    assert_eq!(report.generation, 1);
    assert_eq!(graph.calls.len(), 1);
    assert_eq!(wiki.calls.len(), 1);
    assert_eq!(
        graph.calls[0],
        vec![
            "bundles/alpha/concepts/alpha.md".to_string(),
            "src/lib.rs".to_string()
        ]
    );
    assert_eq!(
        wiki.calls[0],
        vec![RepositoryChange {
            bundle_id: "alpha".into(),
            path: "concepts/alpha.md".into(),
        }]
    );
}

#[test]
fn paths_are_partitioned_for_graph_wiki_or_both() {
    let mut coordinator = coordinated_refresh();
    let mut graph = GraphSpy::default();
    let mut wiki = WikiSpy::default();

    for path in [
        "src/lib.rs",
        "bundles/alpha/assets/diagram.svg",
        "bundles/alpha/concepts/alpha.md",
        ".kwiki/manifest.json",
    ] {
        coordinator.enqueue(
            RepositoryEvent::Modified { path: path.into() },
            Duration::ZERO,
        );
    }
    let report = coordinator
        .shutdown(&mut graph, &mut wiki)
        .expect("partitioned batch");

    assert_eq!(
        report.graph.changed_paths,
        vec![
            "bundles/alpha/concepts/alpha.md".to_string(),
            "src/lib.rs".to_string()
        ]
    );
    assert_eq!(
        report.wiki.changed_paths,
        vec![
            "alpha/assets/diagram.svg".to_string(),
            "alpha/concepts/alpha.md".to_string()
        ]
    );
    assert_eq!(report.graph.status, ConsumerRefreshStatus::Succeeded);
    assert_eq!(report.wiki.status, ConsumerRefreshStatus::Succeeded);
}

#[test]
fn consumer_failures_are_independent_and_report_retryability() {
    let mut coordinator = coordinated_refresh();
    let mut graph = GraphSpy {
        failure: Some(RefreshFailure::new("graph_locked", true)),
        ..GraphSpy::default()
    };
    let mut wiki = WikiSpy::default();
    coordinator.enqueue(
        RepositoryEvent::Modified {
            path: "bundles/alpha/concepts/alpha.md".into(),
        },
        Duration::ZERO,
    );

    let report = coordinator
        .shutdown(&mut graph, &mut wiki)
        .expect("failed graph batch");

    assert_eq!(
        report.graph.status,
        ConsumerRefreshStatus::Failed {
            code: "graph_locked".into(),
            retryable: true,
        }
    );
    assert_eq!(report.wiki.status, ConsumerRefreshStatus::Succeeded);
    assert_eq!(graph.calls.len(), 1);
    assert_eq!(wiki.calls.len(), 1);

    let mut coordinator = coordinated_refresh();
    let mut graph = GraphSpy::default();
    let mut wiki = WikiSpy {
        failure: Some(RefreshFailure::new("projection_invalid", false)),
        ..WikiSpy::default()
    };
    coordinator.enqueue(
        RepositoryEvent::Modified {
            path: "bundles/alpha/concepts/alpha.md".into(),
        },
        Duration::ZERO,
    );
    let report = coordinator
        .shutdown(&mut graph, &mut wiki)
        .expect("failed wiki batch");

    assert_eq!(report.graph.status, ConsumerRefreshStatus::Succeeded);
    assert_eq!(
        report.wiki.status,
        ConsumerRefreshStatus::Failed {
            code: "projection_invalid".into(),
            retryable: false,
        }
    );
    assert_eq!(graph.calls.len(), 1);
    assert_eq!(wiki.calls.len(), 1);
}

#[test]
fn rename_delete_and_shutdown_flush_each_path_once() {
    let mut coordinator = coordinated_refresh();
    let mut graph = GraphSpy::default();
    let mut wiki = WikiSpy::default();
    coordinator.enqueue(
        RepositoryEvent::Renamed {
            from: "bundles/alpha/concepts/old.md".into(),
            to: "bundles/alpha/concepts/new.md".into(),
        },
        Duration::ZERO,
    );
    coordinator.enqueue(
        RepositoryEvent::Deleted {
            path: "bundles/alpha/concepts/removed.md".into(),
        },
        Duration::from_millis(1),
    );

    let report = coordinator
        .shutdown(&mut graph, &mut wiki)
        .expect("shutdown batch");

    assert_eq!(report.generation, 1);
    assert_eq!(
        graph.calls[0],
        vec![
            "bundles/alpha/concepts/new.md".to_string(),
            "bundles/alpha/concepts/old.md".to_string(),
            "bundles/alpha/concepts/removed.md".to_string(),
        ]
    );
    assert_eq!(wiki.calls.len(), 1);
    assert_eq!(wiki.calls[0].len(), 3);
    assert!(!coordinator.has_pending());
    assert!(coordinator.shutdown(&mut graph, &mut wiki).is_none());
    assert_eq!(graph.calls.len(), 1);
    assert_eq!(wiki.calls.len(), 1);
}

fn coordinated_refresh() -> CoordinatedRefresh {
    CoordinatedRefresh::new(
        Duration::from_millis(50),
        vec![WatchedBundle::new("alpha", "bundles/alpha").expect("safe bundle root")],
    )
}

#[derive(Default)]
struct GraphSpy {
    calls: Vec<Vec<String>>,
    failure: Option<RefreshFailure>,
}

impl GraphRefreshConsumer for GraphSpy {
    fn refresh_graph(&mut self, changed_paths: &[String]) -> Result<(), RefreshFailure> {
        self.calls.push(changed_paths.to_vec());
        self.failure.clone().map_or(Ok(()), Err)
    }
}

#[derive(Default)]
struct WikiSpy {
    calls: Vec<Vec<RepositoryChange>>,
    failure: Option<RefreshFailure>,
}

impl WikiRefreshConsumer for WikiSpy {
    fn refresh_wiki(&mut self, changes: &[RepositoryChange]) -> Result<(), RefreshFailure> {
        self.calls.push(changes.to_vec());
        self.failure.clone().map_or(Ok(()), Err)
    }
}

fn sample_bundle() -> Bundle {
    let mut bundle = Bundle {
        id: "alpha".into(),
        root_path: "bundles/alpha".into(),
        okf_version: "0.1".into(),
        title: "Alpha".into(),
        ..Bundle::default()
    };
    bundle.concepts.push(Concept {
        id: "alpha".into(),
        bundle_id: "alpha".into(),
        source_path: "concepts/alpha.md".into(),
        concept_type: "decision".into(),
        body_markdown: "body".into(),
        outbound_links: vec![Link {
            source_id: "alpha".into(),
            raw_href: "linked".into(),
            normalized_target_id: Some("linked".into()),
            fragment: None,
            status: LinkStatus::Resolved,
            context: None,
        }],
        ..Concept::default()
    });
    bundle
}

#[allow(dead_code)]
struct TestDir {
    path: PathBuf,
}

#[allow(dead_code)]
impl TestDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("k-wiki-refresh-{unique:x}"));
        fs::create_dir_all(&path).expect("create temp directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}
