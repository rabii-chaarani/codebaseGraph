//! Secure static and server-side rendering.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
};

use ammonia::{Builder as HtmlSanitizer, UrlRelative};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    model::{
        Bundle, Citation, Concept, Directory, Heading, Link, LinkStatus, LogEntry, WikiProjection,
    },
};

const PAGE_TEMPLATE: &str = include_str!("../../templates/page.html");
const WIKI_STYLESHEET: &str = include_str!("../../assets/wiki.css");
const ROUTE_MANIFEST_PATH: &str = "_k-wiki/routes.json";
const STYLESHEET_PATH: &str = "assets/wiki.css";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderOptions {
    pub base_path: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelatedContextItem {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub href: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderContext {
    pub bundle_context: BTreeMap<String, Vec<RelatedContextItem>>,
    pub concept_context: BTreeMap<String, BTreeMap<String, Vec<RelatedContextItem>>>,
}

impl RenderContext {
    fn bundle_items<'a>(&'a self, bundle_id: &str) -> &'a [RelatedContextItem] {
        self.bundle_context
            .get(bundle_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn concept_items<'a>(&'a self, bundle_id: &str, concept_id: &str) -> &'a [RelatedContextItem] {
        self.concept_context
            .get(bundle_id)
            .and_then(|entries| entries.get(concept_id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    Home,
    Bundle,
    Directory,
    Concept,
    TypeFacet,
    TagFacet,
    Search,
    Graph,
    Changes,
    Diagnostics,
    Asset,
    Manifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RouteTarget {
    pub route: String,
    pub output_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManifestEntry {
    pub kind: RouteKind,
    pub route: String,
    pub output_path: String,
    pub title: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RouteManifest {
    pub entries: Vec<ManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedPage {
    pub kind: RouteKind,
    pub title: String,
    pub route: String,
    pub output_path: PathBuf,
    pub html: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedAsset {
    pub route: String,
    pub output_path: PathBuf,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderSite {
    pub pages: Vec<RenderedPage>,
    pub assets: Vec<RenderedAsset>,
    pub manifest: RouteManifest,
}

impl RenderSite {
    pub fn write_to(&self, output_root: impl AsRef<Path>) -> Result<(), RenderError> {
        let output_root = output_root.as_ref();
        fs::create_dir_all(output_root).map_err(RenderError::io)?;

        for page in &self.pages {
            let destination = contained_output_path(output_root, &page.output_path)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(RenderError::io)?;
            }
            fs::write(destination, page.html.as_bytes()).map_err(RenderError::io)?;
        }

        for asset in &self.assets {
            let destination = contained_output_path(output_root, &asset.output_path)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(RenderError::io)?;
            }
            fs::write(destination, &asset.bytes).map_err(RenderError::io)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteHelper {
    base_segments: Vec<String>,
}

impl RouteHelper {
    pub fn new(base_path: impl AsRef<str>) -> Result<Self, RenderError> {
        let mut base_segments = Vec::new();
        for segment in base_path.as_ref().split('/') {
            if segment.is_empty() {
                continue;
            }
            base_segments.push(encode_segment(segment)?);
        }
        Ok(Self { base_segments })
    }

    pub fn decode_segment(segment: &str) -> Result<String, RenderError> {
        decode_segment(segment)
    }

    pub fn home(&self) -> RouteTarget {
        self.page(&[])
    }

    pub fn bundle(&self, bundle_id: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&["b", &encode_segment(bundle_id)?]))
    }

    pub fn directory(
        &self,
        bundle_id: &str,
        directory_path: &str,
    ) -> Result<RouteTarget, RenderError> {
        let mut segments = vec!["b".to_owned(), encode_segment(bundle_id)?, "d".to_owned()];
        segments.extend(encode_path_segments(directory_path)?);
        Ok(self.page_owned(segments))
    }

    pub fn concept(
        &self,
        bundle_id: &str,
        concept_id: &str,
        fragment: Option<&str>,
    ) -> Result<RouteTarget, RenderError> {
        let mut segments = vec!["b".to_owned(), encode_segment(bundle_id)?, "c".to_owned()];
        segments.extend(encode_path_segments(concept_id)?);
        Ok(self.page_owned_with_fragment(segments, fragment))
    }

    pub fn concept_source_directory(
        &self,
        bundle_id: &str,
        concept_id: &str,
    ) -> Result<RouteTarget, RenderError> {
        let directory_path = concept_parent_path(concept_id);
        if directory_path.is_empty() {
            self.bundle(bundle_id)
        } else {
            self.directory(bundle_id, &directory_path)
        }
    }

    pub fn type_facet(&self, bundle_id: &str, facet: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&[
            "b",
            &encode_segment(bundle_id)?,
            "type",
            &encode_segment(facet)?,
        ]))
    }

    pub fn tag_facet(&self, bundle_id: &str, facet: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&[
            "b",
            &encode_segment(bundle_id)?,
            "tag",
            &encode_segment(facet)?,
        ]))
    }

    pub fn search(&self, bundle_id: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&["b", &encode_segment(bundle_id)?, "search"]))
    }

    pub fn graph(&self, bundle_id: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&["b", &encode_segment(bundle_id)?, "graph"]))
    }

    pub fn changes(&self, bundle_id: &str) -> Result<RouteTarget, RenderError> {
        Ok(self.page(&["b", &encode_segment(bundle_id)?, "changes"]))
    }

    pub fn diagnostics(
        &self,
        bundle_id: &str,
        anchor: Option<&str>,
    ) -> Result<RouteTarget, RenderError> {
        Ok(self.page_with_fragment(&["b", &encode_segment(bundle_id)?, "diagnostics"], anchor))
    }

    pub fn asset(&self, relative_path: &str) -> Result<RouteTarget, RenderError> {
        let mut segments = Vec::new();
        for segment in relative_path.split('/') {
            if segment.is_empty() {
                continue;
            }
            segments.push(encode_segment(segment)?);
        }
        Ok(self.asset_owned(segments))
    }

    fn page(&self, segments: &[&str]) -> RouteTarget {
        let owned = segments
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect();
        self.page_owned(owned)
    }

    fn page_with_fragment(&self, segments: &[&str], fragment: Option<&str>) -> RouteTarget {
        let owned = segments
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect();
        self.page_owned_with_fragment(owned, fragment)
    }

    fn page_owned(&self, segments: Vec<String>) -> RouteTarget {
        self.page_owned_with_fragment(segments, None)
    }

    fn page_owned_with_fragment(
        &self,
        segments: Vec<String>,
        fragment: Option<&str>,
    ) -> RouteTarget {
        let mut route_segments = self.base_segments.clone();
        route_segments.extend(segments.iter().cloned());

        let mut route = String::from("/");
        if !route_segments.is_empty() {
            route.push_str(&route_segments.join("/"));
            route.push('/');
        }
        if let Some(fragment) = fragment {
            route.push('#');
            route.push_str(&escape_fragment_identifier(fragment));
        }

        let mut output_path = PathBuf::new();
        for segment in route_segments {
            output_path.push(segment);
        }
        output_path.push("index.html");

        RouteTarget { route, output_path }
    }

    fn asset_owned(&self, segments: Vec<String>) -> RouteTarget {
        let mut route_segments = self.base_segments.clone();
        route_segments.extend(segments.iter().cloned());

        let route = if route_segments.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", route_segments.join("/"))
        };

        let mut output_path = PathBuf::new();
        for segment in route_segments {
            output_path.push(segment);
        }

        RouteTarget { route, output_path }
    }
}

