use std::collections::BTreeMap;

use okf_wiki::{
    compiler::{
        compile_bundle, compile_projection, concept_route_id, directory_route_id, CompileRequest,
        SourceBundle, SourceDocument,
    },
    model::{IndexSource, LinkStatus},
};
use serde_json::json;

fn compile_single_bundle(bundle: SourceBundle) -> okf_wiki::model::Bundle {
    compile_bundle(bundle)
}

fn bundle(documents: Vec<SourceDocument>) -> SourceBundle {
    SourceBundle {
        id: "docs".into(),
        root_path: "knowledge".into(),
        okf_version: "0.1".into(),
        title: "Knowledge".into(),
        source_revision: Some("rev-1".into()),
        documents,
    }
}

fn concept(path: &str, body: &str) -> SourceDocument {
    SourceDocument::concept(path, "note", body)
}

#[test]
fn compilation_is_deterministic_and_namespaces_routes() {
    let mut guide = concept(
        "guide.md",
        "# Guide\n\nSee [Overview](overview.md#intro).\n\n## Intro\n",
    );
    guide.title = Some("Guide".into());
    guide.tags = vec!["rust".into(), "docs".into(), "rust".into()];
    guide.extensions = BTreeMap::from([("extra".into(), json!("value"))]);

    let mut overview = concept(
        "overview.md",
        "# Overview\n\n## Intro\n\n## Citations\n1. [RFC](https://example.com/rfc)\n",
    );
    overview.title = Some("Overview".into());

    let request = CompileRequest {
        generated_at: "2026-07-30T00:00:00Z".into(),
        source_revision: Some("fixed".into()),
        bundles: vec![bundle(vec![guide.clone(), overview.clone()])],
    };

    let first = compile_projection(request.clone());
    let second = compile_projection(request);
    let first_json = serde_json::to_vec_pretty(&first).expect("serialize first projection");
    let second_json = serde_json::to_vec_pretty(&second).expect("serialize second projection");

    assert_eq!(first_json, second_json);

    let guide = first
        .bundles
        .first()
        .and_then(|bundle| bundle.concepts.iter().find(|concept| concept.id == "guide"))
        .expect("guide concept");
    assert_eq!(
        guide.outbound_links[0].normalized_target_id.as_deref(),
        Some(concept_route_id("docs", "overview").as_str())
    );
    assert_eq!(guide.outbound_links[0].fragment.as_deref(), Some("intro"));
}

#[test]
fn backlinks_update_after_link_add_change_and_removal() {
    let added = compile_single_bundle(bundle(vec![
        concept("intro.md", "See [Guide](guide.md) and [Arch](arch.md)."),
        concept("guide.md", "# Guide"),
        concept("arch.md", "# Architecture"),
    ]));

    let guide = added
        .concepts
        .iter()
        .find(|concept| concept.id == "guide")
        .expect("guide concept");
    let arch = added
        .concepts
        .iter()
        .find(|concept| concept.id == "arch")
        .expect("arch concept");
    assert_eq!(guide.backlinks.len(), 1);
    assert_eq!(guide.backlinks[0].source_id, "intro");
    assert_eq!(arch.backlinks.len(), 1);

    let changed = compile_single_bundle(bundle(vec![
        concept("intro.md", "See [Architecture](arch.md)."),
        concept("guide.md", "# Guide"),
        concept("arch.md", "# Architecture"),
    ]));

    let guide = changed
        .concepts
        .iter()
        .find(|concept| concept.id == "guide")
        .expect("guide concept");
    let arch = changed
        .concepts
        .iter()
        .find(|concept| concept.id == "arch")
        .expect("arch concept");
    assert!(guide.backlinks.is_empty());
    assert_eq!(arch.backlinks.len(), 1);
    assert_eq!(arch.backlinks[0].source_id, "intro");

    let removed = compile_single_bundle(bundle(vec![
        concept("intro.md", "No links remain here."),
        concept("guide.md", "# Guide"),
        concept("arch.md", "# Architecture"),
    ]));

    let guide = removed
        .concepts
        .iter()
        .find(|concept| concept.id == "guide")
        .expect("guide concept");
    let arch = removed
        .concepts
        .iter()
        .find(|concept| concept.id == "arch")
        .expect("arch concept");
    assert!(guide.backlinks.is_empty());
    assert!(arch.backlinks.is_empty());
}

