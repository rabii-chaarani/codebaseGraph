//! Versioned projection storage.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    diagnostic::Diagnostic,
    model::{Bundle, WikiProjection},
    WIKI_SCHEMA_VERSION,
};

pub const STATE_ROOT_DIR: &str = ".kWiki";
pub const STAGING_DIR: &str = "staging";
pub const GENERATIONS_DIR: &str = "generations";
pub const PROJECTIONS_DIR: &str = "projections";
pub const SEARCH_DIR: &str = "search";
pub const CACHE_DIR: &str = "cache";
pub const SITE_DIR: &str = "site";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const CURRENT_FILE: &str = "current.json";
pub const DIAGNOSTICS_FILE: &str = "diagnostics.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GenerationToken {
    pub generation_id: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GenerationPointer {
    pub generation_id: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublishedBundleArtifact {
    pub bundle_id: String,
    pub root_path: String,
    pub source_revision: Option<String>,
    pub projection_path: String,
    pub search_path: Option<String>,
    pub content_hash: String,
    pub output_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionManifest {
    pub schema_version: u32,
    pub generation: GenerationPointer,
    pub last_successful_generation: GenerationPointer,
    pub generated_at: String,
    pub okf_version: String,
    pub source_revision: Option<String>,
    pub build_duration_ms: u64,
    pub bundle_roots: BTreeMap<String, String>,
    pub bundles: BTreeMap<String, PublishedBundleArtifact>,
    pub dependency_edges: BTreeMap<String, BundleDependencyIndex>,
    pub content_hashes: BTreeMap<String, String>,
    pub output_hashes: BTreeMap<String, String>,
    pub diagnostics_path: String,
    pub diagnostics_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundleCacheEntry {
    pub schema_version: u32,
    pub bundle_id: String,
    pub generation_id: String,
    pub content_hash: String,
    pub output_hash: String,
    pub projection_path: String,
    pub search_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundleDependencyIndex {
    pub concept_sources: BTreeMap<String, ConceptDependency>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConceptDependency {
    pub concept_id: String,
    pub source_path: String,
    pub search_document: String,
    pub outgoing_targets: BTreeSet<String>,
    pub ancestor_directories: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundleInvalidation {
    pub concept_pages: BTreeSet<String>,
    pub search_documents: BTreeSet<String>,
    pub backlink_pages: BTreeSet<String>,
    pub directory_pages: BTreeSet<String>,
    pub history_pages: BTreeSet<String>,
    pub outbound_edges: BTreeSet<OutboundEdgeInvalidation>,
    pub aggregate_history: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OutboundEdgeInvalidation {
    pub source_id: String,
    pub target_id: String,
}

#[derive(Clone, Debug)]
pub struct BundlePublication {
    pub bundle: Bundle,
    pub dependency_index: BundleDependencyIndex,
    pub search_artifact: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct PublishRequest {
    pub token: GenerationToken,
    pub generated_at: String,
    pub okf_version: String,
    pub source_revision: Option<String>,
    pub build_duration_ms: u64,
    pub bundles: Vec<BundlePublication>,
    pub diagnostics: Vec<Diagnostic>,
    pub inject_failure: Option<FailurePoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePoint {
    AfterStageWrite,
    BeforePointerSwap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheStatus {
    Hit,
    Mixed,
    Miss,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishOutcome {
    pub cache_status: CacheStatus,
    pub manifest: ProjectionManifest,
    pub published: bool,
}

#[derive(Debug)]
pub enum ProjectionStoreError {
    Io(io::Error),
    Serde(serde_json::Error),
    StaleGeneration { attempted: u64, latest: u64 },
    Validation(String),
    FailureInjected(FailurePoint),
}

impl fmt::Display for ProjectionStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "projection store I/O failed: {error}"),
            Self::Serde(error) => write!(f, "projection store serialization failed: {error}"),
            Self::StaleGeneration { attempted, latest } => write!(
                f,
                "stale generation token {attempted} cannot publish after {latest}"
            ),
            Self::Validation(message) => write!(f, "projection validation failed: {message}"),
            Self::FailureInjected(point) => write!(f, "failure injected at {point:?}"),
        }
    }
}

impl std::error::Error for ProjectionStoreError {}

impl From<io::Error> for ProjectionStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ProjectionStoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}

#[derive(Default)]
struct StoreRuntime {
    latest_issued_sequence: u64,
    latest_published_sequence: u64,
}

#[derive(Clone, Default)]
pub struct ProjectionStore {
    repository_root: PathBuf,
    runtime: Arc<Mutex<StoreRuntime>>,
}

impl BundleDependencyIndex {
    pub fn from_bundle(bundle: &Bundle) -> Self {
        let concept_sources = bundle
            .concepts
            .iter()
            .map(|concept| {
                let outgoing_targets = concept
                    .outbound_links
                    .iter()
                    .filter_map(|link| link.normalized_target_id.clone())
                    .collect::<BTreeSet<_>>();

                (
                    normalize_relative_path(&concept.source_path),
                    ConceptDependency {
                        concept_id: concept.id.clone(),
                        source_path: normalize_relative_path(&concept.source_path),
                        search_document: concept.id.clone(),
                        outgoing_targets,
                        ancestor_directories: ancestor_directories_for_source(&concept.source_path),
                    },
                )
            })
            .collect();

        Self { concept_sources }
    }

    pub fn invalidate_paths<'a>(
        &self,
        changed_paths: impl IntoIterator<Item = &'a str>,
    ) -> BundleInvalidation {
        let mut invalidation = BundleInvalidation::default();

        for path in changed_paths {
            let path = normalize_relative_path(path);

            if let Some(concept) = self.concept_sources.get(&path) {
                invalidation
                    .concept_pages
                    .insert(concept.concept_id.clone());
                invalidation
                    .search_documents
                    .insert(concept.search_document.clone());

                for target in &concept.outgoing_targets {
                    invalidation.backlink_pages.insert(target.clone());
                    invalidation
                        .outbound_edges
                        .insert(OutboundEdgeInvalidation {
                            source_id: concept.concept_id.clone(),
                            target_id: target.clone(),
                        });
                }
                continue;
            }

            if is_named_source(&path, "index.md") {
                let scope_path = parent_scope_for_source(&path);
                for ancestor in ancestor_directories(&scope_path) {
                    invalidation.directory_pages.insert(ancestor);
                }
                continue;
            }

            if is_named_source(&path, "log.md") {
                invalidation
                    .history_pages
                    .insert(parent_scope_for_source(&path));
                invalidation.aggregate_history = true;
            }
        }

        invalidation
    }
}

impl BundleInvalidation {
    pub fn merge(&mut self, other: Self) {
        self.concept_pages.extend(other.concept_pages);
        self.search_documents.extend(other.search_documents);
        self.backlink_pages.extend(other.backlink_pages);
        self.directory_pages.extend(other.directory_pages);
        self.history_pages.extend(other.history_pages);
        self.outbound_edges.extend(other.outbound_edges);
        self.aggregate_history |= other.aggregate_history;
    }
}

impl ProjectionStore {
    pub fn new(repository_root: impl Into<PathBuf>) -> Self {
        Self {
            repository_root: repository_root.into(),
            runtime: Arc::new(Mutex::new(StoreRuntime::default())),
        }
    }

    pub fn state_root(&self) -> PathBuf {
        self.repository_root.join(STATE_ROOT_DIR)
    }

    pub fn begin_generation(&self) -> GenerationToken {
        let mut runtime = self
            .runtime
            .lock()
            .expect("projection runtime mutex poisoned");
        runtime.latest_issued_sequence += 1;
        let sequence = runtime.latest_issued_sequence;
        drop(runtime);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        GenerationToken {
            generation_id: format!("gen-{sequence:020}-{unique:x}"),
            sequence,
        }
    }

    pub fn load_manifest(&self) -> Result<Option<ProjectionManifest>, ProjectionStoreError> {
        read_json_if_exists(self.state_root().join(MANIFEST_FILE))
    }

    pub fn load_current_generation(
        &self,
    ) -> Result<Option<GenerationPointer>, ProjectionStoreError> {
        read_json_if_exists(self.state_root().join(CURRENT_FILE))
    }

    pub fn is_cache_hit(
        &self,
        bundle_id: &str,
        content_hash: &str,
    ) -> Result<bool, ProjectionStoreError> {
        let cache_path = self
            .state_root()
            .join(CACHE_DIR)
            .join(format!("{content_hash}.json"));
        let Some(entry): Option<BundleCacheEntry> = read_json_if_exists(cache_path)? else {
            return Ok(false);
        };

        if entry.bundle_id != bundle_id {
            return Ok(false);
        }

        let generation_root = self
            .state_root()
            .join(GENERATIONS_DIR)
            .join(&entry.generation_id);

        let projection_exists = generation_root.join(&entry.projection_path).exists();
        let search_exists = entry
            .search_path
            .as_deref()
            .map(|path| generation_root.join(path).exists())
            .unwrap_or(true);

        Ok(projection_exists && search_exists)
    }

    pub fn publish(&self, request: PublishRequest) -> Result<PublishOutcome, ProjectionStoreError> {
        self.ensure_layout()?;

        let current_manifest = self.load_manifest()?;
        let diagnostics_bytes = stable_json_bytes(&request.diagnostics)?;
        let diagnostics_hash = hex_digest(&diagnostics_bytes);
        let prepared = request
            .bundles
            .iter()
            .map(PreparedBundle::from_publication)
            .collect::<Result<Vec<_>, _>>()?;

        let cache_status = cache_status(&prepared, current_manifest.as_ref());
        if is_full_cache_hit(
            &request,
            current_manifest.as_ref(),
            &prepared,
            &diagnostics_hash,
        ) {
            return Ok(PublishOutcome {
                cache_status: CacheStatus::Hit,
                manifest: current_manifest.expect("cache hit requires an existing manifest"),
                published: false,
            });
        }

        let stage_root = self
            .state_root()
            .join(STAGING_DIR)
            .join(&request.token.generation_id);
        fs::create_dir_all(stage_root.join(PROJECTIONS_DIR))?;
        fs::create_dir_all(stage_root.join(SEARCH_DIR))?;
        fs::create_dir_all(stage_root.join(SITE_DIR))?;

        let generation_pointer = GenerationPointer {
            generation_id: request.token.generation_id.clone(),
            sequence: request.token.sequence,
        };

        let stage_context = StageWriteContext {
            request: &request,
            current_manifest: current_manifest.as_ref(),
            prepared_bundles: &prepared,
            generation_pointer,
            diagnostics_bytes,
            diagnostics_hash,
        };
        let manifest = self.write_stage(&stage_root, stage_context)?;

        if let Some(FailurePoint::AfterStageWrite) = request.inject_failure {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(ProjectionStoreError::FailureInjected(
                FailurePoint::AfterStageWrite,
            ));
        }

        self.validate_generation(&stage_root, &manifest)?;

        let generation_root = self
            .state_root()
            .join(GENERATIONS_DIR)
            .join(&request.token.generation_id);
        fs::rename(&stage_root, &generation_root)?;

        if let Some(FailurePoint::BeforePointerSwap) = request.inject_failure {
            return Err(ProjectionStoreError::FailureInjected(
                FailurePoint::BeforePointerSwap,
            ));
        }

        let latest = {
            let runtime = self
                .runtime
                .lock()
                .expect("projection runtime mutex poisoned");
            runtime.latest_issued_sequence
        };
        if request.token.sequence < latest {
            let _ = fs::remove_dir_all(&generation_root);
            return Err(ProjectionStoreError::StaleGeneration {
                attempted: request.token.sequence,
                latest,
            });
        }

        write_json_atomically(self.state_root().join(MANIFEST_FILE), &manifest)?;
        write_json_atomically(self.state_root().join(CURRENT_FILE), &manifest.generation)?;
        self.write_cache_entries(&manifest)?;

        let mut runtime = self
            .runtime
            .lock()
            .expect("projection runtime mutex poisoned");
        runtime.latest_published_sequence = request.token.sequence;
        drop(runtime);

        Ok(PublishOutcome {
            cache_status,
            manifest,
            published: true,
        })
    }

    fn ensure_layout(&self) -> Result<(), ProjectionStoreError> {
        fs::create_dir_all(self.state_root().join(STAGING_DIR))?;
        fs::create_dir_all(self.state_root().join(GENERATIONS_DIR))?;
        fs::create_dir_all(self.state_root().join(CACHE_DIR))?;
        fs::create_dir_all(self.state_root().join(SITE_DIR))?;
        Ok(())
    }

    fn write_stage(
        &self,
        stage_root: &Path,
        context: StageWriteContext<'_>,
    ) -> Result<ProjectionManifest, ProjectionStoreError> {
        let mut bundle_roots = BTreeMap::new();
        let mut bundles = BTreeMap::new();
        let mut dependency_edges = BTreeMap::new();
        let mut content_hashes = BTreeMap::new();
        let mut output_hashes = BTreeMap::new();

        for prepared in context.prepared_bundles {
            let projection_path = format!("{PROJECTIONS_DIR}/{}.json", prepared.bundle_id);
            let search_path = prepared
                .search_bytes
                .as_ref()
                .map(|_| format!("{SEARCH_DIR}/{}.idx", prepared.bundle_id));

            if let Some(existing) = context
                .current_manifest
                .and_then(|manifest| manifest.bundles.get(&prepared.bundle_id))
                .filter(|existing| existing.content_hash == prepared.content_hash)
            {
                copy_if_needed(
                    self.absolute_generation_artifact_path(
                        &existing.projection_path,
                        &context
                            .current_manifest
                            .expect("existing bundle requires current manifest")
                            .generation,
                    ),
                    stage_root.join(&projection_path),
                )?;

                if let Some(existing_search_path) = &existing.search_path {
                    let target = stage_root.join(
                        search_path
                            .as_deref()
                            .expect("search path should exist when reusing search artifact"),
                    );
                    copy_if_needed(
                        self.absolute_generation_artifact_path(
                            existing_search_path,
                            &context
                                .current_manifest
                                .expect("existing bundle requires current manifest")
                                .generation,
                        ),
                        target,
                    )?;
                }
            } else {
                fs::write(
                    stage_root.join(&projection_path),
                    &prepared.projection_bytes,
                )?;
                if let Some(search_bytes) = &prepared.search_bytes {
                    let search_path = search_path
                        .as_deref()
                        .expect("search path should exist when writing search artifact");
                    fs::write(stage_root.join(search_path), search_bytes)?;
                }
            }

            bundle_roots.insert(prepared.bundle_id.clone(), prepared.root_path.clone());
            dependency_edges.insert(
                prepared.bundle_id.clone(),
                prepared.dependency_index.clone(),
            );
            content_hashes.insert(prepared.bundle_id.clone(), prepared.content_hash.clone());
            output_hashes.insert(prepared.bundle_id.clone(), prepared.output_hash.clone());
            bundles.insert(
                prepared.bundle_id.clone(),
                PublishedBundleArtifact {
                    bundle_id: prepared.bundle_id.clone(),
                    root_path: prepared.root_path.clone(),
                    source_revision: prepared.source_revision.clone(),
                    projection_path,
                    search_path: search_path.clone(),
                    content_hash: prepared.content_hash.clone(),
                    output_hash: prepared.output_hash.clone(),
                },
            );
        }

        fs::write(stage_root.join(DIAGNOSTICS_FILE), context.diagnostics_bytes)?;
        let manifest = ProjectionManifest {
            schema_version: WIKI_SCHEMA_VERSION,
            generation: context.generation_pointer.clone(),
            last_successful_generation: context.generation_pointer,
            generated_at: context.request.generated_at.clone(),
            okf_version: context.request.okf_version.clone(),
            source_revision: context.request.source_revision.clone(),
            build_duration_ms: context.request.build_duration_ms,
            bundle_roots,
            bundles,
            dependency_edges,
            content_hashes,
            output_hashes,
            diagnostics_path: DIAGNOSTICS_FILE.into(),
            diagnostics_hash: context.diagnostics_hash,
        };

        fs::write(
            stage_root.join(MANIFEST_FILE),
            stable_json_bytes(&manifest)?,
        )?;
        Ok(manifest)
    }

    fn validate_generation(
        &self,
        stage_root: &Path,
        manifest: &ProjectionManifest,
    ) -> Result<(), ProjectionStoreError> {
        let manifest_path = stage_root.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Err(ProjectionStoreError::Validation(format!(
                "missing stage manifest at {}",
                manifest_path.display()
            )));
        }

        let diagnostics_bytes = fs::read(stage_root.join(&manifest.diagnostics_path))?;
        if hex_digest(&diagnostics_bytes) != manifest.diagnostics_hash {
            return Err(ProjectionStoreError::Validation(
                "diagnostics hash does not match manifest".into(),
            ));
        }

        for (bundle_id, bundle) in &manifest.bundles {
            let projection_bytes = fs::read(stage_root.join(&bundle.projection_path))?;
            let search_bytes = match &bundle.search_path {
                Some(path) => Some(fs::read(stage_root.join(path))?),
                None => None,
            };

            let content_hash = stable_bundle_hash(
                &projection_bytes,
                search_bytes.as_deref(),
                manifest.dependency_edges.get(bundle_id).ok_or_else(|| {
                    ProjectionStoreError::Validation(format!(
                        "missing dependency edges for bundle {bundle_id}"
                    ))
                })?,
            )?;
            if content_hash != bundle.content_hash {
                return Err(ProjectionStoreError::Validation(format!(
                    "content hash mismatch for bundle {bundle_id}"
                )));
            }

            let output_hash = stable_output_hash(&projection_bytes, search_bytes.as_deref());
            if output_hash != bundle.output_hash {
                return Err(ProjectionStoreError::Validation(format!(
                    "output hash mismatch for bundle {bundle_id}"
                )));
            }
        }

        Ok(())
    }

    fn write_cache_entries(
        &self,
        manifest: &ProjectionManifest,
    ) -> Result<(), ProjectionStoreError> {
        for bundle in manifest.bundles.values() {
            let entry = BundleCacheEntry {
                schema_version: manifest.schema_version,
                bundle_id: bundle.bundle_id.clone(),
                generation_id: manifest.generation.generation_id.clone(),
                content_hash: bundle.content_hash.clone(),
                output_hash: bundle.output_hash.clone(),
                projection_path: bundle.projection_path.clone(),
                search_path: bundle.search_path.clone(),
            };
            write_json_atomically(
                self.state_root()
                    .join(CACHE_DIR)
                    .join(format!("{}.json", bundle.content_hash)),
                &entry,
            )?;
        }
        Ok(())
    }

    fn absolute_generation_artifact_path(
        &self,
        relative_path: &str,
        generation: &GenerationPointer,
    ) -> PathBuf {
        self.state_root()
            .join(GENERATIONS_DIR)
            .join(&generation.generation_id)
            .join(relative_path)
    }
}

#[derive(Clone)]
struct PreparedBundle {
    bundle_id: String,
    root_path: String,
    source_revision: Option<String>,
    dependency_index: BundleDependencyIndex,
    projection_bytes: Vec<u8>,
    search_bytes: Option<Vec<u8>>,
    content_hash: String,
    output_hash: String,
}

struct StageWriteContext<'a> {
    request: &'a PublishRequest,
    current_manifest: Option<&'a ProjectionManifest>,
    prepared_bundles: &'a [PreparedBundle],
    generation_pointer: GenerationPointer,
    diagnostics_bytes: Vec<u8>,
    diagnostics_hash: String,
}

impl PreparedBundle {
    fn from_publication(publication: &BundlePublication) -> Result<Self, ProjectionStoreError> {
        let mut bundle = publication.bundle.clone();
        bundle.normalize();

        let projection_bytes = stable_json_bytes(&bundle)?;
        let content_hash = stable_bundle_hash(
            &projection_bytes,
            publication.search_artifact.as_deref(),
            &publication.dependency_index,
        )?;
        let output_hash =
            stable_output_hash(&projection_bytes, publication.search_artifact.as_deref());

        Ok(Self {
            bundle_id: bundle.id.clone(),
            root_path: bundle.root_path.clone(),
            source_revision: bundle.source_revision.clone(),
            dependency_index: publication.dependency_index.clone(),
            projection_bytes,
            search_bytes: publication.search_artifact.clone(),
            content_hash,
            output_hash,
        })
    }
}

fn cache_status(
    prepared: &[PreparedBundle],
    current_manifest: Option<&ProjectionManifest>,
) -> CacheStatus {
    let Some(current_manifest) = current_manifest else {
        return CacheStatus::Miss;
    };

    let reused = prepared
        .iter()
        .filter(|bundle| {
            current_manifest
                .bundles
                .get(&bundle.bundle_id)
                .map(|existing| existing.content_hash == bundle.content_hash)
                .unwrap_or(false)
        })
        .count();

    match reused {
        0 => CacheStatus::Miss,
        value if value == prepared.len() => CacheStatus::Hit,
        _ => CacheStatus::Mixed,
    }
}

fn is_full_cache_hit(
    request: &PublishRequest,
    current_manifest: Option<&ProjectionManifest>,
    prepared: &[PreparedBundle],
    diagnostics_hash: &str,
) -> bool {
    let Some(current_manifest) = current_manifest else {
        return false;
    };

    current_manifest.okf_version == request.okf_version
        && current_manifest.source_revision == request.source_revision
        && current_manifest.diagnostics_hash == diagnostics_hash
        && prepared.iter().all(|bundle| {
            current_manifest
                .bundles
                .get(&bundle.bundle_id)
                .map(|existing| existing.content_hash == bundle.content_hash)
                .unwrap_or(false)
        })
}

fn stable_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectionStoreError> {
    Ok(serde_json::to_vec_pretty(value)?)
}

fn stable_bundle_hash(
    projection_bytes: &[u8],
    search_bytes: Option<&[u8]>,
    dependency_index: &BundleDependencyIndex,
) -> Result<String, ProjectionStoreError> {
    let mut hasher = Sha256::new();
    hasher.update(projection_bytes);
    hasher.update(stable_json_bytes(dependency_index)?);
    if let Some(search_bytes) = search_bytes {
        hasher.update(search_bytes);
    }
    Ok(hex_bytes(hasher.finalize().as_slice()))
}

fn stable_output_hash(projection_bytes: &[u8], search_bytes: Option<&[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(projection_bytes);
    if let Some(search_bytes) = search_bytes {
        hasher.update(search_bytes);
    }
    hex_bytes(hasher.finalize().as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_bytes(hasher.finalize().as_slice())
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn copy_if_needed(from: PathBuf, to: PathBuf) -> Result<(), ProjectionStoreError> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)?;
    Ok(())
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn is_named_source(path: &str, file_name: &str) -> bool {
    path == file_name || path.ends_with(&format!("/{file_name}"))
}

fn parent_scope_for_source(path: &str) -> String {
    let path = Path::new(path);
    path.parent().map(path_to_scope).unwrap_or_default()
}

fn ancestor_directories_for_source(path: &str) -> BTreeSet<String> {
    let parent_scope = parent_scope_for_source(path);
    ancestor_directories(&parent_scope)
}

fn ancestor_directories(scope_path: &str) -> BTreeSet<String> {
    let mut ancestors = BTreeSet::new();
    ancestors.insert(String::new());

    let trimmed = scope_path.trim_matches('/');
    if trimmed.is_empty() {
        return ancestors;
    }

    let mut current = String::new();
    for segment in trimmed.split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        ancestors.insert(current.clone());
    }

    ancestors
}

fn path_to_scope(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn read_json_if_exists<T: for<'de> Deserialize<'de>>(
    path: impl AsRef<Path>,
) -> Result<Option<T>, ProjectionStoreError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn write_json_atomically<T: Serialize>(
    path: impl AsRef<Path>,
    value: &T,
) -> Result<(), ProjectionStoreError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, stable_json_bytes(value)?)?;
    fs::rename(temp_path, path)?;
    Ok(())
}

pub fn content_hash_for_projection(
    projection: &WikiProjection,
) -> Result<String, ProjectionStoreError> {
    let mut normalized = projection.clone();
    normalized.normalize();
    Ok(hex_digest(&stable_json_bytes(&normalized)?))
}
