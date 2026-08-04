use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    diagnostic::Diagnostic,
    model::{
        Bundle, Citation, Concept, Directory, Heading, IndexSource, Link, LinkStatus, LogEntry,
        WikiProjection,
    },
    render::{
        RenderContext, RenderOptions, RenderSite, RenderedAsset, Renderer, RouteHelper, RouteKind,
        RouteManifest,
    },
};

#[test]
fn route_helpers_percent_encode_reversibly_and_reject_escape_segments() {
    let routes = RouteHelper::new("/docs/wiki").expect("create route helper");
    let concept = routes
        .concept(
            "engineering handbook",
            "architecture/renderer safety",
            Some("checks"),
        )
        .expect("build concept route");
    assert_eq!(
        concept.route,
        "/docs/wiki/b/engineering%20handbook/c/architecture/renderer%20safety/#checks"
    );
    assert_eq!(
        RouteHelper::decode_segment("engineering%20handbook").expect("decode bundle id"),
        "engineering handbook"
    );
    assert!(routes.bundle("..").is_err());
    assert!(routes.directory("bundle", "../escape").is_err());
}

#[test]
fn concept_pages_sanitize_active_content_and_route_broken_links_to_diagnostics() {
    let renderer = Renderer::new(RenderOptions::default()).expect("create renderer");
    let site = renderer
        .render_site(&sample_projection(), &sample_context())
        .expect("render site");

    let concept_page = site
        .pages
        .iter()
        .find(|page| {
            page.kind == RouteKind::Concept && page.route.contains("/architecture/renderer/")
        })
        .expect("concept page");

    assert!(concept_page.html.contains("Skip to content"));
    assert!(concept_page.html.contains("Table of contents"));
    assert!(concept_page.html.contains("Related graph context"));
    assert!(concept_page
        .html
        .contains("/b/engineering%20handbook/c/engineering/testing/#checks"));
    assert!(concept_page
        .html
        .contains("/b/engineering%20handbook/diagnostics/#diag-"));
    assert!(!concept_page.html.contains("<script"));
    assert!(!concept_page.html.contains("onclick="));
    assert!(!concept_page.html.contains("javascript:"));
    assert!(!concept_page.html.contains("<svg"));
}

#[test]
fn rendered_site_writes_manifest_assets_and_enforces_output_root_containment() {
    let renderer = Renderer::new(RenderOptions::default()).expect("create renderer");
    let site = renderer
        .render_site(&sample_projection(), &sample_context())
        .expect("render site");

    let output_root = unique_output_dir("k-wiki-render");
    if output_root.exists() {
        fs::remove_dir_all(&output_root).expect("clear output dir");
    }
    site.write_to(&output_root).expect("write rendered site");

    let manifest_path = output_root.join("_k-wiki/routes.json");
    let stylesheet_path = output_root.join("assets/wiki.css");
    let concept_path =
        output_root.join("b/engineering%20handbook/c/architecture/renderer/index.html");
    assert!(manifest_path.exists());
    assert!(stylesheet_path.exists());
    assert!(concept_path.exists());

    let manifest = fs::read_to_string(&manifest_path).expect("read manifest");
    assert!(manifest.contains("\"route\": \"/b/engineering%20handbook/c/architecture/renderer/\""));

    let malicious = RenderSite {
        pages: Vec::new(),
        assets: vec![RenderedAsset {
            route: "/escape.txt".into(),
            output_path: PathBuf::from("../escape.txt"),
            content_type: "text/plain".into(),
            bytes: b"nope".to_vec(),
        }],
        manifest: RouteManifest::default(),
    };
    assert!(malicious.write_to(&output_root).is_err());
}

#[test]
fn representative_html_fixtures_stay_deterministic_for_desktop_and_narrow_layouts() {
    let renderer = Renderer::new(RenderOptions::default()).expect("create renderer");
    let site = renderer
        .render_site(&sample_projection(), &sample_context())
        .expect("render site");

    let home = site
        .pages
        .iter()
        .find(|page| page.kind == RouteKind::Home)
        .expect("home page");
    let concept = site
        .pages
        .iter()
        .find(|page| {
            page.kind == RouteKind::Concept && page.route.contains("/architecture/renderer/")
        })
        .expect("concept page");
    let stylesheet = site
        .assets
        .iter()
        .find(|asset| asset.route == "/assets/wiki.css")
        .expect("stylesheet");

    let home_fixture = normalize_whitespace(&home.html);
    assert!(home_fixture.contains("<title>Knowledge Wiki</title>"));
    assert!(home_fixture.contains("Repository bundles"));
    assert!(home_fixture.contains("Engineering Handbook"));

    let concept_fixture = normalize_whitespace(&concept.html);
    assert!(concept_fixture.contains("<p class=\"eyebrow\">decision</p>"));
    assert!(concept_fixture.contains("Browse directory"));
    assert!(concept_fixture.contains("Citations"));
    assert!(concept_fixture.contains("Backlinks"));
    assert!(concept_fixture.contains("Renderer runtime"));

    let css = String::from_utf8(stylesheet.bytes.clone()).expect("stylesheet utf-8");
    assert!(css.contains("@media (max-width: 52rem)"));
    assert!(css.contains(".site-layout"));
}