#[derive(Debug)]
pub struct Renderer {
    options: RenderOptions,
    routes: RouteHelper,
}

impl Renderer {
    pub fn new(options: RenderOptions) -> Result<Self, RenderError> {
        let routes = RouteHelper::new(&options.base_path)?;
        Ok(Self { options, routes })
    }

    pub fn routes(&self) -> &RouteHelper {
        &self.routes
    }

    pub fn render_site(
        &self,
        projection: &WikiProjection,
        context: &RenderContext,
    ) -> Result<RenderSite, RenderError> {
        let stylesheet = self.routes.asset(STYLESHEET_PATH)?;
        let mut pages = Vec::new();

        pages.push(self.render_home_page(projection, &stylesheet.route)?);

        for bundle in &projection.bundles {
            let bundle_index = BundleIndex::new(bundle)?;
            pages.push(self.render_bundle_page(
                bundle,
                &bundle_index,
                context,
                &stylesheet.route,
            )?);

            for directory in bundle
                .directories
                .iter()
                .filter(|directory| !directory.path.is_empty())
            {
                pages.push(self.render_directory_page(
                    bundle,
                    &bundle_index,
                    directory,
                    context,
                    &stylesheet.route,
                )?);
            }

            for concept in &bundle.concepts {
                pages.push(self.render_concept_page(
                    bundle,
                    &bundle_index,
                    concept,
                    context,
                    &stylesheet.route,
                )?);
            }

            for concept_type in &bundle_index.types {
                pages.push(self.render_type_page(
                    bundle,
                    &bundle_index,
                    concept_type,
                    context,
                    &stylesheet.route,
                )?);
            }

            for tag in &bundle_index.tags {
                pages.push(self.render_tag_page(
                    bundle,
                    &bundle_index,
                    tag,
                    context,
                    &stylesheet.route,
                )?);
            }

            pages.push(self.render_search_page(bundle, &bundle_index, &stylesheet.route)?);
            pages.push(self.render_graph_page(
                bundle,
                &bundle_index,
                context,
                &stylesheet.route,
            )?);
            pages.push(self.render_changes_page(bundle, &bundle_index, &stylesheet.route)?);
            pages.push(self.render_diagnostics_page(bundle, &bundle_index, &stylesheet.route)?);
        }

        pages.sort_by(|left, right| left.route.cmp(&right.route));

        let mut manifest_entries = Vec::new();
        for page in &pages {
            manifest_entries.push(ManifestEntry {
                kind: page.kind,
                route: page.route.clone(),
                output_path: page.output_path.to_string_lossy().into_owned(),
                title: page.title.clone(),
            });
        }

        let mut assets = vec![RenderedAsset {
            route: stylesheet.route.clone(),
            output_path: stylesheet.output_path.clone(),
            content_type: "text/css; charset=utf-8".to_owned(),
            bytes: WIKI_STYLESHEET.as_bytes().to_vec(),
        }];

        let manifest_route = self.routes.asset(ROUTE_MANIFEST_PATH)?;
        manifest_entries.push(ManifestEntry {
            kind: RouteKind::Manifest,
            route: manifest_route.route.clone(),
            output_path: manifest_route.output_path.to_string_lossy().into_owned(),
            title: "Route manifest".to_owned(),
        });

        let manifest = RouteManifest {
            entries: manifest_entries,
        };

        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(RenderError::serde)?;
        assets.push(RenderedAsset {
            route: manifest_route.route,
            output_path: manifest_route.output_path,
            content_type: "application/json".to_owned(),
            bytes: manifest_bytes,
        });
        assets.sort_by(|left, right| left.route.cmp(&right.route));

        Ok(RenderSite {
            pages,
            assets,
            manifest,
        })
    }

    fn render_home_page(
        &self,
        projection: &WikiProjection,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.home();
        let mut bundle_cards = String::new();
        for bundle in &projection.bundles {
            let bundle_route = self.routes.bundle(&bundle.id)?;
            let directory_count = bundle.directories.len();
            let concept_count = bundle.concepts.len();
            let diagnostics_count = bundle.diagnostics.len();
            bundle_cards.push_str(&format!(
                "<article class=\"panel\"><h2><a href=\"{href}\">{title}</a></h2><p class=\"muted\">OKF {version}</p><ul class=\"meta-list\"><li>{directories} directories</li><li>{concepts} concepts</li><li>{diagnostics} diagnostics</li></ul></article>",
                href = escape_html(&bundle_route.route),
                title = escape_html(&bundle.title),
                version = escape_html(&bundle.okf_version),
                directories = directory_count,
                concepts = concept_count,
                diagnostics = diagnostics_count,
            ));
        }

        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Knowledge Wiki</p><h1>Repository bundles</h1><p>Published from normalized OKF projections with stable routes, sanitized content, and package-owned assets.</p></section><section aria-labelledby=\"bundle-listing\"><h2 id=\"bundle-listing\">Available bundles</h2><div class=\"panel-grid\">{bundle_cards}</div></section>",
            bundle_cards = bundle_cards
        );

