//! Local implementation of the transport-neutral wiki API.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::{
    api::{
        BundleSummary, GraphContextStatus, HealthResponse, NeighborhoodResponse, OkfWikiApi,
        ProjectionBuiltResponse, SiteRenderedResponse, ValidationProfile, ValidationResponse,
        WikiApiError, WikiOperationExecutor, WikiOperationRequest, WikiOperationResponse,
    },
    authoring::{
        AuthoringError, AuthoringService, AuthoringValidator, CreateBundleRequest,
        CreateBundleResult, CreatePageRequest, CreatePageResult, PopulatePageRequest,
        PopulatePageResult, RefreshNotifier,
    },
    bundle::{discover_bundles, BundleEntryKind, LoadedBundle},
    compiler::{
        compile_projection, CompileRequest, SourceBundle, SourceDocument, SourceDocumentKind,
    },
    conformance::{validate_bundle, ConformanceProfile},
    model::WikiProjection,
    render::{RenderContext, RenderOptions, Renderer},
    search::{SearchIndex, SearchQuery},
    WIKI_SCHEMA_VERSION,
};

pub trait AuthoringOperations: Send + Sync {
    fn create_bundle(
        &self,
        request: CreateBundleRequest,
    ) -> Result<CreateBundleResult, AuthoringError>;
    fn create_page(&self, request: CreatePageRequest) -> Result<CreatePageResult, AuthoringError>;
    fn populate_page(
        &self,
        request: PopulatePageRequest,
    ) -> Result<PopulatePageResult, AuthoringError>;
}

impl<V, R> AuthoringOperations for AuthoringService<V, R>
where
    V: AuthoringValidator + Send + Sync,
    R: RefreshNotifier + Send + Sync,
{
    fn create_bundle(
        &self,
        request: CreateBundleRequest,
    ) -> Result<CreateBundleResult, AuthoringError> {
        AuthoringService::create_bundle(self, request)
    }

    fn create_page(&self, request: CreatePageRequest) -> Result<CreatePageResult, AuthoringError> {
        AuthoringService::create_page(self, request)
    }

    fn populate_page(
        &self,
        request: PopulatePageRequest,
    ) -> Result<PopulatePageResult, AuthoringError> {
        AuthoringService::populate_page(self, request)
    }
}

pub struct LocalWikiService {
    bundle_roots: Vec<PathBuf>,
    projection: Mutex<Option<WikiProjection>>,
    authoring: Option<Box<dyn AuthoringOperations>>,
}

impl LocalWikiService {
    pub fn new(bundle_roots: Vec<PathBuf>) -> Self {
        Self {
            bundle_roots,
            projection: Mutex::new(None),
            authoring: None,
        }
    }

    pub fn with_authoring(mut self, authoring: impl AuthoringOperations + 'static) -> Self {
        self.authoring = Some(Box::new(authoring));
        self
    }

    pub fn into_api(self) -> OkfWikiApi<Self> {
        OkfWikiApi::new(self)
    }

    fn compile(
        &self,
        roots: &[PathBuf],
        generated_at: &str,
        source_revision: Option<String>,
    ) -> Result<WikiProjection, WikiApiError> {
        let loaded = discover_bundles(roots)
            .map_err(|_| WikiApiError::new("bundle_not_found", "bundle could not be loaded"))?;
        let mut diagnostics = loaded
            .iter()
            .map(|bundle| (bundle.id.clone(), bundle.diagnostics.clone()))
            .collect::<BTreeMap<_, _>>();
        let request = CompileRequest {
            generated_at: generated_at.to_string(),
            source_revision,
            bundles: loaded.into_iter().map(source_bundle).collect(),
        };
        let mut projection = compile_projection(request);
        for bundle in &mut projection.bundles {
            bundle
                .diagnostics
                .extend(diagnostics.remove(&bundle.id).unwrap_or_default());
            bundle.normalize();
        }
        Ok(projection)
    }

    fn projection(&self) -> Result<WikiProjection, WikiApiError> {
        let mut guard = self
            .projection
            .lock()
            .map_err(|_| WikiApiError::new("build_in_progress", "wiki state is unavailable"))?;
        if let Some(projection) = guard.as_ref() {
            return Ok(projection.clone());
        }
        let projection = self.compile(&self.bundle_roots, "runtime", None)?;
        *guard = Some(projection.clone());
        Ok(projection)
    }