fn sample_projection() -> WikiProjection {
    let mut projection = WikiProjection::new("2026-07-30T12:00:00Z", Some("rev-123".into()));
    projection.bundles.push(Bundle {
        id: "engineering handbook".into(),
        root_path: "knowledge/engineering".into(),
        okf_version: "0.1".into(),
        title: "Engineering Handbook".into(),
        source_revision: Some("bundle-rev".into()),
        directories: vec![
            Directory {
                path: String::new(),
                title: "Engineering Handbook".into(),
                description: Some("Entry point for engineering knowledge.".into()),
                index_source: IndexSource::Authored,
                body_markdown: "Welcome to the handbook.".into(),
                child_directories: vec!["architecture".into(), "engineering".into()],
                concept_ids: vec!["architecture/renderer".into(), "engineering/testing".into()],
                log_entries: vec![LogEntry {
                    scope_path: String::new(),
                    date: "2026-07-29".into(),
                    category: "release".into(),
                    text: "Published renderer guidance.".into(),
                    links: vec!["architecture/renderer".into()],
                }],
            },
            Directory {
                path: "architecture".into(),
                title: "Architecture".into(),
                description: Some("Architecture decisions and notes.".into()),
                index_source: IndexSource::Authored,
                body_markdown: "Architecture notes.".into(),
                child_directories: Vec::new(),
                concept_ids: vec!["architecture/renderer".into()],
                log_entries: Vec::new(),
            },
            Directory {
                path: "engineering".into(),
                title: "Engineering".into(),
                description: Some("Engineering guides.".into()),
                index_source: IndexSource::Synthetic,
                body_markdown: "Engineering guides.".into(),
                child_directories: Vec::new(),
                concept_ids: vec!["engineering/testing".into()],
                log_entries: Vec::new(),
            },
        ],
        concepts: vec![
            Concept {
                id: "architecture/renderer".into(),
                bundle_id: "engineering handbook".into(),
                source_path: "architecture/renderer.md".into(),
                concept_type: "decision".into(),
                title: Some("Renderer Safety".into()),
                description: Some("Hardens wiki rendering paths and content.".into()),
                resource: Some("docs/renderer".into()),
                tags: vec!["accessibility".into(), "rust".into()],
                timestamp: Some("2026-07-29".into()),
                body_markdown: "# Renderer Safety\n\nA [safe link](../engineering/testing.md#checks).\n\nA [broken link](../missing.md).\n\n<script>alert('boom')</script>\n<svg><script>alert('boom')</script></svg>\n<a href=\"javascript:alert('boom')\" onclick=\"alert('boom')\">unsafe</a>\n\n## Details\n\nPlain text.".into(),
                headings: vec![
                    Heading {
                        level: 1,
                        id: "renderer-safety".into(),
                        text: "Renderer Safety".into(),
                    },
                    Heading {
                        level: 2,
                        id: "details".into(),
                        text: "Details".into(),
                    },
                ],
                outbound_links: vec![
                    Link {
                        source_id: "architecture/renderer".into(),
                        raw_href: "../engineering/testing.md#checks".into(),
                        normalized_target_id: Some("engineering/testing".into()),
                        fragment: Some("checks".into()),
                        status: LinkStatus::Resolved,
                        context: Some("Validation checklist".into()),
                    },
                    Link {
                        source_id: "architecture/renderer".into(),
                        raw_href: "../missing.md".into(),
                        normalized_target_id: None,
                        fragment: None,
                        status: LinkStatus::Broken,
                        context: Some("Missing source".into()),
                    },
                ],
                backlinks: vec![Link {
                    source_id: "engineering/testing".into(),
                    raw_href: "../architecture/renderer.md#details".into(),
                    normalized_target_id: Some("architecture/renderer".into()),
                    fragment: Some("details".into()),
                    status: LinkStatus::Resolved,
                    context: Some("Testing references renderer details".into()),
                }],
                citations: vec![Citation {
                    number: 1,
                    text: "Renderer runtime".into(),
                    href: Some("https://example.com/renderer-runtime".into()),
                }],
                ..Concept::default()
            },
            Concept {
                id: "engineering/testing".into(),
                bundle_id: "engineering handbook".into(),
                source_path: "engineering/testing.md".into(),
                concept_type: "guide".into(),
                title: Some("Testing Guide".into()),
                description: Some("Checklist for renderer verification.".into()),
                tags: vec!["testing".into()],
                timestamp: Some("2026-07-28".into()),
                body_markdown: "# Testing Guide\n\n## Checks\n\nFollow the checks.".into(),
                headings: vec![
                    Heading {
                        level: 1,
                        id: "testing-guide".into(),
                        text: "Testing Guide".into(),
                    },
                    Heading {
                        level: 2,
                        id: "checks".into(),
                        text: "Checks".into(),
                    },
                ],
                outbound_links: vec![Link {
                    source_id: "engineering/testing".into(),
                    raw_href: "../architecture/renderer.md#details".into(),
                    normalized_target_id: Some("architecture/renderer".into()),
                    fragment: Some("details".into()),
                    status: LinkStatus::Resolved,
                    context: Some("Renderer detail".into()),
                }],
                ..Concept::default()
            },
        ],
        diagnostics: vec![Diagnostic::warning(
            "bundle_warning",
            "architecture/renderer.md",
            Some(4),
            "Broken internal link '../missing.md'.",
        )],
    });
    projection.normalize();
    projection
}

fn sample_context() -> RenderContext {
    let mut context = RenderContext::default();
    context.bundle_context.insert(
        "engineering handbook".into(),
        vec![k_wiki::render::RelatedContextItem {
            kind: "graph".into(),
            title: "Runtime surface".into(),
            summary: "Bounded graph summary for the bundle.".into(),
            href: Some("/graph/runtime".into()),
        }],
    );
    context.concept_context.insert(
        "engineering handbook".into(),
        [(
            "architecture/renderer".into(),
            vec![k_wiki::render::RelatedContextItem {
                kind: "definition".into(),
                title: "Renderer runtime".into(),
                summary: "Graph summary for renderer call paths.".into(),
                href: Some("/graph/runtime/renderer".into()),
            }],
        )]
        .into_iter()
        .collect(),
    );
    context
}

fn unique_output_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