        let sidebar = format!(
            "<section class=\"panel\"><h2>Publication</h2><dl class=\"metadata\"><div><dt>Schema</dt><dd>{schema}</dd></div><div><dt>Generated</dt><dd>{generated_at}</dd></div><div><dt>Source revision</dt><dd>{source_revision}</dd></div></dl></section>",
            schema = projection.schema_version,
            generated_at = escape_html(&projection.generated_at),
            source_revision = escape_html(projection.source_revision.as_deref().unwrap_or("unavailable")),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Home,
            route,
            stylesheet_href,
            title: "Knowledge Wiki".to_owned(),
            description: "Published OKF bundle index".to_owned(),
            breadcrumbs: vec![Breadcrumb::current("Home")],
            page_nav: Vec::new(),
            main,
            sidebar,
            footer: "Static routes and assets are generated without client-side code.".to_owned(),
        }))
    }

    fn render_bundle_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.bundle(&bundle.id)?;
        let root_directory = bundle_index.root_directory();
        let root_body = root_directory
            .map(|directory| self.render_directory_body(bundle, bundle_index, directory))
            .transpose()?
            .unwrap_or_else(String::new);

        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Bundle</p><h1>{title}</h1><p>{description}</p></section><section aria-labelledby=\"bundle-summary\"><h2 id=\"bundle-summary\">Summary</h2><dl class=\"metadata\"><div><dt>Bundle id</dt><dd><code>{bundle_id}</code></dd></div><div><dt>Root path</dt><dd><code>{root_path}</code></dd></div><div><dt>OKF version</dt><dd>{okf_version}</dd></div><div><dt>Source revision</dt><dd>{source_revision}</dd></div></dl></section>{root_body}<section aria-labelledby=\"bundle-facets\"><h2 id=\"bundle-facets\">Browse by facet</h2><div class=\"panel-grid\">{type_panel}{tag_panel}</div></section>",
            title = escape_html(&bundle.title),
            description = escape_html(root_directory.and_then(|directory| directory.description.as_deref()).unwrap_or("Bundle overview and stable navigation.")),
            bundle_id = escape_html(&bundle.id),
            root_path = escape_html(&bundle.root_path),
            okf_version = escape_html(&bundle.okf_version),
            source_revision = escape_html(bundle.source_revision.as_deref().unwrap_or("unavailable")),
            root_body = root_body,
            type_panel = self.render_facet_panel(bundle, bundle_index, FacetKind::Type)?,
            tag_panel = self.render_facet_panel(bundle, bundle_index, FacetKind::Tag)?,
        );

        let sidebar = format!(
            "{}{}{}",
            render_link_list_panel("Primary routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel("Related graph context", context.bundle_items(&bundle.id)),
            render_link_list_panel(
                "Top concepts",
                &self.bundle_top_concepts(bundle, bundle_index)?
            ),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Bundle,
            route,
            stylesheet_href,
            title: bundle.title.clone(),
            description: format!("Bundle overview for {}", bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::current(&bundle.title),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Core reading and navigation stay available without JavaScript.".to_owned(),
        }))
    }

    fn render_directory_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        directory: &Directory,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.directory(&bundle.id, &directory.path)?;
        let title = if directory.title.is_empty() {
            directory.path.clone()
        } else {
            directory.title.clone()
        };
        let main = self.render_directory_body(bundle, bundle_index, directory)?;

        let sidebar = format!(
            "{}{}",
            render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel("Related graph context", context.bundle_items(&bundle.id)),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Directory,
            route,
            stylesheet_href,
            title: title.clone(),
            description: directory
                .description
                .clone()
                .unwrap_or_else(|| format!("Directory view for {}", directory.path)),
            breadcrumbs: self.directory_breadcrumbs(bundle, directory)?,
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Directory pages expose authored or synthetic indexes with deterministic hierarchy ordering."
                .to_owned(),
        }))
    }

    fn render_concept_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        concept: &Concept,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.concept(&bundle.id, &concept.id, None)?;
        let rendered_body = self.render_concept_body(bundle, bundle_index, concept)?;
        let citations = render_citations(&concept.citations);
        let backlinks = render_backlinks(bundle, bundle_index, concept, &self.routes)?;
        let source_directory = self
            .routes
            .concept_source_directory(&bundle.id, &concept.id)?;
        let source_context = format!(
            "<section aria-labelledby=\"source-context\"><h2 id=\"source-context\">Source context</h2><dl class=\"metadata\"><div><dt>Source path</dt><dd><code>{source_path}</code></dd></div><div><dt>Browse directory</dt><dd><a href=\"{directory_href}\">{directory_label}</a></dd></div></dl></section>",
            source_path = escape_html(&concept.source_path),
            directory_href = escape_html(&source_directory.route),
            directory_label = escape_html(&display_directory_path(&concept_parent_path(&concept.id))),
        );
        let diagnostics = render_concept_diagnostics(bundle, bundle_index, concept, &self.routes)?;
        let metadata = render_concept_metadata(concept);
        let main = format!(
            "<article><header class=\"hero\"><p class=\"eyebrow\">{concept_type}</p><h1>{title}</h1><p>{description}</p></header>{metadata}<section aria-labelledby=\"concept-body\"><h2 id=\"concept-body\">Body</h2><div class=\"prose\">{body}</div></section>{citations}{backlinks}{source_context}{diagnostics}</article>",
            concept_type = escape_html(&concept.concept_type),
            title = escape_html(concept.display_title()),
            description = escape_html(concept.description.as_deref().unwrap_or("Published concept page.")),
            metadata = metadata,
            body = rendered_body,
            citations = citations,
            backlinks = backlinks,
            source_context = source_context,
            diagnostics = diagnostics,
        );

        let sidebar = format!(
            "{}{}{}",
            render_table_of_contents(&concept.headings),
            render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel(
                "Related graph context",
                context.concept_items(&bundle.id, &concept.id),
            ),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Concept,
            route,
            stylesheet_href,
            title: concept.display_title().to_owned(),
            description: concept
                .description
                .clone()
                .unwrap_or_else(|| format!("Concept page for {}", concept.id)),
            breadcrumbs: self.concept_breadcrumbs(bundle, concept)?,
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Concept pages surface backlinks, citations, diagnostics, and related graph context without inline scripts."
                .to_owned(),
        }))
    }

    fn render_type_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        concept_type: &str,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.type_facet(&bundle.id, concept_type)?;
        let concepts = bundle_index
            .concepts_by_type
            .get(concept_type)
            .cloned()
            .unwrap_or_default();
        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Type</p><h1>{title}</h1><p>{count} concepts tagged with this type.</p></section>{list}",
            title = escape_html(concept_type),
            count = concepts.len(),
            list = render_concept_cards(bundle, &concepts, &self.routes)?,
        );
        let sidebar = format!(
            "{}{}",
            render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel("Related graph context", context.bundle_items(&bundle.id)),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::TypeFacet,
            route,
            stylesheet_href,
            title: format!("Type: {}", concept_type),
            description: format!("Browse {} concepts in {}", concept_type, bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current(concept_type),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Facet pages keep deterministic concept ordering and stable routes.".to_owned(),
        }))
    }

    fn render_tag_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        tag: &str,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.tag_facet(&bundle.id, tag)?;
        let concepts = bundle_index
            .concepts_by_tag
            .get(tag)
            .cloned()
            .unwrap_or_default();
        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Tag</p><h1>{title}</h1><p>{count} concepts tagged with this keyword.</p></section>{list}",
            title = escape_html(tag),
            count = concepts.len(),
            list = render_concept_cards(bundle, &concepts, &self.routes)?,
        );
        let sidebar = format!(
            "{}{}",
            render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel("Related graph context", context.bundle_items(&bundle.id)),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::TagFacet,
            route,
            stylesheet_href,
            title: format!("Tag: {}", tag),
            description: format!("Browse #{} concepts in {}", tag, bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current(tag),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Tag routes stay reversible through centralized segment encoding.".to_owned(),
        }))
    }

    fn render_search_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.search(&bundle.id)?;
        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Search</p><h1>Search {bundle}</h1><p>Server-rendered concept inventory for keyboard-first browsing.</p></section><section aria-labelledby=\"search-form\"><h2 id=\"search-form\">Search form</h2><form class=\"search-form\" action=\"{action}\" method=\"get\"><label for=\"query\">Query</label><input id=\"query\" name=\"q\" type=\"search\" autocomplete=\"off\" placeholder=\"Search title, id, tags, and headings\"><button type=\"submit\">Search</button></form></section>{list}",
            bundle = escape_html(&bundle.title),
            action = escape_html(&route.route),
            list = render_concept_cards(bundle, &bundle_index.sorted_concepts, &self.routes)?,
        );
        let sidebar = render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?);

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Search,
            route,
            stylesheet_href,
            title: format!("Search {}", bundle.title),
            description: format!("Search index for {}", bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current("Search"),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "The search route works as server-rendered navigation even when enhancements are unavailable."
                .to_owned(),
        }))
    }

    fn render_graph_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        context: &RenderContext,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.graph(&bundle.id)?;
        let mut graph_rows = String::new();
        for concept in &bundle_index.sorted_concepts {
            let concept_route = self.routes.concept(&bundle.id, &concept.id, None)?;
            let related = context.concept_items(&bundle.id, &concept.id);
            graph_rows.push_str(&format!(
                "<article class=\"panel\"><h2><a href=\"{href}\">{title}</a></h2><p class=\"muted\">{concept_type}</p><p>{summary}</p>{related}</article>",
                href = escape_html(&concept_route.route),
                title = escape_html(concept.display_title()),
                concept_type = escape_html(&concept.concept_type),
                summary = escape_html(concept.description.as_deref().unwrap_or("No description.")),
                related = render_related_context_inline(related),
            ));
        }
        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Graph neighborhood</p><h1>{bundle}</h1><p>Related graph context is optional and never blocks core wiki reading.</p></section><section aria-labelledby=\"graph-listing\"><h2 id=\"graph-listing\">Concept neighborhoods</h2><div class=\"panel-grid\">{graph_rows}</div></section>",
            bundle = escape_html(&bundle.title),
            graph_rows = graph_rows
        );
        let sidebar = format!(
            "{}{}",
            render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?),
            render_related_context_panel("Bundle-level context", context.bundle_items(&bundle.id)),
        );

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Graph,
            route,
            stylesheet_href,
            title: format!("Graph neighborhood for {}", bundle.title),
            description: format!("Optional graph context for {}", bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current("Graph neighborhood"),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Graph context stays explicitly bounded and degradable.".to_owned(),
        }))
    }

    fn render_changes_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.changes(&bundle.id)?;
        let mut items = bundle_index.change_entries.clone();
        items.sort_by(|left, right| right.sort_key.cmp(&left.sort_key));

        let mut rows = String::new();
        for item in items {
            rows.push_str(&format!(
                "<li><article><h2>{title}</h2><p class=\"muted\">{date}</p><p>{summary}</p>{link}</article></li>",
                title = escape_html(&item.title),
                date = escape_html(&item.date),
                summary = escape_html(&item.summary),
                link = item
                    .href
                    .map(|href| format!("<p><a href=\"{}\">Open</a></p>", escape_html(&href)))
                    .unwrap_or_default(),
            ));
        }

        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Recent changes</p><h1>{bundle}</h1><p>Scoped log entries and timestamped concepts sorted newest first.</p></section><section aria-labelledby=\"change-list\"><h2 id=\"change-list\">Published activity</h2><ol class=\"timeline\">{rows}</ol></section>",
            bundle = escape_html(&bundle.title),
            rows = rows
        );
        let sidebar = render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?);

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Changes,
            route,
            stylesheet_href,
            title: format!("Changes for {}", bundle.title),
            description: format!("Recent changes for {}", bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current("Changes"),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Change aggregation stays deterministic across authored logs and timestamped concepts."
                .to_owned(),
        }))
    }

    fn render_diagnostics_page(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        stylesheet_href: &str,
    ) -> Result<RenderedPage, RenderError> {
        let route = self.routes.diagnostics(&bundle.id, None)?;
        let diagnostics = render_bundle_diagnostics(bundle, &bundle_index.diagnostics());
        let main = format!(
            "<section class=\"hero\"><p class=\"eyebrow\">Diagnostics</p><h1>{bundle}</h1><p>Broken links and validation messages remain visible instead of silently failing.</p></section>{table}",
            bundle = escape_html(&bundle.title),
            table = render_diagnostics_table(&diagnostics),
        );
        let sidebar = render_link_list_panel("Bundle routes", &self.bundle_navigation(bundle)?);

        Ok(self.render_layout(RenderLayout {
            kind: RouteKind::Diagnostics,
            route,
            stylesheet_href,
            title: format!("Diagnostics for {}", bundle.title),
            description: format!("Diagnostics and broken links for {}", bundle.title),
            breadcrumbs: vec![
                Breadcrumb::link("Home", &self.routes.home().route),
                Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
                Breadcrumb::current("Diagnostics"),
            ],
            page_nav: self.bundle_navigation(bundle)?,
            main,
            sidebar,
            footer: "Unsafe or unresolved links point here through stable diagnostic anchors."
                .to_owned(),
        }))
    }

    fn render_directory_body(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        directory: &Directory,
    ) -> Result<String, RenderError> {
        let body = sanitize_markdown(&directory.body_markdown, &HashMap::new(), &[])?;
        let mut sections = String::new();
        sections.push_str(&format!(
            "<section aria-labelledby=\"directory-body\"><h2 id=\"directory-body\">Overview</h2><div class=\"prose\">{body}</div></section>",
            body = body
        ));

        let mut child_links = Vec::new();
        for child in &directory.child_directories {
            let route = self.routes.directory(&bundle.id, child)?;
            let label = bundle_index
                .directories
                .get(child.as_str())
                .map(|directory| {
                    if directory.title.is_empty() {
                        child.as_str()
                    } else {
                        directory.title.as_str()
                    }
                })
                .unwrap_or(child.as_str());
            child_links.push(LinkItem::new(label, &route.route));
        }
        sections.push_str(&render_link_list_panel("Child directories", &child_links));

        let concepts = directory
            .concept_ids
            .iter()
            .filter_map(|concept_id| bundle_index.concepts.get(concept_id.as_str()).copied())
            .collect::<Vec<_>>();
        sections.push_str(&render_concept_cards(bundle, &concepts, &self.routes)?);

        if !directory.log_entries.is_empty() {
            sections.push_str(&render_directory_log_entries(
                &directory.log_entries,
                &self.routes,
                bundle,
            )?);
        }

        Ok(sections)
    }

    fn render_concept_body(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        concept: &Concept,
    ) -> Result<String, RenderError> {
        let diagnostics_route = self.routes.diagnostics(&bundle.id, None)?;
        let mut link_targets = HashMap::new();
        for link in &concept.outbound_links {
            let target = match link.status {
                LinkStatus::External => LinkTarget::href(link.raw_href.clone()),
                LinkStatus::Resolved => {
                    let href = self.resolve_internal_link(bundle, bundle_index, concept, link)?;
                    LinkTarget::href(href)
                }
                LinkStatus::Broken | LinkStatus::Rejected => LinkTarget::href(format!(
                    "{}#{}",
                    diagnostics_route.route,
                    broken_link_anchor(&concept.id, &link.raw_href)
                )),
            };
            link_targets.insert(link.raw_href.clone(), target);
        }

        sanitize_markdown(&concept.body_markdown, &link_targets, &concept.headings)
    }

    fn resolve_internal_link(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        concept: &Concept,
        link: &Link,
    ) -> Result<String, RenderError> {
        let Some(target_id) = link.normalized_target_id.as_deref() else {
            let current = self
                .routes
                .concept(&bundle.id, &concept.id, link.fragment.as_deref())?;
            return Ok(current.route);
        };

        let diagnostics_route = self.routes.diagnostics(&bundle.id, None)?;
        let target_concept = match bundle_index.concepts.get(target_id) {
            Some(target) => *target,
            None => {
                return Ok(format!(
                    "{}#{}",
                    diagnostics_route.route,
                    broken_link_anchor(&concept.id, &link.raw_href)
                ))
            }
        };

        if let Some(fragment) = link.fragment.as_deref() {
            let fragment_exists = target_concept
                .headings
                .iter()
                .any(|heading| heading.id == fragment);
            if !fragment_exists {
                return Ok(format!(
                    "{}#{}",
                    diagnostics_route.route,
                    broken_link_anchor(&concept.id, &link.raw_href)
                ));
            }
        }

        Ok(self
            .routes
            .concept(&bundle.id, target_id, link.fragment.as_deref())?
            .route)
    }

    fn render_facet_panel(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
        facet_kind: FacetKind,
    ) -> Result<String, RenderError> {
        let mut items = Vec::new();
        match facet_kind {
            FacetKind::Type => {
                for value in &bundle_index.types {
                    items.push(LinkItem::new(
                        *value,
                        &self.routes.type_facet(&bundle.id, value)?.route,
                    ));
                }
                Ok(render_link_list_panel("Types", &items))
            }
            FacetKind::Tag => {
                for value in &bundle_index.tags {
                    items.push(LinkItem::new(
                        *value,
                        &self.routes.tag_facet(&bundle.id, value)?.route,
                    ));
                }
                Ok(render_link_list_panel("Tags", &items))
            }
        }
    }

    fn bundle_navigation(&self, bundle: &Bundle) -> Result<Vec<LinkItem>, RenderError> {
        Ok(vec![
            LinkItem::new("Overview", &self.routes.bundle(&bundle.id)?.route),
            LinkItem::new("Search", &self.routes.search(&bundle.id)?.route),
            LinkItem::new("Graph neighborhood", &self.routes.graph(&bundle.id)?.route),
            LinkItem::new("Changes", &self.routes.changes(&bundle.id)?.route),
            LinkItem::new(
                "Diagnostics",
                &self.routes.diagnostics(&bundle.id, None)?.route,
            ),
        ])
    }

    fn bundle_top_concepts(
        &self,
        bundle: &Bundle,
        bundle_index: &BundleIndex<'_>,
    ) -> Result<Vec<LinkItem>, RenderError> {
        let mut items = Vec::new();
        for concept in bundle_index.sorted_concepts.iter().take(6) {
            items.push(LinkItem::new(
                concept.display_title(),
                &self.routes.concept(&bundle.id, &concept.id, None)?.route,
            ));
        }
        Ok(items)
    }

    fn directory_breadcrumbs(
        &self,
        bundle: &Bundle,
        directory: &Directory,
    ) -> Result<Vec<Breadcrumb>, RenderError> {
        let mut breadcrumbs = vec![
            Breadcrumb::link("Home", &self.routes.home().route),
            Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
        ];

        let segments = split_logical_path(&directory.path)?;
        let mut current = Vec::new();
        for (index, segment) in segments.iter().enumerate() {
            current.push(segment.as_str());
            let joined = current.join("/");
            let crumb = if index + 1 == segments.len() {
                Breadcrumb::current(segment)
            } else {
                Breadcrumb::link(segment, &self.routes.directory(&bundle.id, &joined)?.route)
            };
            breadcrumbs.push(crumb);
        }
        Ok(breadcrumbs)
    }

    fn concept_breadcrumbs(
        &self,
        bundle: &Bundle,
        concept: &Concept,
    ) -> Result<Vec<Breadcrumb>, RenderError> {
        let mut breadcrumbs = vec![
            Breadcrumb::link("Home", &self.routes.home().route),
            Breadcrumb::link(&bundle.title, &self.routes.bundle(&bundle.id)?.route),
        ];

        let parent = concept_parent_path(&concept.id);
        if !parent.is_empty() {
            breadcrumbs.push(Breadcrumb::link(
                display_directory_path(&parent),
                &self.routes.directory(&bundle.id, &parent)?.route,
            ));
        }
        breadcrumbs.push(Breadcrumb::current(concept.display_title()));
        Ok(breadcrumbs)
    }

    fn render_layout(&self, layout: RenderLayout<'_>) -> RenderedPage {
        let title = if self.options.base_path.is_empty() {
            layout.title.clone()
        } else {
            format!(
                "{} | {}",
                layout.title,
                self.options.base_path.trim_matches('/')
            )
        };
        let body = PAGE_TEMPLATE
            .replace("__PAGE_TITLE__", &escape_html(&title))
            .replace(
                "__META_DESCRIPTION__",
                &escape_attribute(&layout.description),
            )
            .replace(
                "__STYLESHEET_HREF__",
                &escape_attribute(layout.stylesheet_href),
            )
            .replace("__BREADCRUMBS__", &render_breadcrumbs(&layout.breadcrumbs))
            .replace(
                "__PAGE_NAV__",
                &render_link_list_nav("Page navigation", &layout.page_nav),
            )
            .replace("__MAIN_CONTENT__", &layout.main)
            .replace("__SIDEBAR__", &layout.sidebar)
            .replace("__FOOTER__", &escape_html(&layout.footer));

        RenderedPage {
            kind: layout.kind,
            title: layout.title,
            route: layout.route.route,
            output_path: layout.route.output_path,
            html: body,
        }
    }
}

