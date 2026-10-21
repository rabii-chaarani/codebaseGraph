//! Read-only, advisory observations. An inventory never grants deletion authority.
use super::layout::{direct_bundle_paths, validate_generation_id, DirectLayout};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MaintenanceTarget {
    pub repository_root: PathBuf,
    pub state_root: PathBuf,
    pub storage_root: Option<PathBuf>,
    pub direct_pair: Option<DirectPair>,
    #[serde(default)]
    pub config_path: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DirectPair {
    pub database: PathBuf,
    pub manifest: PathBuf,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InventoryStatus {
    Complete,
    Partial,
    Busy,
    Missing,
    Invalid,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InventoryCategory {
    ActiveGraph,
    ActiveCache,
    LeasedGeneration,
    LiveRun,
    RecoveryEvidence,
    ArtifactTemporary,
    LegacyStaging,
    RetiredDirect,
    Coordination,
    Unknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Disposition {
    Retain,
    PotentialReclamation,
    Deferred,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct InventoryEntry {
    pub path: PathBuf,
    pub category: InventoryCategory,
    pub disposition: Disposition,
    pub logical_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub files: u64,
    pub directories: u64,
    pub reason: String,
}
/// Byte sums count a physical identity once, in deterministic path order. Null
/// totals mean at least one size was unavailable; known subtotals remain useful.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct InventoryTotals {
    pub logical_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub known_logical_bytes: u64,
    pub known_allocated_bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub duplicate_files: u64,
    pub measurements_complete: bool,
    pub observation_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StorageInventory {
    pub observed_at_unix_ms: u64,
    pub target: MaintenanceTarget,
    pub active_generation: Option<String>,
    pub status: InventoryStatus,
    pub entries: Vec<InventoryEntry>,
    pub totals: InventoryTotals,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    identity: Option<(u64, u64)>,
    changed: Option<(i64, i64)>,
    kind: u8,
}
fn stamp(m: &Metadata) -> Stamp {
    Stamp {
        len: m.len(),
        modified: m.modified().ok(),
        identity: identity(m),
        changed: changed(m),
        kind: if m.file_type().is_symlink() {
            2
        } else if m.is_dir() {
            1
        } else {
            0
        },
    }
}
#[cfg(unix)]
fn changed(m: &Metadata) -> Option<(i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    Some((m.ctime(), m.ctime_nsec()))
}
#[cfg(not(unix))]
fn changed(_: &Metadata) -> Option<(i64, i64)> {
    None
}
#[cfg(unix)]
fn identity(m: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((m.dev(), m.ino()))
}
#[cfg(not(unix))]
fn identity(_: &Metadata) -> Option<(u64, u64)> {
    None
}
#[cfg(unix)]
fn allocation(m: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    m.blocks().checked_mul(512)
}
#[cfg(not(unix))]
fn allocation(_: &Metadata) -> Option<u64> {
    None
}

// Check every existing component: leaf checks alone allow symlinked ancestors.
pub(crate) fn real_path(path: &Path) -> bool {
    real_path_with_metadata(path, |path| fs::symlink_metadata(path))
}

fn real_path_with_metadata(
    path: &Path,
    mut metadata: impl FnMut(&Path) -> std::io::Result<Metadata>,
) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let mut prefix = PathBuf::new();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        match component {
            Component::Prefix(value) => {
                #[cfg(windows)]
                let unc_root = match value.kind() {
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_) => false,
                    std::path::Prefix::UNC(server, share)
                    | std::path::Prefix::VerbatimUNC(server, share)
                        if !server.is_empty() && !share.is_empty() =>
                    {
                        true
                    }
                    _ => return false,
                };
                #[cfg(not(windows))]
                let _ = value;
                // Preserve the original spelling. A disk prefix needs RootDir
                // before it can be probed, whereas a complete verbatim UNC root
                // can consist solely of Prefix when its trailing slash is absent.
                prefix.push(component);
                #[cfg(windows)]
                if components.peek().is_none() {
                    return unc_root
                        && metadata(&prefix)
                            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
                }
                continue;
            }
            Component::CurDir => continue,
            Component::ParentDir => {
                if !metadata(&prefix).is_ok_and(|m| m.is_dir()) || !prefix.pop() {
                    return false;
                }
            }
            _ => prefix.push(component),
        }
        match metadata(&prefix) {
            Ok(m) if !m.file_type().is_symlink() => (),
            _ => return false,
        }
    }
    true
}
fn read_observed<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    if !real_path(path) {
        return None;
    }
    let before = fs::symlink_metadata(path).ok()?;
    if !before.is_file() {
        return None;
    }
    let file = File::open(path).ok()?;
    if stamp(&before) != stamp(&file.metadata().ok()?) {
        return None;
    }
    let value = serde_json::from_reader(std::io::BufReader::new(&file)).ok()?;
    if !real_path(path) || stamp(&before) != stamp(&fs::symlink_metadata(path).ok()?) {
        return None;
    }
    Some(value)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    read_observed(path)
}
#[derive(Deserialize)]
struct ManifestKeyView {
    files: BTreeMap<String, ArtifactKeyView>,
}
#[derive(Deserialize)]
struct ArtifactKeyView {
    #[serde(default)]
    artifact_key: Option<String>,
}
fn read_manifest_keys(path: &Path) -> Option<BTreeSet<String>> {
    let manifest: ManifestKeyView = read_observed(path)?;
    let mut keys = BTreeSet::new();
    for entry in manifest.files.into_values() {
        if let Some(key) = entry.artifact_key {
            if !valid_key(&key) {
                return None;
            }
            keys.insert(key);
        }
    }
    Some(keys)
}
#[derive(Debug, PartialEq)]
pub(crate) struct ObservedBinding {
    pub(crate) storage: PathBuf,
    pub(crate) current_direct: Option<DirectPair>,
    config: Option<serde_json::Value>,
}
pub(crate) fn default_direct(target: &MaintenanceTarget) -> DirectPair {
    DirectPair {
        database: target.state_root.join(format!(
            "{}_graph.ldb",
            repository_name(&target.repository_root)
        )),
        manifest: target.state_root.join("manifest.json"),
    }
}
pub(crate) fn same_physical_path(a: &Path, b: &Path) -> bool {
    match (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
        (Ok(a), Ok(b)) if identity(&a).is_some() => identity(&a) == identity(&b),
        _ => match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        },
    }
}
fn physically_within(path: &Path, root: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| same_physical_path(ancestor, root))
}
fn reconcile_observation_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    let mut disjoint: Vec<PathBuf> = Vec::new();
    for candidate in roots {
        if disjoint
            .iter()
            .any(|root| physically_within(&candidate, root))
        {
            continue;
        }
        disjoint.retain(|root| !physically_within(root, &candidate));
        disjoint.push(candidate);
    }
    disjoint.sort();
    disjoint
}
fn managed_direct_roles_overlap(
    storage: &Path,
    direct_paths: &[PathBuf],
    artifacts: &Path,
) -> bool {
    direct_paths
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(artifacts))
        .filter(|path| real_path(path))
        .any(|path| physically_within(path, storage) || physically_within(storage, path))
}
pub(crate) fn bound_roles_overlap(target: &MaintenanceTarget, binding: &ObservedBinding) -> bool {
    bound_roles_overlap_paths(target, &binding.storage, binding.current_direct.as_ref())
}
pub(crate) fn bound_roles_overlap_paths(
    target: &MaintenanceTarget,
    storage: &Path,
    selected: Option<&DirectPair>,
) -> bool {
    let default = default_direct(target);
    let direct = selected.unwrap_or(&default);
    let mut paths = direct_bundle_paths(&direct.database);
    paths.push(direct.manifest.clone());
    real_path(storage)
        && managed_direct_roles_overlap(
            storage,
            &paths,
            &DirectLayout::new(&direct.database, &direct.manifest).artifact_root_path(),
        )
}
fn observed_relative(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_owned());
    }
    path.ancestors()
        .find(|ancestor| same_physical_path(ancestor, root))
        .and_then(|ancestor| path.strip_prefix(ancestor).ok())
        .map(Path::to_owned)
}
fn config_raw_path(
    config: &serde_json::Value,
    key: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(value) = config.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| format!("invalid_configuration_path_{key}"))?;
    let path = Path::new(value);
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        config_path
            .parent()
            .ok_or("configuration_parent_missing")?
            .join(path)
    };
    Ok(Some(absolute))
}
fn config_value_path(
    config: &serde_json::Value,
    key: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(absolute) = config_raw_path(config, key, config_path)? else {
        return Ok(None);
    };
    // Validate the ORIGINAL traversal before using it for binding. Removing
    // `hop/..` first could conceal a symlink or nonexistent component and select
    // an unrelated lexical sibling. Preserve the validated spelling as well.
    if !real_path(&absolute) {
        return Err(format!("configuration_{key}_path_missing_or_symlinked"));
    }
    Ok(Some(absolute))
}
/// Revalidate supplied selectors rather than treating struct construction or
/// deserialization as proof of association. Conventional state has one exact
/// location; a configured root must be named by a currently repository-bound
/// config. Explicit Direct selectors preserve their raw lock/journal spelling.
pub(crate) fn observe_binding(target: &MaintenanceTarget) -> Result<ObservedBinding, String> {
    let conventional = target.repository_root.join(".codebaseGraph");
    if !target.repository_root.is_absolute()
        || !real_path(&target.repository_root)
        || target.state_root != conventional
    {
        return Err("invalid_repository_or_state_identity".into());
    }
    let default_config = conventional.join("config.json");
    let config_path = target.config_path.as_ref().unwrap_or(&default_config);
    let config = match fs::symlink_metadata(config_path) {
        Ok(_) => Some(read_json(config_path).ok_or("configuration_unreadable_or_invalid")?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && target.config_path.is_none() => None,
        _ => return Err("configuration_unreadable_or_invalid".into()),
    };
    let mut managed = false;
    let mut configured_storage = None;
    let default = default_direct(target);
    let mut configured_direct = default.clone();
    if let Some(config) = &config {
        if !config.is_object() {
            return Err("invalid_configuration_object".into());
        }
        let repository = config_value_path(config, "repo_root", config_path)?;
        if let Some(repository) = repository {
            if !real_path(&repository) || !same_physical_path(&repository, &target.repository_root)
            {
                return Err("configuration_repository_binding_mismatch".into());
            }
        } else if config_path != &default_config {
            return Err("external_configuration_repository_binding_required".into());
        }
        match config.get("schema_version").filter(|v| !v.is_null()) {
            Some(version) if matches!(version.as_u64(), Some(2 | 3)) => managed = true,
            Some(version) if version.as_u64() == Some(1) => (),
            None => (),
            _ => return Err("unsupported_configuration_schema".into()),
        }
        if managed {
            configured_storage = config_value_path(config, "storage_root", config_path)?;
        }
        configured_direct.database = config_raw_path(config, "database_path", config_path)?
            .unwrap_or(default.database.clone());
        configured_direct.manifest = config_raw_path(config, "manifest_path", config_path)?
            .unwrap_or(default.manifest.clone());
    }
    let default_storage = conventional.join("storage");
    let expected_storage = configured_storage.clone().unwrap_or(default_storage);
    let storage = target
        .storage_root
        .clone()
        .unwrap_or_else(|| expected_storage.clone());
    if physically_within(&target.repository_root, &storage)
        || physically_within(&conventional, &storage)
    {
        return Err("storage_root_overlaps_repository_or_state_ancestor".into());
    }
    if storage != expected_storage
        && !(real_path(&expected_storage)
            && real_path(&storage)
            && same_physical_path(&expected_storage, &storage))
    {
        return Err("managed_storage_repository_binding_required".into());
    }
    // A caller may select a repository-local pair. External pair observation
    // requires the repository-bound config to register those exact destinations.
    let current_direct = target
        .direct_pair
        .clone()
        .or_else(|| (!managed).then_some(configured_direct.clone()));
    if let Some(pair) = &current_direct {
        let local = pair.database.starts_with(&conventional)
            && pair.manifest.starts_with(&conventional)
            && !pair
                .database
                .components()
                .chain(pair.manifest.components())
                .any(|c| matches!(c, Component::ParentDir));
        if !local && (config.is_none() || pair != &configured_direct) {
            return Err("direct_destination_repository_binding_required".into());
        }
        if pair.database == pair.manifest {
            return Err("ambiguous_direct_pair_retained".into());
        }
    }
    Ok(ObservedBinding {
        storage,
        current_direct,
        config,
    })
}
fn classify_artifact(
    path: &Path,
    root: &Path,
    keys: &BTreeSet<String>,
) -> (InventoryCategory, Disposition, &'static str) {
    let parts: Vec<_> = path
        .strip_prefix(root)
        .unwrap_or(path)
        .iter()
        .map(|p| p.to_string_lossy())
        .collect();
    if parts
        .iter()
        .any(|p| p.starts_with(".tmp-") || p.starts_with(".replaced-"))
    {
        return (
            InventoryCategory::ArtifactTemporary,
            Disposition::Deferred,
            "artifact_producer_or_replacement_recovery_required",
        );
    }
    if parts.len() >= 2 && valid_key(&parts[1]) && parts[0] == parts[1][..2] {
        return if keys.contains(parts[1].as_ref()) {
            (
                InventoryCategory::ActiveCache,
                Disposition::Retain,
                "current_direct_manifest_artifact_key",
            )
        } else {
            (
                InventoryCategory::ActiveCache,
                Disposition::Deferred,
                "direct_key_execution_and_recovery_authority_required",
            )
        };
    }
    (
        InventoryCategory::Unknown,
        Disposition::Unknown,
        "unrecognized_direct_cache_state_retained",
    )
}

struct Scan {
    entries: Vec<InventoryEntry>,
    stamps: BTreeMap<PathBuf, Stamp>,
    seen: BTreeSet<(u64, u64)>,
    duplicates: u64,
    issues: Vec<String>,
}
impl Scan {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            stamps: BTreeMap::new(),
            seen: BTreeSet::new(),
            duplicates: 0,
            issues: Vec::new(),
        }
    }
    fn walk(&mut self, path: &Path) {
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                self.issues
                    .push(format!("unmeasurable {}: {e}", path.display()));
                self.entries.push(unknown(path, "metadata_unavailable"));
                return;
            }
        };
        self.stamps.insert(path.to_owned(), stamp(&metadata));
        if !real_path(path) || (!metadata.is_file() && !metadata.is_dir()) {
            self.issues.push(format!("untraversed {}", path.display()));
            self.entries
                .push(unknown(path, "symlink_or_special_file_retained"));
            return;
        }
        let is_dir = metadata.is_dir();
        let duplicate = !is_dir && identity(&metadata).is_some_and(|id| !self.seen.insert(id));
        if duplicate {
            self.duplicates += 1;
        }
        self.entries.push(InventoryEntry {
            path: path.to_owned(),
            category: InventoryCategory::Unknown,
            disposition: Disposition::Unknown,
            logical_bytes: Some(if is_dir || duplicate {
                0
            } else {
                metadata.len()
            }),
            allocated_bytes: if duplicate {
                Some(0)
            } else {
                allocation(&metadata)
            },
            files: u64::from(!is_dir && !duplicate),
            directories: u64::from(is_dir),
            reason: if duplicate {
                "physical_identity_counted_at_first_path"
            } else {
                "unrecognized_state_retained"
            }
            .into(),
        });
        if is_dir {
            let children = match fs::read_dir(path).and_then(|r| {
                r.map(|e| e.map(|e| e.path()))
                    .collect::<std::io::Result<Vec<_>>>()
            }) {
                Ok(mut children) => {
                    children.sort();
                    children
                }
                Err(e) => {
                    self.issues
                        .push(format!("unreadable_directory {}: {e}", path.display()));
                    if let Some(entry) = self.entries.last_mut() {
                        entry.logical_bytes = None;
                        entry.allocated_bytes = None;
                    }
                    return;
                }
            };
            for child in children {
                self.walk(&child);
            }
        }
    }
}
fn unknown(path: &Path, reason: &str) -> InventoryEntry {
    InventoryEntry {
        path: path.to_owned(),
        category: InventoryCategory::Unknown,
        disposition: Disposition::Unknown,
        logical_bytes: None,
        allocated_bytes: None,
        files: 0,
        directories: 0,
        reason: reason.into(),
    }
}

