//! Incremental wiki refresh coordination.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::time::Duration;

use crate::{
    diagnostic::Diagnostic,
    projection::{BundleDependencyIndex, BundleInvalidation},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryChange {
    pub bundle_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundleRefreshPlan {
    pub bundle_id: String,
    pub changed_paths: Vec<String>,
    pub invalidation: BundleInvalidation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshLease {
    pub sequence: u64,
    pub bundles: Vec<BundleRefreshPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshCompletion {
    Success,
    Failure { diagnostics: Vec<Diagnostic> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryEvent {
    Created { path: String },
    Modified { path: String },
    Renamed { from: String, to: String },
    Deleted { path: String },
}

impl RepositoryEvent {
    fn paths(&self) -> [&str; 2] {
        match self {
            Self::Created { path } | Self::Modified { path } | Self::Deleted { path } => [path, ""],
            Self::Renamed { from, to } => [from, to],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchedBundle {
    pub bundle_id: String,
    pub root: String,
}

impl WatchedBundle {
    pub fn new(bundle_id: impl Into<String>, root: impl AsRef<str>) -> Result<Self, String> {
        let root = normalize_repository_path(root.as_ref())
            .filter(|root| !root.is_empty())
            .ok_or_else(|| "bundle root must be a safe repository-relative path".to_string())?;
        Ok(Self {
            bundle_id: bundle_id.into(),
            root,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshFailure {
    pub code: String,
    pub retryable: bool,
}

impl RefreshFailure {
    pub fn new(code: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            retryable,
        }
    }
}

pub trait GraphRefreshConsumer {
    fn refresh_graph(&mut self, changed_paths: &[String]) -> Result<(), RefreshFailure>;
}

pub trait WikiRefreshConsumer {
    fn refresh_wiki(&mut self, changes: &[RepositoryChange]) -> Result<(), RefreshFailure>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsumerRefreshStatus {
    Skipped,
    Succeeded,
    Failed { code: String, retryable: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumerRefreshReport {
    pub changed_paths: Vec<String>,
    pub status: ConsumerRefreshStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatedRefreshReport {
    pub generation: u64,
    pub graph: ConsumerRefreshReport,
    pub wiki: ConsumerRefreshReport,
}

/// Coalesces one repository event stream before independently dispatching graph
/// and wiki refresh work.
pub struct CoordinatedRefresh {
    debounce: Duration,
    bundles: Vec<WatchedBundle>,
    pending_paths: BTreeSet<String>,
    last_event_at: Option<Duration>,
    generation: u64,
}

impl CoordinatedRefresh {
    pub fn new(debounce: Duration, mut bundles: Vec<WatchedBundle>) -> Self {
        bundles.sort_by(|left, right| {
            right
                .root
                .len()
                .cmp(&left.root.len())
                .then_with(|| left.bundle_id.cmp(&right.bundle_id))
        });
        Self {
            debounce,
            bundles,
            pending_paths: BTreeSet::new(),
            last_event_at: None,
            generation: 0,
        }
    }

    pub fn enqueue(&mut self, event: RepositoryEvent, observed_at: Duration) {
        for path in event.paths().into_iter().filter(|path| !path.is_empty()) {
            if let Some(path) = normalize_repository_path(path) {
                self.pending_paths.insert(path);
            }
        }
        self.last_event_at = Some(
            self.last_event_at
                .map_or(observed_at, |previous| previous.max(observed_at)),
        );
    }

    pub fn has_pending(&self) -> bool {
        !self.pending_paths.is_empty()
    }

    pub fn flush_if_ready<G, W>(
        &mut self,
        now: Duration,
        graph: &mut G,
        wiki: &mut W,
    ) -> Option<CoordinatedRefreshReport>
    where
        G: GraphRefreshConsumer + ?Sized,
        W: WikiRefreshConsumer + ?Sized,
    {
        let last_event_at = self.last_event_at?;
        if now.saturating_sub(last_event_at) < self.debounce {
            return None;
        }
        self.flush(graph, wiki)
    }

    pub fn shutdown<G, W>(
        &mut self,
        graph: &mut G,
        wiki: &mut W,
    ) -> Option<CoordinatedRefreshReport>
    where
        G: GraphRefreshConsumer + ?Sized,
        W: WikiRefreshConsumer + ?Sized,
    {
        self.flush(graph, wiki)
    }

    fn flush<G, W>(&mut self, graph: &mut G, wiki: &mut W) -> Option<CoordinatedRefreshReport>
    where
        G: GraphRefreshConsumer + ?Sized,
        W: WikiRefreshConsumer + ?Sized,
    {
        if self.pending_paths.is_empty() {
            self.last_event_at = None;
            return None;
        }

        let paths = std::mem::take(&mut self.pending_paths);
        self.last_event_at = None;
        let (graph_paths, wiki_changes) = self.partition(paths);
        if graph_paths.is_empty() && wiki_changes.is_empty() {
            return None;
        }

        self.generation += 1;
        let wiki_paths = wiki_changes
            .iter()
            .map(|change| format!("{}/{}", change.bundle_id, change.path))
            .collect();
        let graph_status = dispatch_graph(graph, &graph_paths);
        let wiki_status = dispatch_wiki(wiki, &wiki_changes);

        Some(CoordinatedRefreshReport {
            generation: self.generation,
            graph: ConsumerRefreshReport {
                changed_paths: graph_paths,
                status: graph_status,
            },
            wiki: ConsumerRefreshReport {
                changed_paths: wiki_paths,
                status: wiki_status,
            },
        })
    }

    fn partition(&self, paths: BTreeSet<String>) -> (Vec<String>, Vec<RepositoryChange>) {
        let mut graph_paths = BTreeSet::new();
        let mut wiki_changes = BTreeSet::new();

        for path in paths {
            if is_generated_path(&path) {
                continue;
            }

            if let Some((bundle, relative_path)) = self.bundle_path(&path) {
                wiki_changes.insert((bundle.bundle_id.clone(), relative_path.to_string()));
                if is_markdown(relative_path) {
                    graph_paths.insert(path);
                }
            } else {
                graph_paths.insert(path);
            }
        }

        (
            graph_paths.into_iter().collect(),
            wiki_changes
                .into_iter()
                .map(|(bundle_id, path)| RepositoryChange { bundle_id, path })
                .collect(),
        )
    }

    fn bundle_path<'a>(&'a self, path: &'a str) -> Option<(&'a WatchedBundle, &'a str)> {
        self.bundles.iter().find_map(|bundle| {
            path.strip_prefix(&bundle.root)
                .and_then(|relative| relative.strip_prefix('/'))
                .filter(|relative| !relative.is_empty())
                .map(|relative| (bundle, relative))
        })
    }
}

#[derive(Default)]
pub struct RefreshCoordinator {
    next_sequence: u64,
    active_sequence: Option<u64>,
    pending: BTreeMap<String, BTreeSet<String>>,
    last_failure: Vec<Diagnostic>,
}

impl RefreshCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, change: RepositoryChange) {
        let bundle_id = change.bundle_id;
        let path = normalize_relative_path(&change.path);
        self.pending.entry(bundle_id).or_default().insert(path);
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn last_failure(&self) -> &[Diagnostic] {
        &self.last_failure
    }

    pub fn start_next(
        &mut self,
        dependencies: &BTreeMap<String, BundleDependencyIndex>,
    ) -> Option<RefreshLease> {
        if self.active_sequence.is_some() || self.pending.is_empty() {
            return None;
        }

        self.next_sequence += 1;
        let sequence = self.next_sequence;
        self.active_sequence = Some(sequence);

        let pending = std::mem::take(&mut self.pending);
        let mut bundles = pending
            .into_iter()
            .map(|(bundle_id, changed_paths)| {
                let changed_paths = changed_paths.into_iter().collect::<Vec<_>>();
                let invalidation = dependencies
                    .get(&bundle_id)
                    .map(|index| {
                        index.invalidate_paths(changed_paths.iter().map(|path| path.as_str()))
                    })
                    .unwrap_or_default();

                BundleRefreshPlan {
                    bundle_id,
                    changed_paths,
                    invalidation,
                }
            })
            .collect::<Vec<_>>();

        bundles.sort_by(|left, right| left.bundle_id.cmp(&right.bundle_id));

        Some(RefreshLease { sequence, bundles })
    }

    pub fn complete(&mut self, sequence: u64, completion: RefreshCompletion) -> bool {
        if self.active_sequence != Some(sequence) {
            return false;
        }

        self.active_sequence = None;
        match completion {
            RefreshCompletion::Success => self.last_failure.clear(),
            RefreshCompletion::Failure { diagnostics } => self.last_failure = diagnostics,
        }
        true
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn normalize_repository_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with('/') {
        return None;
    }
    let path = Path::new(&normalized);
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part.to_str()?;
                if part.contains(':') {
                    return None;
                }
                parts.push(part);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

fn is_generated_path(path: &str) -> bool {
    matches!(
        path.split('/').next(),
        Some(".git" | ".codebaseGraph" | ".kwiki" | "target")
    )
}

fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "md" | "mdx"))
}

fn dispatch_graph(
    consumer: &mut (impl GraphRefreshConsumer + ?Sized),
    paths: &[String],
) -> ConsumerRefreshStatus {
    if paths.is_empty() {
        return ConsumerRefreshStatus::Skipped;
    }
    match consumer.refresh_graph(paths) {
        Ok(()) => ConsumerRefreshStatus::Succeeded,
        Err(failure) => ConsumerRefreshStatus::Failed {
            code: failure.code,
            retryable: failure.retryable,
        },
    }
}

fn dispatch_wiki(
    consumer: &mut (impl WikiRefreshConsumer + ?Sized),
    changes: &[RepositoryChange],
) -> ConsumerRefreshStatus {
    if changes.is_empty() {
        return ConsumerRefreshStatus::Skipped;
    }
    match consumer.refresh_wiki(changes) {
        Ok(()) => ConsumerRefreshStatus::Succeeded,
        Err(failure) => ConsumerRefreshStatus::Failed {
            code: failure.code,
            retryable: failure.retryable,
        },
    }
}

#[cfg(feature = "graph-context")]
#[derive(Debug)]
pub struct PublicGraphRefreshConsumer {
    api: codebase_graph::api::CodebaseGraphApi,
    repository_root: std::path::PathBuf,
}

#[cfg(feature = "graph-context")]
impl PublicGraphRefreshConsumer {
    pub fn new(repository_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            api: codebase_graph::api::CodebaseGraphApi::new(),
            repository_root: repository_root.into(),
        }
    }
}

#[cfg(feature = "graph-context")]
impl GraphRefreshConsumer for PublicGraphRefreshConsumer {
    fn refresh_graph(&mut self, changed_paths: &[String]) -> Result<(), RefreshFailure> {
        use codebase_graph::api::{OperationRequest, OutputFormat, RefreshRequest, RepoSelector};

        self.api
            .execute_operation(&OperationRequest::Refresh(RefreshRequest {
                repo: RepoSelector {
                    repo_root: Some(self.repository_root.clone()),
                    config_path: None,
                    db_path: None,
                    manifest_path: None,
                },
                paths: changed_paths.to_vec(),
                mode: "changed".to_string(),
                include_fts: true,
                semantic_enrichment: false,
                semantic_provider_mode: "local_only".to_string(),
                parallel: true,
                progress: false,
                output_format: OutputFormat::Typed,
            }))
            .map(|_| ())
            .map_err(|error| RefreshFailure::new(error.code, error.retryable))
    }
}
