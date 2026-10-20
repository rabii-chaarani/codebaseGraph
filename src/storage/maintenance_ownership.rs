//! Ownership capabilities for maintenance. Inventory observations are never
//! capabilities; every capability below retains the actual OS locks.
use super::atomic::write_json_atomically;
use super::containment::{probe_descriptor, BuildDescriptorV2};
use super::inventory::{
    bound_roles_overlap, default_direct, observe_binding, real_path, same_physical_path,
    DirectPair, MaintenanceTarget, ObservedBinding,
};
use super::layout::{DirectLayout, ManagedLayout};
use super::locks::{
    open_locked, open_shared_coordination, path_identity, try_open_existing_locked,
    try_open_locked_in_existing_parent, FileIdentity, LockMode, LockedFile, WriterLease,
};
use crate::error::NativeError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum MaintenanceAdmission<'a> {
    Acquired(MaintenanceLease<'a>),
    Deferred(String),
    Unknown(String),
}

#[derive(Debug)]
enum DestinationWriter<'a> {
    Owned(WriterLease),
    Borrowed(&'a WriterLease),
}
impl DestinationWriter<'_> {
    fn lease(&self) -> &WriterLease {
        match self {
            Self::Owned(lease) => lease,
            Self::Borrowed(lease) => lease,
        }
    }
}