#[derive(Clone, Copy)]
enum FacetKind {
    Type,
    Tag,
}

struct RenderLayout<'a> {
    kind: RouteKind,
    route: RouteTarget,
    stylesheet_href: &'a str,
    title: String,
    description: String,
    breadcrumbs: Vec<Breadcrumb>,
    page_nav: Vec<LinkItem>,
    main: String,
    sidebar: String,
    footer: String,
}

#[derive(Clone, Debug)]
struct BundleIndex<'a> {
    directories: BTreeMap<&'a str, &'a Directory>,
    concepts: BTreeMap<&'a str, &'a Concept>,
    sorted_concepts: Vec<&'a Concept>,
    concepts_by_type: BTreeMap<&'a str, Vec<&'a Concept>>,
    concepts_by_tag: BTreeMap<&'a str, Vec<&'a Concept>>,
    types: BTreeSet<&'a str>,
    tags: BTreeSet<&'a str>,
    change_entries: Vec<ChangeEntry>,
    derived_diagnostics: Vec<RenderedDiagnostic>,
}

impl<'a> BundleIndex<'a> {
    fn new(bundle: &'a Bundle) -> Result<Self, RenderError> {
        let mut directories = BTreeMap::new();
        for directory in &bundle.directories {
            directories.insert(directory.path.as_str(), directory);
        }

        let mut concepts = BTreeMap::new();
        let mut sorted_concepts = bundle.concepts.iter().collect::<Vec<_>>();
        sorted_concepts.sort_by(|left, right| left.id.cmp(&right.id));

        let mut concepts_by_type: BTreeMap<&str, Vec<&Concept>> = BTreeMap::new();
        let mut concepts_by_tag: BTreeMap<&str, Vec<&Concept>> = BTreeMap::new();
        let mut types = BTreeSet::new();
        let mut tags = BTreeSet::new();
        let mut change_entries = Vec::new();
        let mut derived_diagnostics = Vec::new();

        for concept in &bundle.concepts {
            concepts.insert(concept.id.as_str(), concept);
            concepts_by_type
                .entry(concept.concept_type.as_str())
                .or_default()
                .push(concept);
            types.insert(concept.concept_type.as_str());
            for tag in &concept.tags {
                concepts_by_tag
                    .entry(tag.as_str())
                    .or_default()
                    .push(concept);
                tags.insert(tag.as_str());
            }

            if let Some(timestamp) = &concept.timestamp {
                change_entries.push(ChangeEntry {
                    sort_key: timestamp.clone(),
                    date: timestamp.clone(),
                    title: concept.display_title().to_owned(),
                    summary: concept
                        .description
                        .clone()
                        .unwrap_or_else(|| "Concept timestamp".to_owned()),
                    href: None,
                });
            }

            for link in &concept.outbound_links {
                if matches!(link.status, LinkStatus::Broken | LinkStatus::Rejected) {
                    let code = if link.status == LinkStatus::Broken {
                        "broken_link"
                    } else {
                        "rejected_link"
                    };
                    derived_diagnostics.push(RenderedDiagnostic {
                        anchor: broken_link_anchor(&concept.id, &link.raw_href),
                        severity: DiagnosticSeverity::Warning,
                        code: code.to_owned(),
                        source_path: concept.source_path.clone(),
                        line: None,
                        message: format!(
                            "{} from {}: {}",
                            diagnostic_label(link.status),
                            concept.id,
                            link.raw_href
                        ),
                    });
                }
            }
        }

        for concepts in concepts_by_type.values_mut() {
            concepts.sort_by(|left, right| left.id.cmp(&right.id));
        }
        for concepts in concepts_by_tag.values_mut() {
            concepts.sort_by(|left, right| left.id.cmp(&right.id));
        }

        for directory in &bundle.directories {
            for entry in &directory.log_entries {
                change_entries.push(ChangeEntry {
                    sort_key: entry.date.clone(),
                    date: entry.date.clone(),
                    title: format!(
                        "{}: {}",
                        entry.category,
                        display_directory_path(&entry.scope_path)
                    ),
                    summary: entry.text.clone(),
                    href: None,
                });
            }
        }

        Ok(Self {
            directories,
            concepts,
            sorted_concepts,
            concepts_by_type,
            concepts_by_tag,
            types,
            tags,
            change_entries,
            derived_diagnostics,
        })
    }

