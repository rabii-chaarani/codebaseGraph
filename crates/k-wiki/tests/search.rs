use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    model::{Bundle, Citation, Concept, Heading, WikiProjection},
    search::{SearchIndex, SearchQuery},
};
use serde_json::json;

#[test]
fn exact_identity_and_title_matches_rank_above_body_only_matches() {
    let projection = sample_projection();
    let index = SearchIndex::build(&projection);

    let title_results = index.search(&SearchQuery::new("Launch Readiness Checklist"));
    assert_eq!(title_results[0].concept_id, "launch-checklist");

    let id_results = index.search(&SearchQuery::new("incident-playbook"));
    assert_eq!(id_results[0].concept_id, "incident-playbook");

    let body_results = index.search(&SearchQuery::new("operational resilience"));
    assert_eq!(body_results[0].concept_id, "launch-checklist");
}

#[test]
fn filters_restrict_bundle_type_and_tag_matches() {
    let projection = sample_projection();
    let index = SearchIndex::build(&projection);

    let mut query = SearchQuery::new("launch");
    query.bundle = Some("alpha".into());
    query.concept_type = Some("guide".into());
    query.tags = vec!["release".into()];

    let results = index.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].concept_id, "launch-checklist");
}

#[test]
fn snippets_are_html_escaped_bounded_and_deterministic() {
    let projection = sample_projection();
    let index = SearchIndex::build(&projection);
    let query = SearchQuery::new("alert");

    let first = index.search(&query);
    let second = index.search(&query);

    assert_eq!(first, second);
    let snippet = first[0].snippet.as_deref().expect("snippet");
    assert!(snippet.contains("<mark>alert</mark>"));
    assert!(!snippet.contains("<script>"));
    assert!(snippet.contains("&lt;script&gt;"));
    assert!(snippet.len() <= 160);
}

#[test]
fn serialized_index_round_trips_deterministically() {
    let index = SearchIndex::build(&sample_projection());
    let first = index.to_bytes().expect("serialize search index");
    let second = SearchIndex::from_bytes(&first)
        .expect("deserialize search index")
        .to_bytes()
        .expect("serialize search index again");

    assert_eq!(first, second);
}

fn sample_projection() -> WikiProjection {
    let mut projection = WikiProjection::new("2026-07-30T00:00:00Z", Some("rev-1".into()));
    projection.bundles.push(Bundle {
        id: "alpha".into(),
        root_path: "bundles/alpha".into(),
        okf_version: "0.1".into(),
        title: "Alpha".into(),
        concepts: vec![
            Concept {
                id: "launch-checklist".into(),
                bundle_id: "alpha".into(),
                source_path: "concepts/launch.md".into(),
                concept_type: "guide".into(),
                title: Some("Launch Readiness Checklist".into()),
                description: Some("Release process for safe deployment".into()),
                tags: vec!["release".into(), "ops".into()],
                body_markdown:
                    "Safe <script>alert(1)</script> deployment improves operational resilience."
                        .into(),
                headings: vec![Heading {
                    level: 2,
                    id: "prep".into(),
                    text: "Deployment Preparation".into(),
                }],
                citations: vec![Citation {
                    number: 1,
                    text: "deployment runbook".into(),
                    href: Some("https://example.test/runbook".into()),
                }],
                extensions: BTreeMap::from([("owner".into(), json!("platform"))]),
                ..Concept::default()
            },
            Concept {
                id: "incident-playbook".into(),
                bundle_id: "alpha".into(),
                source_path: "concepts/incident.md".into(),
                concept_type: "guide".into(),
                title: Some("Incident Playbook".into()),
                tags: vec!["ops".into()],
                body_markdown: "This body mentions launch readiness only in passing.".into(),
                ..Concept::default()
            },
        ],
        ..Bundle::default()
    });
    projection.bundles.push(Bundle {
        id: "beta".into(),
        root_path: "bundles/beta".into(),
        okf_version: "0.1".into(),
        title: "Beta".into(),
        concepts: vec![Concept {
            id: "body-only".into(),
            bundle_id: "beta".into(),
            source_path: "concepts/body.md".into(),
            concept_type: "note".into(),
            title: Some("Loose Notes".into()),
            body_markdown: "operational resilience appears only in body text".into(),
            tags: vec!["notes".into()],
            ..Concept::default()
        }],
        ..Bundle::default()
    });
    projection.normalize();
    projection
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
        let path = std::env::temp_dir().join(format!("k-wiki-search-{unique:x}"));
        fs::create_dir_all(&path).expect("create temp directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}
