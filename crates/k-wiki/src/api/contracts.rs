use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    authoring::{
        CreateBundleRequest, CreateBundleResult, CreatePageRequest, CreatePageResult,
        PopulatePageRequest, PopulatePageResult,
    },
    diagnostic::Diagnostic,
    model::{Bundle, Concept, Directory, Link, LogEntry, WikiProjection},
    search::SearchResult,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationProfile {
    Consume,
    Conformant,
    Recommended,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
pub enum WikiOperationRequest {
    Health(HealthRequest),
    ValidateBundle(ValidateBundleRequest),
    CheckLinks(CheckLinksRequest),
    CreateBundle(CreateBundleRequest),
    CreatePage(CreatePageRequest),
    PopulatePage(PopulatePageRequest),
    BuildProjection(BuildProjectionRequest),
    ListBundles(ListBundlesRequest),
    GetDirectory(GetDirectoryRequest),
    GetConcept(GetConceptRequest),
    SearchConcepts(SearchConceptsRequest),
    GetBacklinks(GetBacklinksRequest),
    GetNeighborhood(GetNeighborhoodRequest),
    GetDiagnostics(GetDiagnosticsRequest),
    GetRecentChanges(GetRecentChangesRequest),
    BuildSite(BuildSiteRequest),
    RenderSite(RenderSiteRequest),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HealthRequest {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidateBundleRequest {
    pub bundle_root: PathBuf,
    pub profile: ValidationProfile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CheckLinksRequest {
    pub bundle_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildProjectionRequest {
    pub bundle_roots: Vec<PathBuf>,
    pub generated_at: String,
    pub source_revision: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ListBundlesRequest {
    #[serde(default)]
    pub repository_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetDirectoryRequest {
    pub bundle_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetConceptRequest {
    pub bundle_id: String,
    pub concept_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchConceptsRequest {
    pub text: String,
    pub bundle_id: Option<String>,
    pub concept_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetBacklinksRequest {
    pub bundle_id: String,
    pub concept_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetNeighborhoodRequest {
    pub bundle_id: String,
    pub concept_id: String,
    #[serde(default = "default_neighborhood_depth")]
    pub depth: usize,
    #[serde(default = "default_neighborhood_limit")]
    pub limit: usize,
}

fn default_neighborhood_depth() -> usize {
    1
}

fn default_neighborhood_limit() -> usize {
    20
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetDiagnosticsRequest {
    pub bundle_id: String,
    pub profile: ValidationProfile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetRecentChangesRequest {
    pub bundle_id: String,
    pub path: Option<String>,
    #[serde(default = "default_change_limit")]
    pub limit: usize,
}

fn default_change_limit() -> usize {
    50
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderSiteRequest {
    #[serde(default)]
    pub bundle_ids: Vec<String>,
    pub output_root: PathBuf,
    #[serde(default)]
    pub base_url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildSiteRequest {
    pub bundle_root: PathBuf,
    pub output_root: PathBuf,
    #[serde(default)]
    pub base_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum WikiOperationResponse {
    Health(HealthResponse),
    Validation(ValidationResponse),
    BundleCreated(CreateBundleResult),
    PageCreated(CreatePageResult),
    PagePopulated(PopulatePageResult),
    ProjectionBuilt(ProjectionBuiltResponse),
    Bundles(Vec<BundleSummary>),
    Directory(Directory),
    Concept(Concept),
    Search(Vec<SearchResult>),
    Backlinks(Vec<Link>),
    Neighborhood(NeighborhoodResponse),
    Diagnostics(Vec<Diagnostic>),
    RecentChanges(Vec<LogEntry>),
    SiteRendered(SiteRenderedResponse),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub schema_version: u32,
    pub projection_available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationResponse {
    pub accepted: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectionBuiltResponse {
    pub projection: WikiProjection,
    pub cache_hit: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundleSummary {
    pub id: String,
    pub title: String,
    pub okf_version: String,
    pub concept_count: usize,
    pub directory_count: usize,
}

impl From<&Bundle> for BundleSummary {
    fn from(bundle: &Bundle) -> Self {
        Self {
            id: bundle.id.clone(),
            title: bundle.title.clone(),
            okf_version: bundle.okf_version.clone(),
            concept_count: bundle.concepts.len(),
            directory_count: bundle.directories.len(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeighborhoodResponse {
    pub concept_id: String,
    pub outgoing: Vec<Link>,
    pub backlinks: Vec<Link>,
    pub graph_context: Option<GraphContextSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphContextSummary {
    pub status: GraphContextStatus,
    pub items: Vec<GraphContextItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphContextStatus {
    Available,
    Degraded,
    Disabled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphContextItem {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub path: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SiteRenderedResponse {
    pub route_count: usize,
    pub asset_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WikiApiError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
    pub retryable: bool,
}

impl WikiApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            retryable: false,
        }
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

impl std::fmt::Display for WikiApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WikiApiError {}