/// A current-cohort lease can be acquired before legacy migration is approved.
/// `cohort_verified` establishes destination-wide controlled migration;
/// `artifact_cohort_verified` establishes root-only producer provenance. S3 owns
/// confirmation policy and persistence, using `cohort_record` under this lease.
#[derive(Debug)]
pub(crate) struct MaintenanceLease<'a> {
    writer: DestinationWriter<'a>,
    writer_path: PathBuf,
    state_path: Option<PathBuf>,
    artifact: ArtifactUseLease,
    _database: Vec<DatabaseUseLease>,
    _workers: Vec<LockedFile>,
    _phases: Vec<LockedFile>,
    target: MaintenanceTarget,
    binding: ObservedBinding,
}
impl MaintenanceLease<'_> {
    pub(crate) fn target(&self) -> &MaintenanceTarget {
        &self.target
    }

    pub(crate) fn revalidate(&self) -> Result<(), NativeError> {
        if !self
            .writer
            .lease()
            .verifies(&self.writer_path, LockMode::Exclusive)?
        {
            return Err(invalid("writer_identity_changed"));
        }
        self.artifact.revalidate()?;
        if observe_binding(&self.target).map_err(invalid)? != self.binding
            || bound_roles_overlap(&self.target, &self.binding)
        {
            return Err(invalid("destination_binding_changed"));
        }
        for guard in self._workers.iter().chain(&self._phases) {
            if !guard.is_current()? {
                return Err(invalid("worker_or_phase_identity_changed"));
            }
        }
        for guard in &self._database {
            guard.revalidate()?;
        }
        Ok(())
    }

    /// Root provenance does not establish destination-wide legacy quiescence.
    pub(crate) fn artifact_cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        self.artifact.cohort_verified()
    }

    pub(crate) fn cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        let expected = self.cohort_record()?;
        let path = self.cohort_record_path();
        if !real_path(&path) {
            return Ok(false);
        }
        reject_file_alias(&path)?;
        let identity = path_identity(&path)?;
        let file = fs::File::open(&path)?;
        if super::locks::file_identity(&file)? != identity {
            return Ok(false);
        }
        let observed: DestinationCohortRecord = match serde_json::from_reader(&file) {
            Ok(record) => record,
            Err(_) => return Ok(false),
        };
        Ok(path_identity(&path)? == identity && observed == expected)
    }

    /// Short metadata-only state transition. The caller must prepare validation
    /// and traversal outside this closure. Contention never waits under writer.
    pub(crate) fn with_state_lock<T>(
        &self,
        transition: impl FnOnce() -> Result<T, NativeError>,
    ) -> Result<Option<T>, NativeError> {
        self.revalidate()?;
        let _state = if let Some(path) = &self.state_path {
            let Some(lease) = try_open_locked_in_existing_parent(path, LockMode::Exclusive)? else {
                return Ok(None);
            };
            Some(lease)
        } else {
            None
        };
        transition().map(Some)
    }

    /// Data for S3's durable, explicit controlled-migration record. Returning
    /// this value does not write it or authorize legacy retirement by itself.
    pub(crate) fn cohort_record(&self) -> Result<DestinationCohortRecord, NativeError> {
        self.revalidate()?;
        let pair = self
            .target
            .direct_pair
            .clone()
            .or(self.binding.current_direct.clone());
        let (database, manifest) = match &pair {
            Some(pair) => (
                optional_resource_identity(&pair.database)?,
                optional_resource_identity(&pair.manifest)?,
            ),
            None => (None, None),
        };
        Ok(DestinationCohortRecord {
            version: 1,
            writer_path: self.writer_path.clone(),
            repository: path_identity(&self.target.repository_root)?,
            destination_root: directory_identity(
                self.writer_path
                    .parent()
                    .ok_or_else(|| invalid("destination_parent_missing"))?,
            )?,
            writer: self.writer.lease().identity()?,
            artifact_root: self.artifact.identity,
            artifact_coordination: self.artifact.lock.identity()?,
            direct_pair: pair,
            database,
            manifest,
            resource_use: self
                ._database
                .iter()
                .map(DatabaseUseLease::exclusive_proof)
                .collect::<Result<_, _>>()?,
            origin: DestinationCohortOrigin::ControlledMigration,
        })
    }

    /// S3 may persist this data only after proving fresh creation and durable
    /// coherent ACTIVE publication. It grants no operator-confirmation status
    /// and covers only these exact files/generation, never legacy siblings.
    pub(crate) fn publication_cohort_record(
        &self,
        database: &DatabaseUseLease,
        manifest: &DatabaseUseLease,
    ) -> Result<PublicationCohortRecord, NativeError> {
        self.revalidate()?;
        let database = database.exclusive_proof()?;
        let manifest = manifest.exclusive_proof()?;
        if database.file.is_none() || manifest.file.is_none() {
            return Err(invalid("publication_files_missing"));
        }
        let pair = self
            .target
            .direct_pair
            .as_ref()
            .or(self.binding.current_direct.as_ref());
        let scope = if let Some(pair) = pair {
            if database.resource != absolute(&pair.database)?
                || manifest.resource != absolute(&pair.manifest)?
            {
                return Err(invalid("publication_wrong_direct_destination"));
            }
            PublicationScope::Direct(pair.clone())
        } else {
            let generation = database
                .resource
                .parent()
                .ok_or_else(|| invalid("publication_generation_missing"))?;
            let generations = self.binding.storage.join("generations");
            if database.resource != generation.join("graph.ldb")
                || manifest.resource != generation.join("manifest.json")
                || !real_path(generation)
                || !same_physical_path(
                    generation
                        .parent()
                        .ok_or_else(|| invalid("generation_parent_missing"))?,
                    &generations,
                )
            {
                return Err(invalid("publication_wrong_managed_generation"));
            }
            let name = generation
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("gen-"))
                .ok_or_else(|| invalid("publication_generation_name_invalid"))?;
            super::layout::validate_generation_id(name)?;
            PublicationScope::ManagedGeneration {
                root: generation.to_owned(),
                identity: directory_identity(generation)?,
            }
        };
        Ok(PublicationCohortRecord {
            version: 1,
            origin: PublicationCohortOrigin::GuardedPublication,
            repository: path_identity(&self.target.repository_root)?,
            writer_path: self.writer_path.clone(),
            writer: self.writer.lease().identity()?,
            artifact_root: self.artifact.identity,
            artifact_coordination: self.artifact.lock.identity()?,
            scope,
            database,
            manifest,
        })
    }

    pub(crate) fn direct_publication_cohort_record(
        &self,
    ) -> Result<PublicationCohortRecord, NativeError> {
        let database = self
            ._database
            .first()
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        let manifest = self
            ._database
            .get(1)
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        self.publication_cohort_record(database, manifest)
    }

    pub(crate) fn publication_cohort_verified(
        &self,
        record: &PublicationCohortRecord,
        database: &DatabaseUseLease,
        manifest: &DatabaseUseLease,
    ) -> Result<bool, NativeError> {
        Ok(*record == self.publication_cohort_record(database, manifest)?)
    }

    pub(crate) fn direct_publication_cohort_verified(
        &self,
        record: &PublicationCohortRecord,
    ) -> Result<bool, NativeError> {
        Ok(*record == self.direct_publication_cohort_record()?)
    }

    pub(crate) fn cohort_record_path(&self) -> PathBuf {
        self.writer_path.with_extension("maintenance-cohort.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PublicationCohortRecord {
    version: u32,
    origin: PublicationCohortOrigin,
    repository: FileIdentity,
    writer_path: PathBuf,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    scope: PublicationScope,
    database: DatabaseUseProof,
    manifest: DatabaseUseProof,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublicationCohortOrigin {
    GuardedPublication,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublicationScope {
    Direct(DirectPair),
    ManagedGeneration {
        root: PathBuf,
        identity: FileIdentity,
    },
}

/// S3 persists this record only after its explicit operator-controlled protocol.
/// A normal producer never emits destination-wide migration authority. Exact
/// Direct file identities invalidate the record after replacement/publication;
/// S3 owns the new publication's ACTIVE incarnation and subsequent eligibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DestinationCohortRecord {
    version: u32,
    writer_path: PathBuf,
    repository: FileIdentity,
    destination_root: FileIdentity,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    direct_pair: Option<DirectPair>,
    database: Option<FileIdentity>,
    manifest: Option<FileIdentity>,
    resource_use: Vec<DatabaseUseProof>,
    origin: DestinationCohortOrigin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DestinationCohortOrigin {
    ControlledMigration,
}

fn optional_resource_identity(path: &Path) -> Result<Option<FileIdentity>, NativeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            reject_file_alias(path)?;
            path_identity(path).map(Some)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CohortOrigin {
    GuardedCreation,
    ControlledMigration,
}

/// Exact physical root and stable lock identity, produced only under upgraded
/// producer ownership or explicit operator-controlled migration. No PID/time
/// or mere record presence supplies proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArtifactCohortRecord {
    version: u32,
    root: FileIdentity,
    coordination: FileIdentity,
    origin: CohortOrigin,
}

#[derive(Debug)]
pub(crate) struct ArtifactUseLease {
    root: PathBuf,
    identity: FileIdentity,
    lock: LockedFile,
}
impl ArtifactUseLease {
    pub(crate) fn ensure_root(root: &Path) -> Result<(), NativeError> {
        let root = absolute(root)?;
        fs::create_dir_all(
            root.parent()
                .ok_or_else(|| invalid("artifact_parent_missing"))?,
        )?;
        match fs::create_dir(&root) {
            Ok(()) => {
                let identity = directory_identity(&root)?;
                if let Some(lock) =
                    try_open_locked_in_existing_parent(root.join(".use.lock"), LockMode::Shared)?
                {
                    let lease = Self {
                        root,
                        identity,
                        lock,
                    };
                    lease.write_guarded_creation_record()?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                directory_identity(&root)?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn write_guarded_creation_record(&self) -> Result<(), NativeError> {
        let record = self.record(CohortOrigin::GuardedCreation)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(".use-cohort.json"))?;
        file.write_all(&serde_json::to_vec(&record)?)?;
        file.sync_all()?;
        Ok(())
    }

    pub(crate) fn acquire_shared(root: &Path) -> Result<Self, NativeError> {
        let root = absolute(root)?;
        let parent = root
            .parent()
            .ok_or_else(|| invalid("artifact_parent_missing"))?;
        fs::create_dir_all(parent)?;
        let created = match fs::create_dir(&root) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(e) => return Err(e.into()),
        };
        let identity = directory_identity(&root)?;
        let lock = open_locked(root.join(".use.lock"), LockMode::Shared)?;
        let lease = Self {
            root,
            identity,
            lock,
        };
        lease.revalidate()?;
        // Only the process that created this incarnation can establish guarded
        // creation. Existing unproven roots remain legacy until explicit S3
        // confirmation. A create_new write cannot replace an existing proof.
        if created {
            lease.write_guarded_creation_record()?;
        }
        Ok(lease)
    }

    pub(crate) fn try_exclusive(root: &Path) -> Result<Option<Self>, NativeError> {
        let root = absolute(root)?;
        let identity = directory_identity(&root)?;
        let Some(lock) =
            try_open_locked_in_existing_parent(root.join(".use.lock"), LockMode::Exclusive)?
        else {
            return Ok(None);
        };
        let lease = Self {
            root,
            identity,
            lock,
        };
        lease.revalidate()?;
        Ok(Some(lease))
    }

    fn revalidate(&self) -> Result<(), NativeError> {
        if directory_identity(&self.root)? != self.identity
            || !self
                .lock
                .verifies(&self.root.join(".use.lock"), self.lock_mode())?
        {
            return Err(invalid("artifact_root_incarnation_changed"));
        }
        Ok(())
    }
    fn lock_mode(&self) -> LockMode {
        self.lock.mode()
    }
    fn record(&self, origin: CohortOrigin) -> Result<ArtifactCohortRecord, NativeError> {
        self.revalidate()?;
        Ok(ArtifactCohortRecord {
            version: 1,
            root: self.identity,
            coordination: self.lock.identity()?,
            origin,
        })
    }
    fn cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        let path = self.root.join(".use-cohort.json");
        if !real_path(&path) {
            return Ok(false);
        }
        reject_file_alias(&path)?;
        let identity = path_identity(&path)?;
        let file = fs::File::open(&path)?;
        if super::locks::file_identity(&file)? != identity {
            return Ok(false);
        }
        let record: ArtifactCohortRecord = match serde_json::from_reader(&file) {
            Ok(record) => record,
            Err(_) => return Ok(false),
        };
        Ok(path_identity(&path)? == identity
            && record.version == 1
            && record.root == self.identity
            && record.coordination == self.lock.identity()?)
    }
    pub(crate) fn into_lock(self) -> LockedFile {
        self.lock
    }
}

/// Physical-parent family ownership covers all equivalent filename spellings,
/// including absent-to-created resources. Basename guards remain secondary;
/// filesystem case folding/normalization is never emulated as authority. Raw
/// Direct pair-derived writer/journal keys stay unchanged.
#[derive(Debug)]
pub(crate) struct DatabaseUseLease {
    resource: PathBuf,
    parent: PathBuf,
    parent_identity: FileIdentity,
    family: Arc<LockedFile>,
    lock: Arc<LockedFile>,
}
impl DatabaseUseLease {
    pub(crate) fn acquire_shared(database: &Path) -> Result<Self, NativeError> {
        let (parent, path) = database_lock_path(database, true)?;
        let parent_identity = directory_identity(&parent)?;
        let family = Arc::new(open_shared_coordination(
            &parent.join(".database-use-family.lock"),
        )?);
        let lock = Arc::new(open_shared_coordination(&path)?);
        let lease = Self {
            resource: absolute(database)?,
            parent,
            parent_identity,
            family,
            lock,
        };
        lease.revalidate()?;
        Ok(lease)
    }

    pub(crate) fn try_exclusive(database: &Path) -> Result<Option<Self>, NativeError> {
        Ok(Self::try_exclusive_many(&[database])?.and_then(|mut leases| leases.pop()))
    }

    /// One acquisition group owns each physical parent and each basename guard
    /// once. This is explicit group authority, never process-wide reentrancy.
    /// All guards remain live until the final group member using them drops.
    pub(crate) fn try_exclusive_many(
        databases: &[&Path],
    ) -> Result<Option<Vec<Self>>, NativeError> {
        let mut families: Vec<(FileIdentity, Arc<LockedFile>)> = Vec::new();
        let mut resource_locks: Vec<(FileIdentity, PathBuf, Arc<LockedFile>)> = Vec::new();
        let mut leases = Vec::new();
        for database in databases {
            let (parent, path) = database_lock_path(database, false)?;
            let parent_identity = directory_identity(&parent)?;
            let family = if let Some((_, guard)) = families
                .iter()
                .find(|(identity, _)| *identity == parent_identity)
            {
                Arc::clone(guard)
            } else {
                let Some(guard) = try_open_locked_in_existing_parent(
                    parent.join(".database-use-family.lock"),
                    LockMode::Exclusive,
                )?
                else {
                    return Ok(None);
                };
                let guard = Arc::new(guard);
                families.push((parent_identity, Arc::clone(&guard)));
                guard
            };
            let name = path
                .file_name()
                .ok_or_else(|| invalid("database_coordination_name_missing"))?
                .to_owned();
            let lock = if let Some((_, _, guard)) = resource_locks
                .iter()
                .find(|(identity, key, _)| *identity == parent_identity && key.as_os_str() == name)
            {
                Arc::clone(guard)
            } else {
                let Some(guard) = try_open_locked_in_existing_parent(&path, LockMode::Exclusive)?
                else {
                    return Ok(None);
                };
                let guard = Arc::new(guard);
                resource_locks.push((parent_identity, PathBuf::from(name), Arc::clone(&guard)));
                guard
            };
            let lease = Self {
                resource: absolute(database)?,
                parent,
                parent_identity,
                family,
                lock,
            };
            lease.revalidate()?;
            leases.push(lease);
        }
        Ok(Some(leases))
    }

    fn revalidate(&self) -> Result<(), NativeError> {
        let addressed_parent = fs::canonicalize(
            self.resource
                .parent()
                .ok_or_else(|| invalid("database_parent_missing"))?,
        )?;
        if directory_identity(&self.parent)? != self.parent_identity
            || directory_identity(&addressed_parent)? != self.parent_identity
            || !self.family.is_current()?
            || !self.lock.is_current()?
        {
            return Err(invalid("database_parent_incarnation_changed"));
        }
        Ok(())
    }

    /// Identity evidence, never a transferable deletion capability. S3 retains
    /// this exclusive lease while validating/detaching the corresponding file.
    pub(crate) fn exclusive_proof(&self) -> Result<DatabaseUseProof, NativeError> {
        self.revalidate()?;
        if self.family.mode() != LockMode::Exclusive || self.lock.mode() != LockMode::Exclusive {
            return Err(invalid("database_use_not_exclusive"));
        }
        Ok(DatabaseUseProof {
            resource: self.resource.clone(),
            file: optional_resource_identity(&self.resource)?,
            parent: self.parent_identity,
            family_coordination_path: self.family.path().to_owned(),
            family_coordination: self.family.identity()?,
            coordination_path: self.lock.path().to_owned(),
            coordination: self.lock.identity()?,
        })
    }

    /// Producer conversion retains BOTH guards through the enclosing Direct
    /// session. A member of a shared exclusive group cannot be extracted alone.
    pub(crate) fn into_lock(self) -> Result<LockedFile, NativeError> {
        let mut lock =
            Arc::try_unwrap(self.lock).map_err(|_| invalid("database_use_group_still_shared"))?;
        let family =
            Arc::try_unwrap(self.family).map_err(|_| invalid("database_use_group_still_shared"))?;
        lock.retain(family);
        Ok(lock)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DatabaseUseProof {
    resource: PathBuf,
    file: Option<FileIdentity>,
    parent: FileIdentity,
    family_coordination_path: PathBuf,
    family_coordination: FileIdentity,
    coordination_path: PathBuf,
    coordination: FileIdentity,
}

fn database_lock_path(
    database: &Path,
    create_parent: bool,
) -> Result<(PathBuf, PathBuf), NativeError> {
    let database = absolute(database)?;
    reject_file_alias(&database)?;
    let parent = database
        .parent()
        .ok_or_else(|| invalid("database_parent_missing"))?;
    if create_parent {
        fs::create_dir_all(parent)?;
    }
    let parent = fs::canonicalize(parent)?;
    let basename = database
        .file_name()
        .ok_or_else(|| invalid("database_name_missing"))?
        .to_str()
        .ok_or_else(|| invalid("database_name_not_utf8"))?
        .to_lowercase();
    let hash = format!("{:x}", Sha256::digest(basename.as_bytes()));
    let lock = parent.join(format!(".database-use-{hash}.lock"));
    Ok((parent, lock))
}
fn absolute(path: &Path) -> Result<PathBuf, NativeError> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn directory_identity(path: &Path) -> Result<FileIdentity, NativeError> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(invalid("use_root_not_directory"));
    }
    path_identity(path)
}
fn reject_file_alias(path: &Path) -> Result<(), NativeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("resource_symlink_or_not_file"));
    }
    let file = fs::File::open(path)?;
    if super::locks::file_link_count(&file)? != 1
        || super::locks::file_identity(&file)? != path_identity(path)?
    {
        return Err(invalid("resource_hardlink_or_identity_ambiguous"));
    }
    Ok(())
}
fn invalid(reason: impl Into<String>) -> NativeError {
    NativeError::InvalidInput(reason.into())
}