    fn invalidate_projection(&self) {
        if let Ok(mut projection) = self.projection.lock() {
            *projection = None;
        }
    }

    fn authoring(&self) -> Result<&dyn AuthoringOperations, WikiApiError> {
        self.authoring.as_deref().ok_or_else(|| {
            WikiApiError::new(
                "invalid_request",
                "controlled authoring is not configured for this wiki",
            )
        })
    }
}

impl WikiOperationExecutor for LocalWikiService {
    fn execute(
        &self,
        request: &WikiOperationRequest,
    ) -> Result<WikiOperationResponse, WikiApiError> {
        match request {
            WikiOperationRequest::Health(_) => {
                let projection_available = self
                    .projection
                    .lock()
                    .map(|projection| projection.is_some())
                    .unwrap_or(false);
                Ok(WikiOperationResponse::Health(HealthResponse {
                    ok: true,
                    schema_version: WIKI_SCHEMA_VERSION,
                    projection_available,
                }))
            }
            WikiOperationRequest::ValidateBundle(request) => {
                let bundle_root = self.configured_bundle_root(&request.bundle_root)?;
                let loaded = crate::bundle::load_bundle(&bundle_root).map_err(|_| {
                    WikiApiError::new("bundle_not_found", "bundle could not be loaded")
                })?;
                let report = validate_bundle(&loaded, validation_profile(request.profile));
                Ok(WikiOperationResponse::Validation(ValidationResponse {
                    accepted: report.accepted,
                    diagnostics: report.diagnostics,
                }))
            }
            WikiOperationRequest::CheckLinks(request) => {
                let bundle_root = self.configured_bundle_root(&request.bundle_root)?;
                let loaded = crate::bundle::load_bundle(&bundle_root).map_err(|_| {
                    WikiApiError::new("bundle_not_found", "bundle could not be loaded")
                })?;
                let report = validate_bundle(&loaded, ConformanceProfile::Conformant);
                Ok(WikiOperationResponse::Diagnostics(report.diagnostics))
            }
            WikiOperationRequest::CreateBundle(request) => {
                let result = self
                    .authoring()?
                    .create_bundle(request.clone())
                    .map_err(authoring_error)?;
                self.invalidate_projection();
                Ok(WikiOperationResponse::BundleCreated(result))
            }
            WikiOperationRequest::CreatePage(request) => {
                let result = self
                    .authoring()?
                    .create_page(request.clone())
                    .map_err(authoring_error)?;
                self.invalidate_projection();
                Ok(WikiOperationResponse::PageCreated(result))
            }
            WikiOperationRequest::PopulatePage(request) => {
                let result = self
                    .authoring()?
                    .populate_page(request.clone())
                    .map_err(authoring_error)?;
                self.invalidate_projection();
                Ok(WikiOperationResponse::PagePopulated(result))
            }
            WikiOperationRequest::BuildProjection(request) => {
                let roots = if request.bundle_roots.is_empty() {
                    &self.bundle_roots
                } else {
                    &request.bundle_roots
                };
                let projection = self.compile(
                    roots,
                    &request.generated_at,
                    request.source_revision.clone(),
                )?;
                let cache_hit = self
                    .projection
                    .lock()
                    .map(|current| current.as_ref() == Some(&projection))
                    .unwrap_or(false);
                *self.projection.lock().map_err(|_| {
                    WikiApiError::new("build_in_progress", "wiki state is unavailable")
                })? = Some(projection.clone());
                Ok(WikiOperationResponse::ProjectionBuilt(
                    ProjectionBuiltResponse {
                        projection,
                        cache_hit,
                    },
                ))
            }
            WikiOperationRequest::ListBundles(_) => {
                let projection = self.projection()?;
                Ok(WikiOperationResponse::Bundles(
                    projection.bundles.iter().map(BundleSummary::from).collect(),
                ))
            }
            WikiOperationRequest::GetDirectory(request) => {
                let projection = self.projection()?;
                let directory = projection
                    .bundles
                    .iter()
                    .find(|bundle| bundle.id == request.bundle_id)
                    .and_then(|bundle| {
                        bundle
                            .directories
                            .iter()
                            .find(|directory| directory.path == request.path)
                    })
                    .cloned()
                    .ok_or_else(|| {
                        WikiApiError::new("concept_not_found", "directory was not found")
                    })?;
                Ok(WikiOperationResponse::Directory(directory))
            }
            WikiOperationRequest::GetConcept(request) => {
                let projection = self.projection()?;
                let concept = find_concept(&projection, &request.bundle_id, &request.concept_id)?;
                Ok(WikiOperationResponse::Concept(concept.clone()))
            }
            WikiOperationRequest::SearchConcepts(request) => {
                let projection = self.projection()?;
                let index = SearchIndex::build(&projection);
                let query = SearchQuery {
                    text: request.text.clone(),
                    bundle: request.bundle_id.clone(),
                    concept_type: request.concept_type.clone(),
                    tags: request.tags.clone(),
                    limit: request.limit.min(100),
                };
                Ok(WikiOperationResponse::Search(index.search(&query)))
            }
            WikiOperationRequest::GetBacklinks(request) => {
                let projection = self.projection()?;
                let concept = find_concept(&projection, &request.bundle_id, &request.concept_id)?;
                Ok(WikiOperationResponse::Backlinks(concept.backlinks.clone()))
            }
            WikiOperationRequest::GetNeighborhood(request) => {
                let projection = self.projection()?;
                let concept = find_concept(&projection, &request.bundle_id, &request.concept_id)?;
                let limit = request.limit.clamp(1, 100);
                Ok(WikiOperationResponse::Neighborhood(NeighborhoodResponse {
                    concept_id: concept.id.clone(),
                    outgoing: concept.outbound_links.iter().take(limit).cloned().collect(),
                    backlinks: concept.backlinks.iter().take(limit).cloned().collect(),
                    graph_context: Some(crate::api::GraphContextSummary {
                        status: GraphContextStatus::Disabled,
                        items: Vec::new(),
                    }),
                }))
            }
            WikiOperationRequest::GetDiagnostics(request) => {
                let loaded = discover_bundles(&self.bundle_roots)
                    .map_err(|_| {
                        WikiApiError::new("bundle_not_found", "bundle could not be loaded")
                    })?
                    .into_iter()
                    .find(|bundle| bundle.id == request.bundle_id)
                    .ok_or_else(|| WikiApiError::new("bundle_not_found", "bundle was not found"))?;
                let report = validate_bundle(&loaded, validation_profile(request.profile));
                Ok(WikiOperationResponse::Diagnostics(report.diagnostics))
            }
            WikiOperationRequest::GetRecentChanges(request) => {
                let projection = self.projection()?;
                let mut changes = projection
                    .bundles
                    .iter()
                    .find(|bundle| bundle.id == request.bundle_id)
                    .ok_or_else(|| WikiApiError::new("bundle_not_found", "bundle was not found"))?
                    .directories
                    .iter()
                    .filter(|directory| {
                        request
                            .path
                            .as_ref()
                            .map(|path| directory.path == *path)
                            .unwrap_or(directory.path.is_empty())
                    })
                    .flat_map(|directory| directory.log_entries.clone())
                    .collect::<Vec<_>>();
                changes.sort_by(|left, right| right.date.cmp(&left.date));
                changes.truncate(request.limit.clamp(1, 500));
                Ok(WikiOperationResponse::RecentChanges(changes))
            }
            WikiOperationRequest::BuildSite(request) => {
                let bundle_root = self.configured_bundle_root(&request.bundle_root)?;
                let projection =
                    self.compile(std::slice::from_ref(&bundle_root), "runtime", None)?;
                self.render_projection(projection, &request.output_root, &request.base_url)
            }
            WikiOperationRequest::RenderSite(request) => {
                let mut projection = self.projection()?;
                if !request.bundle_ids.is_empty() {
                    projection
                        .bundles
                        .retain(|bundle| request.bundle_ids.contains(&bundle.id));
                    if projection.bundles.is_empty() {
                        return Err(WikiApiError::new(
                            "bundle_not_found",
                            "no requested bundle was found",
                        ));
                    }
                    projection.normalize();
                }
                self.render_projection(projection, &request.output_root, &request.base_url)
            }
        }
    }
}