/// Observe only supplied repository state. All locks are opened without create;
/// plain File drops only release the observation locks, never run recovery.
pub(crate) fn inspect_storage(target: &MaintenanceTarget) -> StorageInventory {
    inspect_with_hook(target, || {})
}
fn inspect_with_hook(target: &MaintenanceTarget, between_scans: impl FnOnce()) -> StorageInventory {
    let mut result = StorageInventory {
        observed_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        target: target.clone(),
        active_generation: None,
        status: InventoryStatus::Partial,
        entries: Vec::new(),
        totals: InventoryTotals::default(),
        issues: Vec::new(),
    };
    let binding = match observe_binding(target) {
        Ok(binding) => binding,
        Err(reason) => {
            result.status = InventoryStatus::Invalid;
            result.issues.push(reason);
            return result;
        }
    };
    let storage = binding.storage.clone();
    let mut roots = Vec::new();
    match fs::symlink_metadata(&target.state_root) {
        Ok(m) if m.is_dir() && real_path(&target.state_root) => {
            roots.push(target.state_root.clone())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            result.issues.push("state_root_missing".into())
        }
        _ => result.issues.push("invalid_state_root_retained".into()),
    }
    let valid_storage =
        real_path(&storage) && fs::symlink_metadata(&storage).is_ok_and(|m| m.is_dir());
    if valid_storage {
        if physically_within(&target.repository_root, &storage)
            || physically_within(&target.state_root, &storage)
        {
            result.status = InventoryStatus::Invalid;
            result
                .issues
                .push("storage_root_overlaps_repository_or_state_ancestor".into());
            return result;
        }
        if !roots.iter().any(|root| physically_within(&storage, root)) {
            roots.push(storage.clone());
        }
    }
    // Direct scope contains only exact registered files/cache, never its parent.
    let default = default_direct(target);
    let direct = binding.current_direct.as_ref().unwrap_or(&default);
    let layout = DirectLayout::new(&direct.database, &direct.manifest);
    let mut direct_paths = direct_bundle_paths(&direct.database);
    direct_paths.push(direct.manifest.clone());
    let direct_artifacts = layout.artifact_root_path();
    let mut recognized_direct = real_path(&direct.database)
        && real_path(&direct.manifest)
        && fs::symlink_metadata(&direct.database).is_ok_and(|m| m.is_file())
        && fs::symlink_metadata(&direct.manifest).is_ok_and(|m| m.is_file());
    if direct_paths.iter().any(|p| {
        fs::symlink_metadata(p)
            .is_ok_and(|m| m.file_type().is_symlink() || ambiguous_file_identity(&m))
    }) {
        recognized_direct = false;
        result
            .issues
            .push("ambiguous_direct_physical_identity_retained".into());
    }
    if binding.current_direct.is_some() {
        for path in direct_paths
            .iter()
            .chain(std::iter::once(&direct_artifacts))
        {
            if real_path(path)
                && fs::symlink_metadata(path).is_ok_and(|m| {
                    if path == &direct_artifacts {
                        m.is_dir()
                    } else {
                        m.is_file()
                    }
                })
                && !roots.iter().any(|root| physically_within(path, root))
            {
                roots.push(path.clone());
            }
        }
    }
    if bound_roles_overlap(target, &binding) {
        result.status = InventoryStatus::Invalid;
        result
            .issues
            .push("managed_direct_storage_role_overlap".into());
        return result;
    }
    let roots = reconcile_observation_roots(roots);
    if roots.is_empty() {
        result.status = InventoryStatus::Missing;
        result.issues.push("bound_storage_missing".into());
        return result;
    }
    let mut locks = Vec::new();
    let mut busy = false;
    if valid_storage {
        for name in ["writer.lock", "state.lock"] {
            let path = storage.join(name);
            match existing_shared_lock(&path) {
                Ok(file) => {
                    // Reader state sections remain short. Scans below recheck
                    // the pointer/tree and hold only non-reader-facing guards.
                    if name == "writer.lock" {
                        locks.push(file);
                    }
                }
                Err(e) => {
                    busy |= e.kind() == std::io::ErrorKind::WouldBlock;
                    result.issues.push(format!(
                        "observation_lock_unavailable {}: {e}",
                        path.display()
                    ));
                }
            }
        }
    }
    let artifact_roots = if binding.current_direct.is_some() {
        vec![direct_artifacts.clone()]
    } else {
        vec![storage.join("artifacts")]
    };
    let mut artifact_observations = Vec::new();
    for root in artifact_roots {
        match super::maintenance_ownership::observe_artifact_cohort(&root) {
            Ok(Some(guard)) => artifact_observations.push(guard),
            _ => result.issues.push("artifact_root_cohort_unverified".into()),
        }
    }
    let pointer = if valid_storage {
        read_json(&storage.join("active.json"))
    } else {
        None
    };
    result.active_generation = pointer
        .as_ref()
        .filter(|v| v.get("schema_version").and_then(|v| v.as_u64()) == Some(2))
        .and_then(|v| v.get("generation_id"))
        .and_then(|v| v.as_str())
        .filter(|s| validate_generation_id(s).is_ok())
        .map(str::to_owned);
    let mut keys = BTreeSet::new();
    if let Some(id) = &result.active_generation {
        let manifest = storage
            .join("generations")
            .join(format!("gen-{id}"))
            .join("manifest.json");
        match read_manifest_keys(&manifest) {
            Some(manifest_keys) => keys = manifest_keys,
            None => result
                .issues
                .push("active_manifest_unreadable_or_invalid".into()),
        }
    } else if binding.current_direct.is_none() {
        result
            .issues
            .push("active_pointer_missing_or_invalid".into());
    }
    let direct_keys = if binding.current_direct.is_some() && recognized_direct {
        read_manifest_keys(&direct.manifest)
    } else {
        None
    };
    if binding.current_direct.is_some() && direct_keys.is_none() {
        result
            .issues
            .push("current_direct_manifest_unreadable_or_invalid".into());
    }
    direct_paths.push(direct_artifacts.clone());
    let mut scan = Scan::new();
    for root in &roots {
        scan.walk(root);
    }
    let mut lease_observations = BTreeMap::new();
    for entry in &mut scan.entries {
        if entry.logical_bytes.is_none() {
            continue;
        }
        let (category, disposition, reason) = classify(
            &entry.path,
            &storage,
            result.active_generation.as_deref(),
            &keys,
            &target.state_root,
            recognized_direct && binding.current_direct.is_none(),
            &direct_paths,
        );
        if entry.reason != "physical_identity_counted_at_first_path" {
            entry.reason = reason.into();
        }
        entry.category = category;
        entry.disposition = disposition;
        if binding.current_direct.is_some() && recognized_direct {
            if entry.path.starts_with(&direct_artifacts) {
                let (category, disposition, reason) = classify_artifact(
                    &entry.path,
                    &direct_artifacts,
                    direct_keys.as_ref().unwrap_or(&BTreeSet::new()),
                );
                entry.category = category;
                entry.disposition = disposition;
                entry.reason = reason.into();
            } else if direct_paths
                .iter()
                .filter(|p| *p != &direct_artifacts)
                .any(|p| entry.path == *p)
            {
                entry.category = InventoryCategory::ActiveGraph;
                entry.disposition = Disposition::Retain;
                entry.reason = "current_selected_direct_bundle".into();
            }
        }
        if matches!(
            category,
            InventoryCategory::LeasedGeneration | InventoryCategory::LiveRun
        ) {
            if let Some(relative) = observed_relative(&entry.path, &storage) {
                let components: Vec<_> = relative.iter().collect();
                if components.len() >= 2 {
                    let lease_path = storage
                        .join(components[0])
                        .join(components[1])
                        .join("lease.lock");
                    entry.reason = match lease_observations
                        .entry(lease_path.clone())
                        .or_insert_with(|| lease_observation(&lease_path))
                    {
                        LeaseObservation::InUse => {
                            entry.disposition = Disposition::Retain;
                            "existing_lease_in_use_retained"
                        }
                        LeaseObservation::Available => {
                            "lease_available_execution_and_recovery_authority_still_required"
                        }
                        LeaseObservation::Unknown => {
                            "lease_missing_or_unverifiable_execution_may_survive"
                        }
                    }
                    .into();
                }
            }
        }
    }
    between_scans();
    let mut after = Scan::new();
    for root in &roots {
        after.walk(root);
    }
    if scan.stamps != after.stamps
        || observe_binding(target).ok().as_ref() != Some(&binding)
        || direct_keys
            != (if binding.current_direct.is_some() && recognized_direct {
                read_manifest_keys(&direct.manifest)
            } else {
                None
            })
        || pointer
            != (if valid_storage {
                read_json(&storage.join("active.json"))
            } else {
                None
            })
    {
        result
            .issues
            .push("storage_changed_during_observation".into());
    }
    result.issues.extend(scan.issues);
    result.issues.extend(after.issues);
    result.entries = scan.entries;
    let mut totals = InventoryTotals {
        logical_bytes: Some(0),
        allocated_bytes: Some(0),
        duplicate_files: scan.duplicates,
        measurements_complete: true,
        ..Default::default()
    };
    for entry in &result.entries {
        totals.files = totals.files.saturating_add(entry.files);
        totals.directories = totals.directories.saturating_add(entry.directories);
        totals.logical_bytes = totals
            .logical_bytes
            .zip(entry.logical_bytes)
            .and_then(|(a, b)| a.checked_add(b));
        totals.allocated_bytes = totals
            .allocated_bytes
            .zip(entry.allocated_bytes)
            .and_then(|(a, b)| a.checked_add(b));
        totals.known_logical_bytes = totals
            .known_logical_bytes
            .saturating_add(entry.logical_bytes.unwrap_or(0));
        totals.known_allocated_bytes = totals
            .known_allocated_bytes
            .saturating_add(entry.allocated_bytes.unwrap_or(0));
    }
    totals.measurements_complete = totals.logical_bytes.is_some()
        && totals.allocated_bytes.is_some()
        && identity_supported()
        && result.issues.is_empty();
    // Artifact exclusion alone cannot establish coherence of native helpers,
    // startup ownership and recovery state. Keep this distinction explicit.
    result
        .issues
        .push("worker_and_recovery_coherence_unverified".into());
    totals.observation_complete = false;
    result.totals = totals;
    result.status = if busy {
        InventoryStatus::Busy
    } else {
        InventoryStatus::Partial
    };
    result.issues.sort();
    result.issues.dedup();
    drop(locks);
    result
}
fn identity_supported() -> bool {
    cfg!(unix)
}
enum LeaseObservation {
    InUse,
    Available,
    Unknown,
}
fn lease_observation(path: &Path) -> LeaseObservation {
    if !real_path(path) {
        return LeaseObservation::Unknown;
    }
    let Ok(before) = fs::symlink_metadata(path) else {
        return LeaseObservation::Unknown;
    };
    if !before.is_file() {
        return LeaseObservation::Unknown;
    }
    let Ok(file) = File::open(path) else {
        return LeaseObservation::Unknown;
    };
    if file.metadata().ok().as_ref().map(stamp) != Some(stamp(&before)) {
        return LeaseObservation::Unknown;
    }
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            if !real_path(path)
                || fs::symlink_metadata(path).ok().as_ref().map(stamp) != Some(stamp(&before))
            {
                LeaseObservation::Unknown
            } else {
                LeaseObservation::Available
            }
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
        {
            LeaseObservation::InUse
        }
        Err(_) => LeaseObservation::Unknown,
    }
}
fn existing_shared_lock(path: &Path) -> std::io::Result<File> {
    if !real_path(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing_or_symlinked_lock",
        ));
    }
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "non_file_lock",
        ));
    }
    let file = File::open(path)?;
    FileExt::try_lock_shared(&file)?;
    if stamp(&before) != stamp(&file.metadata()?)
        || !real_path(path)
        || stamp(&before) != stamp(&fs::symlink_metadata(path)?)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "lock_identity_changed",
        ));
    }
    Ok(file)
}
#[cfg(unix)]
fn ambiguous_file_identity(metadata: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.is_file() && metadata.nlink() != 1
}
#[cfg(not(unix))]
fn ambiguous_file_identity(_: &Metadata) -> bool {
    false
}
fn repository_name(root: &Path) -> String {
    let normalized: String = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "repository".into()
    } else {
        trimmed.into()
    }
}
fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn classify<'a>(
    path: &Path,
    storage: &Path,
    active: Option<&str>,
    keys: &BTreeSet<String>,
    state: &Path,
    direct: bool,
    direct_paths: &[PathBuf],
) -> (InventoryCategory, Disposition, &'a str) {
    use Disposition::*;
    use InventoryCategory::*;
    if path.starts_with(state.join("native-staging")) {
        return (LegacyStaging, Deferred, "legacy_quiescence_required");
    }
    if direct
        && direct_paths
            .iter()
            .any(|p| path == p || path.starts_with(p))
    {
        return (
            RetiredDirect,
            Deferred,
            "direct_destination_ownership_and_legacy_quiescence_required",
        );
    }
    if let Some(relative) = observed_relative(path, storage) {
        let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).collect();
        match parts.first().map(|s| s.as_ref()) {
            Some("generations") if parts.len() >= 2 => {
                if active.is_some_and(|id| parts[1] == format!("gen-{id}")) {
                    return (ActiveGraph, Retain, "active_generation");
                }
                return (
                    LeasedGeneration,
                    Deferred,
                    "generation_lease_and_recovery_authority_required",
                );
            }
            Some("runs") if parts.len() >= 2 => {
                return (LiveRun, Deferred, "run_execution_or_recovery_may_be_live")
            }
            Some("phases") => return (RecoveryEvidence, Retain, "phase_containment_evidence"),
            Some("artifacts") if parts.len() >= 2 => {
                if parts
                    .iter()
                    .skip(1)
                    .any(|s| s.starts_with(".tmp-") || s.starts_with(".replaced-"))
                {
                    return (
                        ArtifactTemporary,
                        Deferred,
                        "artifact_producer_or_replacement_recovery_required",
                    );
                }
                if parts.len() >= 3 && valid_key(&parts[2]) && parts[1] == parts[2][..2] {
                    if keys.contains(parts[2].as_ref()) {
                        return (ActiveCache, Retain, "active_manifest_artifact_key");
                    }
                    return (
                        ActiveCache,
                        Deferred,
                        "unreferenced_key_run_recovery_and_root_ownership_required",
                    );
                }
            }
            Some(name)
                if parts.len() == 1
                    && (name.ends_with(".lock")
                        || matches!(name, "active.json" | "coordinator.json")) =>
            {
                return (Coordination, Retain, "stable_coordination_metadata")
            }
            Some("worker.json") => {
                return (RecoveryEvidence, Retain, "worker_containment_evidence")
            }
            _ => (),
        }
    }
    if path.file_name().is_some_and(|n| {
        let n = n.to_string_lossy();
        n.contains("journal")
            || n.contains("cleanup-barrier")
            || n.starts_with(".direct-worker")
            || n.starts_with(".direct-workers")
    }) {
        return (RecoveryEvidence, Retain, "unresolved_recovery_evidence");
    }
    if path.extension().is_some_and(|e| e == "lock") {
        return (Coordination, Retain, "stable_lock_inode");
    }
    (
        InventoryCategory::Unknown,
        Disposition::Unknown,
        "unrecognized_state_retained",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        target: MaintenanceTarget,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "inventory-fixture-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let target = MaintenanceTarget {
                repository_root: root.clone(),
                state_root: root.join(".codebaseGraph"),
                storage_root: None,
                direct_pair: None,
                config_path: None,
            };
            Self { root, target }
        }
        fn put(&self, path: &str, bytes: &[u8]) {
            let path = self.target.state_root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        fn active(&self) {
            self.put("config.json", br#"{"schema_version":3}"#);
            self.put("storage/writer.lock", b"");
            self.put("storage/state.lock", b"");
            self.put(
                "storage/active.json",
                br#"{"schema_version":2,"generation_id":"current"}"#,
            );
            self.put("storage/generations/gen-current/graph.ldb", b"graph");
            self.put(
                "storage/generations/gen-current/manifest.json",
                format!(
                    r#"{{"files":{{"a":{{"artifact_key":"{}"}}}}}}"#,
                    "a".repeat(64)
                )
                .as_bytes(),
            );
            self.put(
                &format!("storage/artifacts/aa/{}/partition.json", "a".repeat(64)),
                b"cached",
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    fn tree(root: &Path) -> BTreeMap<PathBuf, (Stamp, Option<Vec<u8>>)> {
        fn walk(path: &Path, out: &mut BTreeMap<PathBuf, (Stamp, Option<Vec<u8>>)>) {
            let m = fs::symlink_metadata(path).unwrap();
            out.insert(
                path.to_owned(),
                (
                    stamp(&m),
                    if m.is_file() {
                        Some(fs::read(path).unwrap())
                    } else {
                        None
                    },
                ),
            );
            if m.is_dir() {
                for entry in fs::read_dir(path).unwrap() {
                    walk(&entry.unwrap().path(), out);
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, &mut out);
        out
    }
    #[cfg(windows)]
    #[test]
    fn dc_s1_windows_canonical_prefix_is_probed_only_as_complete_root() {
        let fixture = Fixture::new();
        let canonical = fs::canonicalize(&fixture.root).unwrap();
        assert!(
            matches!(canonical.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::VerbatimDisk(_) | std::path::Prefix::VerbatimUNC(_, _)))
        );
        assert!(real_path(&canonical));
        assert!(real_path(canonical.ancestors().last().unwrap()));
        assert!(!real_path(&canonical.join("missing-component")));
        assert!(!real_path(Path::new(r"\\.\C:\")));
        // Ordinary default storage observation must reach its real state,
        // rather than fail centrally before any candidate-specific checks.
        let inventory = inspect_storage(&fixture.target);
        assert_eq!(inventory.status, InventoryStatus::Missing);
        assert!(!inventory
            .issues
            .iter()
            .any(|issue| issue == "invalid_repository_or_state_identity"));
    }

    #[cfg(windows)]
    #[test]
    fn dc_s1_windows_prefix_only_malformed_roots_are_rejected_without_io() {
        for input in [
            r"\\?\C:",
            r"\\?\UNC\",
            r"\\?\UNC\server",
            r"\\?\UNC\\share",
            r"\\?\GLOBALROOT",
            r"\\.\C:",
        ] {
            assert!(
                !real_path_with_metadata(Path::new(input), |_| {
                    panic!("malformed or unsupported root must not be probed: {input}")
                }),
                "{input}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn dc_s1_windows_prefix_only_unavailable_unc_root_is_rejected() {
        let root = Path::new(r"\\?\UNC\localhost\codebasegraph-unavailable-share");
        assert_eq!(root.components().count(), 1);
        let mut calls = 0;
        assert!(!real_path_with_metadata(root, |observed| {
            assert_eq!(observed.as_os_str(), root.as_os_str());
            calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        }));
        assert_eq!(calls, 1);
    }

    #[cfg(windows)]
    #[test]
    fn dc_s1_windows_prefix_only_available_unc_root_requires_directory_metadata() {
        let fixture = Fixture::new();
        let directory = fs::symlink_metadata(&fixture.root).unwrap();
        let file = fixture.root.join("file");
        fs::write(&file, b"not a directory").unwrap();
        let file_metadata = fs::symlink_metadata(file).unwrap();
        // Supply deterministic existing metadata: Windows CI need not expose
        // an administrative/network share to exercise its native path parser.
        let root = Path::new(r"\\?\UNC\server\share");
        assert_eq!(root.components().count(), 1);
        let mut calls = 0;
        assert!(real_path_with_metadata(root, |observed| {
            assert_eq!(observed.as_os_str(), root.as_os_str());
            calls += 1;
            Ok(directory.clone())
        }));
        assert_eq!(calls, 1);
        assert!(!real_path_with_metadata(
            root,
            |_| Ok(file_metadata.clone())
        ));
        assert!(!real_path_with_metadata(root, |_| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }));
    }

    #[cfg(windows)]
    #[test]
    fn dc_s1_windows_root_prefix_fix_keeps_symlink_ancestor_rejection() {
        let fixture = Fixture::new();
        let foreign = Fixture::new();
        fs::create_dir(foreign.root.join("nested")).unwrap();
        let alias = fixture.root.join("foreign-alias");
        std::os::windows::fs::symlink_dir(&foreign.root, &alias).unwrap();
        assert!(!real_path(&alias));
        assert!(!real_path(&alias.join("nested")));
        assert!(!real_path(&alias.join("nested/../nested")));
        assert!(!real_path(Path::new(
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\"
        )));
        fs::remove_dir(alias).unwrap();
    }

    #[test]
    fn dc_s1_a1_classifies_and_counts_active_live_recovery_and_legacy() {
        let fixture = Fixture::new();
        fixture.active();
        for (path, bytes) in [
            (
                "storage/runs/run-live/journal.json",
                b"publishing".as_slice(),
            ),
            ("storage/runs/run-orphan/staging/source", b"orphan"),
            ("storage/generations/gen-old/graph.ldb", b"old"),
            ("storage/phases/unknown/proof.json", b"proof"),
            ("native-staging/run-old/import.csv", b"legacy"),
            (
                "storage/artifacts/aa/.replaced-key-nonce/partition.json",
                b"backup",
            ),
            ("storage/artifacts/.tmp-key-nonce/partition.json", b"temp"),
        ] {
            fixture.put(path, bytes);
        }
        let before = tree(&fixture.root);
        let inventory = inspect_storage(&fixture.target);
        for (suffix, category, disposition) in [
            (
                "gen-current/graph.ldb",
                InventoryCategory::ActiveGraph,
                Disposition::Retain,
            ),
            (
                "run-live/journal.json",
                InventoryCategory::LiveRun,
                Disposition::Deferred,
            ),
            (
                "run-orphan/staging/source",
                InventoryCategory::LiveRun,
                Disposition::Deferred,
            ),
            (
                "gen-old/graph.ldb",
                InventoryCategory::LeasedGeneration,
                Disposition::Deferred,
            ),
            (
                "run-old/import.csv",
                InventoryCategory::LegacyStaging,
                Disposition::Deferred,
            ),
            (
                ".replaced-key-nonce/partition.json",
                InventoryCategory::ArtifactTemporary,
                Disposition::Deferred,
            ),
            (
                "unknown/proof.json",
                InventoryCategory::RecoveryEvidence,
                Disposition::Retain,
            ),
        ] {
            let entry = inventory
                .entries
                .iter()
                .find(|e| e.path.ends_with(suffix))
                .unwrap();
            assert_eq!((entry.category, entry.disposition), (category, disposition));
        }
        assert!(inventory
            .entries
            .iter()
            .any(|e| e.category == InventoryCategory::ActiveCache
                && e.disposition == Disposition::Retain));
        let expected: u64 = before
            .values()
            .filter_map(|(_, bytes)| bytes.as_ref().map(|b| b.len() as u64))
            .sum();
        assert_eq!(inventory.totals.logical_bytes, Some(expected));
        assert_eq!(
            inventory.totals.files,
            before.values().filter(|(_, b)| b.is_some()).count() as u64
        );
        assert_eq!(before, tree(&fixture.root));
    }
    #[test]
    fn dc_s1_a2_missing_and_dropped_observations_do_not_create_or_repair() {
        let fixture = Fixture::new();
        let before = tree(&fixture.root);
        assert_eq!(
            inspect_storage(&fixture.target).status,
            InventoryStatus::Missing
        );
        assert_eq!(before, tree(&fixture.root));
        fixture.active();
        fixture.put("storage/runs/run-abandoned/journal.json", b"corrupt");
        fixture.put("storage/.publish.journal.json", b"recoverable");
        let before = tree(&fixture.root);
        drop(inspect_storage(&fixture.target));
        assert_eq!(before, tree(&fixture.root));
        let caught = std::panic::catch_unwind(|| {
            inspect_with_hook(&fixture.target, || panic!("interrupted observation"))
        });
        assert!(caught.is_err());
        assert_eq!(before, tree(&fixture.root));
        let file = File::open(fixture.target.state_root.join("storage/writer.lock")).unwrap();
        FileExt::try_lock_exclusive(&file).unwrap();
    }
    #[test]
    fn dc_s1_a3_constant_pointer_does_not_hide_artifact_and_run_mutations() {
        let fixture = Fixture::new();
        fixture.active();
        fixture.put("storage/runs/run-live/staging/source", b"before");
        let inventory = inspect_with_hook(&fixture.target, || {
            fixture.put("storage/runs/run-live/staging/source", b"after-longer");
            fixture.put("storage/artifacts/.tmp-new/partition.json", b"new");
        });
        assert_eq!(inventory.active_generation.as_deref(), Some("current"));
        assert_eq!(inventory.status, InventoryStatus::Partial);
        assert!(inventory
            .issues
            .iter()
            .any(|s| s == "storage_changed_during_observation"));
        assert!(!inventory.totals.observation_complete);
    }
    #[test]
    fn dc_s1_a3_invalid_pointer_and_foreign_direct_are_retained() {
        let fixture = Fixture::new();
        fixture.active();
        fixture.put(
            "storage/active.json",
            br#"{"schema_version":2,"generation_id":"../escape"}"#,
        );
        let mut target = fixture.target.clone();
        target.direct_pair = Some(DirectPair {
            database: fixture.root.join("custom.ldb"),
            manifest: fixture.root.join("custom.json"),
        });
        let before = tree(&fixture.root);
        let inventory = inspect_storage(&target);
        assert_eq!(inventory.active_generation, None);
        assert_eq!(inventory.status, InventoryStatus::Invalid);
        assert!(inventory
            .issues
            .iter()
            .any(|s| s == "direct_destination_repository_binding_required"));
        assert!(inventory
            .entries
            .iter()
            .all(|e| e.disposition != Disposition::PotentialReclamation));
        assert_eq!(before, tree(&fixture.root));
    }
    #[cfg(unix)]
    #[test]
    fn dc_s1_a3_symlinks_are_not_followed_and_sizes_are_unknown() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        fixture.active();
        fs::create_dir(fixture.root.join("outside")).unwrap();
        fs::write(fixture.root.join("outside/sentinel"), b"outside").unwrap();
        symlink(
            fixture.root.join("outside"),
            fixture.target.state_root.join("foreign"),
        )
        .unwrap();
        let before = tree(&fixture.root);
        let inventory = inspect_storage(&fixture.target);
        let link = inventory
            .entries
            .iter()
            .find(|e| e.path.ends_with("foreign"))
            .unwrap();
        assert_eq!(link.logical_bytes, None);
        assert!(!inventory
            .entries
            .iter()
            .any(|e| e.path.ends_with("foreign/sentinel")));
        assert_eq!(inventory.totals.logical_bytes, None);
        assert_eq!(before, tree(&fixture.root));
    }
    #[test]
    fn dc_s1_a4_repeat_observation_is_deterministic() {
        let fixture = Fixture::new();
        fixture.active();
        let a = inspect_storage(&fixture.target);
        let b = inspect_storage(&fixture.target);
        assert_eq!(a.entries, b.entries);
        assert_eq!(a.totals, b.totals);
        assert_eq!(a.issues, b.issues);
        assert_eq!(serde_json::to_value(a.status).unwrap(), "partial");
    }
    #[cfg(unix)]
    #[test]
    fn physical_hardlinks_count_once_and_allocation_uses_filesystem_blocks() {
        use std::os::unix::fs::MetadataExt;
        let fixture = Fixture::new();
        fixture.active();
        fixture.put("first", b"hardlink-content");
        fs::hard_link(
            fixture.target.state_root.join("first"),
            fixture.target.state_root.join("second"),
        )
        .unwrap();
        let inventory = inspect_storage(&fixture.target);
        assert_eq!(inventory.totals.duplicate_files, 1);
        let second = inventory
            .entries
            .iter()
            .find(|e| e.path.ends_with("second"))
            .unwrap();
        assert_eq!(
            (second.files, second.logical_bytes, second.allocated_bytes),
            (0, Some(0), Some(0))
        );
        let mut seen = BTreeSet::new();
        let expected: u64 = tree(&fixture.target.state_root)
            .keys()
            .filter_map(|p| {
                let m = fs::symlink_metadata(p).unwrap();
                if seen.insert((m.dev(), m.ino())) {
                    Some(m.blocks() * 512)
                } else {
                    None
                }
            })
            .sum();
        assert_eq!(inventory.totals.allocated_bytes, Some(expected));
    }
    #[test]
    fn busy_writer_is_reported_without_waiting_or_mutation() {
        let fixture = Fixture::new();
        fixture.active();
        let before = tree(&fixture.root);
        let lock = File::open(fixture.target.state_root.join("storage/writer.lock")).unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        let inventory = inspect_storage(&fixture.target);
        assert_eq!(inventory.status, InventoryStatus::Busy);
        assert_eq!(before, tree(&fixture.root));
    }
    #[test]
    fn recognized_direct_pair_uses_repository_identity() {
        let fixture = Fixture::new();
        fixture.active();
        fixture.put(
            &format!("{}_graph.ldb", repository_name(&fixture.root)),
            b"direct",
        );
        fixture.put("manifest.json", b"{}");
        let inventory = inspect_storage(&fixture.target);
        assert!(inventory
            .entries
            .iter()
            .any(|e| e.category == InventoryCategory::RetiredDirect
                && e.path.extension().is_some_and(|e| e == "ldb")));
    }
    #[test]
    fn configured_ancestor_root_is_rejected_without_scanning_sources() {
        let fixture = Fixture::new();
        fixture.active();
        let mut target = fixture.target.clone();
        target.storage_root = Some(fixture.root.clone());
        let inventory = inspect_storage(&target);
        assert_eq!(inventory.status, InventoryStatus::Invalid);
        assert!(inventory.entries.is_empty());
    }
    #[test]
    fn existing_live_lease_is_observed_without_recovery() {
        let fixture = Fixture::new();
        fixture.active();
        fixture.put("storage/runs/run-live/lease.lock", b"");
        let file = File::open(
            fixture
                .target
                .state_root
                .join("storage/runs/run-live/lease.lock"),
        )
        .unwrap();
        FileExt::try_lock_exclusive(&file).unwrap();
        let inventory = inspect_storage(&fixture.target);
        assert!(inventory
            .entries
            .iter()
            .any(|e| e.category == InventoryCategory::LiveRun
                && e.reason == "existing_lease_in_use_retained"
                && e.disposition == Disposition::Retain));
    }
    #[cfg(unix)]
    #[test]
    fn hardlinked_direct_identity_is_unknown() {
        let fixture = Fixture::new();
        fixture.active();
        let name = format!("{}_graph.ldb", repository_name(&fixture.root));
        fixture.put(&name, b"direct");
        fs::hard_link(
            fixture.target.state_root.join(&name),
            fixture.target.state_root.join("manifest.json"),
        )
        .unwrap();
        let inventory = inspect_storage(&fixture.target);
        assert!(inventory
            .issues
            .iter()
            .any(|i| i == "ambiguous_direct_physical_identity_retained"));
        assert!(!inventory
            .entries
            .iter()
            .any(|e| e.category == InventoryCategory::RetiredDirect));
    }
    fn write_config(path: &Path, storage: &Path, repository: &Path) {
        fs::write(path,serde_json::to_vec(&serde_json::json!({"schema_version":3,"repo_root":repository,"storage_root":storage})).unwrap()).unwrap();
    }
    #[test]
    fn review_scope_rejects_source_state_and_unregistered_external_storage() {
        let fixture = Fixture::new();
        fixture.active();
        fs::create_dir(fixture.root.join("knowledge")).unwrap();
        fs::write(fixture.root.join("knowledge/source.md"), b"curated source").unwrap();
        let foreign = Fixture::new();
        foreign.put("storage/foreign/sentinel", b"foreign");
        let mut target = fixture.target.clone();
        target.state_root = fixture.root.join("knowledge");
        let before = tree(&fixture.root);
        let outside_before = tree(&foreign.root);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
        target = fixture.target.clone();
        target.storage_root = Some(foreign.target.state_root.join("storage"));
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
        assert_eq!(before, tree(&fixture.root));
        assert_eq!(outside_before, tree(&foreign.root));
    }
    #[test]
    fn review_bound_external_store_survives_missing_conventional_state() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        external.active();
        let storage = external.target.state_root.join("storage");
        let config = fixture.root.join("external-config.json");
        write_config(&config, &storage, &fixture.root);
        let mut target = fixture.target.clone();
        target.config_path = Some(config);
        target.storage_root = Some(storage.clone());
        let before = tree(&fixture.root);
        let external_before = tree(&external.root);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Partial);
        assert_eq!(observation.active_generation.as_deref(), Some("current"));
        assert!(observation.entries.iter().any(|e| e.path
            == storage.join("generations/gen-current/graph.ldb")
            && e.category == InventoryCategory::ActiveGraph));
        assert!(!target.state_root.exists());
        assert_eq!(before, tree(&fixture.root));
        assert_eq!(external_before, tree(&external.root));
    }
    #[test]
    fn review_external_config_requires_matching_repository_binding() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        external.active();
        let storage = external.target.state_root.join("storage");
        let config = fixture.root.join("external-config.json");
        write_config(&config, &storage, &external.root);
        let mut target = fixture.target.clone();
        target.config_path = Some(config);
        target.storage_root = Some(storage);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
    }
    fn direct_fixture(fixture: &Fixture) -> DirectPair {
        let pair = default_direct(&fixture.target);
        fixture.put(
            pair.database.file_name().unwrap().to_str().unwrap(),
            b"selected direct",
        );
        fixture.put("manifest.json",serde_json::to_string(&serde_json::json!({"schema_version":2,"files":{"a":{"artifact_key":"b".repeat(64)}}})).unwrap().as_bytes());
        let layout = DirectLayout::new(&pair.database, &pair.manifest);
        let artifact = layout
            .artifact_root_path()
            .join("bb")
            .join("b".repeat(64))
            .join("partition.json");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(artifact, b"current cache").unwrap();
        pair
    }
    fn assert_current_direct(observation: &StorageInventory, pair: &DirectPair) {
        assert!(observation.entries.iter().any(|e| e.path == pair.database
            && e.category == InventoryCategory::ActiveGraph
            && e.disposition == Disposition::Retain));
        assert!(observation
            .entries
            .iter()
            .any(|e| e.path.ends_with("partition.json")
                && e.category == InventoryCategory::ActiveCache
                && e.disposition == Disposition::Retain));
        assert!(!observation
            .entries
            .iter()
            .any(|e| e.path == pair.database && e.category == InventoryCategory::RetiredDirect));
    }
    #[test]
    fn review_default_direct_only_is_current_graph_and_cache() {
        let fixture = Fixture::new();
        let pair = direct_fixture(&fixture);
        let before = tree(&fixture.root);
        let observation = inspect_storage(&fixture.target);
        assert_current_direct(&observation, &pair);
        assert_eq!(before, tree(&fixture.root));
    }
    #[test]
    fn review_explicit_direct_alongside_managed_is_current() {
        let fixture = Fixture::new();
        fixture.active();
        let pair = direct_fixture(&fixture);
        let mut target = fixture.target.clone();
        target.direct_pair = Some(pair.clone());
        assert_current_direct(&inspect_storage(&target), &pair);
    }
    #[test]
    fn review_managed_config_makes_unselected_default_direct_former() {
        let fixture = Fixture::new();
        fixture.active();
        let pair = direct_fixture(&fixture);
        let observation = inspect_storage(&fixture.target);
        assert!(observation.entries.iter().any(|e| e.path == pair.database
            && e.category == InventoryCategory::RetiredDirect
            && e.disposition == Disposition::Deferred));
        assert!(observation
            .entries
            .iter()
            .any(|e| e.path.ends_with("gen-current/graph.ldb")
                && e.category == InventoryCategory::ActiveGraph));
    }
    #[test]
    fn review_case_alias_roots_are_rejected_without_double_counting() {
        let fixture = Fixture::new();
        fixture.active();
        let alias = fixture.root.join(".CODEBASEGRAPH");
        if !alias.exists() {
            eprintln!("case-alias scenario unavailable on this case-sensitive fixture filesystem");
            return;
        }
        assert!(same_physical_path(&alias, &fixture.target.state_root));
        let baseline = inspect_storage(&fixture.target);
        let mut target = fixture.target.clone();
        target.storage_root = Some(alias);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
        assert!(observation.totals.directories <= baseline.totals.directories);
    }
    #[test]
    fn review_current_configuration_is_revalidated_on_every_observation() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        external.active();
        let storage = external.target.state_root.join("storage");
        let config = fixture.root.join("external-config.json");
        write_config(&config, &storage, &fixture.root);
        let mut target = fixture.target.clone();
        target.config_path = Some(config.clone());
        target.storage_root = Some(storage.clone());
        let observation =
            inspect_with_hook(&target, || write_config(&config, &storage, &external.root));
        assert!(observation
            .issues
            .iter()
            .any(|i| i == "storage_changed_during_observation"));
        let second = inspect_storage(&target);
        assert_eq!(second.status, InventoryStatus::Invalid);
        assert!(second.entries.is_empty());
    }
    #[test]
    fn review_external_current_direct_config_observes_exact_bundle_without_local_state() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        let pair = direct_fixture(&external);
        let config = fixture.root.join("direct-config.json");
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"repo_root":fixture.root,"database_path":pair.database,"manifest_path":pair.manifest})).unwrap()).unwrap();
        fs::write(external.root.join("foreign-sentinel"), b"foreign source").unwrap();
        let mut target = fixture.target.clone();
        target.config_path = Some(config);
        let before = tree(&external.root);
        let observation = inspect_storage(&target);
        assert_current_direct(&observation, &pair);
        assert!(!observation
            .entries
            .iter()
            .any(|e| e.path.ends_with("foreign-sentinel")));
        assert!(!target.state_root.exists());
        assert_eq!(before, tree(&external.root));
    }
    #[test]
    fn review_configured_storage_cannot_be_overridden_with_unselected_local_root() {
        let fixture = Fixture::new();
        fixture.active();
        let external = Fixture::new();
        external.active();
        let config = fixture.target.state_root.join("config.json");
        write_config(
            &config,
            &external.target.state_root.join("storage"),
            &fixture.root,
        );
        let mut target = fixture.target.clone();
        target.storage_root = Some(target.state_root.join("storage"));
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
    }
    // Fixture-only DOS/UNC spelling. Joining a canonical verbatim Windows
    // prefix normalizes dot/parent components before the fixture can test raw
    // address keys; an ordinary prefix preserves the intended distinct input.
    #[cfg(windows)]
    fn ordinary_windows_fixture_path(path: &Path) -> PathBuf {
        use std::path::Prefix;
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            panic!("absolute Windows fixture prefix");
        };
        let mut result = match prefix.kind() {
            Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
            Prefix::VerbatimUNC(server, share) => {
                let mut root = std::ffi::OsString::from(r"\\");
                root.push(server);
                root.push(r"\");
                root.push(share);
                root.push(r"\");
                PathBuf::from(root)
            }
            _ => return path.to_owned(),
        };
        for component in path.components() {
            if let Component::Normal(name) = component {
                result.push(name);
            }
        }
        assert_eq!(
            fs::canonicalize(&result).unwrap(),
            fs::canonicalize(path).unwrap()
        );
        result
    }

    #[test]
    fn review_raw_config_direct_spelling_selects_matching_cache() {
        for address in ["./", "../"] {
            let owner = Fixture::new();
            let external = Fixture::new();
            let config_parent = if address == "../" {
                external.root.join("config")
            } else {
                external.root.clone()
            };
            fs::create_dir_all(&config_parent).unwrap();
            #[cfg(windows)]
            let config_parent = ordinary_windows_fixture_path(&config_parent);
            let raw_db = format!("{address}graph.ldb");
            let raw_manifest = format!("{address}manifest.json");
            let pair = DirectPair {
                database: config_parent.join(&raw_db),
                manifest: config_parent.join(&raw_manifest),
            };
            fs::write(&pair.database, b"raw addressed graph").unwrap();
            fs::write(
                &pair.manifest,
                serde_json::to_vec(
                    &serde_json::json!({"files":{"a":{"artifact_key":"c".repeat(64)}}}),
                )
                .unwrap(),
            )
            .unwrap();
            let raw_layout = DirectLayout::new(&pair.database, &pair.manifest);
            let artifact = raw_layout
                .artifact_root_path()
                .join("cc")
                .join("c".repeat(64))
                .join("partition.json");
            fs::create_dir_all(artifact.parent().unwrap()).unwrap();
            fs::write(&artifact, b"raw-key cache").unwrap();
            let normalized_layout = DirectLayout::new(
                fs::canonicalize(&pair.database).unwrap(),
                fs::canonicalize(&pair.manifest).unwrap(),
            );
            assert_ne!(
                raw_layout.artifact_root_path(),
                normalized_layout.artifact_root_path()
            );
            let config = config_parent.join("config.json");
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"repo_root":owner.root,"database_path":raw_db,"manifest_path":raw_manifest})).unwrap()).unwrap();
            let mut target = owner.target.clone();
            target.config_path = Some(config);
            let before = tree(&external.root);
            let observation = inspect_storage(&target);
            assert!(
                observation.entries.iter().any(|e| e.path == artifact
                    && e.category == InventoryCategory::ActiveCache
                    && e.disposition == Disposition::Retain),
                "missing active raw-key cache for {address}: {:?}",
                observation.issues
            );
            assert_eq!(
                observe_binding(&target)
                    .unwrap()
                    .current_direct
                    .unwrap()
                    .database
                    .as_os_str(),
                pair.database.as_os_str()
            );
            assert_eq!(before, tree(&external.root));
            assert!(!target.state_root.exists());
        }
    }
    fn overlapping_role_fixture(managed_inside_direct: bool) {
        let owner = Fixture::new();
        let external = Fixture::new();
        let parent = if managed_inside_direct {
            external.root.clone()
        } else {
            external.root.join("managed")
        };
        fs::create_dir_all(&parent).unwrap();
        let pair = DirectPair {
            database: parent.join("graph.ldb"),
            manifest: parent.join("manifest.json"),
        };
        fs::write(&pair.database, b"graph").unwrap();
        fs::write(&pair.manifest, br#"{"files":{}}"#).unwrap();
        let artifacts = DirectLayout::new(&pair.database, &pair.manifest).artifact_root_path();
        fs::create_dir_all(&artifacts).unwrap();
        let managed = if managed_inside_direct {
            artifacts.join("managed")
        } else {
            parent
        };
        fs::create_dir_all(managed.join("generations/gen-current")).unwrap();
        fs::write(
            managed.join("active.json"),
            br#"{"schema_version":2,"generation_id":"current"}"#,
        )
        .unwrap();
        fs::write(
            managed.join("generations/gen-current/manifest.json"),
            br#"{"files":{}}"#,
        )
        .unwrap();
        let config = external.root.join("config.json");
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"repo_root":owner.root,"schema_version":3,"storage_root":managed,"database_path":pair.database,"manifest_path":pair.manifest})).unwrap()).unwrap();
        let mut target = owner.target.clone();
        target.config_path = Some(config);
        target.storage_root = Some(managed);
        target.direct_pair = Some(pair);
        let before = tree(&external.root);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
        assert_eq!(observation.totals.directories, 0);
        assert!(observation
            .issues
            .iter()
            .any(|i| i == "managed_direct_storage_role_overlap"));
        assert_eq!(before, tree(&external.root));
    }
    #[test]
    fn review_managed_root_inside_direct_cache_is_rejected() {
        overlapping_role_fixture(true);
    }
    #[test]
    fn review_direct_cache_inside_managed_root_is_rejected() {
        overlapping_role_fixture(false);
    }
    #[test]
    fn review_complete_root_reconciliation_is_insertion_order_independent() {
        let fixture = Fixture::new();
        fixture.active();
        let ancestor = fixture.target.state_root.clone();
        let descendant = ancestor.join("storage");
        for roots in [
            vec![descendant.clone(), ancestor.clone()],
            vec![ancestor.clone(), descendant.clone()],
        ] {
            let roots = reconcile_observation_roots(roots);
            assert_eq!(roots, vec![ancestor.clone()]);
            let mut scan = Scan::new();
            for root in roots {
                scan.walk(&root);
            }
            let expected = tree(&ancestor)
                .values()
                .filter(|(stamp, _)| stamp.kind == 1)
                .count() as u64;
            assert_eq!(
                scan.entries.iter().map(|e| e.directories).sum::<u64>(),
                expected
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn review_config_storage_parent_traversal_cannot_erase_symlink_or_missing_component() {
        use std::os::unix::fs::symlink;
        for symlinked in [true, false] {
            let owner = Fixture::new();
            let external = Fixture::new();
            let config_parent = external.root.join("config");
            let actual_parent = external.root.join("actual");
            fs::create_dir_all(config_parent.join("storage")).unwrap();
            fs::create_dir_all(actual_parent.join("child")).unwrap();
            fs::create_dir_all(actual_parent.join("storage")).unwrap();
            fs::write(
                config_parent.join("storage/unconfigured-sibling"),
                b"must not observe",
            )
            .unwrap();
            fs::write(
                actual_parent.join("storage/raw-destination"),
                b"must not follow symlink",
            )
            .unwrap();
            if symlinked {
                symlink(actual_parent.join("child"), config_parent.join("hop")).unwrap();
            }
            let config = config_parent.join("config.json");
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":3,"repo_root":owner.root,"storage_root":"hop/../storage"})).unwrap()).unwrap();
            let mut target = owner.target.clone();
            target.config_path = Some(config);
            let before = tree(&external.root);
            let observation = inspect_storage(&target);
            assert_eq!(observation.status, InventoryStatus::Invalid);
            assert!(observation.entries.is_empty());
            assert!(observation
                .issues
                .iter()
                .any(|i| i == "configuration_storage_root_path_missing_or_symlinked"));
            assert_eq!(before, tree(&external.root));
        }
    }
    #[cfg(unix)]
    #[test]
    fn review_config_repository_parent_traversal_cannot_erase_symlink() {
        use std::os::unix::fs::symlink;
        let owner = Fixture::new();
        let external = Fixture::new();
        external.active();
        fs::create_dir(external.root.join("child")).unwrap();
        symlink(external.root.join("child"), owner.root.join("hop")).unwrap();
        let config = owner.root.join("external-config.json");
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":3,"repo_root":"hop/..","storage_root":external.target.state_root.join("storage")})).unwrap()).unwrap();
        let mut target = owner.target.clone();
        target.config_path = Some(config);
        let before = tree(&external.root);
        let observation = inspect_storage(&target);
        assert_eq!(observation.status, InventoryStatus::Invalid);
        assert!(observation.entries.is_empty());
        assert!(observation
            .issues
            .iter()
            .any(|i| i == "configuration_repo_root_path_missing_or_symlinked"));
        assert_eq!(before, tree(&external.root));
    }
    #[test]
    fn review_real_directory_dot_parent_config_paths_remain_supported() {
        let owner = Fixture::new();
        let external = Fixture::new();
        external.active();
        fs::create_dir(owner.root.join("real-hop")).unwrap();
        fs::create_dir(external.target.state_root.join("real-hop")).unwrap();
        for storage in ["./storage", "real-hop/../storage"] {
            let config = external.target.state_root.join("bound-config.json");
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":3,"repo_root":owner.root.join("real-hop/.."),"storage_root":storage})).unwrap()).unwrap();
            let mut target = owner.target.clone();
            target.config_path = Some(config);
            let observation = inspect_storage(&target);
            assert_eq!(observation.active_generation.as_deref(), Some("current"));
            assert!(observation
                .entries
                .iter()
                .any(|e| e.path.ends_with("gen-current/graph.ldb")
                    && e.category == InventoryCategory::ActiveGraph));
            assert!(!target.state_root.exists());
        }
    }
}