/// Pure, noncreating observation held through the inventory scans. The caller
/// may release state.lock before traversal; writer/use ownership and repeated
/// tree observations remain necessary and uncertainty stays partial.
pub(crate) fn observe_artifact_cohort(
    root: &Path,
) -> Result<Option<ArtifactUseLease>, NativeError> {
    let root = absolute(root)?;
    let identity = directory_identity(&root)?;
    let Some(lock) = try_open_existing_locked(&root.join(".use.lock"), LockMode::Exclusive)? else {
        return Ok(None);
    };
    let lease = ArtifactUseLease {
        root,
        identity,
        lock,
    };
    if lease.cohort_verified()? {
        Ok(Some(lease))
    } else {
        Ok(None)
    }
}

pub(crate) fn claim_maintenance(
    target: &MaintenanceTarget,
) -> Result<MaintenanceAdmission<'static>, NativeError> {
    claim(target, None)
}
pub(crate) fn claim_maintenance_with_owned<'a>(
    target: &MaintenanceTarget,
    writer: &'a WriterLease,
) -> Result<MaintenanceAdmission<'a>, NativeError> {
    claim(target, Some(writer))
}

fn claim<'a>(
    target: &MaintenanceTarget,
    owned: Option<&'a WriterLease>,
) -> Result<MaintenanceAdmission<'a>, NativeError> {
    let binding = match observe_binding(target) {
        Ok(binding) => binding,
        Err(reason) => return Ok(MaintenanceAdmission::Unknown(reason)),
    };
    if bound_roles_overlap(target, &binding) {
        return Ok(MaintenanceAdmission::Unknown(
            "managed_direct_storage_role_overlap".into(),
        ));
    }
    // Explicit Direct selection names its own destination. Otherwise managed
    // storage takes precedence when configured; default-only installs use Direct.
    let direct = target
        .direct_pair
        .clone()
        .or(binding.current_direct.clone());
    let (
        writer_path,
        worker_path,
        worker_state,
        workers_root,
        phase_root,
        state_path,
        artifact_root,
    ) = if let Some(pair) = &direct {
        let layout = DirectLayout::new(&pair.database, &pair.manifest);
        (
            layout.writer_lock_path(),
            layout.worker_lock_path(),
            layout.worker_state_path(),
            layout.worker_workspace_root_path(),
            layout.phase_workspace_root(),
            None,
            layout.artifact_root_path(),
        )
    } else {
        let layout = ManagedLayout::new(&binding.storage);
        (
            layout.writer_lock_path(),
            layout.worker_lock_path(),
            layout.worker_state_path(),
            binding.storage.join("workers"),
            layout.phase_workspace_root(),
            Some(layout.state_lock_path()),
            layout.artifacts_root(),
        )
    };
    if !real_path(writer_path.parent().unwrap_or_else(|| Path::new(".")))
        || !real_path(&artifact_root)
    {
        return Ok(MaintenanceAdmission::Unknown(
            "destination_missing_or_symlinked".into(),
        ));
    }
    let writer = if let Some(writer) = owned {
        if !writer.verifies(&writer_path, LockMode::Exclusive)? {
            return Err(invalid("owned_writer_wrong_destination_or_mode"));
        }
        DestinationWriter::Borrowed(writer)
    } else {
        let Some(writer) = try_open_locked_in_existing_parent(&writer_path, LockMode::Exclusive)?
        else {
            return Ok(MaintenanceAdmission::Deferred("writer_in_use".into()));
        };
        DestinationWriter::Owned(writer)
    };
    let Some(worker) = try_open_locked_in_existing_parent(&worker_path, LockMode::Exclusive)?
    else {
        return Ok(MaintenanceAdmission::Deferred("worker_in_use".into()));
    };
    let mut phases = Vec::new();
    for root in [&workers_root, &phase_root] {
        if let Err(reason) = guard_registry(root, &mut phases) {
            return Ok(if reason == "startup_in_use" {
                MaintenanceAdmission::Deferred(reason)
            } else {
                MaintenanceAdmission::Unknown(reason)
            });
        }
    }
    // Legacy v1 is not process authority; preserve its evidence until controlled
    // migration. v2 can only be idle when the existing guardian proves emptiness.
    let worker_metadata = match fs::symlink_metadata(&worker_state) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Ok(MaintenanceAdmission::Unknown(
                "worker_state_inspection_unknown".into(),
            ))
        }
    };
    if let Some(metadata) = worker_metadata {
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || !real_path(&worker_state)
            || reject_file_alias(&worker_state).is_err()
        {
            return Ok(MaintenanceAdmission::Unknown(
                "worker_state_identity_unknown".into(),
            ));
        }
        let value: serde_json::Value = match fs::File::open(&worker_state)
            .map_err(NativeError::from)
            .and_then(|f| serde_json::from_reader(f).map_err(NativeError::from))
        {
            Ok(v) => v,
            Err(_) => {
                return Ok(MaintenanceAdmission::Unknown(
                    "worker_state_unreadable".into(),
                ))
            }
        };
        if value.get("version").and_then(|v| v.as_u64()) == Some(2) {
            let descriptor: BuildDescriptorV2 = match serde_json::from_value(value) {
                Ok(d) => d,
                Err(_) => {
                    return Ok(MaintenanceAdmission::Unknown(
                        "worker_descriptor_unknown".into(),
                    ))
                }
            };
            if !matches!(probe_descriptor(&descriptor), Ok(true)) {
                return Ok(MaintenanceAdmission::Unknown(
                    "worker_containment_unknown".into(),
                ));
            }
        } else {
            return Ok(MaintenanceAdmission::Unknown(
                "legacy_quiescence_required".into(),
            ));
        }
    }
    let mut databases = Vec::new();
    if let Some(pair) = &direct {
        for path in [&pair.database, &pair.manifest] {
            if !real_path(path.parent().unwrap_or_else(|| Path::new(".")))
                || reject_file_alias(path).is_err()
            {
                return Ok(MaintenanceAdmission::Unknown(
                    "direct_resource_identity_unknown".into(),
                ));
            }
        }
        if resources_overlap(&pair.database, &pair.manifest) {
            return Ok(MaintenanceAdmission::Unknown(
                "direct_pair_resource_overlap".into(),
            ));
        }
        if let Some(reason) = overlapping_pairs(target, pair)? {
            return Ok(MaintenanceAdmission::Unknown(reason));
        }
        let Some(group) = DatabaseUseLease::try_exclusive_many(&[&pair.database, &pair.manifest])?
        else {
            return Ok(MaintenanceAdmission::Deferred(
                "database_parent_in_use".into(),
            ));
        };
        databases = group;
    }
    let Some(artifact) = ArtifactUseLease::try_exclusive(&artifact_root)? else {
        return Ok(MaintenanceAdmission::Deferred(
            "artifact_root_in_use".into(),
        ));
    };
    let lease = MaintenanceLease {
        writer,
        writer_path,
        state_path,
        artifact,
        _database: databases,
        _workers: vec![worker],
        _phases: phases,
        target: target.clone(),
        binding,
    };
    lease.revalidate()?;
    Ok(MaintenanceAdmission::Acquired(lease))
}

