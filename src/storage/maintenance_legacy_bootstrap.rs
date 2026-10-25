//! Actual legacy enrollment and scoped mutation. The lower guard module alone
//! supplies noncreating ordinary native/producer use checks.
use super::inventory::{default_direct, DirectPair, MaintenanceTarget};
use super::layout::DirectLayout;
use super::locks::{
    self, FileIdentity, FixedControlError, FixedControlKind as Kind,
    FixedControlProblem as Problem, FixedControlScratch, FixedFileStamp, FixedLockedFile, LockMode,
    RootedDirectory,
};
use super::maintenance_admission::PreparedTargetBinding;
use super::maintenance_budget::{CleanupMemory, MemoryReservation, MutationGrant};
use super::maintenance_controls as codec;
use super::maintenance_legacy_guard::*;
use super::maintenance_legacy_guard::{
    component, invalid, Binding, Record, BINDING_BYTES, GATE, OWNER, SELECTOR, SLOT0, SLOT1, STATE,
    STATE_BYTES,
};
use super::maintenance_scheduler::LegacyQuiescenceConfirmation;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};
#[derive(Clone, Copy)]
struct Entry {
    stamp: Option<FixedFileStamp>,
    kind: Kind,
}
impl Entry {
    fn capture(
        path: &Path,
        kind: Kind,
        optional: bool,
        scratch: &mut FixedControlScratch,
    ) -> Result<Self, FixedControlError> {
        let stamp = match locks::open_fixed_control(path, kind, false, scratch) {
            Ok(file) => Some(FixedFileStamp::capture(&file, kind)?),
            Err(FixedControlError::Io(e))
                if optional && e.kind() == std::io::ErrorKind::NotFound =>
            {
                None
            }
            Err(e) => return Err(e),
        };
        Ok(Self { stamp, kind })
    }
}
pub(crate) enum LegacyScope<'a> {
    DefaultDirect {
        pair: &'a DirectPair,
        artifact_root: &'a Path,
    },
    ManagedGeneration {
        storage_root: &'a Path,
        generation_root: &'a Path,
    },
    ManagedAdministration {
        storage_root: &'a Path,
        generation_root: &'a Path,
    },
}
pub(crate) struct PreparedLegacyScope {
    kind: LegacyScopeKind,
    cohort_scope: [u8; 32],
    root: PathBuf,
    repository: PathBuf,
    artifact: PathBuf,
    member: PathBuf,
    database: PathBuf,
    manifest: PathBuf,
    writer: PathBuf,
    family: PathBuf,
    native: PathBuf,
    root_use: PathBuf,
    paths: [PathBuf; 3],
    entries: [Entry; 10],
    binding: PreparedTargetBinding,
    nonce: [u8; 32],
    owner_nonce: [u8; 32],
    coordination: [[u8; locks::COORDINATION_HEADER_BYTES]; 3],
    enrollment_waiting: bool,
    birth: Option<super::maintenance_budget::MemoryBox<PreparedSourceBirth>>,
    observation: Option<super::maintenance_budget::MemoryBox<PreparedSourceObservation>>,
    publication: Option<
        super::maintenance_budget::MemoryBox<
            super::maintenance_legacy_guard::PreparedSourcePublication,
        >,
    >,
    job: [u8; 32],
    key: [u8; 64],
    member_name: [u8; 128],
    member_len: usize,
    freeze_cache: bool,
    scratch: FixedControlScratch,
    confirmation_source: Option<super::maintenance_admission::LegacyConfirmationSource>,
    confirmation_caches: [(PathBuf, Entry); 2],
    administration_selector: Option<(PathBuf, Entry)>,
    enrollment_runs: Option<(PathBuf, Entry)>,
    _floor: MemoryReservation,
}