    fn root_directory(&self) -> Option<&'a Directory> {
        self.directories.get("").copied()
    }

    fn diagnostics(&self) -> Vec<RenderedDiagnostic> {
        self.derived_diagnostics.clone()
    }
}

#[derive(Clone, Debug)]
struct ChangeEntry {
    sort_key: String,
    date: String,
    title: String,
    summary: String,
    href: Option<String>,
}

#[derive(Clone, Debug)]
struct RenderedDiagnostic {
    anchor: String,
    severity: DiagnosticSeverity,
    code: String,
    source_path: String,
    line: Option<usize>,
    message: String,
}

#[derive(Clone, Debug)]
struct Breadcrumb {
    label: String,
    href: Option<String>,
}

impl Breadcrumb {
    fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

#[derive(Clone, Debug)]
struct LinkItem {
    label: String,
    href: String,
}

impl LinkItem {
    fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
        }
    }
}

#[derive(Clone, Debug)]
struct LinkTarget {
    href: String,
}

impl LinkTarget {
    fn href(href: impl Into<String>) -> Self {
        Self { href: href.into() }
    }
}

#[derive(Debug)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn io(error: std::io::Error) -> Self {
        Self::new(format!("renderer I/O failed: {error}"))
    }

    fn serde(error: serde_json::Error) -> Self {
        Self::new(format!("renderer serialization failed: {error}"))
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

fn sanitize_markdown(
    markdown: &str,
    link_targets: &HashMap<String, LinkTarget>,
    headings: &[Heading],
) -> Result<String, RenderError> {
    let mut options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    options.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(markdown, options);
    let mut heading_index = 0usize;
    let mut rewritten_events = Vec::new();

    for event in parser {
        let event = match event {
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                let id = headings
                    .get(heading_index)
                    .map(|heading| CowStr::from(heading.id.clone()));
                heading_index += 1;
                Event::Start(Tag::Heading {
                    level,
                    id,
                    classes: classes.into_iter().map(CowStr::into_static).collect(),
                    attrs: attrs
                        .into_iter()
                        .map(|(key, value)| (key.into_static(), value.map(CowStr::into_static)))
                        .collect(),
                })
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let href = link_targets
                    .get(dest_url.as_ref())
                    .map(|target| target.href.clone())
                    .unwrap_or_else(|| dest_url.to_string());
                Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::from(href),
                    title: title.into_static(),
                    id: id.into_static(),
                })
            }
            Event::Html(html) | Event::InlineHtml(html) => Event::Html(html.into_static()),
            other => other.into_static(),
        };
        rewritten_events.push(event);
    }

    let mut rendered = String::new();
    html::push_html(&mut rendered, rewritten_events.into_iter());

    let tags = HashSet::from([
        "a",
        "blockquote",
        "br",
        "code",
        "del",
        "em",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "li",
        "ol",
        "p",
        "pre",
        "strong",
        "table",
        "tbody",
        "td",
        "th",
        "thead",
        "tr",
        "ul",
    ]);
    let clean_content_tags = HashSet::from(["math", "script", "style", "svg", "template"]);
    let mut tag_attributes = HashMap::new();
    tag_attributes.insert("a", HashSet::from(["href", "title"]));
    tag_attributes.insert("th", HashSet::from(["colspan", "rowspan", "scope"]));
    tag_attributes.insert("td", HashSet::from(["colspan", "rowspan"]));
    tag_attributes.insert("h1", HashSet::from(["id"]));
    tag_attributes.insert("h2", HashSet::from(["id"]));
    tag_attributes.insert("h3", HashSet::from(["id"]));
    tag_attributes.insert("h4", HashSet::from(["id"]));
    tag_attributes.insert("h5", HashSet::from(["id"]));
    tag_attributes.insert("h6", HashSet::from(["id"]));
    let schemes = HashSet::from(["http", "https", "mailto"]);

    let sanitized = HtmlSanitizer::default()
        .tags(tags)
        .clean_content_tags(clean_content_tags)
        .tag_attributes(tag_attributes)
        .url_schemes(schemes)
        .url_relative(UrlRelative::PassThrough)
        .link_rel(Some("nofollow noopener noreferrer"))
        .clean(&rendered)
        .to_string();

    Ok(sanitized)
}