/// Hold every existing lifecycle lease through detachment; no retirement or
/// deletion is performed here. Missing lease beside startup/launch state is
/// unknown, and existing v1/ambiguous guardian evidence is preserved unchanged.
fn guard_registry(root: &Path, guards: &mut Vec<LockedFile>) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("phase_registry_identity_unknown".into());
    }
    if root
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".lifecycle-"))
    {
        let lease = try_open_existing_locked(&root.join("lease"), LockMode::Exclusive)
            .map_err(|e| format!("startup_lease_unknown: {e}"))?
            .ok_or("startup_in_use")?;
        if !super::containment::lifecycle_empty_with_owned(root, &lease)? {
            return Err("phase_containment_unknown".into());
        }
        guards.push(lease);
        return Ok(());
    }
    let mut has_files = false;
    let mut has_lifecycle = false;
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            return Err("phase_registry_symlink".into());
        }
        if ty.is_dir() {
            has_lifecycle |= entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".lifecycle-"));
            guard_registry(&entry.path(), guards)?;
        } else {
            has_files = true;
        }
    }
    if has_files && !has_lifecycle {
        return Err("phase_execution_evidence_without_lifecycle".into());
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PairRegistration {
    database: PathBuf,
    manifest: PathBuf,
}

/// Stable registration records are evidence of possible overlap, never proof
/// of quiescence. Keep them after a session so an alternate pair cannot vanish
/// from the retirement decision as soon as its current reader exits.
pub(crate) fn register_direct_pair(layout: &DirectLayout) -> Result<(), NativeError> {
    let first = layout.registration_path();
    let second = layout
        .manifest_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(
            first
                .file_name()
                .ok_or_else(|| invalid("registration_name_missing"))?,
        );
    let record = PairRegistration {
        database: absolute(layout.db_path())?,
        manifest: absolute(layout.manifest_path())?,
    };
    for path in [first, second] {
        if path.exists() {
            reject_file_alias(&path)?;
            let existing: PairRegistration = serde_json::from_slice(&fs::read(&path)?)?;
            if existing.database != record.database || existing.manifest != record.manifest {
                return Err(invalid("direct_registration_conflict"));
            }
        } else {
            write_json_atomically(&path, &record)?;
        }
    }
    Ok(())
}
fn overlapping_pairs(
    target: &MaintenanceTarget,
    pair: &DirectPair,
) -> Result<Option<String>, NativeError> {
    let mut pairs = vec![default_direct(target)];
    for resource in [&pair.database, &pair.manifest] {
        let parent = resource
            .parent()
            .ok_or_else(|| invalid("resource_parent_missing"))?;
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".direct-registration-")
            {
                if !real_path(&entry.path()) || reject_file_alias(&entry.path()).is_err() {
                    return Ok(Some("direct_registration_identity_unknown".into()));
                }
                let record: PairRegistration =
                    match serde_json::from_slice(&fs::read(entry.path())?) {
                        Ok(record) => record,
                        Err(_) => return Ok(Some("direct_registration_invalid".into())),
                    };
                pairs.push(DirectPair {
                    database: record.database,
                    manifest: record.manifest,
                });
            }
        }
    }
    for other in pairs {
        if other == *pair {
            continue;
        }
        if resources_overlap(&other.database, &pair.database)
            || resources_overlap(&other.manifest, &pair.manifest)
            || resources_overlap(&other.database, &pair.manifest)
            || resources_overlap(&other.manifest, &pair.database)
        {
            return Ok(Some("direct_registered_pair_overlap".into()));
        }
    }
    Ok(None)
}
fn resources_overlap(a: &Path, b: &Path) -> bool {
    // Actual distinct physical files are positive evidence on a case-sensitive
    // filesystem. The conservative coordination-key fallback is for absent or
    // otherwise unproven identities, not a reason to conflate known files.
    if let (Ok(a), Ok(b)) = (path_identity(a), path_identity(b)) {
        return a == b;
    }
    same_physical_path(a, b)
        || match (database_lock_path(a, false), database_lock_path(b, false)) {
            (Ok((_, a)), Ok((_, b))) => a == b,
            _ => true,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::direct::DirectStore;
    use crate::storage::managed::ManagedStore;
    use std::time::{Duration, Instant};

    struct Fixture {
        root: PathBuf,
        target: MaintenanceTarget,
        layout: ManagedLayout,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "cleanup-s2-{}",
                super::super::layout::managed_generation_id()
            ));
            fs::create_dir(&root).unwrap();
            let root = fs::canonicalize(root).unwrap();
            let state = root.join(".codebaseGraph");
            fs::create_dir(&state).unwrap();
            let layout = ManagedLayout::new(state.join("storage"));
            layout.ensure_roots().unwrap();
            fs::write(state.join("config.json"), br#"{"schema_version":2}"#).unwrap();
            for path in [layout.writer_lock_path(), layout.state_lock_path()] {
                drop(open_locked(path, LockMode::Exclusive).unwrap());
            }
            let target = MaintenanceTarget {
                repository_root: root.clone(),
                state_root: state,
                storage_root: None,
                direct_pair: None,
                config_path: None,
            };
            Self {
                root,
                target,
                layout,
            }
        }
        fn direct(&self) -> (MaintenanceTarget, DirectLayout) {
            let pair = default_direct(&self.target);
            let layout = DirectLayout::new(&pair.database, &pair.manifest);
            fs::write(&pair.database, b"db").unwrap();
            fs::write(&pair.manifest, br#"{"files":{}}"#).unwrap();
            drop(ArtifactUseLease::acquire_shared(&layout.artifact_root_path()).unwrap());
            let mut target = self.target.clone();
            target.direct_pair = Some(pair);
            (target, layout)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn acquired(admission: MaintenanceAdmission<'_>) -> MaintenanceLease<'_> {
        match admission {
            MaintenanceAdmission::Acquired(lease) => lease,
            other => panic!("expected acquired: {other:?}"),
        }
    }

    #[test]
    fn dc_s2_family_replaced_parent_incarnation_invalidates_held_use() {
        let fixture = Fixture::new();
        let parent = fixture.root.join("replaceable-parent");
        fs::create_dir(&parent).unwrap();
        let database = parent.join("graph.ldb");
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        fs::rename(&parent, fixture.root.join("detached-parent")).unwrap();
        fs::create_dir(&parent).unwrap();
        fs::write(&database, b"fresh incarnation").unwrap();
        assert!(shared.revalidate().is_err());
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_family_group_deduplicates_same_parent_and_retains_all_guards() {
        let fixture = Fixture::new();
        let database = fixture.root.join("same-parent.ldb");
        let manifest = fixture.root.join("same-parent.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        let mut group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[1].family));
        let first = group[0].exclusive_proof().unwrap();
        let second = group[1].exclusive_proof().unwrap();
        assert_eq!(first.family_coordination, second.family_coordination);
        assert_ne!(first.coordination, second.coordination);
        // Dropping one resource cannot release its sibling's family authority.
        drop(group.remove(0));
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(group);
        assert!(
            DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn dc_s2_family_group_handles_distinct_parents_and_duplicate_resource_keys() {
        let fixture = Fixture::new();
        let first = fixture.root.join("first");
        let second = fixture.root.join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let database = first.join("db.ldb");
        let manifest = second.join("manifest.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        let group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest, &database])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[2].family));
        assert!(Arc::ptr_eq(&group[0].lock, &group[2].lock));
        assert!(!Arc::ptr_eq(&group[0].family, &group[1].family));
        assert_ne!(
            group[0].exclusive_proof().unwrap().family_coordination,
            group[1].exclusive_proof().unwrap().family_coordination
        );
    }

    #[test]
    fn dc_s2_family_producer_conversion_holds_parent_until_session_release() {
        let fixture = Fixture::new();
        let database = fixture.root.join("producer.ldb");
        let unrelated = fixture.root.join("other-name.ldb");
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        let converted = shared.into_lock().unwrap();
        assert!(DatabaseUseLease::try_exclusive(&unrelated)
            .unwrap()
            .is_none());
        // Shared operations in the same family continue to coexist.
        drop(DatabaseUseLease::acquire_shared(&unrelated).unwrap());
        fs::write(&database, b"created while owned").unwrap();
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(converted);
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_family_recreated_coordination_invalidates_destination_proof() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let lease = acquired(claim_maintenance(&target).unwrap());
        let record = lease.cohort_record().unwrap();
        write_json_atomically(&lease.cohort_record_path(), &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        let family = layout
            .db_path()
            .parent()
            .unwrap()
            .join(".database-use-family.lock");
        drop(lease);
        fs::rename(&family, family.with_extension("previous-family")).unwrap();
        drop(open_locked(&family, LockMode::Shared).unwrap());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_family_shared_existing_readable_coordination_requires_no_write_permission() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let database = fixture.root.join("owned.ldb");
        let unrelated = fixture.root.join("other-user.ldb");
        drop(DatabaseUseLease::acquire_shared(&database).unwrap());
        let family = fixture.root.join(".database-use-family.lock");
        fs::set_permissions(&family, fs::Permissions::from_mode(0o444)).unwrap();
        let shared = DatabaseUseLease::acquire_shared(&unrelated).unwrap();
        assert_eq!(shared.family.mode(), LockMode::Shared);
        drop(shared);
        fs::set_permissions(family, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_family_windows_shared_readonly_coordination_requires_no_write_permission() {
        let fixture = Fixture::new();
        let database = fixture.root.join("owned.ldb");
        let unrelated = fixture.root.join("other-user.ldb");
        drop(DatabaseUseLease::acquire_shared(&database).unwrap());
        let family = fixture.root.join(".database-use-family.lock");
        let mut permissions = fs::metadata(&family).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&family, permissions).unwrap();
        let result = DatabaseUseLease::acquire_shared(&unrelated);
        let mut permissions = fs::metadata(&family).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&family, permissions).unwrap();
        assert!(result.is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_unicode_nfc_nfd_existing_and_creation_aliases_exclude() {
        let fixture = Fixture::new();
        let database = fixture.root.join("caf\u{e9}.ldb");
        let alias = fixture.root.join("cafe\u{301}.ldb");
        // Guard the absent resource through creation under another spelling.
        let shared = DatabaseUseLease::acquire_shared(&alias).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        fs::write(&database, b"database").unwrap();
        assert_eq!(
            path_identity(&database).unwrap(),
            path_identity(&alias).unwrap()
        );
        assert_eq!(
            super::super::locks::file_link_count(&fs::File::open(&alias).unwrap()).unwrap(),
            1
        );
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(shared);
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_none());
        drop(shared);
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_long_s_consumer_blocks_ascii_direct_even_with_valid_record() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let lease = acquired(claim_maintenance(&target).unwrap());
        write_json_atomically(&lease.cohort_record_path(), &lease.cohort_record().unwrap())
            .unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        let name = layout.db_path().file_name().unwrap().to_str().unwrap();
        assert!(name.contains('s'));
        let alias = layout
            .db_path()
            .with_file_name(name.replacen('s', "\u{17f}", 1));
        assert_eq!(
            path_identity(&alias).unwrap(),
            path_identity(layout.db_path()).unwrap()
        );
        let shared = DatabaseUseLease::acquire_shared(&alias).unwrap();
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_none());
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Deferred(reason) if reason == "database_parent_in_use")
        );
        drop(shared);
        assert!(acquired(claim_maintenance(&target).unwrap())
            .cohort_verified()
            .unwrap());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_parent_aliases_deduplicate_physical_family() {
        let fixture = Fixture::new();
        let parent = fixture.root.join("caf\u{e9}");
        let alias = fixture.root.join("cafe\u{301}");
        fs::create_dir(&parent).unwrap();
        let database = parent.join("db.ldb");
        let manifest = alias.join("manifest.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        assert_eq!(
            path_identity(&parent).unwrap(),
            path_identity(&alias).unwrap()
        );
        let group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[1].family));
    }

    #[test]
    fn dc_s2_review_managed_read_resolution_completes_with_paused_collector() {
        use std::sync::mpsc;
        let fixture = Fixture::new();
        let generation = fixture.layout.generation("readable").unwrap();
        generation.ensure_root().unwrap();
        fs::write(generation.db_path(), b"committed database fixture").unwrap();
        fs::write(generation.manifest_path(), br#"{"files":{}}"#).unwrap();
        fs::write(generation.ready_path(), b"").unwrap();
        write_json_atomically(
            &generation.metadata_path(),
            &serde_json::json!({"schema_version":2,"generation_id":"readable"}),
        )
        .unwrap();
        write_json_atomically(
            &fixture.layout.active_pointer_path(),
            &serde_json::json!({"schema_version":2,"generation_id":"readable"}),
        )
        .unwrap();
        let collector = acquired(claim_maintenance(&fixture.target).unwrap());
        let store = ManagedStore::new(fixture.layout.clone());
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let result = store
                .cleanup()
                .and_then(|_| store.resolve_active_read())
                .map(|snapshot| snapshot.map(|snapshot| snapshot.generation_id));
            tx.send(result).unwrap();
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2))
                .expect("reader must finish while collector remains paused")
                .unwrap(),
            Some("readable".into())
        );
        assert!(collector.artifact_cohort_verified().unwrap());
        drop(collector);
        reader.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_review_dangling_worker_state_is_unknown_and_preserved() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        symlink(fixture.root.join("missing-worker-evidence"), &state).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(reason) if reason == "worker_state_identity_unknown")
        );
        assert!(fs::symlink_metadata(state)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_review_unreadable_worker_state_is_unknown_and_preserved() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        fs::write(&state, b"unreadable or invalid containment evidence").unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0)).unwrap();
        let observed = claim_maintenance(&fixture.target).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            matches!(observed, MaintenanceAdmission::Unknown(reason) if reason == "worker_state_unreadable" || reason == "worker_state_identity_unknown")
        );
        assert_eq!(
            fs::read(state).unwrap(),
            b"unreadable or invalid containment evidence"
        );
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_unreadable_windows_worker_state_is_unknown_and_preserved() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        fs::write(&state, b"unavailable containment evidence").unwrap();
        let exclusive = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&state)
            .unwrap();
        assert!(matches!(
            claim_maintenance(&fixture.target).unwrap(),
            MaintenanceAdmission::Unknown(_)
        ));
        drop(exclusive);
        assert!(state.is_file());
    }

    #[test]
    fn dc_s2_review_legacy_database_new_cache_requires_destination_migration_proof() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        // The legacy database existed before this cache was created.
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!lease.cohort_verified().unwrap());
        let record = lease.cohort_record().unwrap();
        assert_eq!(record.resource_use.len(), 2);
        let path = lease.cohort_record_path();
        write_json_atomically(&path, &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        // A recreated use-lock inode invalidates otherwise identical file proof.
        let (_, use_path) = database_lock_path(layout.db_path(), false).unwrap();
        fs::rename(&use_path, use_path.with_extension("old-use-lock")).unwrap();
        drop(open_locked(&use_path, LockMode::Shared).unwrap());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
        write_json_atomically(&path, &lease.cohort_record().unwrap()).unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        let old = fixture.target.state_root.join("old-database-incarnation");
        fs::rename(layout.db_path(), old).unwrap();
        fs::write(layout.db_path(), b"new database incarnation").unwrap();
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_review_publication_proof_does_not_bless_legacy_destination_or_sibling() {
        let fixture = Fixture::new();
        let collector = acquired(claim_maintenance(&fixture.target).unwrap());
        let mut use_leases = Vec::new();
        for id in ["fresh", "legacy"] {
            let generation = fixture.layout.generation(id).unwrap();
            generation.ensure_root().unwrap();
            fs::write(generation.db_path(), id).unwrap();
            fs::write(generation.manifest_path(), br#"{"files":{}}"#).unwrap();
            let group = DatabaseUseLease::try_exclusive_many(&[
                &generation.db_path(),
                &generation.manifest_path(),
            ])
            .unwrap()
            .unwrap();
            let mut group = group.into_iter();
            use_leases.push((group.next().unwrap(), group.next().unwrap()));
        }
        let (fresh_db, fresh_manifest) = &use_leases[0];
        let publication = collector
            .publication_cohort_record(fresh_db, fresh_manifest)
            .unwrap();
        assert!(collector
            .publication_cohort_verified(&publication, fresh_db, fresh_manifest)
            .unwrap());
        let (legacy_db, legacy_manifest) = &use_leases[1];
        assert!(!collector
            .publication_cohort_verified(&publication, legacy_db, legacy_manifest)
            .unwrap());
        write_json_atomically(&collector.cohort_record_path(), &publication).unwrap();
        assert!(!collector.cohort_verified().unwrap());
        assert_eq!(
            serde_json::to_value(publication).unwrap()["origin"],
            "guarded_publication"
        );
        assert_eq!(
            serde_json::to_value(collector.cohort_record().unwrap()).unwrap()["origin"],
            "controlled_migration"
        );
        drop(collector);
        drop(use_leases);
        let (target, _) = fixture.direct();
        let direct = acquired(claim_maintenance(&target).unwrap());
        let publication = direct.direct_publication_cohort_record().unwrap();
        assert!(direct
            .direct_publication_cohort_verified(&publication)
            .unwrap());
        write_json_atomically(&direct.cohort_record_path(), &publication).unwrap();
        assert!(!direct.cohort_verified().unwrap());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dc_s2_review_linux_case_only_distinct_registrations_remain_distinct() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let upper = layout.db_path().with_file_name(
            layout
                .db_path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_uppercase(),
        );
        fs::write(&upper, b"distinct case-sensitive database").unwrap();
        assert_ne!(
            path_identity(&upper).unwrap(),
            path_identity(layout.db_path()).unwrap()
        );
        let other_manifest = fixture.target.state_root.join("other-manifest.json");
        fs::write(&other_manifest, b"{}").unwrap();
        register_direct_pair(&DirectLayout::new(&upper, &other_manifest)).unwrap();
        assert!(!resources_overlap(&upper, layout.db_path()));
        assert!(matches!(
            claim_maintenance(&target).unwrap(),
            MaintenanceAdmission::Acquired(_)
        ));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn dc_s2_review_hardlinked_coordination_invalidates_held_authority() {
        let fixture = Fixture::new();
        let path = fixture.layout.writer_lock_path();
        let writer = open_locked(&path, LockMode::Exclusive).unwrap();
        let alias = fixture.root.join("held-writer-alias.lock");
        fs::hard_link(&path, &alias).unwrap();
        assert!(!writer.verifies(&path, LockMode::Exclusive).unwrap());
        assert!(try_open_existing_locked(&alias, LockMode::Exclusive).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_windows_case_alias_shares_resource_coordination() {
        let fixture = Fixture::new();
        let database = fixture.root.join("CaseOnly.ldb");
        let alias = fixture.root.join("caseonly.LDB");
        fs::write(&database, b"database").unwrap();
        assert_eq!(
            path_identity(&database).unwrap(),
            path_identity(&alias).unwrap()
        );
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_none());
        drop(shared);
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_windows_resource_and_coordination_hardlinks_are_rejected() {
        let fixture = Fixture::new();
        let database = fixture.root.join("native-windows.ldb");
        fs::write(&database, b"database").unwrap();
        let alias = fixture.root.join("native-windows-alias.ldb");
        fs::hard_link(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&database).is_err());
        assert!(DatabaseUseLease::try_exclusive(&alias).is_err());
        let writer_path = fixture.layout.writer_lock_path();
        let writer = open_locked(&writer_path, LockMode::Exclusive).unwrap();
        let lock_alias = fixture.root.join("writer-alias.lock");
        fs::hard_link(&writer_path, &lock_alias).unwrap();
        assert!(!writer.verifies(&writer_path, LockMode::Exclusive).unwrap());
        assert!(try_open_existing_locked(&lock_alias, LockMode::Exclusive).is_err());
    }

    #[test]
    fn dc_s2_shared_manifest_registration_across_database_parents_is_retained() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let other_parent = fixture.root.join("other-db-parent");
        fs::create_dir(&other_parent).unwrap();
        let alternate = DirectLayout::new(other_parent.join("other.ldb"), layout.manifest_path());
        register_direct_pair(&alternate).unwrap();
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
        );
    }

    #[test]
    fn dc_s2_external_bound_managed_store_does_not_require_local_state() {
        let fixture = Fixture::new();
        let external = ManagedLayout::new(fixture.root.join("external-storage"));
        external.ensure_roots().unwrap();
        drop(open_locked(external.writer_lock_path(), LockMode::Exclusive).unwrap());
        let config_path = fixture.root.join("external-config.json");
        write_json_atomically(&config_path, &serde_json::json!({"schema_version":2, "repo_root": fixture.root, "storage_root":external.storage_root()})).unwrap();
        fs::remove_dir_all(&fixture.target.state_root).unwrap();
        let mut target = fixture.target.clone();
        target.config_path = Some(config_path);
        target.storage_root = Some(external.storage_root().to_owned());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!fixture.target.state_root.exists());
    }

    #[test]
    fn dc_s2_current_v2_receipt_and_unknown_v1_follow_existing_guardian_policy() {
        use crate::storage::containment::{ContainmentProof, ExitOutcome};
        let fixture = Fixture::new();
        let lifecycle = fixture
            .layout
            .phase_workspace_root()
            .join("phase-1/.lifecycle-complete");
        fs::create_dir_all(&lifecycle).unwrap();
        drop(open_locked(lifecycle.join("lease"), LockMode::Exclusive).unwrap());
        let descriptor = BuildDescriptorV2 {
            version: 2,
            build_id: "build".into(),
            lifecycle_dir: lifecycle.clone(),
            endpoint: "unused".into(),
            nonce: "unused".into(),
            worker_pid: std::process::id(),
            guardian_pid: std::process::id(),
            proof: ContainmentProof::Pending,
        };
        write_json_atomically(&lifecycle.join("proof.json"), &descriptor).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "phase_containment_unknown")
        );
        let receipt = ExitOutcome {
            success: true,
            code: Some(0),
            signal: None,
            high_water_bytes: 0,
            budget_reason: None,
            cancelled: false,
            containment_empty: true,
            supervision_error: None,
        };
        write_json_atomically(&lifecycle.join("receipt.json"), &receipt).unwrap();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(lifecycle.join("proof.json").exists());
        drop(lease);
        write_json_atomically(
            &fixture.layout.worker_state_path(),
            &serde_json::json!({"version":1,"pid":std::process::id()}),
        )
        .unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "legacy_quiescence_required")
        );
        assert!(fixture.layout.worker_state_path().exists());
    }

    #[test]
    fn dc_s2_waiter_on_detached_incarnation_cannot_write_fresh_public_root() {
        use std::sync::mpsc;
        let fixture = Fixture::new();
        let root = fixture.layout.artifacts_root();
        let exclusive = open_locked(root.join(".use.lock"), LockMode::Exclusive).unwrap();
        let (opened_tx, opened_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();
        let producer_root = root.clone();
        let producer = std::thread::spawn(move || {
            super::super::locks::before_lock_test_hook(move || {
                opened_tx.send(()).unwrap();
                start_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            });
            ArtifactUseLease::acquire_shared(&producer_root)
        });
        opened_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        fs::rename(&root, fixture.layout.storage_root().join("detached")).unwrap();
        drop(ArtifactUseLease::acquire_shared(&root).unwrap());
        start_tx.send(()).unwrap();
        drop(exclusive);
        assert!(producer.join().unwrap().is_err());
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
    }

    #[test]
    fn dc_s2_worker_launch_evidence_without_lifecycle_is_unknown() {
        let fixture = Fixture::new();
        let root = fixture
            .layout
            .storage_root()
            .join("workers/worker-incomplete");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("request.json"), b"execution evidence").unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "phase_execution_evidence_without_lifecycle")
        );
        assert!(root.join("request.json").exists());
    }

    #[test]
    fn dc_s2_exact_borrowed_writer_requires_exclusive_exact_destination() {
        let fixture = Fixture::new();
        let writer = open_locked(fixture.layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let lease = acquired(claim_maintenance_with_owned(&fixture.target, &writer).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert_eq!(lease.target(), &fixture.target);
        drop(lease);
        drop(writer);
        let shared = open_locked(fixture.layout.writer_lock_path(), LockMode::Shared).unwrap();
        assert!(claim_maintenance_with_owned(&fixture.target, &shared).is_err());
        let foreign = open_locked(fixture.root.join("foreign.lock"), LockMode::Exclusive).unwrap();
        assert!(claim_maintenance_with_owned(&fixture.target, &foreign).is_err());
    }

    #[test]
    fn dc_s2_writer_worker_and_startup_contention_are_nonblocking() {
        let fixture = Fixture::new();
        for (path, reason) in [
            (fixture.layout.writer_lock_path(), "writer_in_use"),
            (fixture.layout.worker_lock_path(), "worker_in_use"),
        ] {
            let held = open_locked(path, LockMode::Exclusive).unwrap();
            let started = Instant::now();
            assert!(
                matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == reason)
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(held);
        }
        let startup = fixture
            .layout
            .phase_workspace_root()
            .join(".lifecycle-starting");
        fs::create_dir_all(&startup).unwrap();
        let held = open_locked(startup.join("lease"), LockMode::Exclusive).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "startup_in_use")
        );
        drop(held);
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(
            try_open_existing_locked(&startup.join("lease"), LockMode::Exclusive)
                .unwrap()
                .is_none()
        );
        drop(lease);
    }

    #[test]
    fn dc_s2_missing_startup_lease_keeps_unknown_evidence() {
        let fixture = Fixture::new();
        let startup = fixture
            .layout
            .phase_workspace_root()
            .join(".lifecycle-unknown");
        fs::create_dir_all(&startup).unwrap();
        fs::write(startup.join("request.json"), b"launch evidence").unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r.contains("startup_lease_unknown"))
        );
        assert_eq!(
            fs::read(startup.join("request.json")).unwrap(),
            b"launch evidence"
        );
        assert!(!startup.join("lease").exists());
    }

    #[test]
    fn dc_s2_owned_state_transition_never_waits_or_holds_state_during_validation() {
        let fixture = Fixture::new();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        let state = open_locked(fixture.layout.state_lock_path(), LockMode::Shared).unwrap();
        assert_eq!(lease.with_state_lock(|| Ok(7)).unwrap(), None);
        drop(state);
        assert_eq!(lease.with_state_lock(|| Ok(7)).unwrap(), Some(7));
    }

    #[test]
    fn dc_s2_producer_and_collector_exclude_whole_artifact_operation() {
        let fixture = Fixture::new();
        let producer = ArtifactUseLease::acquire_shared(&fixture.layout.artifacts_root()).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "artifact_root_in_use")
        );
        fs::write(
            fixture.layout.artifacts_root().join("produced"),
            b"still protected",
        )
        .unwrap();
        assert!(
            ArtifactUseLease::try_exclusive(&fixture.layout.artifacts_root())
                .unwrap()
                .is_none()
        );
        drop(producer);
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(try_open_existing_locked(
            &fixture.layout.artifacts_root().join(".use.lock"),
            LockMode::Shared
        )
        .unwrap()
        .is_none());
        drop(lease);
    }

    #[test]
    fn dc_s2_direct_reader_and_alternate_manifest_exclude_collector() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let reader = DirectStore::new(layout.clone())
            .unwrap()
            .begin_read()
            .unwrap();
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "writer_in_use")
        );
        let alternate = DirectLayout::new(
            layout.db_path(),
            fixture.target.state_root.join("other-manifest.json"),
        );
        let mut alt_target = target.clone();
        alt_target.direct_pair = Some(DirectPair {
            database: alternate.db_path().to_owned(),
            manifest: alternate.manifest_path().to_owned(),
        });
        drop(ArtifactUseLease::acquire_shared(&alternate.artifact_root_path()).unwrap());
        assert!(
            matches!(claim_maintenance(&alt_target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
        );
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_none());
        drop(reader);
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_direct_borrowed_session_truthfully_defers_active_companions() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let session = DirectStore::new(layout).unwrap().begin_write().unwrap();
        let started = Instant::now();
        assert!(
            matches!(claim_maintenance_with_owned(&target, session.writer_lease().unwrap()).unwrap(), MaintenanceAdmission::Deferred(r) if r == "database_parent_in_use")
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(session);
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_managed_borrowed_session_can_admit_without_reacquiring_writer() {
        let fixture = Fixture::new();
        let session = ManagedStore::new(fixture.layout.clone())
            .begin_write()
            .unwrap();
        let lease = acquired(
            claim_maintenance_with_owned(&fixture.target, session.writer_lease().unwrap()).unwrap(),
        );
        assert!(lease.artifact_cohort_verified().unwrap());
        drop(lease);
        drop(session);
    }

    #[test]
    fn dc_s2_registered_shared_manifest_and_shared_database_are_retained() {
        for shared_database in [true, false] {
            let fixture = Fixture::new();
            let (target, layout) = fixture.direct();
            let alternate = if shared_database {
                DirectLayout::new(
                    layout.db_path(),
                    fixture.target.state_root.join("alternate.json"),
                )
            } else {
                DirectLayout::new(
                    fixture.target.state_root.join("alternate.ldb"),
                    layout.manifest_path(),
                )
            };
            register_direct_pair(&alternate).unwrap();
            assert!(
                matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
            );
        }
    }

    #[test]
    fn dc_s2_guarded_creation_and_controlled_proof_are_incarnation_bound() {
        let fixture = Fixture::new();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!lease.cohort_verified().unwrap());
        let record = lease.cohort_record().unwrap();
        let path = lease.cohort_record_path();
        write_json_atomically(&path, &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        let old = fixture.layout.artifacts_root();
        let artifact_record = fs::read(old.join(".use-cohort.json")).unwrap();
        let detached = fixture.layout.storage_root().join("detached-artifacts");
        fs::rename(&old, &detached).unwrap();
        drop(ArtifactUseLease::acquire_shared(&old).unwrap());
        assert!(lease.revalidate().is_err());
        drop(lease);
        // Copying the old proof onto a new incarnation is not authority.
        fs::write(old.join(".use-cohort.json"), artifact_record).unwrap();
        let fresh = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(!fresh.artifact_cohort_verified().unwrap());
        assert!(!fresh.cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_preexisting_root_never_becomes_modern_from_guard_acquisition() {
        let fixture = Fixture::new();
        let root = fixture.root.join("preexisting");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("legacy"), b"unknown").unwrap();
        drop(ArtifactUseLease::acquire_shared(&root).unwrap());
        assert!(!root.join(".use-cohort.json").exists());
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
    }

    #[test]
    fn dc_s2_inventory_cohort_observer_is_noncreating_and_validates_lock_identity() {
        let fixture = Fixture::new();
        let root = fixture.root.join("missing");
        assert!(observe_artifact_cohort(&root).is_err());
        assert!(!root.exists());
        let root = fixture.root.join("existing");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".use-cohort.json"), b"true").unwrap();
        assert!(observe_artifact_cohort(&root).is_err());
        assert!(!root.join(".use.lock").exists());
        let root = fixture.layout.artifacts_root();
        let before = fs::read(root.join(".use-cohort.json")).unwrap();
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
        assert_eq!(fs::read(root.join(".use-cohort.json")).unwrap(), before);
        fs::remove_file(root.join(".use.lock")).unwrap();
        drop(open_locked(root.join(".use.lock"), LockMode::Shared).unwrap());
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_hardlinks_symlinks_and_case_aliases_cannot_bypass_resource_guard() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let database = fixture.root.join("Graph.ldb");
        fs::write(&database, b"db").unwrap();
        let held = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(
            DatabaseUseLease::try_exclusive(&fixture.root.join("graph.ldb"))
                .unwrap()
                .is_none()
        );
        let alias = fixture.root.join("alias.ldb");
        symlink(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&alias).is_err());
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&alias).is_err());
        drop(held);
    }

    #[test]
    fn dc_s2_replaced_writer_inode_does_not_validate_borrowed_authority() {
        let fixture = Fixture::new();
        let path = fixture.layout.writer_lock_path();
        let writer = open_locked(&path, LockMode::Exclusive).unwrap();
        fs::remove_file(&path).unwrap();
        drop(open_locked(&path, LockMode::Shared).unwrap());
        assert!(claim_maintenance_with_owned(&fixture.target, &writer).is_err());
    }
}