pub(crate) fn prepare_legacy_scope(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    scope: LegacyScope<'_>,
    freeze_cache: bool,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyScope>, FixedControlError> {
    let mut prepared = prepare_legacy_scope_layout(target, binding, scope, freeze_cache, memory)?;
    prepare_legacy_scope_source(&mut prepared, target, memory)?;
    prepared.observation =
        PreparedSourceObservation::read(&prepared.root, &prepared.paths, None, memory)?;
    Ok(prepared)
}

// Layout construction and source observation use distinct stack frames. The
// returned charged allocation owns every field before any source is observed.
#[inline(never)]
fn prepare_legacy_scope_layout(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    scope: LegacyScope<'_>,
    freeze_cache: bool,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyScope>, FixedControlError> {
    let mut charge = std::mem::size_of::<PreparedLegacyScope>() + 65536;
    for p in [
        target.repository_root.as_path(),
        target.state_root.as_path(),
    ] {
        charge = charge
            .checked_add(
                p.as_os_str()
                    .as_encoded_bytes()
                    .len()
                    .checked_mul(32)
                    .ok_or(FixedControlError::Resource(
                        super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
                    ))?,
            )
            .ok_or(FixedControlError::Resource(
                super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
            ))?;
    }
    let floor = memory
        .reserve(charge)
        .map_err(FixedControlError::Resource)?;
    let administrative = matches!(&scope, LegacyScope::ManagedAdministration { .. });
    // Fixed branch layout keeps every intended path and physical binding explicit without a dynamic container.
    #[allow(clippy::type_complexity)]
    let (kind, root, artifact, member, database, manifest, writer, key, member_name, member_len): (
        LegacyScopeKind,
        PathBuf,
        PathBuf,
        PathBuf,
        PathBuf,
        PathBuf,
        PathBuf,
        [u8; 64],
        [u8; 128],
        usize,
    ) = match scope {
        LegacyScope::DefaultDirect {
            pair,
            artifact_root,
        } => {
            if *pair != default_direct(target)
                || pair.database.parent() != Some(target.state_root.as_path())
                || pair.manifest != target.state_root.join("manifest.json")
                || artifact_root.parent() != Some(target.state_root.as_path())
            {
                return Err(invalid(Problem::InvalidRecord));
            }
            let layout = DirectLayout::new(&pair.database, &pair.manifest);
            let conventional = target.state_root.join("artifacts");
            if artifact_root != conventional && artifact_root != layout.artifact_root_path() {
                return Err(invalid(Problem::InvalidRecord));
            }
            if pair.database.to_str().is_none() || pair.manifest.to_str().is_none() {
                return Err(invalid(Problem::InvalidRecord));
            }
            let mut hash = Sha256::new();
            hash.update(pair.database.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash.update(pair.manifest.as_os_str().as_encoded_bytes());
            let digest = hash.finalize();
            let mut key = [0; 64];
            let digits = b"0123456789abcdef";
            for (i, b) in digest.iter().enumerate() {
                key[2 * i] = digits[(b >> 4) as usize];
                key[2 * i + 1] = digits[(b & 15) as usize];
            }
            (
                LegacyScopeKind::DefaultDirect,
                target.state_root.clone(),
                artifact_root.to_owned(),
                target.state_root.clone(),
                pair.database.clone(),
                pair.manifest.clone(),
                layout.writer_lock_path(),
                key,
                [0; 128],
                0,
            )
        }
        LegacyScope::ManagedGeneration {
            storage_root,
            generation_root,
        }
        | LegacyScope::ManagedAdministration {
            storage_root,
            generation_root,
        } => {
            if super::maintenance_projection::project_binding(target, memory)
                .map_err(|_| invalid(Problem::InvalidRecord))?
                .storage
                != storage_root
                || generation_root.parent() != Some(storage_root.join("generations").as_path())
            {
                return Err(invalid(Problem::InvalidRecord));
            }
            let name = generation_root
                .file_name()
                .ok_or_else(|| invalid(Problem::InvalidRecord))?
                .as_encoded_bytes();
            if !name.starts_with(b"gen-") || name.len() > 128 || !component(name) {
                return Err(invalid(Problem::InvalidRecord));
            }
            let mut member_name = [0; 128];
            member_name[..name.len()].copy_from_slice(name);
            (
                if administrative {
                    LegacyScopeKind::ManagedAdministration
                } else {
                    LegacyScopeKind::ManagedGeneration
                },
                storage_root.to_owned(),
                storage_root.join("artifacts"),
                generation_root.to_owned(),
                generation_root.join("graph.ldb"),
                generation_root.join("manifest.json"),
                storage_root.join("writer.lock"),
                [0; 64],
                member_name,
                name.len(),
            )
        }
    };
    let (_, native) = super::maintenance_ownership::database_lock_path(&database, false)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let family = database
        .parent()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?
        .join(".database-use-family.lock");
    let root_use = artifact.join(".use.lock");
    let paths = [
        root.join(std::str::from_utf8(if administrative { ADMIN_GATE } else { GATE }).unwrap()),
        root.join(std::str::from_utf8(if administrative { ADMIN_STATE } else { STATE }).unwrap()),
        root.join(std::str::from_utf8(if administrative { ADMIN_OWNER } else { OWNER }).unwrap()),
    ];
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    let entries = [
        Entry::capture(&root, Kind::Directory, false, &mut scratch)?,
        Entry::capture(
            &target.repository_root,
            Kind::Directory,
            false,
            &mut scratch,
        )?,
        Entry::capture(&artifact, Kind::Directory, false, &mut scratch)?,
        Entry::capture(
            &member,
            Kind::Directory,
            matches!(
                kind,
                LegacyScopeKind::ManagedGeneration | LegacyScopeKind::ManagedAdministration
            ),
            &mut scratch,
        )?,
        Entry::capture(&database, Kind::File, true, &mut scratch)?,
        Entry::capture(&manifest, Kind::File, true, &mut scratch)?,
        Entry::capture(&writer, Kind::File, true, &mut scratch)?,
        Entry::capture(&family, Kind::File, true, &mut scratch)?,
        Entry::capture(&native, Kind::File, true, &mut scratch)?,
        Entry::capture(&root_use, Kind::File, true, &mut scratch)?,
    ];
    let mut nonce = [0; 32];
    let mut owner_nonce = [0; 32];
    let mut job = [0; 32];
    for out in [&mut nonce, &mut owner_nonce, &mut job] {
        getrandom::fill(out).map_err(|_| invalid(Problem::InvalidRecord))?;
    }
    let default_pair = default_direct(target);
    let conventional = target.state_root.join("artifacts");
    let keyed =
        DirectLayout::new(&default_pair.database, &default_pair.manifest).artifact_root_path();
    let confirmation_caches = [
        (
            conventional.clone(),
            Entry::capture(&conventional, Kind::Directory, true, &mut scratch)?,
        ),
        (
            keyed.clone(),
            Entry::capture(&keyed, Kind::Directory, true, &mut scratch)?,
        ),
    ];
    let cohort_scope = legacy_cohort_scope(target, &root, &artifact, memory)?;
    let administration_selector = if matches!(
        kind,
        LegacyScopeKind::ManagedAdministration | LegacyScopeKind::ManagedGeneration
    ) {
        let path = root.join("active.json");
        let entry = Entry::capture(&path, Kind::File, false, &mut scratch)?;
        if kind == LegacyScopeKind::ManagedGeneration {
            use std::io::Read;
            let mut pointer = locks::open_fixed_control(&path, Kind::File, false, &mut scratch)?;
            let stamp = entry.stamp.ok_or_else(|| invalid(Problem::Changed))?;
            if stamp.len() > 1024 {
                return Err(invalid(Problem::InvalidRecord));
            }
            let mut bytes = [0; 1024];
            pointer.read_exact(&mut bytes[..stamp.len() as usize])?;
            let active: super::managed::ActiveGeneration =
                serde_json::from_slice(&bytes[..stamp.len() as usize])
                    .map_err(|_| invalid(Problem::InvalidRecord))?;
            super::layout::validate_generation_id(&active.generation_id)
                .map_err(|_| invalid(Problem::InvalidRecord))?;
            if active.schema_version != 2
                || super::layout::ManagedLayout::new(&root)
                    .generation(&active.generation_id)
                    .map_err(|_| invalid(Problem::InvalidRecord))?
                    .root()
                    == member
            {
                return Err(invalid(Problem::Busy));
            }
            stamp.recheck(&pointer)?;
        }
        Some((path, entry))
    } else {
        None
    };
    let enrollment_runs = if kind == LegacyScopeKind::ManagedGeneration {
        Some(prepare_empty_runs(&root, memory)?)
    } else {
        None
    };
    super::maintenance_budget::MemoryBox::new(
        PreparedLegacyScope {
            enrollment_waiting: false,
            birth: None,
            observation: None,
            coordination: [[0; locks::COORDINATION_HEADER_BYTES]; 3],
            publication: None,
            kind,
            cohort_scope,
            root,
            repository: target.repository_root.clone(),
            artifact,
            member,
            database,
            manifest,
            writer,
            family,
            native,
            root_use,
            paths,
            entries,
            binding: binding.duplicate(memory)?,
            nonce,
            owner_nonce,
            job,
            key,
            member_name,
            member_len,
            freeze_cache: freeze_cache && kind == LegacyScopeKind::DefaultDirect,
            scratch,
            confirmation_source: None,
            confirmation_caches,
            administration_selector,
            enrollment_runs,
            _floor: floor,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn pending_scope_capsule_boxed(
    prepared: &PreparedLegacyScope,
    memory: &CleanupMemory,
) -> Result<
    Option<
        super::maintenance_budget::MemoryBox<
            super::maintenance_legacy_guard::SourceCreationCapsule,
        >,
    >,
    FixedControlError,
> {
    let capsule = {
        let directory = RootedDirectory::open_readonly_root(
            &prepared.root,
            prepared.entries[0].stamp.unwrap().identity(),
        )?;
        match directory
            .open_readonly_file(prepared.paths[1].file_name().unwrap().as_encoded_bytes())
        {
            Ok(file) => super::maintenance_legacy_guard::read_creation_capsule(&directory, &file)?
                .filter(|c| {
                    locks::created_file_nonce(&file).ok().flatten() == Some(c.permission.job)
                }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        }
    };
    capsule
        .map(|capsule| {
            super::maintenance_budget::MemoryBox::new(capsule, memory)
                .map_err(FixedControlError::Resource)
        })
        .transpose()
}
#[inline(never)]
fn prepare_legacy_scope_source(
    prepared: &mut PreparedLegacyScope,
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    let pending_capsule = pending_scope_capsule_boxed(prepared, memory)?;
    prepared.confirmation_source = if prepared.kind == LegacyScopeKind::DefaultDirect {
        if let Some(capsule) = pending_capsule.as_ref() {
            capsule.permission.root_default_source
        } else if let Some(accepted) =
            accepted_source_observation(&prepared.root, &prepared.paths, false, memory)?.filter(
                |accepted| accepted.1.phase != LegacyPhase::Active || accepted.1.operator_pending,
            )
        {
            accepted.1.root_default_source
        } else {
            Some(super::maintenance_admission::capture_legacy_confirmation_source(target, memory)?)
        }
    } else {
        None
    };
    if let Some(capsule) = pending_capsule.as_ref() {
        prepared.job = capsule.permission.job;
    }
    let observed_job = accepted_source_observation(&prepared.root, &prepared.paths, false, memory)?
        .map(|accepted| accepted.1.job)
        .unwrap_or(prepared.job);
    prepared.nonce = existing_coordination_nonce(&prepared.paths[0])?.unwrap_or(prepared.nonce);
    prepared.owner_nonce =
        existing_coordination_nonce(&prepared.paths[2])?.unwrap_or(prepared.owner_nonce);
    let use_nonce = existing_coordination_nonce(&prepared.root_use)?.unwrap_or(observed_job);
    prepared.coordination = [
        locks::encode_coordination_incarnation(prepared.nonce),
        locks::encode_coordination_incarnation(prepared.owner_nonce),
        locks::encode_coordination_incarnation(use_nonce),
    ];
    Ok(())
}
/// This value can only borrow actual held exclusive guards inside enrollment.
/// It is neither serialized nor constructible from names, IDs or a bool.
pub(crate) struct LegacyEnrollmentAuthority<'a> {
    kind: LegacyScopeKind,
    directory: &'a RootedDirectory,
    artifact: &'a RootedDirectory,
    member: &'a RootedDirectory,
    writer: &'a FixedLockedFile,
    root_use: &'a FixedLockedFile,
    _family: &'a FixedLockedFile,
    _native: &'a FixedLockedFile,
    _gate: &'a FixedLockedFile,
    _confirmation: Option<&'a LegacyQuiescenceConfirmation<'a>>,
    incarnation: [u8; 32],
}
impl LegacyEnrollmentAuthority<'_> {
    pub(crate) fn directory(&self) -> &RootedDirectory {
        self.directory
    }
    pub(crate) fn artifact_directory(&self) -> &RootedDirectory {
        self.artifact
    }
    pub(crate) fn member_directory(&self) -> &RootedDirectory {
        self.member
    }
    pub(crate) fn writer_identity(&self) -> Result<FileIdentity, FixedControlError> {
        Ok(FixedFileStamp::capture(self.writer.file(), Kind::File)?.identity())
    }
    pub(crate) fn artifact_use_identity(&self) -> Result<FileIdentity, FixedControlError> {
        Ok(FixedFileStamp::capture(self.root_use.file(), Kind::File)?.identity())
    }
    pub(crate) fn artifact_use_guard(&self) -> &FixedLockedFile {
        self.root_use
    }
    pub(crate) fn proposed_artifact_incarnation(&self) -> [u8; 32] {
        self.incarnation
    }
}
fn checked_entry(
    path: &Path,
    entry: Entry,
    scratch: &mut FixedControlScratch,
    grant: &mut MutationGrant,
) -> Result<File, FixedControlError> {
    grant.take_entry()?;
    let file = locks::open_fixed_control(path, entry.kind, false, scratch)?;
    let current = FixedFileStamp::capture(&file, entry.kind)?;
    let expected = entry.stamp.ok_or_else(|| invalid(Problem::Changed))?;
    if entry.kind == Kind::Directory {
        if current.identity() != expected.identity() {
            return Err(invalid(Problem::Changed));
        }
    } else {
        expected.recheck(&file)?;
    }
    Ok(file)
}
fn lock_entry(
    path: &Path,
    entry: Entry,
    parent: &RootedDirectory,
    scratch: &mut FixedControlScratch,
    grant: &mut MutationGrant,
) -> Result<(FixedLockedFile, bool), FixedControlError> {
    grant.take_entry()?;
    let (file, created) = if let Some(stamp) = entry.stamp {
        let f = locks::open_fixed_control(path, Kind::File, true, scratch)?;
        stamp.recheck(&f)?;
        (f, false)
    } else {
        let name = path
            .file_name()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?
            .as_encoded_bytes();
        (parent.create_new(name)?, true)
    };
    let guard = FixedLockedFile::try_lock(file, LockMode::Exclusive)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    Ok((guard, created))
}

fn init_coordination(
    file: &File,
    image: &[u8; locks::COORDINATION_HEADER_BYTES],
    grant: &mut MutationGrant,
) -> Result<(), FixedControlError> {
    if file.metadata()?.len() != 0 {
        return Err(invalid(Problem::InvalidRecord));
    }
    write_range(file, 0, image, grant)
}
fn init_owner_coordination(
    file: &File,
    image: &[u8; locks::COORDINATION_HEADER_BYTES],
    grant: &mut MutationGrant,
) -> Result<(), FixedControlError> {
    init_coordination(file, image, grant)?;
    #[cfg(windows)]
    {
        grant.take_entries(2)?;
        locks::set_created_file_nonce(file, image[8..40].try_into().unwrap())?;
        grant.check()?;
    }
    Ok(())
}

pub(crate) struct LegacySourceOwner {
    state_stamp: FixedFileStamp,
    state_pin: File,
    gate_pin: File,
    gate_header: [u8; locks::COORDINATION_HEADER_BYTES],
    artifact_header: [u8; locks::COORDINATION_HEADER_BYTES],
    owner_header: [u8; locks::COORDINATION_HEADER_BYTES],
    commit: codec::CommitFact,
    owner: FixedLockedFile,
    staging: bool,
    pending_publication: bool,
    _root_pin: File,
    binding: Binding,
    record: Record,
    state_path: PathBuf,
    gate_path: PathBuf,
    _floor: MemoryReservation,
}
/// A closed operational source version, not program origin or deletion proof.
#[derive(Clone, Copy)]
pub(crate) struct LegacyClosedVersion {
    root: FileIdentity,
    state: FileIdentity,
    gate: FileIdentity,
    nonce: [u8; 32],
    version: u64,
    job: [u8; 32],
    phase: LegacyPhase,
    member: FileIdentity,
    database: Option<FixedFileStamp>,
    manifest: Option<FixedFileStamp>,
    cache: FileIdentity,
}
impl LegacyClosedVersion {
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.root
    }
    pub(crate) fn member_identity(&self) -> FileIdentity {
        self.member
    }
    pub(crate) fn phase(&self) -> LegacyPhase {
        self.phase
    }
    pub(crate) fn cache_identity(&self) -> FileIdentity {
        self.cache
    }
    pub(crate) fn database_stamp(&self) -> Option<FixedFileStamp> {
        self.database
    }
    pub(crate) fn manifest_stamp(&self) -> Option<FixedFileStamp> {
        self.manifest
    }
    pub(crate) fn context_digest(&self) -> [u8; 32] {
        let mut bytes = [0; 320];
        id(&mut bytes[0..16], self.root);
        id(&mut bytes[16..32], self.state);
        id(&mut bytes[32..48], self.gate);
        bytes[48..80].copy_from_slice(&self.nonce);
        bytes[80..88].copy_from_slice(&self.version.to_le_bytes());
        bytes[88..120].copy_from_slice(&self.job);
        bytes[120] = match self.phase {
            LegacyPhase::Active => 0,
            LegacyPhase::Preparing => 1,
            LegacyPhase::Retiring => 2,
            LegacyPhase::Removed => 3,
        };
        id(&mut bytes[128..144], self.member);
        if let Some(stamp) = self.database {
            stamp.encode_checkpoint((&mut bytes[144..224]).try_into().unwrap());
        }
        if let Some(stamp) = self.manifest {
            stamp.encode_checkpoint((&mut bytes[224..304]).try_into().unwrap());
        }
        id(&mut bytes[304..320], self.cache);
        Sha256::digest(bytes).into()
    }
}
impl LegacySourceOwner {
    pub(crate) fn closed_version(&self) -> LegacyClosedVersion {
        LegacyClosedVersion {
            root: self.binding.root,
            state: self.binding.state,
            gate: self.binding.gate,
            nonce: self.binding.nonce,
            version: self.record.version,
            job: self.record.job,
            phase: self.record.phase,
            member: self.record.member,
            database: self.record.database,
            manifest: self.record.manifest,
            cache: self.record.cache,
        }
    }
    pub(crate) fn is_current(&self, memory: &CleanupMemory) -> Result<bool, FixedControlError> {
        let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
        let parent = self
            .state_path
            .parent()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        let root = RootedDirectory::open_readonly_root(parent, self.binding.root)?;
        let gate = locks::try_open_existing_locked(&self.gate_path, LockMode::Shared)
            .map_err(|_| invalid(Problem::Changed))?
            .ok_or_else(|| invalid(Problem::Busy))?;
        let administrative = self.binding.kind == LegacyScopeKind::ManagedAdministration;
        let gate_name = if self.staging {
            STAGING_GATE
        } else if administrative {
            ADMIN_GATE
        } else {
            GATE
        };
        let state_name = if self.staging {
            STAGING_STATE
        } else if administrative {
            ADMIN_STATE
        } else {
            STATE
        };
        let owner_name = if self.staging {
            STAGING_OWNER
        } else if administrative {
            ADMIN_OWNER
        } else {
            OWNER
        };
        let gate_file = root.open_readonly_file(gate_name)?;
        let state = root.open_readonly_file(state_name)?;
        let _ = owner_name;
        let matches = self
            .matches_current_files(&root, &gate_file, &state)
            .is_ok()
            && gate.identity().map_err(|_| invalid(Problem::Changed))? == self.binding.gate;
        Ok(matches)
    }
}

impl PreparedLegacyScope {
    pub(crate) fn artifact_root(&self) -> &Path {
        &self.artifact
    }
    pub(crate) fn database_path(&self) -> &Path {
        &self.database
    }
    pub(crate) fn manifest_path(&self) -> &Path {
        &self.manifest
    }
}
/// Administrative enrollment and source closure. The returned owner retains
/// only the dedicated janitor lifecycle lease; all reader-facing exclusives
/// are dropped before the caller performs association or any payload scan.
fn with_legacy_enrollment_inner<R>(
    prepared: &mut PreparedLegacyScope,
    confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    cohort: Option<&AcceptedLegacyRootCohort>,
    mut automatic: Option<&mut super::maintenance_ownership::PreparedAutomaticDirect>,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    let administrative = prepared.kind == LegacyScopeKind::ManagedAdministration;
    if automatic.is_some() && prepared.kind != LegacyScopeKind::DefaultDirect {
        return Err(invalid(Problem::Changed));
    }
    if prepared.publication_recovery_pending() {
        prepared.recover_publication(None, grant, memory)?;
        prepared.enrollment_waiting = true;
        return Err(invalid(Problem::Busy));
    }
    if let Some(cohort) = cohort {
        let allowed = match prepared.kind {
            LegacyScopeKind::ManagedGeneration => cohort.matches(prepared, memory)?,
            LegacyScopeKind::DefaultDirect => {
                cohort.current(memory)?
                    && cohort.default_source.is_some()
                    && (cohort.default_source == prepared.confirmation_source
                        || prepared.observation.as_ref().is_some_and(|o| {
                            o.observed.record.phase == LegacyPhase::Active
                                && o.observed.record.operator_pending
                                && o.observed.record.root_default_source == cohort.default_source
                                && o.observed.record.operator_generation
                                    == cohort.operator_generation
                        }))
            }
            _ => false,
        };
        if !allowed {
            return Err(invalid(Problem::Changed));
        }
    }
    grant.check()?;
    if grant.remaining_entries() < 48 {
        return Err(FixedControlError::Grant(
            super::maintenance_budget::GrantExhausted::Entries,
        ));
    }
    if !prepared.binding.is_current()? {
        return Err(invalid(Problem::Changed));
    }
    if confirmation.is_some_and(|c| c.binding_digest() != prepared.binding.digest()) {
        return Err(invalid(Problem::Changed));
    }
    if prepared.kind == LegacyScopeKind::DefaultDirect
        && confirmation.is_some_and(|c| c.source() != prepared.confirmation_source.as_ref())
    {
        return Err(invalid(Problem::Changed));
    }

    let floor = memory
        .reserve(
            std::mem::size_of::<LegacySourceOwner>()
                + 24576
                + prepared.root.as_os_str().as_encoded_bytes().len() * 4,
        )
        .map_err(FixedControlError::Resource)?;
    let scratch = &mut prepared.scratch;
    let root_file = checked_entry(&prepared.root, prepared.entries[0], scratch, grant)?;
    let repository_file = checked_entry(&prepared.repository, prepared.entries[1], scratch, grant)?;
    let artifact_file = checked_entry(&prepared.artifact, prepared.entries[2], scratch, grant)?;
    let member_file = checked_entry(&prepared.member, prepared.entries[3], scratch, grant)?;
    let root = RootedDirectory::open_fixed(
        &prepared.root,
        FixedFileStamp::capture(&root_file, Kind::Directory)?.identity(),
        true,
        scratch,
    )?;
    let artifact = RootedDirectory::open_fixed(
        &prepared.artifact,
        FixedFileStamp::capture(&artifact_file, Kind::Directory)?.identity(),
        true,
        scratch,
    )?;
    let member = RootedDirectory::open_fixed(
        &prepared.member,
        FixedFileStamp::capture(&member_file, Kind::Directory)?.identity(),
        true,
        scratch,
    )?;
    // Missing administrative controls require the actual captured human request.
    let gate_entry = Entry::capture(&prepared.paths[0], Kind::File, true, scratch)?;
    let state_entry = Entry::capture(&prepared.paths[1], Kind::File, true, scratch)?;
    let owner_entry = Entry::capture(&prepared.paths[2], Kind::File, true, scratch)?;
    if (gate_entry.stamp.is_none() || state_entry.stamp.is_none() || owner_entry.stamp.is_none())
        && confirmation.is_none()
        && cohort.is_none()
        && automatic.is_none()
    {
        return Err(invalid(Problem::InvalidRecord));
    }
    let (writer, _) = lock_entry(&prepared.writer, prepared.entries[6], &root, scratch, grant)?;
    // The actual writer excludes pointer publication. Compare the prepared
    // noncurrent selection before accepting even a pending source enrollment.
    if let Some((path, entry)) = &prepared.administration_selector {
        drop(checked_entry(path, *entry, scratch, grant)?);
    }
    if let Some((path, entry)) = &prepared.enrollment_runs {
        grant.take_entry()?;
        match (
            locks::open_fixed_control(path, Kind::Directory, false, scratch),
            entry.stamp,
        ) {
            (Ok(file), Some(stamp)) => stamp.recheck(&file)?,
            (Err(FixedControlError::Io(e)), None) if e.kind() == std::io::ErrorKind::NotFound => {}
            (Err(error), _) => return Err(error),
            _ => return Err(invalid(Problem::Changed)),
        }
    }
    let (gate, new_gate) = lock_entry(&prepared.paths[0], gate_entry, &root, scratch, grant)?;
    if new_gate {
        init_coordination(gate.file(), &prepared.coordination[0], grant)?;
    }
    let (owner, new_owner) = lock_entry(&prepared.paths[2], owner_entry, &root, scratch, grant)?;
    if new_owner {
        init_owner_coordination(owner.file(), &prepared.coordination[1], grant)?;
    }
    if raw_coordination(gate.file())? != prepared.coordination[0]
        || raw_coordination(owner.file())? != prepared.coordination[1]
    {
        return Err(invalid(Problem::Changed));
    }
    let gate_nonce = prepared.nonce;
    let owner_nonce = prepared.owner_nonce;
    let mut early_guards = if let Some(capability) = automatic.as_mut() {
        let (family, _) = lock_entry(
            &prepared.family,
            prepared.entries[7],
            &member,
            scratch,
            grant,
        )?;
        let (native, _) = lock_entry(
            &prepared.native,
            prepared.entries[8],
            &member,
            scratch,
            grant,
        )?;
        let (root_use, _) = lock_entry(
            &prepared.root_use,
            prepared.entries[9],
            &artifact,
            scratch,
            grant,
        )?;
        capability.verify(&root, &artifact, &writer, &root_use, grant)?;
        Some((family, native, root_use))
    } else {
        None
    };
    grant.take_entry()?;
    let (state, new_state) = if let Some(stamp) = state_entry.stamp {
        let file = locks::open_fixed_control(&prepared.paths[1], Kind::File, true, scratch)?;
        stamp.recheck(&file)?;
        (file, false)
    } else {
        (
            root.create_new(if administrative { ADMIN_STATE } else { STATE })?,
            true,
        )
    };
    let mut binding = Binding {
        kind: prepared.kind,
        root: root.identity(),
        gate: FixedFileStamp::capture(gate.file(), Kind::File)?.identity(),
        state: FixedFileStamp::capture(&state, Kind::File)?.identity(),
        owner: FixedFileStamp::capture(owner.file(), Kind::File)?.identity(),
        nonce: gate_nonce,
        owner_nonce,
        key: prepared.key,
        database: [0; 256],
        database_len: 0,
        cache: [0; 256],
        cache_len: 0,
        repository: FixedFileStamp::capture(&repository_file, Kind::Directory)?.identity(),
    };
    let database_name = prepared
        .database
        .file_name()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?
        .as_encoded_bytes();
    let cache_name = prepared
        .artifact
        .file_name()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?
        .as_encoded_bytes();
    if !component(database_name) || !component(cache_name) {
        return Err(invalid(Problem::InvalidRecord));
    }
    binding.database[..database_name.len()].copy_from_slice(database_name);
    binding.database_len = database_name.len();
    binding.cache[..cache_name.len()].copy_from_slice(cache_name);
    binding.cache_len = cache_name.len();
    let mut record = Record {
        phase: LegacyPhase::Active,
        version: 1,
        member: member.identity(),
        database: prepared.entries[4].stamp,
        manifest: prepared.entries[5].stamp,
        cache: artifact.identity(),
        root_use: prepared.entries[9]
            .stamp
            .map(|s| s.identity())
            .unwrap_or(FileIdentity::from_fixed_words(0, 0)),
        confirmation: if administrative {
            prepared.cohort_scope
        } else {
            prepared.binding.digest()
        },
        job: prepared.job,
        member_name: prepared.member_name,
        member_len: prepared.member_len,
        freeze_cache: prepared.freeze_cache && prepared.kind == LegacyScopeKind::DefaultDirect,
        operator_generation: confirmation
            .map(|c| c.generation())
            .unwrap_or_else(|| cohort.map(|c| c.operator_generation).unwrap_or(0)),
        operator_pending: true,
        pending_source_stamp: prepared.entries[2].stamp,
        root_default_source: if administrative || prepared.kind == LegacyScopeKind::DefaultDirect {
            confirmation
                .and_then(|c| c.source().copied())
                .or_else(|| cohort.and_then(|c| c.default_source))
        } else {
            None
        },
    };
    // A rejected queued request must never leave an accepted permission for a
    // substituted source. Repeat all request-time source facts before birth;
    // the initial Active record claims permission, never quiescence.
    if prepared.kind == LegacyScopeKind::DefaultDirect && confirmation.is_some() {
        let file = checked_entry(&prepared.database, prepared.entries[4], scratch, grant)?;
        drop(file);
        let file = checked_entry(&prepared.manifest, prepared.entries[5], scratch, grant)?;
        drop(file);
        for (path, entry) in &prepared.confirmation_caches {
            grant.take_entry()?;
            match (
                locks::open_fixed_control(path, Kind::Directory, false, scratch),
                entry.stamp,
            ) {
                (Ok(file), Some(stamp))
                    if FixedFileStamp::capture(&file, Kind::Directory)?.identity()
                        == stamp.identity() => {}
                (Err(FixedControlError::Io(e)), None)
                    if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(invalid(Problem::Changed)),
            }
        }
    }
    if new_state
        && prepared.kind == LegacyScopeKind::DefaultDirect
        && confirmation.is_some()
        && prepared
            .confirmation_source
            .as_ref()
            .is_none_or(|source| source.version.is_some())
    {
        return Err(invalid(Problem::Changed));
    }
    let mut commit = if new_state {
        if prepared.entries[4].stamp.is_none() || prepared.entries[5].stamp.is_none() {
            return Err(invalid(Problem::InvalidRecord));
        }
        if confirmation.is_none() && cohort.is_none() && automatic.is_none() {
            return Err(invalid(Problem::InvalidRecord));
        }
        let birth = prepared
            .birth
            .as_mut()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        if let Some(confirmation) = confirmation {
            if birth.capsule.mode != 1
                || birth.capsule.permission.operator_generation != confirmation.generation()
                || birth.capsule.permission.root_default_source != confirmation.source().copied()
                    && matches!(
                        prepared.kind,
                        LegacyScopeKind::ManagedAdministration | LegacyScopeKind::DefaultDirect
                    )
            {
                return Err(invalid(Problem::Changed));
            }
        } else if automatic.is_some() {
            if birth.capsule.mode != 3 {
                return Err(invalid(Problem::Changed));
            }
        } else if cohort.is_some() {
            if birth.capsule.mode != 2 {
                return Err(invalid(Problem::Changed));
            }
        } else {
            return Err(invalid(Problem::InvalidRecord));
        }
        birth.publish_created(&root, &state, binding, record, grant)?;
        return Err(invalid(Problem::Busy));
    } else if prepared.observation.is_none() {
        let birth = prepared
            .birth
            .as_mut()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        match birth.capsule.mode {
            1 => {
                if birth.capsule.permission.confirmation
                    != if administrative {
                        prepared.cohort_scope
                    } else {
                        prepared.binding.digest()
                    }
                {
                    return Err(invalid(Problem::Changed));
                }
            }
            2 => {
                if cohort.is_none() {
                    return Err(invalid(Problem::InvalidRecord));
                }
            }
            3 => {
                if automatic.is_none() {
                    return Err(invalid(Problem::InvalidRecord));
                }
            }
            _ => return Err(invalid(Problem::InvalidRecord)),
        }
        birth.promote(&root, &state, gate.file(), owner.file(), grant)?;
        return Err(invalid(Problem::Busy));
    } else {
        let observed = prepared
            .observation
            .as_ref()
            .ok_or_else(|| invalid(Problem::Changed))?
            .verify(&root, gate.file(), owner.file(), &state)?;
        if observed.binding != binding {
            return Err(invalid(Problem::Changed));
        }
        if prepared.kind == LegacyScopeKind::DefaultDirect && confirmation.is_some() {
            let actual = super::maintenance_admission::legacy_confirmation_version_from_records(
                &observed.binding,
                &observed.record,
            );
            if prepared
                .confirmation_source
                .as_ref()
                .and_then(|source| source.version)
                != Some(actual)
            {
                return Err(invalid(Problem::Changed));
            }
        }
        if prepared.kind == LegacyScopeKind::DefaultDirect
            && cohort.is_some()
            && observed.record.phase == LegacyPhase::Active
            && !observed.record.operator_pending
        {
            return Err(invalid(Problem::Changed));
        }
        if automatic.is_some()
            && observed.record.phase == LegacyPhase::Active
            && !observed.record.operator_pending
        {
            return Err(invalid(Problem::Changed));
        }
        binding = observed.binding;
        record = observed.record;
        // Active permits upgraded producers. Reclosure captures their current
        // source under all real exclusions; a closed version remains immutable.
        if !(administrative || cohort.is_some() && record.phase == LegacyPhase::Removed)
            && record.phase != LegacyPhase::Active
            && (record.cache != artifact.identity() || record.member != member.identity())
        {
            return Err(invalid(Problem::Changed));
        }
        if !(administrative || cohort.is_some() && record.phase == LegacyPhase::Removed)
            && record.phase != LegacyPhase::Active
            && record.phase != LegacyPhase::Retiring
            && (record.cache != artifact.identity()
                || record.member != member.identity()
                || record.database.map(|s| s.identity())
                    != prepared.entries[4].stamp.map(|s| s.identity())
                || record.manifest.map(|s| s.identity())
                    != prepared.entries[5].stamp.map(|s| s.identity()))
        {
            return Err(invalid(Problem::Changed));
        }
        observed.commit
    };
    if !administrative
        && cohort.is_none()
        && record.phase == LegacyPhase::Active
        && record.operator_pending
        && confirmation.is_none()
        && (record.database != prepared.entries[4].stamp
            || record.manifest != prepared.entries[5].stamp
            || record.cache != artifact.identity()
            || record.member != member.identity()
            || record.pending_source_stamp != prepared.entries[2].stamp)
    {
        return Err(invalid(Problem::Changed));
    }
    let (family, native, root_use) = if let Some(guards) = early_guards.take() {
        guards
    } else {
        let (family, _) = lock_entry(
            &prepared.family,
            prepared.entries[7],
            &member,
            scratch,
            grant,
        )?;
        let (native, _) = lock_entry(
            &prepared.native,
            prepared.entries[8],
            &member,
            scratch,
            grant,
        )?;
        let (root_use, _) = lock_entry(
            &prepared.root_use,
            prepared.entries[9],
            &artifact,
            scratch,
            grant,
        )?;
        (family, native, root_use)
    };
    if let Some((path, entry)) = &prepared.administration_selector {
        let file = checked_entry(path, *entry, scratch, grant)?;
        drop(file);
    }
    let observe_payload = |path: &Path,
                           entry: Entry,
                           scratch: &mut FixedControlScratch,
                           grant: &mut MutationGrant|
     -> Result<Option<FixedFileStamp>, FixedControlError> {
        if record.phase == LegacyPhase::Retiring {
            return Ok(None);
        }
        let file = checked_entry(path, entry, scratch, grant)?;
        Ok(Some(FixedFileStamp::capture(&file, Kind::File)?))
    };
    let database_stamp = observe_payload(&prepared.database, prepared.entries[4], scratch, grant)?;
    let manifest_stamp = observe_payload(&prepared.manifest, prepared.entries[5], scratch, grant)?;
    if prepared.kind == LegacyScopeKind::DefaultDirect && record.phase == LegacyPhase::Active {
        if let Some(confirmation) = confirmation {
            if confirmation.source() != prepared.confirmation_source.as_ref() {
                return Err(invalid(Problem::Changed));
            }
            for (path, entry) in &prepared.confirmation_caches {
                grant.take_entry()?;
                match (
                    locks::open_fixed_control(path, Kind::Directory, false, scratch),
                    entry.stamp,
                ) {
                    (Ok(file), Some(stamp))
                        if FixedFileStamp::capture(&file, Kind::Directory)?.identity()
                            == stamp.identity() => {}
                    (Err(FixedControlError::Io(e)), None)
                        if e.kind() == std::io::ErrorKind::NotFound => {}
                    _ => return Err(invalid(Problem::Changed)),
                }
            }
        }
    }
    if administrative && record.confirmation != prepared.cohort_scope {
        return Err(invalid(Problem::Changed));
    }
    if record.phase == LegacyPhase::Active
        || (cohort.is_some() && record.phase == LegacyPhase::Removed)
    {
        if confirmation.is_none()
            && cohort.is_none()
            && automatic.is_none()
            && (!record.operator_pending
                || record.confirmation
                    != if administrative {
                        prepared.cohort_scope
                    } else {
                        prepared.binding.digest()
                    })
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        if let Some(confirmation) = confirmation {
            record.operator_generation = confirmation.generation();
            record.confirmation = if administrative {
                prepared.cohort_scope
            } else {
                prepared.binding.digest()
            };
        }
        record.operator_pending = false;
        record.pending_source_stamp = None;
        record.phase = LegacyPhase::Preparing;
        record.version = commit
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        record.member = member.identity();
        record.member_name = prepared.member_name;
        record.member_len = prepared.member_len;
        record.cache = artifact.identity();
        record.database = database_stamp;
        record.manifest = manifest_stamp;
        record.root_use = FixedFileStamp::capture(root_use.file(), Kind::File)?.identity();
        record.job = prepared.job;
        let Some(publication) = prepared.publication.as_ref() else {
            prepared.enrollment_waiting = true;
            return Err(invalid(Problem::Busy));
        };
        commit = publication.publish(&state, binding, commit, record, grant)?;
    } else if !matches!(record.phase, LegacyPhase::Preparing | LegacyPhase::Retiring) {
        return Err(invalid(Problem::Busy));
    }
    if root_use.file().metadata()?.len() == 0 {
        init_coordination(root_use.file(), &prepared.coordination[2], grant)?;
    }
    if raw_coordination(root_use.file())? != prepared.coordination[2] {
        return Err(invalid(Problem::Changed));
    }
    let incarnation = prepared.coordination[2][8..40].try_into().unwrap();
    let authority = LegacyEnrollmentAuthority {
        kind: prepared.kind,
        directory: &root,
        artifact: &artifact,
        member: &member,
        writer: &writer,
        root_use: &root_use,
        _family: &family,
        _native: &native,
        _gate: &gate,
        _confirmation: confirmation,
        incarnation,
    };
    let result = apply(&authority, grant)?;
    // The callback can initialize own control headers, never change source bytes.
    if !administrative && record.phase == LegacyPhase::Preparing {
        let file = checked_entry(&prepared.database, prepared.entries[4], scratch, grant)?;
        record.database.unwrap().recheck(&file)?;
        drop(file);
        let file = checked_entry(&prepared.manifest, prepared.entries[5], scratch, grant)?;
        record.manifest.unwrap().recheck(&file)?;
    }
    // Remember only coordination files genuinely created/held by this admitted
    // enrollment, including controlled header initialization by the callback.
    for (index, file) in [
        (6, writer.file()),
        (7, family.file()),
        (8, native.file()),
        (9, root_use.file()),
    ] {
        prepared.entries[index].stamp = Some(FixedFileStamp::capture(file, Kind::File)?);
    }
    let _ = commit;
    let owner = LegacySourceOwner {
        state_stamp: FixedFileStamp::capture(&state, Kind::File)?,
        state_pin: state.try_clone()?,
        gate_pin: gate.file().try_clone()?,
        gate_header: raw_coordination(gate.file())?,
        artifact_header: prepared.coordination[2],
        owner_header: raw_coordination(owner.file())?,
        commit,
        owner,
        staging: false,
        pending_publication: false,
        _root_pin: root.pin_handle()?,
        binding,
        record,
        state_path: prepared.paths[1].clone(),
        gate_path: prepared.paths[0].clone(),
        _floor: floor,
    };
    Ok((result, owner))
}

/// Reacquire the actual source exclusions for a short effect. The dedicated
/// owner lease may live across preparation; reader-facing exclusions never do.
pub(crate) fn with_closed_source<R>(
    prepared: &mut PreparedLegacyScope,
    owner: &LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<R, FixedControlError> {
    grant.check()?;
    let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
    if prepared.kind == LegacyScopeKind::ManagedAdministration
        || !prepared.binding.is_current()?
        || !owner.is_current(memory)?
        || owner.binding.root != prepared.entries[0].stamp.unwrap().identity()
        || owner.pending_publication
        || owner.record.phase == LegacyPhase::Active
    {
        return Err(invalid(Problem::Changed));
    }
    let scratch = &mut prepared.scratch;
    let root_file = checked_entry(&prepared.root, prepared.entries[0], scratch, grant)?;
    let artifact_file = checked_entry(&prepared.artifact, prepared.entries[2], scratch, grant)?;
    let member_file = checked_entry(&prepared.member, prepared.entries[3], scratch, grant)?;
    let root = RootedDirectory::open(
        &prepared.root,
        FixedFileStamp::capture(&root_file, Kind::Directory)?.identity(),
    )?;
    let artifact = RootedDirectory::open(
        &prepared.artifact,
        FixedFileStamp::capture(&artifact_file, Kind::Directory)?.identity(),
    )?;
    let member = RootedDirectory::open(
        &prepared.member,
        FixedFileStamp::capture(&member_file, Kind::Directory)?.identity(),
    )?;
    let (writer, _) = lock_entry(&prepared.writer, prepared.entries[6], &root, scratch, grant)?;
    let gate_file = locks::open_fixed_control(&prepared.paths[0], Kind::File, true, scratch)?;
    let gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    let state = locks::open_fixed_control(&prepared.paths[1], Kind::File, true, scratch)?;
    owner.matches_current_files(&root, gate.file(), &state)?;
    let (family, _) = lock_entry(
        &prepared.family,
        prepared.entries[7],
        &member,
        scratch,
        grant,
    )?;
    let (native, _) = lock_entry(
        &prepared.native,
        prepared.entries[8],
        &member,
        scratch,
        grant,
    )?;
    let (root_use, _) = lock_entry(
        &prepared.root_use,
        prepared.entries[9],
        &artifact,
        scratch,
        grant,
    )?;
    if FixedFileStamp::capture(root_use.file(), Kind::File)?.identity() != owner.record.root_use {
        return Err(invalid(Problem::Changed));
    }
    if raw_coordination(root_use.file())? != owner.artifact_header {
        return Err(invalid(Problem::Changed));
    }
    if owner.record.phase == LegacyPhase::Preparing {
        let db = checked_entry(&prepared.database, prepared.entries[4], scratch, grant)?;
        owner
            .record
            .database
            .ok_or_else(|| invalid(Problem::Changed))?
            .recheck(&db)?;
        drop(db);
        let manifest = checked_entry(&prepared.manifest, prepared.entries[5], scratch, grant)?;
        owner
            .record
            .manifest
            .ok_or_else(|| invalid(Problem::Changed))?
            .recheck(&manifest)?;
    }
    let authority = LegacyEnrollmentAuthority {
        kind: prepared.kind,
        directory: &root,
        artifact: &artifact,
        member: &member,
        writer: &writer,
        root_use: &root_use,
        _family: &family,
        _native: &native,
        _gate: &gate,
        _confirmation: None,
        incarnation: owner.artifact_header[8..40].try_into().unwrap(),
    };
    let result = apply(&authority, grant)?;
    grant.check()?;
    Ok(result)
}

/// Abort is explicit, fallible and admitted. No filesystem mutation occurs in
/// Drop, and a stale worker cannot reopen a newer source version.
fn transition_source(
    prepared: &mut PreparedLegacyScope,
    owner: &mut LegacySourceOwner,
    next: LegacyPhase,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    let mut updated = owner.record;
    updated.version = updated
        .version
        .checked_add(1)
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    updated.phase = next;
    updated.operator_pending = false;
    updated.pending_source_stamp = None;
    let publication = prepared
        .publication
        .take()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let result = with_closed_source(prepared, owner, grant, memory, &mut |authority, grant| {
        let state = authority.directory.open_existing(STATE)?;
        publication.publish(&state, owner.binding, owner.commit, updated, grant)
    });
    prepared.publication = Some(publication);
    owner.commit = result?;
    owner.record = updated;
    owner.state_stamp = FixedFileStamp::capture(&owner.state_pin, Kind::File)?;
    Ok(())
}
/// Matching producer facts may advance a genuinely closed source into effects.
/// Actual destination admission/reference exclusion remains with the consumer.
pub(crate) fn begin_legacy_retirement(
    prepared: &mut PreparedLegacyScope,
    owner: &mut LegacySourceOwner,
    association: &super::maintenance_legacy_association::LegacyAssociation,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if owner.is_administrative()
        || owner.record.phase != LegacyPhase::Preparing
        || owner.record.database != Some(association.database_stamp())
        || owner.record.manifest != Some(association.manifest_stamp())
        || owner.record.cache != association.artifact_root_identity()
    {
        return Err(invalid(Problem::Changed));
    }
    transition_source(prepared, owner, LegacyPhase::Retiring, grant, memory)
}

/// Orphan staging is an independent source scope; absent DB/manifest fields are
/// represented explicitly and cannot satisfy the pair enrollment interface.
pub(crate) struct PreparedLegacyStagingScope {
    root: PathBuf,
    staging: PathBuf,
    paths: [PathBuf; 3],
    root_stamp: FixedFileStamp,
    staging_stamp: FixedFileStamp,
    cohort_scope: [u8; 32],
    repository: FileIdentity,
    binding: super::maintenance_budget::MemoryBox<PreparedTargetBinding>,
    nonce: [u8; 32],
    owner_nonce: [u8; 32],
    coordination: [[u8; locks::COORDINATION_HEADER_BYTES]; 3],
    enrollment_waiting: bool,
    birth: Option<super::maintenance_budget::MemoryBox<PreparedSourceBirth>>,
    observation: Option<super::maintenance_budget::MemoryBox<PreparedSourceObservation>>,
    publication: Option<
        super::maintenance_budget::MemoryBox<
            super::maintenance_legacy_guard::PreparedSourcePublication,
        >,
    >,
    job: [u8; 32],
    scratch: super::maintenance_budget::MemoryBox<FixedControlScratch>,
    _floor: MemoryReservation,
}
pub(crate) struct LegacyStagingEnrollmentAuthority<'a> {
    root: &'a RootedDirectory,
    staging: &'a RootedDirectory,
    _gate: &'a FixedLockedFile,
    version: LegacyClosedVersion,
}
impl LegacyStagingEnrollmentAuthority<'_> {
    pub(crate) fn staging_directory(&self) -> &RootedDirectory {
        self.staging
    }
    pub(crate) fn closed_version(&self) -> LegacyClosedVersion {
        self.version
    }
}
// Preserve standalone value callers; joined-worker staging uses the charged
// core directly so no raw scope Result/value is copied above source observation.
#[allow(dead_code)]
pub(crate) fn prepare_legacy_staging_scope(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    staging_root: &Path,
    memory: &CleanupMemory,
) -> Result<PreparedLegacyStagingScope, FixedControlError> {
    prepare_legacy_staging_scope_boxed(target, binding, staging_root, memory)
        .map(super::maintenance_budget::MemoryBox::into_inner)
}

#[inline(never)]
fn staging_binding_boxed(
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedTargetBinding>, FixedControlError> {
    let binding = binding.duplicate(memory)?;
    super::maintenance_budget::MemoryBox::new(binding, memory).map_err(FixedControlError::Resource)
}

/// Identical independent staging-source preparation behind its charged owner;
/// parent/admin observations supply no origin or payload deletion authority.
#[inline(never)]
pub(crate) fn prepare_legacy_staging_scope_boxed(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    staging_root: &Path,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyStagingScope>, FixedControlError> {
    let charge = std::mem::size_of::<PreparedLegacyStagingScope>()
        .checked_add(32768)
        .and_then(|v| {
            v.checked_add(
                target
                    .state_root
                    .as_os_str()
                    .as_encoded_bytes()
                    .len()
                    .checked_mul(8)?,
            )
        })
        .ok_or(FixedControlError::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))?;
    let floor = memory
        .reserve(charge)
        .map_err(FixedControlError::Resource)?;
    if target.state_root != target.repository_root.join(".codebaseGraph")
        || staging_root != target.state_root.join("native-staging")
    {
        return Err(invalid(Problem::InvalidRecord));
    }
    let mut scratch = source_observation_scratch_boxed(memory)?;
    let root = locks::open_fixed_control(&target.state_root, Kind::Directory, false, &mut scratch)?;
    let staging = locks::open_fixed_control(staging_root, Kind::Directory, false, &mut scratch)?;
    let repository = locks::open_fixed_control(
        &target.repository_root,
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let mut nonce = [0; 32];
    let mut owner_nonce = [0; 32];
    let mut job = [0; 32];
    for bytes in [&mut nonce, &mut owner_nonce, &mut job] {
        getrandom::fill(bytes).map_err(|_| invalid(Problem::InvalidRecord))?;
    }
    let configured = super::maintenance_projection::project_binding(target, memory)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let cohort_scope = legacy_cohort_scope(
        target,
        &configured.storage,
        &configured.storage.join("artifacts"),
        memory,
    )?;
    let paths = [
        target
            .state_root
            .join(std::str::from_utf8(STAGING_GATE).unwrap()),
        target
            .state_root
            .join(std::str::from_utf8(STAGING_STATE).unwrap()),
        target
            .state_root
            .join(std::str::from_utf8(STAGING_OWNER).unwrap()),
    ];
    nonce = existing_coordination_nonce(&paths[0])?.unwrap_or(nonce);
    owner_nonce = existing_coordination_nonce(&paths[2])?.unwrap_or(owner_nonce);
    let coordination = [
        locks::encode_coordination_incarnation(nonce),
        locks::encode_coordination_incarnation(owner_nonce),
        locks::encode_coordination_incarnation(job),
    ];
    let observation = PreparedSourceObservation::read(&target.state_root, &paths, None, memory)?;
    super::maintenance_budget::MemoryBox::new(
        PreparedLegacyStagingScope {
            coordination,
            publication: None,
            cohort_scope,
            root: target.state_root.clone(),
            staging: staging_root.to_owned(),
            enrollment_waiting: false,
            birth: None,
            observation,
            paths,
            root_stamp: FixedFileStamp::capture(&root, Kind::Directory)?,
            staging_stamp: FixedFileStamp::capture(&staging, Kind::Directory)?,
            repository: FixedFileStamp::capture(&repository, Kind::Directory)?.identity(),
            binding: staging_binding_boxed(binding, memory)?,
            nonce,
            owner_nonce,
            job,
            scratch,
            _floor: floor,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}
fn with_legacy_staging_enrollment_inner<R>(
    prepared: &mut PreparedLegacyStagingScope,
    confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    cohort: Option<&AcceptedLegacyRootCohort>,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyStagingEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    if prepared.publication_recovery_pending() {
        prepared.recover_publication(None, grant, memory)?;
        prepared.enrollment_waiting = true;
        return Err(invalid(Problem::Busy));
    }
    if let Some(cohort) = cohort {
        if prepared.cohort_scope != cohort.scope || !cohort.current(memory)? {
            return Err(invalid(Problem::Changed));
        }
    }
    grant.check()?;
    if !prepared.binding.is_current()?
        || confirmation.is_some_and(|c| c.binding_digest() != prepared.binding.digest())
    {
        return Err(invalid(Problem::Changed));
    }
    let floor = memory
        .reserve(
            std::mem::size_of::<LegacySourceOwner>()
                + 24576
                + prepared.root.as_os_str().as_encoded_bytes().len() * 4,
        )
        .map_err(FixedControlError::Resource)?;
    let root = RootedDirectory::open_fixed(
        &prepared.root,
        prepared.root_stamp.identity(),
        true,
        &mut prepared.scratch,
    )?;
    let staging = RootedDirectory::open_fixed(
        &prepared.staging,
        prepared.staging_stamp.identity(),
        true,
        &mut prepared.scratch,
    )?;
    let gate_entry = Entry::capture(&prepared.paths[0], Kind::File, true, &mut prepared.scratch)?;
    let state_entry = Entry::capture(&prepared.paths[1], Kind::File, true, &mut prepared.scratch)?;
    let owner_entry = Entry::capture(&prepared.paths[2], Kind::File, true, &mut prepared.scratch)?;
    if (state_entry.stamp.is_none() || owner_entry.stamp.is_none())
        && confirmation.is_none()
        && cohort.is_none()
    {
        return Err(invalid(Problem::InvalidRecord));
    }
    let (gate, created) = lock_entry(
        &prepared.paths[0],
        gate_entry,
        &root,
        &mut prepared.scratch,
        grant,
    )?;
    if created {
        init_coordination(gate.file(), &prepared.coordination[0], grant)?;
    }
    if raw_coordination(gate.file())? != prepared.coordination[0] {
        return Err(invalid(Problem::Changed));
    }
    let nonce = prepared.nonce;
    let (owner, created) = lock_entry(
        &prepared.paths[2],
        owner_entry,
        &root,
        &mut prepared.scratch,
        grant,
    )?;
    if created {
        init_owner_coordination(owner.file(), &prepared.coordination[1], grant)?;
    }
    if raw_coordination(owner.file())? != prepared.coordination[1] {
        return Err(invalid(Problem::Changed));
    }
    let owner_nonce = prepared.owner_nonce;
    grant.take_entry()?;
    let (state, created) = match state_entry.stamp {
        Some(stamp) => {
            let file = locks::open_fixed_control(
                &prepared.paths[1],
                Kind::File,
                true,
                &mut prepared.scratch,
            )?;
            stamp.recheck(&file)?;
            (file, false)
        }
        None => (root.create_new(STAGING_STATE)?, true),
    };
    let mut cache = [0; 256];
    cache[..14].copy_from_slice(b"native-staging");
    let expected = Binding {
        kind: LegacyScopeKind::NativeStaging,
        root: root.identity(),
        gate: FixedFileStamp::capture(gate.file(), Kind::File)?.identity(),
        state: FixedFileStamp::capture(&state, Kind::File)?.identity(),
        owner: FixedFileStamp::capture(owner.file(), Kind::File)?.identity(),
        nonce,
        owner_nonce,
        key: [0; 64],
        database: [0; 256],
        database_len: 0,
        cache,
        cache_len: 14,
        repository: prepared.repository,
    };
    let (binding, mut record, mut commit) = if created {
        if confirmation.is_none() && cohort.is_none() {
            return Err(invalid(Problem::InvalidRecord));
        }
        let record = Record {
            phase: LegacyPhase::Active,
            version: 1,
            member: staging.identity(),
            database: None,
            manifest: None,
            cache: staging.identity(),
            root_use: expected.gate,
            confirmation: prepared.binding.digest(),
            job: prepared.job,
            member_name: [0; 128],
            member_len: 0,
            freeze_cache: false,
            operator_generation: confirmation
                .map(|c| c.generation())
                .unwrap_or_else(|| cohort.map(|c| c.operator_generation).unwrap_or(0)),
            operator_pending: true,
            pending_source_stamp: Some(prepared.staging_stamp),
            root_default_source: None,
        };
        let birth = prepared
            .birth
            .as_mut()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        if let Some(confirmation) = confirmation {
            if birth.capsule.mode != 1
                || birth.capsule.permission.operator_generation != confirmation.generation()
            {
                return Err(invalid(Problem::Changed));
            }
        } else if cohort.is_none() || birth.capsule.mode != 2 {
            return Err(invalid(Problem::InvalidRecord));
        }
        birth.publish_created(&root, &state, expected, record, grant)?;
        return Err(invalid(Problem::Busy));
    } else if prepared.observation.is_none() {
        let birth = prepared
            .birth
            .as_mut()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        if birth.capsule.mode == 1 {
            if birth.capsule.permission.confirmation != prepared.binding.digest() {
                return Err(invalid(Problem::Changed));
            }
        } else if birth.capsule.mode != 2 || cohort.is_none() {
            return Err(invalid(Problem::InvalidRecord));
        }
        birth.promote(&root, &state, gate.file(), owner.file(), grant)?;
        return Err(invalid(Problem::Busy));
    } else {
        let o = prepared
            .observation
            .as_ref()
            .ok_or_else(|| invalid(Problem::Changed))?
            .verify(&root, gate.file(), owner.file(), &state)?;
        if o.binding != expected
            || o.record.member != staging.identity()
            || o.record.database.is_some()
            || o.record.manifest.is_some()
        {
            return Err(invalid(Problem::Changed));
        }
        (o.binding, o.record, o.commit)
    };
    if record.phase == LegacyPhase::Active
        && record.operator_pending
        && confirmation.is_none()
        && record.pending_source_stamp != Some(prepared.staging_stamp)
    {
        return Err(invalid(Problem::Changed));
    }
    if record.phase == LegacyPhase::Active {
        if confirmation.is_none()
            && cohort.is_none()
            && (!record.operator_pending || record.confirmation != prepared.binding.digest())
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        if let Some(confirmation) = confirmation {
            record.operator_generation = confirmation.generation();
            record.confirmation = prepared.binding.digest();
        }
        record.operator_pending = false;
        record.pending_source_stamp = None;
        record.phase = LegacyPhase::Preparing;
        record.version = record
            .version
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        record.job = prepared.job;
        let Some(publication) = prepared.publication.as_ref() else {
            prepared.enrollment_waiting = true;
            return Err(invalid(Problem::Busy));
        };
        commit = publication.publish(&state, binding, commit, record, grant)?;
    } else if record.phase != LegacyPhase::Preparing {
        return Err(invalid(Problem::Busy));
    }
    let owner = LegacySourceOwner {
        state_stamp: FixedFileStamp::capture(&state, Kind::File)?,
        state_pin: state.try_clone()?,
        gate_pin: gate.file().try_clone()?,
        gate_header: raw_coordination(gate.file())?,
        artifact_header: prepared.coordination[2],
        owner_header: raw_coordination(owner.file())?,
        commit,
        owner,
        staging: true,
        pending_publication: false,
        _root_pin: root.pin_handle()?,
        binding,
        record,
        state_path: prepared.paths[1].clone(),
        gate_path: prepared.paths[0].clone(),
        _floor: floor,
    };
    let authority = LegacyStagingEnrollmentAuthority {
        root: &root,
        staging: &staging,
        _gate: &gate,
        version: owner.closed_version(),
    };
    let result = apply(&authority, grant)?;
    let _ = commit;
    Ok((result, owner))
}

pub(crate) fn with_closed_staging_source<R>(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyStagingEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<R, FixedControlError> {
    grant.check()?;
    let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
    if !owner.staging
        || owner.pending_publication
        || owner.record.phase == LegacyPhase::Active
        || !prepared.binding.is_current()?
        || !owner.is_current(memory)?
    {
        return Err(invalid(Problem::Changed));
    }
    let root = RootedDirectory::open_fixed(
        &prepared.root,
        prepared.root_stamp.identity(),
        true,
        &mut prepared.scratch,
    )?;
    let staging = RootedDirectory::open_fixed(
        &prepared.staging,
        prepared.staging_stamp.identity(),
        true,
        &mut prepared.scratch,
    )?;
    grant.take_entry()?;
    let gate_file =
        locks::open_fixed_control(&prepared.paths[0], Kind::File, true, &mut prepared.scratch)?;
    let gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    let state = root.open_existing(STAGING_STATE)?;
    owner.matches_current_files(&root, gate.file(), &state)?;
    apply(
        &LegacyStagingEnrollmentAuthority {
            root: &root,
            staging: &staging,
            _gate: &gate,
            version: owner.closed_version(),
        },
        grant,
    )
}
#[cfg(test)]
pub(crate) fn abort_legacy_staging_preparation(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &mut LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if owner.record.phase != LegacyPhase::Preparing {
        return Err(invalid(Problem::InvalidRecord));
    }
    transition_staging_source(prepared, owner, LegacyPhase::Active, grant, memory)
}
fn transition_staging_source(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &mut LegacySourceOwner,
    next: LegacyPhase,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    let mut updated = owner.record;
    updated.phase = next;
    updated.operator_pending = false;
    updated.pending_source_stamp = None;
    updated.version = updated
        .version
        .checked_add(1)
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let publication = prepared
        .publication
        .take()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let result =
        with_closed_staging_source(prepared, owner, grant, memory, &mut |authority, grant| {
            let state = authority.root.open_existing(STAGING_STATE)?;
            publication.publish(&state, owner.binding, owner.commit, updated, grant)
        });
    prepared.publication = Some(publication);
    owner.commit = result?;
    owner.record = updated;
    owner.state_stamp = FixedFileStamp::capture(&owner.state_pin, Kind::File)?;
    Ok(())
}
pub(crate) fn begin_legacy_staging_retirement(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &mut LegacySourceOwner,
    facts: &super::maintenance_legacy_staging::StagingAssociation,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if owner.record.phase != LegacyPhase::Preparing
        || facts.root_identity() != owner.record.member
        || facts.repository_identity() != owner.binding.repository
        || facts.source_context() != owner.closed_version().context_digest()
    {
        return Err(invalid(Problem::Changed));
    }
    transition_staging_source(prepared, owner, LegacyPhase::Retiring, grant, memory)
}

/// Resumption reacquires a real dedicated owner lease; serialized versions
/// alone remain facts and never become authority. No source version is relabeled.
fn reopen_owner(
    root_path: &Path,
    paths: &[PathBuf; 3],
    expected: &LegacyClosedVersion,
    staging: bool,
    memory: &CleanupMemory,
) -> Result<LegacySourceOwner, FixedControlError> {
    let floor = memory
        .reserve(
            std::mem::size_of::<LegacySourceOwner>()
                + 24576
                + root_path.as_os_str().as_encoded_bytes().len() * 4,
        )
        .map_err(FixedControlError::Resource)?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    let root = RootedDirectory::open_fixed(root_path, expected.root, false, &mut scratch)?;
    let gate_file = locks::open_fixed_control(&paths[0], Kind::File, false, &mut scratch)?;
    let gate = FixedLockedFile::try_lock(gate_file, LockMode::Shared)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    let owner_file = locks::open_fixed_control(&paths[2], Kind::File, true, &mut scratch)?;
    let owner = FixedLockedFile::try_lock(owner_file, LockMode::Exclusive)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    let state = locks::open_fixed_control(&paths[1], Kind::File, false, &mut scratch)?;
    let (observed, _) = super::maintenance_legacy_guard::prepare_source_observation(
        &root,
        gate.file(),
        &state,
        paths[2]
            .file_name()
            .map(|n| n.as_encoded_bytes())
            .ok_or_else(|| invalid(Problem::InvalidRecord))?,
        owner.file(),
        memory,
    )?;
    let result = LegacySourceOwner {
        state_stamp: observed.state_stamp,
        state_pin: state.try_clone()?,
        gate_pin: gate.file().try_clone()?,
        gate_header: raw_coordination(gate.file())?,
        artifact_header: if staging {
            [0; locks::COORDINATION_HEADER_BYTES]
        } else {
            let artifact = root
                .open_directory_readonly(&observed.binding.cache[..observed.binding.cache_len])?;
            raw_coordination(&artifact.open_readonly_file(b".use.lock")?)?
        },
        owner_header: raw_coordination(owner.file())?,
        commit: observed.commit,
        owner,
        staging,
        pending_publication: observed.pending,
        _root_pin: root.pin_handle()?,
        binding: observed.binding,
        record: observed.record,
        state_path: paths[1].clone(),
        gate_path: paths[0].clone(),
        _floor: floor,
    };
    if result.closed_version().context_digest() != expected.context_digest()
        || FixedFileStamp::capture(result.owner.file(), Kind::File)?.identity()
            != result.binding.owner
        || locks::read_coordination_incarnation(result.owner.file())
            .map_err(|_| invalid(Problem::InvalidRecord))?
            != Some(result.binding.owner_nonce)
    {
        return Err(invalid(Problem::Changed));
    }
    Ok(result)
}
pub(crate) fn reopen_legacy_source_owner(
    prepared: &mut PreparedLegacyScope,
    expected: &LegacyClosedVersion,
    memory: &CleanupMemory,
) -> Result<LegacySourceOwner, FixedControlError> {
    if !prepared.binding.is_current()? {
        return Err(invalid(Problem::Changed));
    }
    reopen_owner(&prepared.root, &prepared.paths, expected, false, memory)
}
pub(crate) fn reopen_legacy_staging_owner(
    prepared: &mut PreparedLegacyStagingScope,
    expected: &LegacyClosedVersion,
    memory: &CleanupMemory,
) -> Result<LegacySourceOwner, FixedControlError> {
    if !prepared.binding.is_current()? {
        return Err(invalid(Problem::Changed));
    }
    reopen_owner(&prepared.root, &prepared.paths, expected, true, memory)
}
impl LegacyClosedVersion {
    pub(crate) fn encode_checkpoint(&self, out: &mut [u8; 512]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLGCV01");
        out[8] = match self.phase {
            LegacyPhase::Active => 0,
            LegacyPhase::Preparing => 1,
            LegacyPhase::Retiring => 2,
            LegacyPhase::Removed => 3,
        };
        out[9] = u8::from(self.database.is_some()) | (u8::from(self.manifest.is_some()) << 1);
        id(&mut out[16..32], self.root);
        id(&mut out[32..48], self.state);
        id(&mut out[48..64], self.gate);
        out[64..96].copy_from_slice(&self.nonce);
        out[96..104].copy_from_slice(&self.version.to_le_bytes());
        out[104..136].copy_from_slice(&self.job);
        id(&mut out[136..152], self.member);
        if let Some(s) = self.database {
            s.encode_checkpoint((&mut out[152..232]).try_into().unwrap());
        }
        if let Some(s) = self.manifest {
            s.encode_checkpoint((&mut out[232..312]).try_into().unwrap());
        }
        id(&mut out[312..328], self.cache);
        super::maintenance_legacy_guard::checksum(out);
    }
    pub(crate) fn decode_checkpoint(bytes: &[u8; 512]) -> Result<Self, FixedControlError> {
        if &bytes[..8] != b"CGLGCV01"
            || !super::maintenance_legacy_guard::checked(bytes)
            || bytes[9] > 3
            || bytes[10..16]
                .iter()
                .chain(bytes[328..480].iter())
                .any(|b| *b != 0)
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let phase = match bytes[8] {
            1 => LegacyPhase::Preparing,
            2 => LegacyPhase::Retiring,
            3 => LegacyPhase::Removed,
            _ => return Err(invalid(Problem::InvalidRecord)),
        };
        let stamp =
            |present: bool, wire: &[u8]| -> Result<Option<FixedFileStamp>, FixedControlError> {
                if present {
                    Ok(Some(FixedFileStamp::decode_checkpoint(
                        wire.try_into().unwrap(),
                    )?))
                } else if wire.iter().any(|b| *b != 0) {
                    Err(invalid(Problem::InvalidRecord))
                } else {
                    Ok(None)
                }
            };
        let result = Self {
            root: super::maintenance_legacy_guard::read_id(&bytes[16..32]),
            state: super::maintenance_legacy_guard::read_id(&bytes[32..48]),
            gate: super::maintenance_legacy_guard::read_id(&bytes[48..64]),
            nonce: bytes[64..96].try_into().unwrap(),
            version: u64::from_le_bytes(bytes[96..104].try_into().unwrap()),
            job: bytes[104..136].try_into().unwrap(),
            phase,
            member: super::maintenance_legacy_guard::read_id(&bytes[136..152]),
            database: stamp(bytes[9] & 1 != 0, &bytes[152..232])?,
            manifest: stamp(bytes[9] & 2 != 0, &bytes[232..312])?,
            cache: super::maintenance_legacy_guard::read_id(&bytes[312..328]),
        };
        if result.version == 0 {
            return Err(invalid(Problem::InvalidRecord));
        }
        Ok(result)
    }
}
impl LegacyClosedVersion {
    /// Links a current retirement phase to the unchanged original preparation
    /// source. This does not rename either version or validate payload facts.
    pub(crate) fn same_job_source(&self, original: &Self) -> bool {
        let phase_ok = (self.phase == original.phase && self.version == original.version)
            || (original.phase == LegacyPhase::Preparing
                && self.phase == LegacyPhase::Retiring
                && original.version.checked_add(1) == Some(self.version))
            || (self.phase == LegacyPhase::Removed
                && ((original.phase == LegacyPhase::Preparing
                    && original.version.checked_add(2) == Some(self.version))
                    || (original.phase == LegacyPhase::Retiring
                        && original.version.checked_add(1) == Some(self.version))));
        phase_ok
            && self.root == original.root
            && self.state == original.state
            && self.gate == original.gate
            && self.nonce == original.nonce
            && self.job == original.job
            && self.member == original.member
            && self.database == original.database
            && self.manifest == original.manifest
            && self.cache == original.cache
    }
}

#[cfg(test)]
mod tests {
    use super::super::maintenance_budget::MemoryBox;
    use super::*;
    use std::cell::RefCell;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    fn target() -> MaintenanceTarget {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "legacy-enrollment-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        let state = root.join(".codebaseGraph");
        std::fs::create_dir_all(state.join("storage/artifacts")).unwrap();
        std::fs::create_dir(state.join("artifacts")).unwrap();
        std::fs::create_dir(state.join("native-staging")).unwrap();
        std::fs::write(state.join("config.json"), br#"{"schema_version":3}"#).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let state = root.join(".codebaseGraph");
        let target = MaintenanceTarget {
            repository_root: root,
            state_root: state,
            storage_root: None,
            direct_pair: None,
            config_path: None,
        };
        let pair = default_direct(&target);
        std::fs::write(
            &pair.database,
            b"control exclusion fixture, not origin evidence",
        )
        .unwrap();
        std::fs::write(&pair.manifest, b"{}").unwrap();
        target
    }
    #[test]
    fn dc_legacy_real_dispatcher_claim_enrolls_and_explicit_rebuild_invalidates_original_owner() {
        struct Work {
            scope: MemoryBox<PreparedLegacyScope>,
            memory: CleanupMemory,
            pair: DirectPair,
            owner: Option<LegacySourceOwner>,
            done: bool,
        }
        let target = target();
        let cleanup = target.repository_root.clone();
        let seen = Arc::new(AtomicBool::new(false));
        let output = seen.clone();
        let request_target = target.clone();
        // A legacy target challenge belongs to the retained runtime witness;
        // recapturing it would generate a new challenge on every preparation.
        let mut retained_binding: Option<MemoryBox<PreparedTargetBinding>> = None;
        let snapshot = super::super::maintenance_scheduler::with_claimed_legacy_confirmation(
            Some(&request_target),
            Arc::new(crate::coordinator::ExecutionDispatcher::default()),
            move |memory, request, parked: Option<MemoryBox<RefCell<Work>>>| {
                let resuming = parked.is_some();
                let work = if let Some(work) = parked {
                    work
                } else {
                    let binding = retained_binding.get_or_insert_with(|| {
                        MemoryBox::new(
                            super::super::maintenance_admission::prepare_target_binding(
                                &target, memory,
                            )
                            .unwrap(),
                            memory,
                        )
                        .unwrap()
                    });
                    let pair = default_direct(&target);
                    let scope = prepare_legacy_scope(
                        &target,
                        binding,
                        LegacyScope::DefaultDirect {
                            pair: &pair,
                            artifact_root: &target.state_root.join("artifacts"),
                        },
                        true,
                        memory,
                    )
                    .unwrap();
                    MemoryBox::new(
                        RefCell::new(Work {
                            scope,
                            memory: memory.clone(),
                            pair,
                            owner: None,
                            done: false,
                        }),
                        memory,
                    )
                    .unwrap()
                };
                let binding;
                {
                    let mut work = work.borrow_mut();
                    let Work {
                        scope,
                        memory,
                        pair,
                        owner,
                        done,
                    } = &mut *work;
                    binding =
                        super::super::maintenance_scheduler::TargetBinding(scope.binding.digest());
                    if let Some(owner) = owner.as_ref() {
                        // All codec work and the actual primary rebuild hook run
                        // after the previous collector permit has been released.
                        let original = owner.closed_version();
                        let mut wire = [0; 512];
                        original.encode_checkpoint(&mut wire);
                        let restored = LegacyClosedVersion::decode_checkpoint(&wire).unwrap();
                        assert!(restored.same_job_source(&original));
                        assert!(super::super::maintenance_legacy_guard::before_native_open(
                            &pair.database
                        )
                        .is_err());
                        let writer = locks::open_locked(
                            DirectLayout::new(&pair.database, &pair.manifest).writer_lock_path(),
                            LockMode::Exclusive,
                        )
                        .unwrap();
                        super::super::maintenance_legacy_guard::reactivate_legacy_source(
                            &pair.database,
                            &pair.manifest,
                            &writer,
                        )
                        .unwrap();
                        drop(writer);
                        assert!(!owner.is_current(memory).unwrap());
                        assert!(super::super::maintenance_legacy_guard::before_native_open(
                            &pair.database
                        )
                        .unwrap()
                        .is_some());
                        *done = true;
                        output.store(true, Ordering::Release);
                    } else if resuming {
                        scope.refresh_enrollment_preparation(memory).unwrap();
                    } else if request.is_some() {
                        scope.prepare_enrollment_metadata(request, memory).unwrap();
                    }
                }
                Ok((binding, work))
            },
            move |work, token, grant| {
                let mut work = work.borrow_mut();
                let Work {
                    scope,
                    memory,
                    owner,
                    done,
                    ..
                } = &mut *work;
                if *done {
                    return Ok(super::super::maintenance_scheduler::AttemptOutcome::Complete);
                }
                match with_legacy_enrollment(scope, token, grant, memory, &mut |_, _| Ok(())) {
                    Ok((_, actual)) => {
                        *owner = Some(actual);
                        Ok(super::super::maintenance_scheduler::AttemptOutcome::Pending)
                    }
                    Err(
                        FixedControlError::Invalid(Problem::Busy) | FixedControlError::Grant(_),
                    ) if scope.enrollment_pending() => {
                        Ok(super::super::maintenance_scheduler::AttemptOutcome::Pending)
                    }
                    Err(error) => panic!("actual source enrollment failed: {error:?}"),
                }
            },
        );
        assert!(seen.load(Ordering::Acquire), "{snapshot:?}");
        assert_eq!(
            snapshot.last_attempt.unwrap().outcome,
            super::super::maintenance_scheduler::AttemptOutcome::Complete
        );
        std::fs::remove_dir_all(cleanup).unwrap();
    }
    #[test]
    fn dc_legacy_real_dispatcher_claim_closes_and_aborts_only_staging_scope() {
        struct Work {
            scope: MemoryBox<PreparedLegacyStagingScope>,
            memory: CleanupMemory,
            path: PathBuf,
            owner: Option<LegacySourceOwner>,
            done: bool,
        }
        let target = target();
        let cleanup = target.repository_root.clone();
        let seen = Arc::new(AtomicBool::new(false));
        let output = seen.clone();
        let request_target = target.clone();
        // A legacy target challenge belongs to the retained runtime witness;
        // recapturing it would generate a new challenge on every preparation.
        let mut retained_binding: Option<MemoryBox<PreparedTargetBinding>> = None;
        let snapshot = super::super::maintenance_scheduler::with_claimed_legacy_confirmation(
            Some(&request_target),
            Arc::new(crate::coordinator::ExecutionDispatcher::default()),
            move |memory, request, parked: Option<MemoryBox<RefCell<Work>>>| {
                let resuming = parked.is_some();
                let work = if let Some(work) = parked {
                    work
                } else {
                    let binding = retained_binding.get_or_insert_with(|| {
                        MemoryBox::new(
                            super::super::maintenance_admission::prepare_target_binding(
                                &target, memory,
                            )
                            .unwrap(),
                            memory,
                        )
                        .unwrap()
                    });
                    let path = target.state_root.join("native-staging");
                    let scope =
                        prepare_legacy_staging_scope(&target, binding, &path, memory).unwrap();
                    MemoryBox::new(
                        RefCell::new(Work {
                            scope: MemoryBox::new(scope, memory).unwrap(),
                            memory: memory.clone(),
                            path,
                            owner: None,
                            done: false,
                        }),
                        memory,
                    )
                    .unwrap()
                };
                let binding;
                {
                    let mut work = work.borrow_mut();
                    let Work {
                        scope,
                        memory,
                        path,
                        owner,
                        done,
                    } = &mut *work;
                    binding =
                        super::super::maintenance_scheduler::TargetBinding(scope.binding.digest());
                    if let Some(owner) = owner.as_ref() {
                        if owner.closed_version().phase() == LegacyPhase::Active {
                            assert!(
                                super::super::maintenance_legacy_guard::before_staging_producer(
                                    path
                                )
                                .unwrap()
                                .is_some()
                            );
                            *done = true;
                            output.store(true, Ordering::Release);
                        } else {
                            assert!(
                                super::super::maintenance_legacy_guard::before_staging_producer(
                                    path
                                )
                                .is_err()
                            );
                            scope
                                .prepare_transition(owner, LegacyPhase::Active, memory)
                                .unwrap();
                        }
                    } else if resuming {
                        scope.refresh_enrollment_preparation(memory).unwrap();
                    } else if request.is_some() {
                        scope.prepare_enrollment_metadata(request, memory).unwrap();
                    }
                }
                Ok((binding, work))
            },
            move |work, token, grant| {
                let mut work = work.borrow_mut();
                let Work {
                    scope,
                    memory,
                    owner,
                    done,
                    ..
                } = &mut *work;
                if *done {
                    return Ok(super::super::maintenance_scheduler::AttemptOutcome::Complete);
                }
                if let Some(owner) = owner.as_mut() {
                    abort_legacy_staging_preparation(scope, owner, grant, memory).unwrap();
                    return Ok(super::super::maintenance_scheduler::AttemptOutcome::Pending);
                }
                match with_legacy_staging_enrollment(
                    scope,
                    token,
                    grant,
                    memory,
                    &mut |_, _| Ok(()),
                ) {
                    Ok((_, actual)) => {
                        *owner = Some(actual);
                        Ok(super::super::maintenance_scheduler::AttemptOutcome::Pending)
                    }
                    Err(
                        FixedControlError::Invalid(Problem::Busy) | FixedControlError::Grant(_),
                    ) if scope.enrollment_pending() => {
                        Ok(super::super::maintenance_scheduler::AttemptOutcome::Pending)
                    }
                    Err(error) => panic!("actual staging enrollment failed: {error:?}"),
                }
            },
        );
        assert!(seen.load(Ordering::Acquire), "{snapshot:?}");
        assert_eq!(
            snapshot.last_attempt.unwrap().outcome,
            super::super::maintenance_scheduler::AttemptOutcome::Complete
        );
        std::fs::remove_dir_all(cleanup).unwrap();
    }
}

#[inline(never)]
fn source_observation_scratch_boxed(
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<FixedControlScratch>, FixedControlError> {
    let scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    super::maintenance_budget::MemoryBox::new(scratch, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
fn accepted_source_observation(
    root_path: &Path,
    paths: &[PathBuf; 3],
    _staging: bool,
    memory: &CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<(Binding, Record)>>, FixedControlError> {
    let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
    let mut scratch = source_observation_scratch_boxed(memory)?;
    let root_file = match locks::open_fixed_control(root_path, Kind::Directory, false, &mut scratch)
    {
        Ok(f) => f,
        Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let root = RootedDirectory::open_fixed(
        root_path,
        FixedFileStamp::capture(&root_file, Kind::Directory)?.identity(),
        false,
        &mut scratch,
    )?;
    let gate = match locks::open_fixed_control(&paths[0], Kind::File, false, &mut scratch) {
        Ok(f) => f,
        Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let gate =
        FixedLockedFile::try_lock(gate, LockMode::Shared)?.ok_or_else(|| invalid(Problem::Busy))?;
    let state = match locks::open_fixed_control(&paths[1], Kind::File, false, &mut scratch) {
        Ok(f) => f,
        Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    match super::maintenance_legacy_guard::prepare_source_observation_boxed(
        &root,
        gate.file(),
        &state,
        paths[2]
            .file_name()
            .map(|n| n.as_encoded_bytes())
            .ok_or_else(|| invalid(Problem::InvalidRecord))?,
        memory,
    ) {
        Ok(data) => super::maintenance_budget::MemoryBox::new(
            (data.observed.binding, data.observed.record),
            memory,
        )
        .map(Some)
        .map_err(FixedControlError::Resource),
        Err(error) => {
            if let Some(capsule) =
                super::maintenance_legacy_guard::read_creation_capsule(&root, &state)?
            {
                if locks::created_file_nonce(&state)? == Some(capsule.permission.job) {
                    return super::maintenance_budget::MemoryBox::new(
                        (capsule.template, capsule.permission),
                        memory,
                    )
                    .map(Some)
                    .map_err(FixedControlError::Resource);
                }
            }
            match error {
                FixedControlError::Invalid(Problem::Busy) => Err(invalid(Problem::Busy)),
                _ => Ok(None),
            }
        }
    }
}
impl PreparedLegacyScope {
    pub(crate) fn has_accepted_job(
        &mut self,
        memory: &CleanupMemory,
    ) -> Result<bool, FixedControlError> {
        if !self.binding.is_current()? {
            return Ok(false);
        }
        let Some(accepted) = accepted_source_observation(&self.root, &self.paths, false, memory)?
        else {
            return Ok(false);
        };
        let (binding, record) = *accepted;
        if self.kind == LegacyScopeKind::ManagedAdministration {
            return Ok(binding.kind == self.kind
                && binding.root == self.entries[0].stamp.unwrap().identity()
                && record.cache == self.entries[2].stamp.unwrap().identity()
                && record.confirmation == self.cohort_scope
                && matches!(record.phase, LegacyPhase::Preparing | LegacyPhase::Active));
        }
        if binding.kind != self.kind
            || binding.key != self.key
            || binding.root != self.entries[0].stamp.unwrap().identity()
            || self.entries[3]
                .stamp
                .is_some_and(|s| record.member != s.identity())
            || (self.entries[3].stamp.is_none() && record.phase != LegacyPhase::Retiring)
            || (self.kind == LegacyScopeKind::ManagedGeneration
                && record.member_name[..record.member_len] != self.member_name[..self.member_len])
            || record.cache != self.entries[2].stamp.unwrap().identity()
        {
            return Ok(false);
        }
        let same =
            record.database == self.entries[4].stamp && record.manifest == self.entries[5].stamp;
        Ok(match record.phase {
            LegacyPhase::Preparing => same,
            LegacyPhase::Retiring => true,
            LegacyPhase::Active => {
                same && record.operator_pending
                    && record.confirmation == self.binding.digest()
                    && record.pending_source_stamp == self.entries[2].stamp
            }
            LegacyPhase::Removed => false,
        })
    }
}
impl PreparedLegacyStagingScope {}

/// Completion authority deliberately has no fabricated member/native guard when
/// a previous admitted removal already made that member or its locks absent.
pub(crate) struct LegacyCompletionAuthority<'a> {
    root: &'a RootedDirectory,
    parent: &'a RootedDirectory,
    member_name: &'a [u8],
    _writer: &'a FixedLockedFile,
    _gate: &'a FixedLockedFile,
    artifact_use: Option<&'a FixedLockedFile>,
}
impl LegacyCompletionAuthority<'_> {
    pub(crate) fn directory(&self) -> &RootedDirectory {
        self.root
    }
    pub(crate) fn member_parent(&self) -> &RootedDirectory {
        self.parent
    }
    pub(crate) fn member_name(&self) -> &[u8] {
        self.member_name
    }
    pub(crate) fn artifact_use_guard(&self) -> Option<&FixedLockedFile> {
        self.artifact_use
    }
}
pub(crate) fn with_legacy_root_completion<R>(
    prepared: &mut PreparedLegacyScope,
    owner: &LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyCompletionAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<R, FixedControlError> {
    let _floor = memory.reserve(32768).map_err(FixedControlError::Resource)?;
    if !prepared.binding.is_current()?
        || !owner.is_current(memory)?
        || owner.pending_publication
        || owner.record.phase != LegacyPhase::Retiring
    {
        return Err(invalid(Problem::Changed));
    }
    let scratch = &mut prepared.scratch;
    let root = RootedDirectory::open_fixed(&prepared.root, owner.binding.root, true, scratch)?;
    let (writer, _) = lock_entry(&prepared.writer, prepared.entries[6], &root, scratch, grant)?;
    let gate = FixedLockedFile::try_lock(root.open_existing(GATE)?, LockMode::Exclusive)?
        .ok_or_else(|| invalid(Problem::Busy))?;
    let state = root.open_existing(STATE)?;
    owner.matches_current_files(&root, gate.file(), &state)?;
    let artifact =
        match RootedDirectory::open_fixed(&prepared.artifact, owner.record.cache, true, scratch) {
            Ok(d) => Some(d),
            Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
    let artifact_use = if let Some(artifact) = artifact.as_ref() {
        let guard =
            FixedLockedFile::try_lock(artifact.open_existing(b".use.lock")?, LockMode::Exclusive)?
                .ok_or_else(|| invalid(Problem::Busy))?;
        if FixedFileStamp::capture(guard.file(), Kind::File)?.identity() != owner.record.root_use {
            return Err(invalid(Problem::Changed));
        }
        Some(guard)
    } else {
        None
    };
    let parent = if prepared.kind == LegacyScopeKind::ManagedGeneration {
        root.open_directory(b"generations")?
    } else {
        RootedDirectory::open_fixed(&prepared.root, owner.binding.root, true, scratch)?
    };
    let name = if prepared.kind == LegacyScopeKind::ManagedGeneration {
        &owner.record.member_name[..owner.record.member_len]
    } else {
        &owner.binding.database[..owner.binding.database_len]
    };
    apply(
        &LegacyCompletionAuthority {
            root: &root,
            parent: &parent,
            member_name: name,
            _writer: &writer,
            _gate: &gate,
            artifact_use: artifact_use.as_ref(),
        },
        grant,
    )
}
/// Operational completion only: no data is removed here. The role's opaque
/// cohort/journal proof authorizes its own callback; this final publication
/// checks actual named absence again under the retained exclusion kernel.
pub(crate) fn finish_legacy_retirement(
    prepared: &mut PreparedLegacyScope,
    owner: &mut LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    let mut next = owner.record;
    next.phase = LegacyPhase::Removed;
    next.version = next
        .version
        .checked_add(1)
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let publication = prepared
        .publication
        .take()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let result =
        with_legacy_root_completion(prepared, owner, grant, memory, &mut |authority, grant| {
            let absent = |result: std::io::Result<File>| -> Result<(), FixedControlError> {
                match result {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                    Ok(_) => Err(invalid(Problem::Changed)),
                }
            };
            if owner.binding.kind == LegacyScopeKind::ManagedGeneration {
                match authority
                    .parent
                    .open_directory_readonly(authority.member_name)
                {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                    Ok(_) => return Err(invalid(Problem::Changed)),
                }
            } else {
                absent(
                    authority
                        .root
                        .open_readonly_file(&owner.binding.database[..owner.binding.database_len]),
                )?;
                absent(authority.root.open_readonly_file(b"manifest.json"))?;
            }
            let state = authority.root.open_existing(STATE)?;
            publication.publish(&state, owner.binding, owner.commit, next, grant)
        });
    prepared.publication = Some(publication);
    owner.commit = result?;
    owner.record = next;
    owner.state_stamp = FixedFileStamp::capture(&owner.state_pin, Kind::File)?;
    Ok(())
}
fn observed_version(binding: Binding, record: Record) -> LegacyClosedVersion {
    LegacyClosedVersion {
        root: binding.root,
        state: binding.state,
        gate: binding.gate,
        nonce: binding.nonce,
        version: record.version,
        job: record.job,
        phase: record.phase,
        member: record.member,
        database: record.database,
        manifest: record.manifest,
        cache: record.cache,
    }
}
pub(crate) fn observe_legacy_source_version(
    prepared: &mut PreparedLegacyScope,
    memory: &CleanupMemory,
) -> Result<Option<LegacyClosedVersion>, FixedControlError> {
    if !prepared.binding.is_current()? {
        return Ok(None);
    }
    let Some(accepted) =
        accepted_source_observation(&prepared.root, &prepared.paths, false, memory)?
    else {
        return Ok(None);
    };
    let (binding, record) = *accepted;
    if binding.kind != prepared.kind
        || binding.root != prepared.entries[0].stamp.unwrap().identity()
        || binding.key != prepared.key
        || !matches!(
            record.phase,
            LegacyPhase::Preparing | LegacyPhase::Retiring | LegacyPhase::Removed
        )
    {
        return Ok(None);
    }
    Ok(Some(observed_version(binding, record)))
}
pub(crate) fn observe_legacy_staging_source_version(
    prepared: &mut PreparedLegacyStagingScope,
    memory: &CleanupMemory,
) -> Result<Option<LegacyClosedVersion>, FixedControlError> {
    if !prepared.binding.is_current()? {
        return Ok(None);
    }
    let Some(accepted) =
        accepted_source_observation(&prepared.root, &prepared.paths, true, memory)?
    else {
        return Ok(None);
    };
    let (binding, record) = *accepted;
    if binding.kind != LegacyScopeKind::NativeStaging
        || binding.root != prepared.root_stamp.identity()
        || record.member != prepared.staging_stamp.identity()
        || !matches!(
            record.phase,
            LegacyPhase::Preparing | LegacyPhase::Retiring | LegacyPhase::Removed
        )
    {
        return Ok(None);
    }
    Ok(Some(observed_version(binding, record)))
}

fn legacy_cohort_scope(
    target: &MaintenanceTarget,
    store: &Path,
    artifact: &Path,
    memory: &CleanupMemory,
) -> Result<[u8; 32], FixedControlError> {
    let _floor = memory.reserve(8192).map_err(FixedControlError::Resource)?;
    let configured = super::maintenance_projection::project_binding(target, memory)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    let mut hash = Sha256::new();
    hash.update(b"CGLegacyRootCohortScope01");
    for path in [
        &target.repository_root,
        &target.state_root,
        &configured.storage,
    ] {
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update([0]);
    }
    for path in [store, artifact] {
        let file = locks::open_fixed_control(path, Kind::Directory, false, &mut scratch)?;
        let identity = FixedFileStamp::capture(&file, Kind::Directory)?.identity();
        let (a, b) = identity.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
    }
    for identity in [configured.repository, configured.state] {
        let (a, b) = identity.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
    }
    hash.update(configured.config_path.as_os_str().as_encoded_bytes());
    if let Some(stamp) = configured.config_stamp {
        hash.update(stamp.binding_digest());
    }
    Ok(hash.finalize().into())
}
/// Only replay of genuine root-administration controls plus verified future-use
/// coverage constructs this capability. It carries no payload origin or finality.
pub(crate) struct AcceptedLegacyRootCohort {
    target_binding: std::cell::RefCell<super::maintenance_budget::MemoryBox<PreparedTargetBinding>>,
    gate_header: [u8; locks::COORDINATION_HEADER_BYTES],
    owner_header: [u8; locks::COORDINATION_HEADER_BYTES],
    artifact_identity: FileIdentity,
    operator_generation: u64,
    scope: [u8; 32],
    binding: Binding,
    record_stamp: FixedFileStamp,
    cohort_stamp: FixedFileStamp,
    scope_stamp: FixedFileStamp,
    default_source: Option<super::maintenance_admission::LegacyConfirmationSource>,
    root: PathBuf,
    _root_pin: File,
    _state_pin: File,
    _artifact_pin: File,
    _floor: MemoryReservation,
}
impl AcceptedLegacyRootCohort {
    fn matches(
        &self,
        prepared: &PreparedLegacyScope,
        memory: &CleanupMemory,
    ) -> Result<bool, FixedControlError> {
        if prepared.cohort_scope != self.scope
            || prepared.entries[0].stamp.map(|s| s.identity()) != Some(self.binding.root)
        {
            return Ok(false);
        }
        self.current(memory)
    }
    fn current(&self, memory: &CleanupMemory) -> Result<bool, FixedControlError> {
        let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
        if !self.target_binding.borrow_mut().is_current()? {
            return Ok(false);
        }
        let root = RootedDirectory::open_readonly_root(&self.root, self.binding.root)?;
        let gate = locks::try_open_existing_locked(
            &self.root.join(std::str::from_utf8(ADMIN_GATE).unwrap()),
            LockMode::Shared,
        )
        .map_err(|_| invalid(Problem::Changed))?
        .ok_or_else(|| invalid(Problem::Busy))?;
        let gate_file = root.open_readonly_file(ADMIN_GATE)?;
        let state = root.open_readonly_file(ADMIN_STATE)?;
        self.record_stamp.recheck(&state)?;
        self.record_stamp.recheck(&self._state_pin)?;
        let owner = root.open_readonly_file(ADMIN_OWNER)?;
        if FixedFileStamp::capture(&gate_file, Kind::File)?.identity() != self.binding.gate
            || FixedFileStamp::capture(&owner, Kind::File)?.identity() != self.binding.owner
            || gate.identity().map_err(|_| invalid(Problem::Changed))? != self.binding.gate
            || raw_coordination(&gate_file)? != self.gate_header
            || raw_coordination(&owner)? != self.owner_header
        {
            return Ok(false);
        }
        let artifact = root.open_directory_readonly(b"artifacts")?;
        if artifact.identity() != self.artifact_identity {
            return Ok(false);
        }
        self.cohort_stamp
            .recheck(&artifact.open_readonly_file(b".use-cohort.json")?)?;
        self.scope_stamp
            .recheck(&artifact.open_readonly_file(b".publication.binding")?)?;
        Ok(true)
    }
}
// Preserve the existing value API for standalone callers; worker preparation
// returns the charged opaque cohort directly without copying its raw Result.
#[allow(dead_code)]
pub(crate) fn observe_accepted_root_cohort(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<Option<AcceptedLegacyRootCohort>, FixedControlError> {
    observe_accepted_root_cohort_boxed(target, binding, memory)
        .map(|cohort| cohort.map(super::maintenance_budget::MemoryBox::into_inner))
}

#[inline(never)]
fn duplicate_cohort_binding_boxed(
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedTargetBinding>, FixedControlError> {
    let binding = binding.duplicate(memory)?;
    super::maintenance_budget::MemoryBox::new(binding, memory).map_err(FixedControlError::Resource)
}

/// Same administrative enrollment-only observations, placed behind a charged
/// owner before returning to any joined-worker preparation stage.
#[inline(never)]
pub(crate) fn observe_accepted_root_cohort_boxed(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<AcceptedLegacyRootCohort>>, FixedControlError>
{
    let floor = memory
        .reserve(
            std::mem::size_of::<AcceptedLegacyRootCohort>()
                + 65536
                + target.repository_root.as_os_str().as_encoded_bytes().len() * 16,
        )
        .map_err(FixedControlError::Resource)?;
    let mut current = duplicate_cohort_binding_boxed(binding, memory)?;
    if !current.is_current()? {
        return Ok(None);
    }
    let configured = super::maintenance_projection::project_binding(target, memory)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    if configured.direct.is_some() {
        return Ok(None);
    }
    let root_path = configured.storage;
    let root = RootedDirectory::open_readonly_root(
        &root_path,
        locks::path_identity(&root_path).map_err(|_| invalid(Problem::Changed))?,
    )?;
    let state = match root.open_readonly_file(ADMIN_STATE) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let gate = match root.open_readonly_file(ADMIN_GATE) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let observed = match super::maintenance_legacy_guard::observe_files_named(
        &root,
        &gate,
        &state,
        ADMIN_OWNER,
    ) {
        Ok(o) => o,
        Err(_) => return Ok(None),
    };
    if observed.binding.kind != LegacyScopeKind::ManagedAdministration
        || observed.record.phase != LegacyPhase::Preparing
    {
        return Ok(None);
    }
    let artifact = root.open_directory_readonly(b"artifacts")?;
    if artifact.identity() != observed.record.cache {
        return Ok(None);
    }
    let scope = legacy_cohort_scope(target, &root_path, &root_path.join("artifacts"), memory)?;
    if scope != observed.record.confirmation {
        return Ok(None);
    }
    let layout = super::layout::ManagedLayout::new(&root_path);
    let domain = super::maintenance_ownership::ReferenceDomain::managed(&layout)
        .map_err(|_| invalid(Problem::Changed))?;
    if !matches!(
        super::maintenance_ownership::observe_reference_epoch(&domain)
            .map_err(|_| invalid(Problem::Changed))?,
        super::maintenance_ownership::EpochObservation::Verified(_)
    ) {
        return Ok(None);
    }
    let cohort_stamp = FixedFileStamp::capture(
        &artifact.open_readonly_file(b".use-cohort.json")?,
        Kind::File,
    )?;
    let scope_stamp = FixedFileStamp::capture(
        &artifact.open_readonly_file(b".publication.binding")?,
        Kind::File,
    )?;
    super::maintenance_budget::MemoryBox::new(
        AcceptedLegacyRootCohort {
            target_binding: std::cell::RefCell::new(duplicate_cohort_binding_boxed(
                binding, memory,
            )?),
            gate_header: raw_coordination(&gate)?,
            owner_header: raw_coordination(&root.open_readonly_file(ADMIN_OWNER)?)?,
            artifact_identity: artifact.identity(),
            operator_generation: observed.record.operator_generation,
            scope,
            binding: observed.binding,
            record_stamp: observed.state_stamp,
            cohort_stamp,
            scope_stamp,
            default_source: observed.record.root_default_source,
            root: root_path,
            _root_pin: root.pin_handle()?,
            _state_pin: state,
            _artifact_pin: artifact.pin_handle()?,
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
pub(crate) fn with_legacy_enrollment<R>(
    prepared: &mut PreparedLegacyScope,
    confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    with_legacy_enrollment_inner(prepared, confirmation, None, None, grant, memory, apply)
}
pub(crate) fn with_legacy_enrollment_in_cohort<R>(
    prepared: &mut PreparedLegacyScope,
    cohort: &AcceptedLegacyRootCohort,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    with_legacy_enrollment_inner(prepared, None, Some(cohort), None, grant, memory, apply)
}
impl LegacySourceOwner {
    pub(crate) fn is_administrative(&self) -> bool {
        self.binding.kind == LegacyScopeKind::ManagedAdministration
    }
}

#[cfg(test)]
pub(crate) fn with_legacy_staging_enrollment<R>(
    prepared: &mut PreparedLegacyStagingScope,
    confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyStagingEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    with_legacy_staging_enrollment_inner(prepared, confirmation, None, grant, memory, apply)
}
pub(crate) fn with_legacy_staging_enrollment_in_cohort<R>(
    prepared: &mut PreparedLegacyStagingScope,
    cohort: &AcceptedLegacyRootCohort,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyStagingEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    with_legacy_staging_enrollment_inner(prepared, None, Some(cohort), grant, memory, apply)
}

pub(crate) fn finish_legacy_staging_cohort(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &mut LegacySourceOwner,
    done: &super::maintenance_legacy_engine::LegacyStagingCohortDone,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if owner.record.phase != LegacyPhase::Retiring
        || !owner
            .closed_version()
            .same_job_source(done.original_source())
        || done.directory_stamp().identity() != owner.record.member
    {
        return Err(invalid(Problem::Changed));
    }
    with_closed_staging_source(prepared, owner, grant, memory, &mut |authority, grant| {
        grant.take_entry()?;
        done.directory_stamp()
            .recheck(&authority.staging_directory().pin_handle()?)?;
        Ok(())
    })?;
    transition_staging_source(prepared, owner, LegacyPhase::Removed, grant, memory)
}

impl super::maintenance_ownership::administrative_coverage_seal::Sealed
    for LegacyEnrollmentAuthority<'_>
{
}
impl super::maintenance_ownership::AdministrativeCoverageAuthority
    for LegacyEnrollmentAuthority<'_>
{
    fn directory(&self) -> &RootedDirectory {
        self.directory()
    }
    fn artifact_directory(&self) -> &RootedDirectory {
        self.artifact_directory()
    }
    fn writer_identity(&self) -> Result<FileIdentity, FixedControlError> {
        self.writer_identity()
    }
    fn artifact_use_identity(&self) -> Result<FileIdentity, FixedControlError> {
        self.artifact_use_identity()
    }
    fn proposed_artifact_incarnation(&self) -> [u8; 32] {
        self.proposed_artifact_incarnation()
    }
    fn is_administrative(&self) -> bool {
        self.kind == LegacyScopeKind::ManagedAdministration
    }
}

pub(crate) fn reopen_completed_legacy_staging(
    prepared: &mut PreparedLegacyStagingScope,
    owner: &mut LegacySourceOwner,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if owner.record.phase != LegacyPhase::Removed {
        return Err(invalid(Problem::Changed));
    }
    transition_staging_source(prepared, owner, LegacyPhase::Active, grant, memory)
}

fn raw_coordination(
    file: &File,
) -> Result<[u8; locks::COORDINATION_HEADER_BYTES], FixedControlError> {
    let mut header = [0; locks::COORDINATION_HEADER_BYTES];
    locks::read_fixed_range(file, 0, &mut header)?;
    Ok(header)
}
impl LegacySourceOwner {
    fn matches_current_files(
        &self,
        root: &RootedDirectory,
        gate: &File,
        state: &File,
    ) -> Result<(), FixedControlError> {
        self.state_stamp.recheck(state)?;
        self.state_stamp.recheck(&self.state_pin)?;
        let owner_name = if self.staging {
            STAGING_OWNER
        } else if self.binding.kind == LegacyScopeKind::ManagedAdministration {
            ADMIN_OWNER
        } else {
            OWNER
        };
        let named_owner = root.open_metadata_file(owner_name)?;
        if FixedFileStamp::capture(&named_owner, Kind::File)?.identity() != self.binding.owner
            || FixedFileStamp::capture(&self.gate_pin, Kind::File)?.identity() != self.binding.gate
        {
            return Err(invalid(Problem::Changed));
        }
        if root.identity() != self.binding.root
            || FixedFileStamp::capture(gate, Kind::File)?.identity() != self.binding.gate
            || FixedFileStamp::capture(self.owner.file(), Kind::File)?.identity()
                != self.binding.owner
            || raw_coordination(gate)? != self.gate_header
            || raw_coordination(self.owner.file())? != self.owner_header
            || self.owner.mode() != LockMode::Exclusive
        {
            return Err(invalid(Problem::Changed));
        }
        Ok(())
    }
}
impl LegacySourceOwner {}

impl PreparedLegacyScope {
    pub(crate) fn prepare_transition(
        &mut self,
        owner: &LegacySourceOwner,
        next: LegacyPhase,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        if !owner.is_current(memory)? {
            return Err(invalid(Problem::Changed));
        }
        let mut after = owner.record;
        after.phase = next;
        after.operator_pending = false;
        after.pending_source_stamp = None;
        after.version = after
            .version
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        self.publication = Some(
            super::maintenance_legacy_guard::PreparedSourcePublication::prepare(
                owner.binding,
                owner.commit,
                after,
                memory,
            )?,
        );
        Ok(())
    }
}
impl PreparedLegacyStagingScope {
    pub(crate) fn prepare_transition(
        &mut self,
        owner: &LegacySourceOwner,
        next: LegacyPhase,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        if !owner.is_current(memory)? {
            return Err(invalid(Problem::Changed));
        }
        let mut after = owner.record;
        after.phase = next;
        after.operator_pending = false;
        after.pending_source_stamp = None;
        after.version = after
            .version
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        self.publication = Some(
            super::maintenance_legacy_guard::PreparedSourcePublication::prepare(
                owner.binding,
                owner.commit,
                after,
                memory,
            )?,
        );
        Ok(())
    }
}

pub(crate) fn observe_legacy_member_name(
    prepared: &mut PreparedLegacyScope,
    memory: &CleanupMemory,
) -> Result<Option<([u8; 128], u8)>, FixedControlError> {
    if prepared.kind != LegacyScopeKind::ManagedGeneration || !prepared.binding.is_current()? {
        return Ok(None);
    }
    let Some(accepted) =
        accepted_source_observation(&prepared.root, &prepared.paths, false, memory)?
    else {
        return Ok(None);
    };
    let (binding, record) = *accepted;
    if binding.kind != prepared.kind
        || binding.root != prepared.entries[0].stamp.unwrap().identity()
        || record.member_len == 0
        || record.member_len > 128
    {
        return Ok(None);
    }
    Ok(Some((record.member_name, record.member_len as u8)))
}

struct PreparedSourceObservation {
    observed: super::maintenance_budget::MemoryBox<super::maintenance_legacy_guard::Observation>,
    recovery: Option<
        super::maintenance_budget::MemoryBox<
            super::maintenance_legacy_guard::PreparedSourceRecovery,
        >,
    >,
    state: File,
    gate: File,
    owner: File,
    gate_header: [u8; locks::COORDINATION_HEADER_BYTES],
    owner_header: [u8; locks::COORDINATION_HEADER_BYTES],
    _floor: MemoryReservation,
}
impl PreparedSourceObservation {
    fn read(
        root_path: &Path,
        paths: &[PathBuf; 3],
        held_owner: Option<&File>,
        memory: &CleanupMemory,
    ) -> Result<Option<super::maintenance_budget::MemoryBox<Self>>, FixedControlError> {
        let floor = memory
            .reserve(std::mem::size_of::<Self>() * 2 + 16384)
            .map_err(FixedControlError::Resource)?;
        let root = RootedDirectory::open_readonly_root(
            root_path,
            locks::path_identity(root_path).map_err(|_| invalid(Problem::Changed))?,
        )?;
        let state = match root.open_readonly_file(
            paths[1]
                .file_name()
                .ok_or_else(|| invalid(Problem::InvalidRecord))?
                .as_encoded_bytes(),
        ) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let gate = root.open_readonly_file(
            paths[0]
                .file_name()
                .ok_or_else(|| invalid(Problem::InvalidRecord))?
                .as_encoded_bytes(),
        )?;
        let owner = root.open_metadata_file(
            paths[2]
                .file_name()
                .ok_or_else(|| invalid(Problem::InvalidRecord))?
                .as_encoded_bytes(),
        )?;
        let opened;
        let owner_file = if let Some(held) = held_owner {
            held
        } else {
            opened = root.open_readonly_file(paths[2].file_name().unwrap().as_encoded_bytes())?;
            &opened
        };
        let data =
            match super::maintenance_legacy_guard::prepare_source_observation_with_owner_boxed(
                &root,
                &gate,
                &state,
                paths[2].file_name().unwrap().as_encoded_bytes(),
                owner_file,
                memory,
            ) {
                Ok(o) => o,
                Err(FixedControlError::Invalid(Problem::Busy)) => {
                    if let Some(capsule) =
                        super::maintenance_legacy_guard::read_creation_capsule(&root, &state)?
                    {
                        if locks::created_file_nonce(&state)? == Some(capsule.permission.job) {
                            return Ok(None);
                        }
                    }
                    return Err(invalid(Problem::Busy));
                }
                Err(_) => return Ok(None),
            };
        let super::maintenance_legacy_guard::PreparedSourceObservationData { observed, recovery } =
            data.into_inner();
        let gate_header = raw_coordination(&gate)?;
        let owner_header = raw_coordination(owner_file)?;
        super::maintenance_budget::MemoryBox::new(
            Self {
                observed,
                recovery,
                state,
                gate,
                owner,
                gate_header,
                owner_header,
                _floor: floor,
            },
            memory,
        )
        .map(Some)
        .map_err(FixedControlError::Resource)
    }
    fn verify(
        &self,
        root: &RootedDirectory,
        gate: &File,
        owner: &File,
        state: &File,
    ) -> Result<&super::maintenance_legacy_guard::Observation, FixedControlError> {
        self.observed.state_stamp.recheck(state)?;
        self.observed.state_stamp.recheck(&self.state)?;
        if root.identity() != self.observed.binding.root
            || FixedFileStamp::capture(gate, Kind::File)?.identity() != self.observed.binding.gate
            || FixedFileStamp::capture(&self.gate, Kind::File)?.identity()
                != self.observed.binding.gate
            || FixedFileStamp::capture(&self.owner, Kind::File)?.identity()
                != self.observed.binding.owner
            || FixedFileStamp::capture(owner, Kind::File)?.identity() != self.observed.binding.owner
            || raw_coordination(gate)? != self.gate_header
            || raw_coordination(owner)? != self.owner_header
        {
            return Err(invalid(Problem::Changed));
        }
        Ok(&self.observed)
    }
}

/// A separate upgraded-protocol lane. No user permission or root cohort is
/// fabricated: the actual guarded-creation E capability is verified under the
/// same real writer/native/root guards before any source-state publication.
pub(crate) fn with_legacy_enrollment_automatic<R>(
    prepared: &mut PreparedLegacyScope,
    automatic: &mut super::maintenance_ownership::PreparedAutomaticDirect,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    apply: &mut impl FnMut(
        &LegacyEnrollmentAuthority<'_>,
        &mut MutationGrant,
    ) -> Result<R, FixedControlError>,
) -> Result<(R, LegacySourceOwner), FixedControlError> {
    with_legacy_enrollment_inner(prepared, None, None, Some(automatic), grant, memory, apply)
}

struct PreparedSourceBirth {
    capsule: super::maintenance_legacy_guard::SourceCreationCapsule,
    capsule_image: [u8; super::maintenance_legacy_guard::SOURCE_CAPSULE_BYTES],
    promotion: Option<super::maintenance_budget::MemoryBox<SourceBirthPromotion>>,
    pending: bool,
    _floor: MemoryReservation,
}
struct SourceBirthPromotion {
    image: [u8; STATE_BYTES as usize],
    binding: Binding,
    state_stamp: FixedFileStamp,
    state_pin: File,
    final_nonce: [u8; 32],
    _floor: MemoryReservation,
}
impl PreparedSourceBirth {
    #[inline(never)]
    fn prepare(
        root_path: &Path,
        paths: &[PathBuf; 3],
        capsule: super::maintenance_legacy_guard::SourceCreationCapsule,
        memory: &CleanupMemory,
    ) -> Result<super::maintenance_budget::MemoryBox<Self>, FixedControlError> {
        let floor = memory
            .reserve(std::mem::size_of::<Self>() * 2 + 16384)
            .map_err(FixedControlError::Resource)?;
        let capsule_image = capsule.encode();
        let root = RootedDirectory::open_readonly_root(root_path, capsule.template.root)?;
        let state = match root.open_readonly_file(paths[1].file_name().unwrap().as_encoded_bytes())
        {
            Ok(f) => Some(f),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let promotion = if let Some(state) = state {
            let state_before = FixedFileStamp::capture(&state, Kind::File)?;
            let actual = super::maintenance_legacy_guard::read_creation_capsule(&root, &state)?
                .ok_or_else(|| invalid(Problem::InvalidRecord))?;
            if actual.encode() != capsule_image
                || locks::created_file_nonce(&state)? != Some(capsule.permission.job)
            {
                return Err(invalid(Problem::Changed));
            }
            let gate = root.open_readonly_file(paths[0].file_name().unwrap().as_encoded_bytes())?;
            let owner =
                root.open_readonly_file(paths[2].file_name().unwrap().as_encoded_bytes())?;
            let gate_before = FixedFileStamp::capture(&gate, Kind::File)?;
            let owner_before = FixedFileStamp::capture(&owner, Kind::File)?;
            let mut binding = capsule.template;
            binding.gate = FixedFileStamp::capture(&gate, Kind::File)?.identity();
            binding.owner = FixedFileStamp::capture(&owner, Kind::File)?.identity();
            binding.state = FixedFileStamp::capture(&state, Kind::File)?.identity();
            binding.nonce = locks::read_coordination_incarnation(&gate)
                .map_err(|_| invalid(Problem::Changed))?
                .ok_or_else(|| invalid(Problem::Changed))?;
            binding.owner_nonce = locks::read_coordination_incarnation(&owner)
                .map_err(|_| invalid(Problem::Changed))?
                .ok_or_else(|| invalid(Problem::Changed))?;
            let mut record = capsule.permission;
            if binding.kind == LegacyScopeKind::NativeStaging {
                record.root_use = binding.gate;
            }
            let mut image = [0; STATE_BYTES as usize];
            image[..BINDING_BYTES].copy_from_slice(&binding.encode());
            let mut slot = [0; codec::SLOT_BYTES];
            let fact = codec::encode_slot(&mut slot, &binding.nonce, 1, &record.encode())
                .map_err(|_| invalid(Problem::InvalidRecord))?;
            image[SLOT0 as usize..SLOT1 as usize].copy_from_slice(&slot);
            codec::encode_commit(
                &mut image[SELECTOR as usize..SELECTOR as usize + codec::COMMIT_BYTES],
                &binding.nonce,
                codec::CommitFact {
                    generation: 1,
                    active: 0,
                    slot_digest: fact.digest,
                },
            )
            .map_err(|_| invalid(Problem::InvalidRecord))?;
            let mut raw = [0; STATE_BYTES as usize];
            locks::read_fixed_range(&state, 0, &mut raw)?;
            let mut unwritten = false;
            for (actual, expected) in raw.iter().zip(image.iter()) {
                if *actual != *expected {
                    if *actual != 0 {
                        return Err(invalid(Problem::Changed));
                    }
                    unwritten = true;
                } else if unwritten && *actual != 0 {
                    return Err(invalid(Problem::Changed));
                }
            }
            let mut hash = Sha256::new();
            hash.update(b"CGLegacySourceBirthCommitted01");
            hash.update(capsule.permission.job);
            hash.update(&image[..BINDING_BYTES]);
            let final_nonce = hash.finalize().into();
            state_before.recheck(&state)?;
            gate_before.recheck(&gate)?;
            owner_before.recheck(&owner)?;
            Some(
                super::maintenance_budget::MemoryBox::new(
                    SourceBirthPromotion {
                        image,
                        binding,
                        state_stamp: state_before,
                        state_pin: state,
                        final_nonce,
                        _floor: memory
                            .reserve(std::mem::size_of::<SourceBirthPromotion>() * 2)
                            .map_err(FixedControlError::Resource)?,
                    },
                    memory,
                )
                .map_err(FixedControlError::Resource)?,
            )
        } else {
            None
        };
        super::maintenance_budget::MemoryBox::new(
            Self {
                capsule,
                capsule_image,
                promotion,
                pending: false,
                _floor: floor,
            },
            memory,
        )
        .map_err(FixedControlError::Resource)
    }
    fn publish_created(
        &mut self,
        root: &RootedDirectory,
        state: &File,
        binding: Binding,
        record: Record,
        grant: &mut MutationGrant,
    ) -> Result<(), FixedControlError> {
        let mut template = binding;
        template.gate = FileIdentity::from_fixed_words(0, 0);
        template.owner = FileIdentity::from_fixed_words(0, 0);
        template.state = FileIdentity::from_fixed_words(0, 0);
        template.nonce = record.job;
        template.owner_nonce = self.capsule.template.owner_nonce;
        let mut permission = record;
        permission.root_use = FileIdentity::from_fixed_words(0, 0);
        if template != self.capsule.template || permission != self.capsule.permission {
            return Err(invalid(Problem::Changed));
        }
        if state.metadata()?.len() != 0 {
            return Err(invalid(Problem::Changed));
        }
        grant.take_entry()?;
        locks::set_created_file_nonce(state, &record.job)?;
        write_range(
            state,
            super::maintenance_legacy_guard::SOURCE_CAPSULE_OFFSET,
            &self.capsule_image,
            grant,
        )?;
        grant.take_entry()?;
        root.sync_namespace()?;
        self.pending = true;
        Ok(())
    }
    fn promote(
        &mut self,
        root: &RootedDirectory,
        state: &File,
        gate: &File,
        owner: &File,
        grant: &mut MutationGrant,
    ) -> Result<(), FixedControlError> {
        let promotion = self
            .promotion
            .as_ref()
            .ok_or_else(|| invalid(Problem::Changed))?;
        promotion.state_stamp.recheck(state)?;
        promotion.state_stamp.recheck(&promotion.state_pin)?;
        if promotion.binding.root != root.identity()
            || promotion.binding.gate != FixedFileStamp::capture(gate, Kind::File)?.identity()
            || promotion.binding.owner != FixedFileStamp::capture(owner, Kind::File)?.identity()
            || promotion.binding.state != FixedFileStamp::capture(state, Kind::File)?.identity()
            || locks::created_file_nonce(state)? != Some(self.capsule.permission.job)
        {
            return Err(invalid(Problem::Changed));
        }
        for (offset, end) in [
            (0, SLOT0),
            (SLOT0, SLOT1),
            (SLOT1, SELECTOR),
            (SELECTOR, STATE_BYTES),
        ] {
            write_range(
                state,
                offset,
                &promotion.image[offset as usize..end as usize],
                grant,
            )?;
        }
        grant.take_entry()?;
        locks::replace_created_file_nonce(
            state,
            &self.capsule.permission.job,
            &promotion.final_nonce,
        )?;
        grant.take_entry()?;
        state.sync_all()?;
        self.pending = true;
        Ok(())
    }
}

impl PreparedLegacyScope {
    pub(crate) fn prepare_enrollment_metadata(
        &mut self,
        request: Option<(
            u64,
            Option<super::maintenance_admission::LegacyConfirmationSource>,
        )>,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(request.map(|(g, s)| (1, g, s)), memory)
    }
    pub(crate) fn prepare_enrollment_in_cohort(
        &mut self,
        cap: &AcceptedLegacyRootCohort,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(
            Some((
                2,
                cap.operator_generation,
                if self.kind == LegacyScopeKind::DefaultDirect {
                    cap.default_source
                } else {
                    None
                },
            )),
            memory,
        )
    }
    pub(crate) fn prepare_enrollment_automatic(
        &mut self,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(Some((3, 0, None)), memory)
    }
    pub(crate) fn refresh_enrollment_preparation(
        &mut self,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(None, memory)
    }
    pub(crate) fn enrollment_pending(&self) -> bool {
        self.enrollment_waiting || self.birth.as_ref().is_some_and(|b| b.pending)
    }
    fn prepare_enrollment_images(
        &mut self,
        request: Option<(
            u8,
            u64,
            Option<super::maintenance_admission::LegacyConfirmationSource>,
        )>,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        if self.kind == LegacyScopeKind::ManagedGeneration {
            self.enrollment_runs = Some(prepare_empty_runs(&self.root, memory)?);
        }
        self.observation = PreparedSourceObservation::read(&self.root, &self.paths, None, memory)?;
        for (i, path) in [
            (6, &self.writer),
            (7, &self.family),
            (8, &self.native),
            (9, &self.root_use),
        ] {
            self.entries[i] = Entry::capture(path, Kind::File, true, &mut self.scratch)?;
        }
        let root = RootedDirectory::open_readonly_root(
            &self.root,
            self.entries[0].stamp.unwrap().identity(),
        )?;
        let state = root.open_readonly_file(self.paths[1].file_name().unwrap().as_encoded_bytes());
        if self.observation.is_none() {
            let saved = match state {
                Ok(file) => super::maintenance_legacy_guard::read_creation_capsule(&root, &file)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            let capsule = if let Some(saved) = saved {
                self.job = saved.permission.job;
                if saved.template.kind != self.kind
                    || saved.template.key != self.key
                    || saved.template.repository != self.entries[1].stamp.unwrap().identity()
                    || saved.permission.cache != self.entries[2].stamp.unwrap().identity()
                    || self.kind != LegacyScopeKind::ManagedAdministration
                        && (saved.permission.database != self.entries[4].stamp
                            || saved.permission.manifest != self.entries[5].stamp
                            || saved.permission.member
                                != self.entries[3]
                                    .stamp
                                    .ok_or_else(|| invalid(Problem::Changed))?
                                    .identity())
                {
                    return Err(invalid(Problem::Changed));
                }
                saved
            } else {
                let Some((mode, generation, source)) = request else {
                    return Ok(());
                };
                let mut database = [0; 256];
                let name = self.database.file_name().unwrap().as_encoded_bytes();
                database[..name.len()].copy_from_slice(name);
                let mut cache = [0; 256];
                let cache_name = self.artifact.file_name().unwrap().as_encoded_bytes();
                cache[..cache_name.len()].copy_from_slice(cache_name);
                let zero = FileIdentity::from_fixed_words(0, 0);
                super::maintenance_legacy_guard::SourceCreationCapsule {
                    mode,
                    template: Binding {
                        kind: self.kind,
                        root: root.identity(),
                        gate: zero,
                        state: zero,
                        owner: zero,
                        nonce: self.job,
                        owner_nonce: [0; 32],
                        key: self.key,
                        database,
                        database_len: name.len(),
                        cache,
                        cache_len: cache_name.len(),
                        repository: self.entries[1].stamp.unwrap().identity(),
                    },
                    permission: Record {
                        phase: LegacyPhase::Active,
                        version: 1,
                        member: self.entries[3]
                            .stamp
                            .ok_or_else(|| invalid(Problem::Changed))?
                            .identity(),
                        database: self.entries[4].stamp,
                        manifest: self.entries[5].stamp,
                        cache: self.entries[2].stamp.unwrap().identity(),
                        root_use: zero,
                        confirmation: if self.kind == LegacyScopeKind::ManagedAdministration {
                            self.cohort_scope
                        } else {
                            self.binding.digest()
                        },
                        job: self.job,
                        member_name: self.member_name,
                        member_len: self.member_len,
                        freeze_cache: self.freeze_cache
                            && self.kind == LegacyScopeKind::DefaultDirect,
                        operator_generation: generation,
                        operator_pending: true,
                        pending_source_stamp: self.entries[2].stamp,
                        root_default_source: if matches!(
                            self.kind,
                            LegacyScopeKind::ManagedAdministration | LegacyScopeKind::DefaultDirect
                        ) {
                            source
                        } else {
                            None
                        },
                    },
                }
            };
            self.birth = Some(PreparedSourceBirth::prepare(
                &self.root,
                &self.paths,
                capsule,
                memory,
            )?);
            return Ok(());
        }
        let observed = &self.observation.as_ref().unwrap().observed;
        if !matches!(
            observed.record.phase,
            LegacyPhase::Active | LegacyPhase::Removed
        ) {
            return Ok(());
        }
        let Some(root_use) = self.entries[9].stamp else {
            return Ok(());
        };
        let mut after = observed.record;
        after.phase = LegacyPhase::Preparing;
        after.operator_pending = false;
        after.pending_source_stamp = None;
        after.version = observed
            .commit
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        if let Some((_, generation, _)) = request {
            after.operator_generation = generation;
            after.confirmation = if self.kind == LegacyScopeKind::ManagedAdministration {
                self.cohort_scope
            } else {
                self.binding.digest()
            };
        }
        after.member = self.entries[3]
            .stamp
            .ok_or_else(|| invalid(Problem::Changed))?
            .identity();
        after.member_name = self.member_name;
        after.member_len = self.member_len;
        after.cache = self.entries[2].stamp.unwrap().identity();
        after.database = self.entries[4].stamp;
        after.manifest = self.entries[5].stamp;
        after.root_use = root_use.identity();
        after.job = self.job;
        self.publication = Some(
            super::maintenance_legacy_guard::PreparedSourcePublication::prepare(
                observed.binding,
                observed.commit,
                after,
                memory,
            )?,
        );
        Ok(())
    }
}

fn existing_coordination_nonce(path: &Path) -> Result<Option<[u8; 32]>, FixedControlError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    let root = RootedDirectory::open_readonly_root(
        parent,
        locks::path_identity(parent).map_err(|_| invalid(Problem::Changed))?,
    )?;
    let file = match root.open_readonly_file(
        path.file_name()
            .ok_or_else(|| invalid(Problem::InvalidRecord))?
            .as_encoded_bytes(),
    ) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    locks::read_coordination_incarnation(&file).map_err(|_| invalid(Problem::InvalidRecord))
}

impl PreparedLegacyStagingScope {
    #[cfg(test)]
    pub(crate) fn prepare_enrollment_metadata(
        &mut self,
        request: Option<(
            u64,
            Option<super::maintenance_admission::LegacyConfirmationSource>,
        )>,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(request.map(|(generation, _)| (1, generation)), memory)
    }
    pub(crate) fn prepare_enrollment_in_cohort(
        &mut self,
        cap: &AcceptedLegacyRootCohort,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(Some((2, cap.operator_generation)), memory)
    }
    pub(crate) fn refresh_enrollment_preparation(
        &mut self,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.prepare_enrollment_images(None, memory)
    }
    pub(crate) fn enrollment_pending(&self) -> bool {
        self.enrollment_waiting || self.birth.as_ref().is_some_and(|b| b.pending)
    }
    fn prepare_enrollment_images(
        &mut self,
        request: Option<(u8, u64)>,
        memory: &CleanupMemory,
    ) -> Result<(), FixedControlError> {
        self.observation = PreparedSourceObservation::read(&self.root, &self.paths, None, memory)?;
        if let Some(observation) = self.observation.as_ref() {
            if !matches!(
                observation.observed.record.phase,
                LegacyPhase::Active | LegacyPhase::Removed
            ) {
                return Ok(());
            }
            let mut after = observation.observed.record;
            after.phase = LegacyPhase::Preparing;
            after.operator_pending = false;
            after.pending_source_stamp = None;
            after.version = after
                .version
                .checked_add(1)
                .ok_or_else(|| invalid(Problem::InvalidRecord))?;
            after.job = self.job;
            if let Some((_, generation)) = request {
                after.operator_generation = generation;
                after.confirmation = self.binding.digest();
            }
            self.publication = Some(
                super::maintenance_legacy_guard::PreparedSourcePublication::prepare(
                    observation.observed.binding,
                    observation.observed.commit,
                    after,
                    memory,
                )?,
            );
            return Ok(());
        }
        let root = RootedDirectory::open_readonly_root(&self.root, self.root_stamp.identity())?;
        let state = root.open_readonly_file(STAGING_STATE);
        let saved = match state {
            Ok(file) => super::maintenance_legacy_guard::read_creation_capsule(&root, &file)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let capsule = if let Some(saved) = saved {
            if saved.template.kind != LegacyScopeKind::NativeStaging
                || saved.template.repository != self.repository
                || saved.permission.member != self.staging_stamp.identity()
                || saved.permission.database.is_some()
                || saved.permission.manifest.is_some()
            {
                return Err(invalid(Problem::Changed));
            }
            self.job = saved.permission.job;
            saved
        } else {
            let Some((mode, generation)) = request else {
                return Ok(());
            };
            let zero = FileIdentity::from_fixed_words(0, 0);
            let mut cache = [0; 256];
            cache[..14].copy_from_slice(b"native-staging");
            super::maintenance_legacy_guard::SourceCreationCapsule {
                mode,
                template: Binding {
                    kind: LegacyScopeKind::NativeStaging,
                    root: root.identity(),
                    gate: zero,
                    state: zero,
                    owner: zero,
                    nonce: self.job,
                    owner_nonce: [0; 32],
                    key: [0; 64],
                    database: [0; 256],
                    database_len: 0,
                    cache,
                    cache_len: 14,
                    repository: self.repository,
                },
                permission: Record {
                    phase: LegacyPhase::Active,
                    version: 1,
                    member: self.staging_stamp.identity(),
                    database: None,
                    manifest: None,
                    cache: self.staging_stamp.identity(),
                    root_use: zero,
                    confirmation: self.binding.digest(),
                    job: self.job,
                    member_name: [0; 128],
                    member_len: 0,
                    freeze_cache: false,
                    operator_generation: generation,
                    operator_pending: true,
                    pending_source_stamp: Some(self.staging_stamp),
                    root_default_source: None,
                },
            }
        };
        self.birth = Some(PreparedSourceBirth::prepare(
            &self.root,
            &self.paths,
            capsule,
            memory,
        )?);
        Ok(())
    }
}

fn prepare_empty_runs(
    root: &Path,
    memory: &CleanupMemory,
) -> Result<(PathBuf, Entry), FixedControlError> {
    let _floor = memory
        .reserve(16384 + root.as_os_str().as_encoded_bytes().len() * 2)
        .map_err(FixedControlError::Resource)?;
    let path = root.join("runs");
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    let entry = Entry::capture(&path, Kind::Directory, true, &mut scratch)?;
    if let Some(stamp) = entry.stamp {
        let directory = RootedDirectory::open_readonly_root(&path, stamp.identity())?;
        let mut bytes = [0; 4096];
        let mut nonempty = false;
        directory.visit_entries(&mut bytes, &mut |_| {
            nonempty = true;
            Ok(())
        })?;
        stamp.recheck(&directory.pin_handle()?)?;
        if nonempty {
            return Err(invalid(Problem::Busy));
        }
    }
    Ok((path, entry))
}

impl PreparedLegacyScope {
    pub(crate) fn publication_recovery_pending(&self) -> bool {
        self.observation
            .as_ref()
            .is_some_and(|o| o.recovery.is_some())
    }
    pub(crate) fn refresh_publication_recovery(
        &mut self,
        memory: &CleanupMemory,
        owner: Option<&LegacySourceOwner>,
    ) -> Result<bool, FixedControlError> {
        self.observation = PreparedSourceObservation::read(
            &self.root,
            &self.paths,
            owner.map(|o| o.owner.file()),
            memory,
        )?;
        Ok(self.publication_recovery_pending())
    }
    pub(crate) fn recover_publication(
        &mut self,
        owner: Option<&mut LegacySourceOwner>,
        grant: &mut MutationGrant,
        memory: &CleanupMemory,
    ) -> Result<bool, FixedControlError> {
        let Some(plan) = self.observation.as_ref().and_then(|o| o.recovery.as_ref()) else {
            return Ok(false);
        };
        if !self.binding.is_current()? {
            return Err(invalid(Problem::Changed));
        }
        let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
        grant.take_entries(12)?;
        let root = RootedDirectory::open_fixed(
            &self.root,
            plan.before.binding.root,
            true,
            &mut self.scratch,
        )?;
        let writer_file = checked_entry(&self.writer, self.entries[6], &mut self.scratch, grant)?;
        let _writer = FixedLockedFile::try_lock(writer_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        let gate_file =
            locks::open_fixed_control(&self.paths[0], Kind::File, true, &mut self.scratch)?;
        let gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        // Existing pair/cache locks only: recovery cannot create/adopt a missing namespace.
        let mut held = [None, None, None];
        for (slot, path) in held
            .iter_mut()
            .zip([&self.family, &self.native, &self.root_use])
        {
            match locks::open_fixed_control(path, Kind::File, true, &mut self.scratch) {
                Ok(file) => {
                    *slot = Some(
                        FixedLockedFile::try_lock(file, LockMode::Exclusive)?
                            .ok_or_else(|| invalid(Problem::Busy))?,
                    )
                }
                Err(FixedControlError::Io(e))
                    if e.kind() == std::io::ErrorKind::NotFound
                        && plan.after.phase == LegacyPhase::Removed => {}
                Err(e) => return Err(e),
            }
        }
        recover_with_owner(
            &root,
            &gate,
            &self.paths,
            plan,
            owner,
            &mut self.scratch,
            grant,
        )
    }
}
impl PreparedLegacyStagingScope {
    pub(crate) fn publication_recovery_pending(&self) -> bool {
        self.observation
            .as_ref()
            .is_some_and(|o| o.recovery.is_some())
    }
    pub(crate) fn refresh_publication_recovery(
        &mut self,
        memory: &CleanupMemory,
        owner: Option<&LegacySourceOwner>,
    ) -> Result<bool, FixedControlError> {
        self.observation = PreparedSourceObservation::read(
            &self.root,
            &self.paths,
            owner.map(|o| o.owner.file()),
            memory,
        )?;
        Ok(self.publication_recovery_pending())
    }
    pub(crate) fn recover_publication(
        &mut self,
        owner: Option<&mut LegacySourceOwner>,
        grant: &mut MutationGrant,
        memory: &CleanupMemory,
    ) -> Result<bool, FixedControlError> {
        let Some(plan) = self.observation.as_ref().and_then(|o| o.recovery.as_ref()) else {
            return Ok(false);
        };
        if !self.binding.is_current()? {
            return Err(invalid(Problem::Changed));
        }
        let _floor = memory.reserve(24576).map_err(FixedControlError::Resource)?;
        grant.take_entries(12)?;
        let root = RootedDirectory::open_fixed(
            &self.root,
            plan.before.binding.root,
            true,
            &mut self.scratch,
        )?;
        let staging = RootedDirectory::open_fixed(
            &self.staging,
            self.staging_stamp.identity(),
            true,
            &mut self.scratch,
        )?;
        if staging.identity() != plan.before.record.member {
            return Err(invalid(Problem::Changed));
        }
        let gate_file =
            locks::open_fixed_control(&self.paths[0], Kind::File, true, &mut self.scratch)?;
        let gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        recover_with_owner(
            &root,
            &gate,
            &self.paths,
            plan,
            owner,
            &mut self.scratch,
            grant,
        )
    }
}
#[inline(never)]
fn recover_with_owner(
    root: &RootedDirectory,
    gate: &FixedLockedFile,
    paths: &[PathBuf; 3],
    plan: &super::maintenance_legacy_guard::PreparedSourceRecovery,
    owner: Option<&mut LegacySourceOwner>,
    scratch: &mut FixedControlScratch,
    grant: &mut MutationGrant,
) -> Result<bool, FixedControlError> {
    grant.take_entries(4)?;
    let state = locks::open_fixed_control(&paths[1], Kind::File, true, scratch)?;
    let temporary;
    let held = if let Some(owner) = owner.as_ref() {
        if owner.binding != plan.before.binding
            || owner.commit != plan.before.commit
            || owner.record != plan.before.record
        {
            return Err(invalid(Problem::Changed));
        }
        &owner.owner
    } else {
        let file = locks::open_fixed_control(&paths[2], Kind::File, true, scratch)?;
        temporary = FixedLockedFile::try_lock(file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        &temporary
    };
    plan.apply(root, gate, held, &state, grant)?;
    if let Some(owner) = owner {
        owner.commit = plan.commit;
        owner.record = plan.after;
        owner.state_stamp = FixedFileStamp::capture(&state, Kind::File)?;
        owner.pending_publication = false;
    }
    Ok(true)
}

#[cfg(test)]
#[path = "maintenance_legacy_source_recovery_tests.rs"]
mod source_recovery_tests;

#[cfg(test)]
mod cohort_preparation_layout_tests {
    use super::*;
    #[test]
    fn accepted_administrative_cohort_keeps_binding_image_out_of_fixed_owner_layout() {
        // A small enrollment-only owner crosses preparation stages. Original
        // inline binding copies exceeded this bound; its image stays charged.
        assert!(std::mem::size_of::<AcceptedLegacyRootCohort>() <= 2048);
        let _: fn(AcceptedLegacyRootCohort) = |cohort| {
            let _: std::cell::RefCell<
                super::super::maintenance_budget::MemoryBox<PreparedTargetBinding>,
            > = cohort.target_binding;
        };
    }
}

#[cfg(test)]
mod retained_source_observation_layout_tests {
    use super::*;
    #[test]
    fn retained_source_observer_keeps_snapshot_image_out_of_fixed_owner_layout() {
        // Measured fixed owner is224B locally; keep its actual source facts in
        // charged storage without imposing relative sizes on other protocols.
        assert!(std::mem::size_of::<PreparedSourceObservation>() <= 512);
        let _: fn(PreparedSourceObservation) = |observation| {
            let _: super::super::maintenance_budget::MemoryBox<
                super::super::maintenance_legacy_guard::Observation,
            > = observation.observed;
        };
    }
}

#[cfg(test)]
mod staging_scope_layout_tests {
    use super::*;
    #[test]
    fn prepared_staging_scope_keeps_binding_and_scratch_images_boxed() {
        assert!(std::mem::size_of::<PreparedLegacyStagingScope>() <= 2048);
        let _: fn(PreparedLegacyStagingScope) = |scope| {
            let _: super::super::maintenance_budget::MemoryBox<PreparedTargetBinding> =
                scope.binding;
            let _: super::super::maintenance_budget::MemoryBox<FixedControlScratch> = scope.scratch;
        };
    }
}