fn render_concept_metadata(concept: &Concept) -> String {
    let mut rows = vec![
        (
            "Concept id".to_owned(),
            format!("<code>{}</code>", escape_html(&concept.id)),
        ),
        ("Type".to_owned(), escape_html(&concept.concept_type)),
    ];

    if !concept.tags.is_empty() {
        rows.push((
            "Tags".to_owned(),
            concept
                .tags
                .iter()
                .map(|tag| format!("<code>{}</code>", escape_html(tag)))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }

    if let Some(timestamp) = &concept.timestamp {
        rows.push(("Timestamp".to_owned(), escape_html(timestamp)));
    }

    if let Some(resource) = &concept.resource {
        rows.push(("Resource".to_owned(), escape_html(resource)));
    }

    if !concept.extensions.is_empty() {
        let extensions = concept
            .extensions
            .iter()
            .map(|(key, value)| {
                format!(
                    "<li><code>{}</code>: {}</li>",
                    escape_html(key),
                    escape_html(&value.to_string())
                )
            })
            .collect::<Vec<_>>()
            .join("");
        rows.push((
            "Extensions".to_owned(),
            format!("<ul class=\"extension-list\">{extensions}</ul>"),
        ));
    }

    render_metadata_section("Metadata", &rows)
}

fn render_citations(citations: &[Citation]) -> String {
    if citations.is_empty() {
        return String::new();
    }

    let items = citations
        .iter()
        .map(|citation| {
            let link = citation
                .href
                .as_deref()
                .map(|href| format!(" <a href=\"{}\">Source</a>", escape_html(href)))
                .unwrap_or_default();
            format!(
                "<li><strong>[{number}]</strong> {text}{link}</li>",
                number = citation.number,
                text = escape_html(&citation.text),
                link = link,
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<section aria-labelledby=\"citations\"><h2 id=\"citations\">Citations</h2><ol class=\"citation-list\">{items}</ol></section>"
    )
}

fn render_backlinks(
    bundle: &Bundle,
    bundle_index: &BundleIndex<'_>,
    concept: &Concept,
    routes: &RouteHelper,
) -> Result<String, RenderError> {
    if concept.backlinks.is_empty() {
        return Ok(String::new());
    }

    let mut items = String::new();
    for backlink in &concept.backlinks {
        let title = bundle_index
            .concepts
            .get(backlink.source_id.as_str())
            .map(|source| source.display_title())
            .unwrap_or(backlink.source_id.as_str());
        let href = routes.concept(
            &bundle.id,
            &backlink.source_id,
            backlink.fragment.as_deref(),
        )?;
        items.push_str(&format!(
            "<li><a href=\"{href}\">{title}</a><p class=\"muted\">{context}</p></li>",
            href = escape_html(&href.route),
            title = escape_html(title),
            context = escape_html(backlink.context.as_deref().unwrap_or("Referenced concept")),
        ));
    }

    Ok(format!(
        "<section aria-labelledby=\"backlinks\"><h2 id=\"backlinks\">Backlinks</h2><ul class=\"link-list\">{items}</ul></section>"
    ))
}

fn render_concept_diagnostics(
    bundle: &Bundle,
    bundle_index: &BundleIndex<'_>,
    concept: &Concept,
    routes: &RouteHelper,
) -> Result<String, RenderError> {
    let diagnostics = bundle_index
        .diagnostics()
        .into_iter()
        .filter(|diagnostic| diagnostic.source_path == concept.source_path)
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        return Ok(String::new());
    }

    let diagnostics_route = routes.diagnostics(&bundle.id, None)?.route;
    let items = diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "<li><a href=\"{route}#{anchor}\">{code}</a>: {message}</li>",
                route = escape_html(&diagnostics_route),
                anchor = escape_html(&diagnostic.anchor),
                code = escape_html(&diagnostic.code),
                message = escape_html(&diagnostic.message),
            )
        })
        .collect::<Vec<_>>()
        .join("");
    Ok(format!(
        "<section aria-labelledby=\"concept-diagnostics\"><h2 id=\"concept-diagnostics\">Diagnostics</h2><ul class=\"link-list\">{items}</ul></section>"
    ))
}