impl LocalWikiService {
    fn configured_bundle_root(&self, requested: &std::path::Path) -> Result<PathBuf, WikiApiError> {
        let requested = requested
            .canonicalize()
            .map_err(|_| WikiApiError::new("bundle_not_found", "bundle could not be loaded"))?;
        self.bundle_roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .find(|root| *root == requested)
            .ok_or_else(|| {
                WikiApiError::new("bundle_not_found", "bundle is not configured for this wiki")
            })
    }

    fn render_projection(
        &self,
        projection: WikiProjection,
        output_root: &std::path::Path,
        base_url: &str,
    ) -> Result<WikiOperationResponse, WikiApiError> {
        let renderer = Renderer::new(RenderOptions {
            base_path: base_url.to_string(),
        })
        .map_err(|_| WikiApiError::new("render_failed", "site could not be rendered"))?;
        let site = renderer
            .render_site(&projection, &RenderContext::default())
            .map_err(|_| WikiApiError::new("render_failed", "site could not be rendered"))?;
        site.write_to(output_root)
            .map_err(|_| WikiApiError::new("render_failed", "site could not be written"))?;
        Ok(WikiOperationResponse::SiteRendered(SiteRenderedResponse {
            route_count: site.pages.len(),
            asset_count: site.assets.len(),
        }))
    }
}