#[test]
fn broken_fragment_external_and_rejected_links_remain_visible() {
    let compiled = compile_single_bundle(bundle(vec![
        concept(
            "guide/intro.md",
            "\
[Missing](missing.md)\n\
[Broken Fragment](../target.md#not-there)\n\
[External](https://example.com)\n\
[Unsafe](javascript:alert(1))\n\
[Escape](../../outside.md)\n",
        ),
        concept("target.md", "# Target\n\n## There\n"),
    ]));

    let intro = compiled
        .concepts
        .iter()
        .find(|concept| concept.id == "guide/intro")
        .expect("intro concept");

    assert_eq!(intro.outbound_links.len(), 5);
    let missing = intro
        .outbound_links
        .iter()
        .find(|link| link.raw_href == "missing.md")
        .expect("missing link");
    let broken_fragment = intro
        .outbound_links
        .iter()
        .find(|link| link.raw_href == "../target.md#not-there")
        .expect("broken fragment link");
    let external = intro
        .outbound_links
        .iter()
        .find(|link| link.raw_href == "https://example.com")
        .expect("external link");
    let unsafe_link = intro
        .outbound_links
        .iter()
        .find(|link| link.raw_href == "javascript:alert(1)")
        .expect("unsafe link");
    let escaping = intro
        .outbound_links
        .iter()
        .find(|link| link.raw_href == "../../outside.md")
        .expect("escaping link");

    assert_eq!(missing.status, LinkStatus::Broken);
    assert_eq!(
        missing.normalized_target_id.as_deref(),
        Some(concept_route_id("docs", "guide/missing").as_str())
    );
    assert_eq!(broken_fragment.status, LinkStatus::Broken);
    assert_eq!(broken_fragment.fragment.as_deref(), Some("not-there"));
    assert_eq!(external.status, LinkStatus::External);
    assert_eq!(unsafe_link.status, LinkStatus::Rejected);
    assert_eq!(escaping.status, LinkStatus::Rejected);

    let broken = compiled
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "broken_link")
        .count();
    let rejected = compiled
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "rejected_link")
        .count();

    assert_eq!(broken, 2);
    assert_eq!(rejected, 2);
}

#[test]
fn directory_indexes_history_and_citations_are_compiled_deterministically() {
    let mut root_index = SourceDocument::index(
        "index.md",
        "# Knowledge\n\nWelcome to the authored home page.\n",
    );
    root_index.title = Some("Knowledge".into());
    root_index.description = Some("Home".into());

    let mut guide = concept(
        "guides/guide.md",
        "\
# Guide\n\n\
## Deep Dive\n\n\
## Citations\n\
1. [Spec](https://example.com/spec)\n\
2. Internal design note\n",
    );
    guide.title = Some("Guide".into());

    let compiled = compile_single_bundle(bundle(vec![
        root_index,
        guide,
        SourceDocument::log(
            "guides/log.md",
            "\
## 2026-07-30\n\
### Decisions\n\
- Added guide body [Guide](guide.md)\n\
## 2026-07-29\n\
- Earlier draft\n",
        ),
    ]));

    let root = compiled
        .directories
        .iter()
        .find(|directory| directory.path.is_empty())
        .expect("root directory");
    let guides = compiled
        .directories
        .iter()
        .find(|directory| directory.path == "guides")
        .expect("guides directory");
    let guide = compiled
        .concepts
        .iter()
        .find(|concept| concept.id == "guides/guide")
        .expect("guide concept");

    assert_eq!(root.index_source, IndexSource::Authored);
    assert_eq!(root.description.as_deref(), Some("Home"));
    assert_eq!(guides.index_source, IndexSource::Synthetic);
    assert_eq!(guides.concept_ids, vec!["guides/guide"]);
    assert!(guides.body_markdown.contains("## Concepts"));
    assert_eq!(root.log_entries.len(), 2);
    assert_eq!(guides.log_entries.len(), 2);
    assert_eq!(root.log_entries[0].date, "2026-07-30");
    assert_eq!(root.log_entries[0].category, "Decisions");
    assert_eq!(root.log_entries[0].scope_path, "guides");
    assert_eq!(
        guide
            .headings
            .iter()
            .map(|heading| heading.id.as_str())
            .collect::<Vec<_>>(),
        vec!["citations", "deep-dive", "guide"]
    );
    assert_eq!(guide.citations.len(), 2);
    assert_eq!(
        guide.citations[0].href.as_deref(),
        Some("https://example.com/spec")
    );
    assert_eq!(guide.citations[1].text, "Internal design note");
}

#[test]
fn relative_directory_and_fragment_routes_resolve_canonically() {
    let compiled = compile_single_bundle(bundle(vec![
        concept(
            "guides/start.md",
            "## Deep Dive\n\nSee [Local](#deep-dive) and [Guides Root](./).",
        ),
        SourceDocument::index("guides/index.md", "# Guides\n\n## Deep Dive\n"),
    ]));

    let start = compiled
        .concepts
        .iter()
        .find(|concept| concept.id == "guides/start")
        .expect("start concept");

    assert_eq!(start.outbound_links.len(), 2);
    assert_eq!(
        start.outbound_links[0].normalized_target_id.as_deref(),
        Some(concept_route_id("docs", "guides/start").as_str())
    );
    assert_eq!(
        start.outbound_links[0].fragment.as_deref(),
        Some("deep-dive")
    );
    assert_eq!(
        start.outbound_links[1].normalized_target_id.as_deref(),
        Some(directory_route_id("docs", "guides").as_str())
    );
}