fn render_table_of_contents(headings: &[Heading]) -> String {
    if headings.is_empty() {
        return String::new();
    }
    let items = headings
        .iter()
        .filter(|heading| heading.level > 1)
        .map(|heading| {
            format!(
                "<li class=\"toc-level-{level}\"><a href=\"#{id}\">{text}</a></li>",
                level = heading.level,
                id = escape_html(&heading.id),
                text = escape_html(&heading.text),
            )
        })
        .collect::<Vec<_>>()
        .join("");
    if items.is_empty() {
        return String::new();
    }
    format!(
        "<nav class=\"panel\" aria-labelledby=\"toc-heading\"><h2 id=\"toc-heading\">Table of contents</h2><ol class=\"toc-list\">{items}</ol></nav>"
    )
}

fn render_metadata_section(title: &str, rows: &[(String, String)]) -> String {
    let body = rows
        .iter()
        .map(|(term, value)| {
            format!(
                "<div><dt>{}</dt><dd>{}</dd></div>",
                escape_html(term),
                value
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        "<section aria-labelledby=\"metadata-heading\"><h2 id=\"metadata-heading\">{title}</h2><dl class=\"metadata\">{body}</dl></section>",
        title = escape_html(title),
        body = body,
    )
}

fn render_link_list_panel(title: &str, items: &[LinkItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| {
            format!(
                "<li><a href=\"{href}\">{label}</a></li>",
                href = escape_html(&item.href),
                label = escape_html(&item.label),
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        "<section class=\"panel\"><h2>{title}</h2><ul class=\"link-list\">{body}</ul></section>",
        title = escape_html(title),
        body = body,
    )
}

fn render_link_list_nav(title: &str, items: &[LinkItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| {
            format!(
                "<li><a href=\"{href}\">{label}</a></li>",
                href = escape_html(&item.href),
                label = escape_html(&item.label),
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        "<nav aria-label=\"{title}\"><ul class=\"page-nav\">{body}</ul></nav>",
        title = escape_attribute(title),
        body = body,
    )
}

fn render_related_context_panel(title: &str, items: &[RelatedContextItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| {
            let link = item
                .href
                .as_deref()
                .map(|href| format!(" <a href=\"{}\">Open</a>", escape_html(href)))
                .unwrap_or_default();
            format!(
                "<li><strong>{kind}</strong> {title}<p>{summary}</p>{link}</li>",
                kind = escape_html(&item.kind),
                title = escape_html(&item.title),
                summary = escape_html(&item.summary),
                link = link,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        "<section class=\"panel\"><h2>{title}</h2><ul class=\"context-list\">{body}</ul></section>",
        title = escape_html(title),
        body = body,
    )
}

fn render_related_context_inline(items: &[RelatedContextItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| {
            item.href
                .as_deref()
                .map(|href| {
                    format!(
                        "<li><a href=\"{}\">{}</a></li>",
                        escape_html(href),
                        escape_html(&item.title)
                    )
                })
                .unwrap_or_else(|| format!("<li>{}</li>", escape_html(&item.title)))
        })
        .collect::<Vec<_>>()
        .join("");
    format!("<ul class=\"inline-list\">{body}</ul>")
}

fn render_breadcrumbs(items: &[Breadcrumb]) -> String {
    let body = items
        .iter()
        .map(|item| match &item.href {
            Some(href) => format!(
                "<li><a href=\"{href}\">{label}</a></li>",
                href = escape_html(href),
                label = escape_html(&item.label),
            ),
            None => format!(
                "<li><span aria-current=\"page\">{label}</span></li>",
                label = escape_html(&item.label),
            ),
        })
        .collect::<Vec<_>>()
        .join("");
    format!("<nav class=\"breadcrumbs\" aria-label=\"Breadcrumbs\"><ol>{body}</ol></nav>")
}

fn render_concept_cards(
    bundle: &Bundle,
    concepts: &[&Concept],
    routes: &RouteHelper,
) -> Result<String, RenderError> {
    if concepts.is_empty() {
        return Ok(
            "<section class=\"panel\"><h2>Concepts</h2><p>No concepts published.</p></section>"
                .to_owned(),
        );
    }

    let mut cards = String::new();
    for concept in concepts {
        let href = routes.concept(&bundle.id, &concept.id, None)?;
        cards.push_str(&format!(
            "<article class=\"panel\"><h2><a href=\"{href}\">{title}</a></h2><p class=\"muted\">{concept_type}</p><p>{description}</p></article>",
            href = escape_html(&href.route),
            title = escape_html(concept.display_title()),
            concept_type = escape_html(&concept.concept_type),
            description = escape_html(concept.description.as_deref().unwrap_or("No description.")),
        ));
    }

    Ok(format!(
        "<section aria-labelledby=\"concept-listing\"><h2 id=\"concept-listing\">Concepts</h2><div class=\"panel-grid\">{cards}</div></section>"
    ))
}

fn render_directory_log_entries(
    entries: &[LogEntry],
    routes: &RouteHelper,
    bundle: &Bundle,
) -> Result<String, RenderError> {
    let mut rows = String::new();
    for entry in entries {
        let href = if entry.scope_path.is_empty() {
            routes.bundle(&bundle.id)?.route
        } else {
            routes.directory(&bundle.id, &entry.scope_path)?.route
        };
        rows.push_str(&format!(
            "<li><article><h3><a href=\"{href}\">{category}</a></h3><p class=\"muted\">{date}</p><p>{text}</p></article></li>",
            href = escape_html(&href),
            category = escape_html(&entry.category),
            date = escape_html(&entry.date),
            text = escape_html(&entry.text),
        ));
    }

    Ok(format!(
        "<section aria-labelledby=\"directory-log\"><h2 id=\"directory-log\">Log entries</h2><ol class=\"timeline\">{rows}</ol></section>"
    ))
}

fn render_diagnostics_table(diagnostics: &[RenderedDiagnostic]) -> String {
    if diagnostics.is_empty() {
        return "<section class=\"panel\"><h2>Diagnostics</h2><p>No diagnostics published.</p></section>".to_owned();
    }

    let rows = diagnostics
        .iter()
        .map(|diagnostic| {
            let line = diagnostic
                .line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "n/a".to_owned());
            format!(
                "<tr id=\"{anchor}\"><th scope=\"row\">{severity}</th><td><code>{code}</code></td><td><code>{source}</code></td><td>{line}</td><td>{message}</td></tr>",
                anchor = escape_html(&diagnostic.anchor),
                severity = escape_html(&format!("{:?}", diagnostic.severity).to_lowercase()),
                code = escape_html(&diagnostic.code),
                source = escape_html(&diagnostic.source_path),
                line = escape_html(&line),
                message = escape_html(&diagnostic.message),
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<section aria-labelledby=\"diagnostic-table\"><h2 id=\"diagnostic-table\">Diagnostics</h2><div class=\"table-wrap\"><table><thead><tr><th scope=\"col\">Severity</th><th scope=\"col\">Code</th><th scope=\"col\">Source</th><th scope=\"col\">Line</th><th scope=\"col\">Message</th></tr></thead><tbody>{rows}</tbody></table></div></section>"
    )
}

fn diagnostic_label(status: LinkStatus) -> &'static str {
    match status {
        LinkStatus::Broken => "Broken internal link",
        LinkStatus::Rejected => "Rejected unsafe link",
        LinkStatus::External => "External link",
        LinkStatus::Resolved => "Resolved link",
    }
}

fn broken_link_anchor(concept_id: &str, raw_href: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in concept_id.bytes().chain(raw_href.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("diag-{hash:016x}")
}

fn contained_output_path(output_root: &Path, relative_path: &Path) -> Result<PathBuf, RenderError> {
    if relative_path.is_absolute() {
        return Err(RenderError::new(format!(
            "output path escapes render root: {}",
            relative_path.display()
        )));
    }

    let mut contained = PathBuf::from(output_root);
    for component in relative_path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => contained.push(segment),
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(RenderError::new(format!(
                    "output path escapes render root: {}",
                    relative_path.display()
                )))
            }
        }
    }
    Ok(contained)
}

fn encode_path_segments(path: &str) -> Result<Vec<String>, RenderError> {
    split_logical_path(path)?
        .into_iter()
        .map(|segment| encode_segment(&segment))
        .collect()
}

fn split_logical_path(path: &str) -> Result<Vec<String>, RenderError> {
    let mut parts = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        validate_logical_segment(segment)?;
        parts.push(segment.to_owned());
    }
    Ok(parts)
}

fn concept_parent_path(concept_id: &str) -> String {
    let mut segments = concept_id
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() <= 1 {
        String::new()
    } else {
        segments.pop();
        segments.join("/")
    }
}

fn display_directory_path(path: &str) -> String {
    if path.is_empty() {
        "Bundle root".to_owned()
    } else {
        path.replace('/', " / ")
    }
}

fn encode_segment(segment: &str) -> Result<String, RenderError> {
    validate_logical_segment(segment)?;
    let mut encoded = String::new();
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(*byte))
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    Ok(encoded)
}

fn decode_segment(segment: &str) -> Result<String, RenderError> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(RenderError::new(format!(
                        "invalid percent-encoding in route segment: {segment}"
                    )));
                }
                let value = hex_value(bytes[index + 1])? << 4 | hex_value(bytes[index + 2])?;
                decoded.push(value);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    let decoded = String::from_utf8(decoded)
        .map_err(|_| RenderError::new(format!("route segment is not valid utf-8: {segment}")))?;
    validate_logical_segment(&decoded)?;
    Ok(decoded)
}

fn validate_logical_segment(segment: &str) -> Result<(), RenderError> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(RenderError::new(format!(
            "route segment is not allowed: {segment}"
        )));
    }
    if segment.contains('/') || segment.contains('\\') {
        return Err(RenderError::new(format!(
            "route segment contains a path separator: {segment}"
        )));
    }
    if segment.chars().any(|character| character.is_control()) {
        return Err(RenderError::new(format!(
            "route segment contains control characters: {segment}"
        )));
    }
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attribute(value: &str) -> String {
    escape_html(value)
}

fn escape_fragment_identifier(fragment: &str) -> String {
    fragment
        .chars()
        .map(|character| match character {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | ':' | '.' => character.to_string(),
            _ => format!("{:x}", character as u32),
        })
        .collect()
}

fn hex_value(value: u8) -> Result<u8, RenderError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(RenderError::new(format!(
            "invalid percent-encoded nibble: {}",
            char::from(value)
        ))),
    }
}

fn render_bundle_diagnostics(
    bundle: &Bundle,
    derived: &[RenderedDiagnostic],
) -> Vec<RenderedDiagnostic> {
    let mut diagnostics = bundle
        .diagnostics
        .iter()
        .map(render_external_diagnostic)
        .collect::<Vec<_>>();
    diagnostics.extend(derived.iter().cloned());
    diagnostics.sort_by(|left, right| {
        (
            &left.source_path,
            &left.code,
            &left.message,
            &left.anchor,
            left.line,
        )
            .cmp(&(
                &right.source_path,
                &right.code,
                &right.message,
                &right.anchor,
                right.line,
            ))
    });
    diagnostics
}

fn render_external_diagnostic(diagnostic: &Diagnostic) -> RenderedDiagnostic {
    RenderedDiagnostic {
        anchor: broken_link_anchor(&diagnostic.source_path, &diagnostic.message),
        severity: diagnostic.severity,
        code: diagnostic.code.clone(),
        source_path: diagnostic.source_path.clone(),
        line: diagnostic.line,
        message: diagnostic.message.clone(),
    }
}