fn validation_profile(profile: ValidationProfile) -> ConformanceProfile {
    match profile {
        ValidationProfile::Consume => ConformanceProfile::Consume,
        ValidationProfile::Conformant => ConformanceProfile::Conformant,
        ValidationProfile::Recommended => ConformanceProfile::Recommended,
    }
}

fn source_bundle(bundle: LoadedBundle) -> SourceBundle {
    let documents = bundle
        .entries
        .iter()
        .map(|entry| {
            let frontmatter = entry.frontmatter();
            SourceDocument {
                path: entry.source_path.clone(),
                kind: match entry.kind {
                    BundleEntryKind::Concept => SourceDocumentKind::Concept,
                    BundleEntryKind::Index => SourceDocumentKind::Index,
                    BundleEntryKind::Log => SourceDocumentKind::Log,
                },
                title: frontmatter.and_then(|value| value.fields.title.clone()),
                description: frontmatter.and_then(|value| value.fields.description.clone()),
                concept_type: frontmatter.and_then(|value| value.fields.type_name.clone()),
                resource: frontmatter.and_then(|value| value.fields.resource.clone()),
                tags: frontmatter
                    .map(|value| value.fields.tags.clone())
                    .unwrap_or_default(),
                timestamp: frontmatter.and_then(|value| value.fields.timestamp.clone()),
                extensions: frontmatter
                    .map(|value| value.extensions.clone())
                    .unwrap_or_default(),
                body_markdown: entry.document.body_markdown.clone(),
            }
        })
        .collect();
    SourceBundle {
        id: bundle.id.clone(),
        root_path: bundle.id,
        okf_version: bundle.okf_version.unwrap_or_else(|| "0.1".to_string()),
        title: bundle
            .title
            .unwrap_or_else(|| "Knowledge Bundle".to_string()),
        source_revision: None,
        documents,
    }
}

fn find_concept<'a>(
    projection: &'a WikiProjection,
    bundle_id: &str,
    concept_id: &str,
) -> Result<&'a crate::model::Concept, WikiApiError> {
    projection
        .bundles
        .iter()
        .find(|bundle| bundle.id == bundle_id)
        .and_then(|bundle| {
            bundle
                .concepts
                .iter()
                .find(|concept| concept.id == concept_id)
        })
        .ok_or_else(|| WikiApiError::new("concept_not_found", "concept was not found"))
}

fn authoring_error(error: AuthoringError) -> WikiApiError {
    WikiApiError::new(error.code(), safe_authoring_message(error.message()))
}

fn safe_authoring_message(message: &str) -> String {
    if Path::new(message).is_absolute()
        || message.contains("`/")
        || message.contains(" /")
        || message.contains(":\\")
    {
        "authoring operation failed".to_string()
    } else {
        message.to_string()
    }
}
