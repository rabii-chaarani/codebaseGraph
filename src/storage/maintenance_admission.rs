//! Bounded mutation admission. Prepared values own advisory facts/buffers only;
//! actual non-Send guards exist solely during a borrowed hot-frame callback.
use super::inventory::MaintenanceTarget;
use super::layout::{DirectLayout, ManagedLayout};
use super::locks::{
    open_fixed_control, FixedControlError, FixedControlKind as Kind,
    FixedControlProblem as Problem, FixedControlScratch, FixedFileStamp, FixedLockedFile, LockMode,
    RootedDirectory,
};
use super::maintenance_budget::{
    CleanupMemory, MemoryBox, MemoryReservation, MutationGrant, ReservedBytes,
};
use super::maintenance_ownership::{
    self as ownership, CanonicalEpochAdvance, EpochLock, PreparedEpoch, ReferenceDomain,
    ReferenceEpoch,
};
use super::maintenance_projection::{project_binding, BoundConfiguration, ProjectionFailure};
use std::path::{Path, PathBuf};

pub(crate) type AdmissionError = FixedControlError;
pub(crate) type EpochAdvance = CanonicalEpochAdvance;
fn invalid(problem: Problem) -> AdmissionError {
    AdmissionError::Invalid(problem)
}
fn projection(error: ProjectionFailure) -> AdmissionError {
    match error {
        ProjectionFailure::Resource(error) => AdmissionError::Resource(error),
        _ => invalid(Problem::InvalidRecord),
    }
}
#[derive(Clone)]
struct Entry {
    path: PathBuf,
    kind: Kind,
    stamp: Option<FixedFileStamp>,
}
impl Entry {
    fn prepare(
        path: PathBuf,
        kind: Kind,
        optional: bool,
        scratch: &mut FixedControlScratch,
    ) -> Result<Self, AdmissionError> {
        let stamp = match open_fixed_control(&path, kind, false, scratch) {
            Ok(file) => Some(FixedFileStamp::capture(&file, kind)?),
            Err(AdmissionError::Io(error))
                if optional && error.kind() == std::io::ErrorKind::NotFound =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        Ok(Self { path, kind, stamp })
    }
    fn check(
        &self,
        scratch: &mut FixedControlScratch,
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        check_entry(&self.path, self.kind, self.stamp, scratch, grant)
    }
    fn lock(
        &self,
        scratch: &mut FixedControlScratch,
        grant: &mut MutationGrant,
    ) -> Result<FixedLockedFile, AdmissionError> {
        grant.take_entry()?;
        let file = open_fixed_control(&self.path, Kind::File, true, scratch)?;
        self.stamp
            .ok_or_else(|| invalid(Problem::InvalidRecord))?
            .recheck(&file)?;
        FixedLockedFile::try_lock(file, LockMode::Exclusive)?.ok_or_else(|| invalid(Problem::Busy))
    }
}
fn check_entry(
    path: &Path,
    kind: Kind,
    stamp: Option<FixedFileStamp>,
    scratch: &mut FixedControlScratch,
    grant: &mut MutationGrant,
) -> Result<(), AdmissionError> {
    grant.take_entry()?;
    check_entry_identity(path, kind, stamp, scratch)
}
fn check_entry_identity(
    path: &Path,
    kind: Kind,
    stamp: Option<FixedFileStamp>,
    scratch: &mut FixedControlScratch,
) -> Result<(), AdmissionError> {
    match (open_fixed_control(path, kind, false, scratch), stamp) {
        (Ok(file), Some(stamp)) => stamp.recheck(&file),
        (Err(AdmissionError::Io(error)), None) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(())
        }
        (Err(error), _) => Err(error),
        _ => Err(invalid(Problem::Changed)),
    }
}

/// OWNED and movable preparation. No self references or storage authority.
pub(crate) struct PreparedCanonical {
    epoch: PreparedEpoch,
    binding: BoundConfiguration,
    repository: Entry,
    state: Entry,
    locks: [Option<Entry>; 10],
    selector_index: usize,
    root: Entry,
    destination: Entry,
    publisher_binding: Entry,
    origin: RootOriginBinding,
    workers: Entry,
    phases: Entry,
    worker_state: Entry,
    direct: bool,
    read: ReservedBytes,
    scratch: FixedControlScratch,
    directory_scratch: [u8; 4096],
    _floor: MemoryReservation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootOriginBinding {
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) coordination: super::locks::FileIdentity,
    pub(crate) incarnation: [u8; 32],
    pub(crate) cohort: super::locks::FileIdentity,
    pub(crate) publication_binding: super::locks::FileIdentity,
}
pub(crate) struct CanonicalMutation<'a> {
    directory: &'a RootedDirectory,
    root_use: &'a FixedLockedFile,
    selector: &'a FixedLockedFile,
    origin: RootOriginBinding,
    advance: EpochAdvance,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl CanonicalMutation<'_> {
    pub(crate) fn directory(&self) -> &RootedDirectory {
        self.directory
    }
    pub(crate) fn origin_scope(&self) -> RootOriginBinding {
        self.origin
    }
    pub(crate) fn epoch(&self) -> EpochAdvance {
        self.advance
    }
}

/// Capture E BEFORE complete reference/origin preparation. The caller owns that
/// complete reference result; this seal binds its same immutable E to current
/// configuration, physical namespace and constant-count lock/registry checks.
pub(crate) fn prepare_canonical(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<PreparedCanonical, AdmissionError> {
    let binding = project_binding(target, memory).map_err(projection)?;
    let floor = memory
        .reserve(
            super::maintenance_projection::path_charge(
                [
                    Some(target.repository_root.as_path()),
                    Some(target.state_root.as_path()),
                    Some(binding.storage.as_path()),
                    Some(binding.config_path.as_path()),
                    Some(domain.artifact_root()),
                    binding.direct.as_ref().map(|p| p.database.as_path()),
                    binding.direct.as_ref().map(|p| p.manifest.as_path()),
                ]
                .into_iter()
                .flatten(),
                32,
                std::mem::size_of::<PreparedCanonical>(),
            )
            .map_err(AdmissionError::Resource)?,
        )
        .map_err(AdmissionError::Resource)?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    let mut read = memory
        .reserve(2048)
        .map_err(AdmissionError::Resource)?
        .bytes(2048)
        .map_err(AdmissionError::Resource)?;
    let (actual, worker, worker_state, workers, phases, state_lock, direct) =
        if let Some(pair) = &binding.direct {
            let layout = DirectLayout::new(&pair.database, &pair.manifest);
            (
                ReferenceDomain::direct(&layout),
                layout.worker_lock_path(),
                layout.worker_state_path(),
                layout.worker_workspace_root_path(),
                layout.phase_workspace_root(),
                None,
                true,
            )
        } else {
            let layout = ManagedLayout::new(&binding.storage);
            (
                ReferenceDomain::managed(&layout),
                layout.worker_lock_path(),
                layout.worker_state_path(),
                binding.storage.join("workers"),
                layout.phase_workspace_root(),
                Some(layout.state_lock_path()),
                false,
            )
        };
    let actual = actual.map_err(|_| invalid(Problem::InvalidRecord))?;
    if !actual.same_scope(domain) {
        return Err(invalid(Problem::Changed));
    }
    let epoch = PreparedEpoch::prepare(domain, expected, memory, &mut read, &mut scratch)?;
    let frame = epoch.frame();
    let mut locks: [Option<Entry>; 10] = std::array::from_fn(|_| None);
    locks[0] = Some(Entry::prepare(
        frame.paths.writer.to_owned(),
        Kind::File,
        false,
        &mut scratch,
    )?);
    locks[1] = Some(Entry::prepare(
        frame.paths.root_use.to_owned(),
        Kind::File,
        false,
        &mut scratch,
    )?);
    locks[2] = Some(Entry::prepare(worker, Kind::File, false, &mut scratch)?);
    let mut count = 3;
    if let Some(path) = state_lock {
        locks[count] = Some(Entry::prepare(path, Kind::File, false, &mut scratch)?);
        count += 1;
    }
    if let Some(pair) = &binding.direct {
        for resource in [&pair.database, &pair.manifest] {
            let (parent, basename) = ownership::database_lock_path(resource, false)
                .map_err(|_| invalid(Problem::InvalidPath))?;
            for path in [parent.join(".database-use-family.lock"), basename] {
                let entry = Entry::prepare(path, Kind::File, false, &mut scratch)?;
                if locks
                    .iter()
                    .flatten()
                    .any(|old| old.stamp.map(|s| s.identity()) == entry.stamp.map(|s| s.identity()))
                {
                    continue;
                }
                locks[count] = Some(entry);
                count += 1;
            }
        }
    }
    let destination = Entry::prepare(
        frame.paths.destination.to_owned(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let publisher_binding = Entry::prepare(
        frame.paths.root.join(".publication.binding"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    let (root_id, coordination, incarnation, cohort) = epoch.root_facts();
    let mut binding_file =
        open_fixed_control(&publisher_binding.path, Kind::File, false, &mut scratch)?;
    let binding_stamp = publisher_binding.stamp.unwrap();
    let len = super::locks::read_fixed_control(&mut binding_file, &binding_stamp, &mut read)?;
    let publisher = super::maintenance_controls::PublisherBinding::decode(&read.as_slice()[..len])
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    if publisher.root != root_id
        || publisher.coordination != coordination
        || publisher.incarnation != incarnation
    {
        return Err(invalid(Problem::Changed));
    }
    let selector = Entry::prepare(
        frame.paths.root.join(".publication.selector"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    if selector.stamp.unwrap().identity() != publisher.selector {
        return Err(invalid(Problem::Changed));
    }
    locks[count] = Some(selector);
    let origin = RootOriginBinding {
        root: root_id,
        coordination,
        incarnation,
        cohort,
        publication_binding: binding_stamp.identity(),
    };
    let root = Entry::prepare(
        frame.paths.root.to_owned(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let repository = Entry::prepare(
        target.repository_root.clone(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let state = Entry::prepare(
        target.state_root.clone(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let workers = Entry::prepare(workers, Kind::Directory, true, &mut scratch)?;
    let phases = Entry::prepare(phases, Kind::Directory, true, &mut scratch)?;
    let worker_state = Entry::prepare(worker_state, Kind::File, true, &mut scratch)?;
    // A descriptor is process authority, not an inert file. Until its bounded
    // projection/kernel proof is supplied, retain it rather than infer idle.
    if worker_state.stamp.is_some() {
        return Err(invalid(Problem::Busy));
    }
    Ok(PreparedCanonical {
        epoch,
        binding,
        repository,
        state,
        locks,
        selector_index: count,
        root,
        destination,
        publisher_binding,
        origin,
        workers,
        phases,
        worker_state,
        direct,
        read,
        scratch,
        directory_scratch: [0; 4096],
        _floor: floor,
    })
}

/// Fresh grant starts in the real dispatcher BEFORE this call. `effect_entries`
/// reserves the ENTIRE known callback effect cost before E changes. Each effect
/// uses check_time; it must not try to renew the grant or park this authority.
pub(crate) fn with_prepared_canonical(
    prepared: &mut PreparedCanonical,
    grant: &mut MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(&CanonicalMutation<'_>, &mut MutationGrant) -> Result<(), AdmissionError>,
) -> Result<EpochAdvance, AdmissionError> {
    grant.check()?;
    let PreparedCanonical {
        epoch,
        binding,
        repository,
        state,
        locks,
        selector_index,
        root,
        destination,
        publisher_binding,
        origin,
        workers,
        phases,
        worker_state,
        direct,
        read,
        scratch,
        directory_scratch,
        ..
    } = prepared;
    let mut held: [Option<FixedLockedFile>; 10] = std::array::from_fn(|_| None);
    for (index, entry) in locks.iter().enumerate() {
        if let Some(entry) = entry {
            held[index] = Some(entry.lock(scratch, grant)?);
        }
    }
    let root_identity = root
        .stamp
        .ok_or_else(|| invalid(Problem::InvalidRecord))?
        .identity();
    grant.take_entry()?;
    let directory = RootedDirectory::open_fixed(&root.path, root_identity, true, scratch)?;
    grant.take_entry()?;
    let _destination_directory = RootedDirectory::open_fixed(
        &destination.path,
        destination.stamp.unwrap().identity(),
        true,
        scratch,
    )?;
    let frame = epoch.frame();
    ownership::with_canonical_epoch_handles(
        &frame,
        EpochLock::Fixed(held[0].as_ref().unwrap()),
        EpochLock::Fixed(held[1].as_ref().unwrap()),
        read,
        scratch,
        grant,
        effect_entries,
        &mut |scratch, grant| {
            repository.check(scratch, grant)?;
            state.check(scratch, grant)?;
            if repository.stamp.map(|s| s.identity()) != Some(binding.repository)
                || state.stamp.map(|s| s.identity()) != Some(binding.state)
            {
                return Err(invalid(Problem::Changed));
            }
            check_entry(
                &binding.config_path,
                Kind::File,
                binding.config_stamp,
                scratch,
                grant,
            )?;
            publisher_binding.check(scratch, grant)?;
            worker_state.check(scratch, grant)?;
            for entry in locks.iter().flatten() {
                entry.check(scratch, grant)?;
            }
            workers.check(scratch, grant)?;
            phases.check(scratch, grant)?;
            if let Some(stamp) = workers.stamp {
                grant.take_entry()?;
                let worker_directory =
                    RootedDirectory::open_fixed(&workers.path, stamp.identity(), false, scratch)?;
                if !worker_directory.contains_only(
                    if *direct && phases.stamp.is_some() {
                        Some(b"phases")
                    } else {
                        None
                    },
                    directory_scratch,
                    grant,
                )? {
                    return Err(invalid(Problem::Busy));
                }
            }
            if let Some(stamp) = phases.stamp {
                grant.take_entry()?;
                let phase_directory =
                    RootedDirectory::open_fixed(&phases.path, stamp.identity(), false, scratch)?;
                if !phase_directory.contains_only(None, directory_scratch, grant)? {
                    return Err(invalid(Problem::Busy));
                }
            }
            Ok(())
        },
        &mut |advance, grant| {
            apply(
                &CanonicalMutation {
                    directory: &directory,
                    root_use: held[1].as_ref().unwrap(),
                    selector: held[*selector_index].as_ref().unwrap(),
                    origin: *origin,
                    advance,
                    _same_thread: std::marker::PhantomData,
                },
                grant,
            )
        },
    )
}

pub(crate) fn observe_maintenance_binding(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<[u8; 32], AdmissionError> {
    let mut binding = prepare_target_binding(target, memory)?;
    if binding.is_legacy() {
        return Err(invalid(Problem::InvalidRecord));
    }
    binding.is_current()?;
    Ok(binding.digest())
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn private_metadata_owner_keeps_preparation_images_in_charged_boxes() {
        // The former inline scratch/epoch/config owner exceeded this fixed
        // boundary; the shipping boxed factory must keep its result compact.
        assert!(std::mem::size_of::<super::PreparedPrivateMetadata>() <= 2048);
        let _boxed_fields = |value: &super::PreparedPrivateMetadata| {
            let _: &super::MemoryBox<super::FixedControlScratch> = &value.scratch;
            let _: &super::MemoryBox<super::PreparedEpoch> = &value._epoch;
            let _: &super::MemoryBox<super::BoundConfiguration> = &value._binding;
        };
    }
    use super::super::layout::{create_fresh_reference_root, managed_generation_id};
    use super::super::locks::{open_locked, try_open_existing_locked};
    use super::super::maintenance_ownership::{
        initialize_fresh_reference_domain, observe_reference_epoch, EpochObservation,
        ReferenceMutation,
    };
    use super::*;
    use std::fs;
    pub(crate) struct Fixture {
        root: PathBuf,
        pub(crate) target: MaintenanceTarget,
        pub(crate) layout: ManagedLayout,
        pub(crate) domain: ReferenceDomain,
        pub(crate) memory: CleanupMemory,
    }
    impl Fixture {
        pub(crate) fn new() -> Self {
            let root = std::env::temp_dir().join(format!("admit-{}", managed_generation_id()));
            fs::create_dir(&root).unwrap();
            let root = fs::canonicalize(root).unwrap();
            let state = root.join(".codebaseGraph");
            fs::create_dir(&state).unwrap();
            fs::write(state.join("config.json"), br#"{"schema_version":3}"#).unwrap();
            let layout = ManagedLayout::new(state.join("storage"));
            let fresh = create_fresh_reference_root(layout.storage_root())
                .unwrap()
                .unwrap();
            layout.ensure_roots().unwrap();
            fs::create_dir(layout.storage_root().join("workers")).unwrap();
            fs::create_dir(layout.phase_workspace_root()).unwrap();
            for path in [
                layout.writer_lock_path(),
                layout.worker_lock_path(),
                layout.state_lock_path(),
            ] {
                drop(open_locked(path, LockMode::Exclusive).unwrap());
            }
            let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
            let domain = ReferenceDomain::managed(&layout).unwrap();
            assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
            drop(writer);
            Self {
                target: MaintenanceTarget {
                    repository_root: root.clone(),
                    state_root: state,
                    storage_root: None,
                    direct_pair: None,
                    config_path: None,
                },
                root,
                layout,
                domain,
                memory: CleanupMemory::new(8 * 1024 * 1024).unwrap(),
            }
        }
        pub(crate) fn epoch(&self) -> ReferenceEpoch {
            match observe_reference_epoch(&self.domain).unwrap() {
                EpochObservation::Verified(epoch) => epoch,
                other => panic!("{other:?}"),
            }
        }
        pub(crate) fn prepared(&self) -> PreparedCanonical {
            prepare_canonical(&self.target, &self.domain, &self.epoch(), &self.memory).unwrap()
        }
        pub(crate) fn unlocked(&self) {
            for path in [
                self.layout.writer_lock_path(),
                self.layout.worker_lock_path(),
                self.layout.state_lock_path(),
            ] {
                assert!(try_open_existing_locked(&path, LockMode::Exclusive)
                    .unwrap()
                    .is_some());
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn simultaneous_preparations_fit_one_account_without_maximum_path_overcharges() {
        let f = Fixture::new();
        let before = f.memory.used();
        let witness = prepare_target_binding(&f.target, &f.memory).unwrap();
        let duplicate = witness.duplicate(&f.memory).unwrap();
        let first = f.prepared();
        let second = f.prepared();
        let metadata =
            prepare_private_metadata(&f.target, &first.origin_scope(), &f.memory).unwrap();
        assert!(f.memory.used() <= f.memory.limit());
        drop((metadata, second, first, duplicate, witness));
        assert_eq!(f.memory.used(), before);
    }
    #[test]
    fn legacy_request_source_replacement_invalidates_confirmation_without_rebinding_runtime() {
        let f = Fixture::new();
        let pair = super::super::inventory::default_direct(&f.target);
        fs::write(&pair.database, b"old database").unwrap();
        fs::write(&pair.manifest, b"old manifest").unwrap();
        fs::create_dir(f.target.state_root.join("artifacts")).unwrap();
        let target_before = prepare_target_binding(&f.target, &f.memory)
            .unwrap()
            .digest();
        let requested = prepare_legacy_confirmation_source(&f.target, &f.memory).unwrap();
        assert_eq!(
            requested.facts(),
            capture_legacy_confirmation_source(&f.target, &f.memory).unwrap()
        );
        let replacement = f.target.state_root.join("new-graph");
        fs::write(&replacement, b"new database").unwrap();
        fs::rename(replacement, &pair.database).unwrap();
        let mut scratch = FixedControlScratch::new(
            f.memory
                .reserve(FixedControlScratch::required_bytes())
                .unwrap(),
        )
        .unwrap();
        assert!(requested
            .recheck(
                &mut scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2))
            )
            .is_err());
        assert_ne!(
            requested.facts(),
            capture_legacy_confirmation_source(&f.target, &f.memory).unwrap()
        );
        // An explicit Direct rebuild need not change the Managed runtime. The
        // independently queued source witness still rejects the new source.
        assert_eq!(
            target_before,
            prepare_target_binding(&f.target, &f.memory)
                .unwrap()
                .digest()
        );
        assert_ne!(
            super::super::locks::file_identity(requested.pins[0].as_ref().unwrap()).unwrap(),
            super::super::locks::path_identity(&pair.database).unwrap()
        );
    }
    #[test]
    fn legacy_request_source_missing_cache_is_noncreating_and_cannot_approve_later_birth() {
        let f = Fixture::new();
        let pair = super::super::inventory::default_direct(&f.target);
        let cache = f.target.state_root.join("artifacts");
        let requested = prepare_legacy_confirmation_source(&f.target, &f.memory).unwrap();
        assert!(!pair.database.exists());
        assert!(!pair.manifest.exists());
        assert!(!cache.exists());
        assert_eq!(requested.facts().database, None);
        assert_eq!(requested.facts().caches[0], None);
        fs::create_dir(&cache).unwrap();
        let mut scratch = FixedControlScratch::new(
            f.memory
                .reserve(FixedControlScratch::required_bytes())
                .unwrap(),
        )
        .unwrap();
        assert!(requested
            .recheck(
                &mut scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2))
            )
            .is_err());
        assert_ne!(
            requested.facts(),
            capture_legacy_confirmation_source(&f.target, &f.memory).unwrap()
        );
    }
    #[test]
    fn private_metadata_composes_actual_canonical_root_and_selector_without_reacquisition() {
        let f = Fixture::new();
        let mut canonical = f.prepared();
        let origin = canonical.origin_scope();
        let mut metadata = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        let update = prepare_metadata_control(
            &origin,
            metadata.committed().unwrap(),
            b"canonical-composed",
            &f.memory,
        )
        .unwrap();
        let mut called = false;
        with_prepared_canonical(
            &mut canonical,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2)),
            update.effects(),
            &mut |authority, grant| {
                assert_eq!(authority.root_use.mode(), LockMode::Exclusive);
                assert_eq!(authority.selector.mode(), LockMode::Exclusive);
                with_reserved_canonical_private_metadata(
                    &mut metadata,
                    authority,
                    grant,
                    update.effects(),
                    &mut |mutation, grant| {
                        called = true;
                        mutation.publish_control(&update, grant)
                    },
                )
            },
        )
        .unwrap();
        assert!(called);
        f.unlocked();
        let current = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        assert_eq!(current.payload(), Some(&b"canonical-composed"[..]));
    }

    #[test]
    fn canonical_metadata_composition_rejects_another_root_before_callback() {
        let f = Fixture::new();
        let other = Fixture::new();
        let mut canonical = f.prepared();
        let other_origin = other.prepared().origin_scope();
        let mut metadata =
            prepare_private_metadata(&other.target, &other_origin, &other.memory).unwrap();
        let before = metadata.committed();
        let mut called = false;
        let result = with_prepared_canonical(
            &mut canonical,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2)),
            0,
            &mut |authority, grant| {
                with_reserved_canonical_private_metadata(
                    &mut metadata,
                    authority,
                    grant,
                    0,
                    &mut |_, _| {
                        called = true;
                        Ok(())
                    },
                )
            },
        );
        assert!(result.is_err());
        assert!(!called);
        assert_eq!(metadata.committed(), before);
        assert_eq!(other.epoch().counter(), 0);
        f.unlocked();
        other.unlocked();
    }

    #[test]
    fn private_metadata_composes_actual_owned_root_without_reacquisition() {
        let f = Fixture::new();
        let origin = f.prepared().origin_scope();
        let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        let path = prepared.root_use.path.clone();
        let before = prepared.committed().unwrap();
        let update = prepare_metadata_control(&origin, before, b"owned-root", &f.memory).unwrap();
        let shared_file =
            open_fixed_control(&path, Kind::File, false, &mut prepared.scratch).unwrap();
        let shared = FixedLockedFile::try_lock(shared_file, LockMode::Shared)
            .unwrap()
            .unwrap();
        let mut called = false;
        assert!(with_reserved_private_metadata_owned_root(
            &mut prepared,
            &shared,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2)),
            update.effects(),
            &mut |_, _| {
                called = true;
                Ok(())
            }
        )
        .is_err());
        assert!(!called);
        drop(shared);
        let file = open_fixed_control(&path, Kind::File, false, &mut prepared.scratch).unwrap();
        let exclusive = FixedLockedFile::try_lock(file, LockMode::Exclusive)
            .unwrap()
            .unwrap();
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        grant.take_entries(update.effects()).unwrap();
        with_reserved_private_metadata_owned_root(
            &mut prepared,
            &exclusive,
            &mut grant,
            update.effects(),
            &mut |mutation, grant| mutation.publish_control(&update, grant),
        )
        .unwrap();
        // The callback borrows the actual lease; it remains owned afterward.
        assert_eq!(exclusive.mode(), LockMode::Exclusive);
        assert!(try_open_existing_locked(&path, LockMode::Exclusive)
            .unwrap()
            .is_none());
        drop(exclusive);
        let refreshed = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        assert_eq!(refreshed.committed(), Some(update.next()));
        assert_eq!(f.epoch().counter(), 0);
    }
    #[test]
    fn private_metadata_stages_over_one_hundred_frames_without_advancing_source_epoch() {
        let f = Fixture::new();
        let origin = f.prepared().origin_scope();
        let producer = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        let source_epoch = 1;
        for index in 0u64..101 {
            let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
            let before = prepared.committed().unwrap();
            let offset = prepared.stream_len();
            let intent =
                prepare_metadata_control(&origin, before, &index.to_le_bytes(), &f.memory).unwrap();
            let ready =
                prepare_metadata_control(&origin, intent.next(), &index.to_le_bytes(), &f.memory)
                    .unwrap();
            with_private_metadata(
                &mut prepared,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(2)),
                14,
                &mut |mutation, grant| {
                    mutation.publish_control(&intent, grant)?;
                    mutation.append_frame(offset, &index.to_le_bytes(), grant)?;
                    mutation.publish_control(&ready, grant)
                },
            )
            .unwrap();
        }
        drop(producer);
        assert_eq!(f.epoch().counter(), source_epoch);
        let prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        assert_eq!(prepared.stream_len(), 808);
        assert_eq!(prepared.committed().unwrap().generation, 202);
        assert_eq!(
            std::fs::read(f.layout.artifacts_root().join(".publication.stream"))
                .unwrap()
                .len(),
            808
        );
    }
    #[test]
    fn private_metadata_resumes_each_durable_control_cut_from_same_bound_files() {
        for cut in 1..=3 {
            let f = Fixture::new();
            let origin = f.prepared().origin_scope();
            let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
            let update = prepare_metadata_control(
                &origin,
                prepared.committed().unwrap(),
                b"exact intended metadata",
                &f.memory,
            )
            .unwrap();
            METADATA_WRITE_CUT.with(|value| *value.borrow_mut() = Some(cut));
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = with_private_metadata(
                    &mut prepared,
                    &mut MutationGrant::start(),
                    6,
                    &mut |mutation, grant| mutation.publish_control(&update, grant),
                );
            }));
            assert!(outcome.is_err());
            f.unlocked();
            let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
            if let Some(resume) = prepare_metadata_recovery(&prepared, &f.memory).unwrap() {
                with_private_metadata(
                    &mut prepared,
                    &mut MutationGrant::start(),
                    resume.effects(),
                    &mut |mutation, grant| mutation.publish_control(&resume, grant),
                )
                .unwrap();
            }
            let prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
            assert_eq!(prepared.committed(), Some(update.next()));
            assert_eq!(
                prepared.payload(),
                Some(b"exact intended metadata".as_slice())
            );
            assert_eq!(f.epoch().counter(), 0);
        }
    }
    #[test]
    fn private_metadata_partial_frame_and_reset_preserve_unknown_bytes() {
        use std::io::Write;
        let f = Fixture::new();
        let origin = f.prepared().origin_scope();
        let path = f.layout.artifacts_root().join(".publication.stream");
        let frame = [7u8; 512];
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all(&frame[..39]).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        with_private_metadata(
            &mut prepared,
            &mut MutationGrant::start(),
            3,
            &mut |mutation, grant| mutation.append_frame(0, &frame, grant),
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), frame);
        let mut prepared = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"unknown tail").unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert!(with_private_metadata(
            &mut prepared,
            &mut MutationGrant::start(),
            2,
            &mut |mutation, grant| mutation.reset_stream(512, grant)
        )
        .is_err());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 524);
        // Unknown bytes remain untouched. Only a separate fully verified
        // owned prefix is eligible for S3's durable ResetIntent.
        let clean = Fixture::new();
        let clean_origin = clean.prepared().origin_scope();
        let clean_path = clean.layout.artifacts_root().join(".publication.stream");
        std::fs::write(&clean_path, frame).unwrap();
        let mut clean_prepared =
            prepare_private_metadata(&clean.target, &clean_origin, &clean.memory).unwrap();
        with_private_metadata(
            &mut clean_prepared,
            &mut MutationGrant::start(),
            2,
            &mut |mutation, grant| mutation.reset_stream(512, grant),
        )
        .unwrap();
        assert_eq!(std::fs::metadata(clean_path).unwrap().len(), 0);
        assert_eq!(f.epoch().counter(), 0);
        assert_eq!(std::fs::metadata(path).unwrap().len(), 524);
    }
    #[test]
    fn target_binding_duplicate_retains_actual_legacy_objects_and_same_challenge() {
        let f = Fixture::new();
        let root = f.layout.artifacts_root();
        let coordination = root.join(".use.lock");
        std::fs::write(&coordination, b"").unwrap();
        let mut witness = prepare_target_binding(&f.target, &f.memory).unwrap();
        assert!(witness.is_legacy());
        let mut duplicate = witness.duplicate(&f.memory).unwrap();
        assert_eq!(witness.digest(), duplicate.digest());
        assert!(witness.is_current().unwrap());
        std::fs::write(&coordination, b"different generation").unwrap();
        assert!(witness.is_current().is_err());
        assert!(duplicate.is_current().is_err());
        drop(witness);
        assert!(duplicate.is_current().is_err());
    }

    #[test]
    fn local_epoch_invalidates_current_counter_while_unrelated_shared_producer_stays_live() {
        let f = Fixture::new();
        let before = f.epoch().counter();
        let producer = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        let mut scratch = FixedControlScratch::new(
            f.memory
                .reserve(FixedControlScratch::required_bytes())
                .unwrap(),
        )
        .unwrap();
        let mut read = f.memory.reserve(2048).unwrap().bytes(2048).unwrap();
        let prepared =
            PreparedEpoch::prepare_local(&f.domain, &f.memory, &mut read, &mut scratch).unwrap();
        let advance = prepared
            .with_local_invalidation(
                &mut read,
                &mut scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                0,
                &mut |advance, _| {
                    assert_eq!(advance.before, before + 1);
                    assert_eq!(advance.after, before + 2);
                    assert!(matches!(
                        observe_reference_epoch(&f.domain).unwrap(),
                        EpochObservation::Pending(_)
                    ));
                    assert!(producer
                        .artifact_root_producer_capability()
                        .unwrap()
                        .is_some());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(advance.after, before + 2);
        drop(producer);
        assert_eq!(f.epoch().counter(), before + 2);
    }

    #[test]
    fn canonical_ready_holds_actual_guards_and_compares_before_own_epoch() {
        let f = Fixture::new();
        let before = f.epoch().counter();
        let mut prepared = f.prepared();
        let mut called = false;
        let result = with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::start(),
            0,
            &mut |authority, _| {
                called = true;
                assert_eq!(authority.epoch().before, before);
                assert_eq!(authority.epoch().after, before + 1);
                assert_eq!(
                    authority.directory().identity(),
                    super::super::locks::path_identity(&f.layout.artifacts_root()).unwrap()
                );
                assert!(try_open_existing_locked(
                    &f.layout.writer_lock_path(),
                    LockMode::Exclusive
                )
                .unwrap()
                .is_none());
                Ok(())
            },
        )
        .unwrap();
        assert!(called);
        assert_eq!(result.after, before + 1);
        f.unlocked();
        assert_eq!(f.epoch().counter(), before + 1);
    }
    #[test]
    fn canonical_stale_epoch_and_changed_config_never_enter_callback() {
        let f = Fixture::new();
        let mut prepared = f.prepared();
        drop(ReferenceMutation::begin_in_domain(&f.domain).unwrap());
        let current = f.epoch().counter();
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::start(),
            0,
            &mut |_, _| panic!("stale callback")
        )
        .is_err());
        assert_eq!(f.epoch().counter(), current);
        f.unlocked();
        let mut prepared = f.prepared();
        fs::write(
            f.target.state_root.join("config.json"),
            br#"{"schema_version":3,"ignored":true}"#,
        )
        .unwrap();
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::start(),
            0,
            &mut |_, _| panic!("changed callback")
        )
        .is_err());
        assert_eq!(f.epoch().counter(), current);
        f.unlocked();
    }
    #[test]
    fn canonical_large_registry_is_bounded_nonmutating_and_unknown_siblings_survive() {
        let f = Fixture::new();
        let workers = f.layout.storage_root().join("workers");
        fs::create_dir_all(&workers).unwrap();
        for index in 0..1000 {
            fs::write(workers.join(format!("unknown-{index}")), b"foreign").unwrap();
        }
        let current = f.epoch().counter();
        let mut prepared = f.prepared();
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::start(),
            0,
            &mut |_, _| panic!("nonempty callback")
        )
        .is_err());
        assert_eq!(f.epoch().counter(), current);
        assert_eq!(fs::read_dir(workers).unwrap().count(), 1000);
        f.unlocked();
    }
    #[test]
    fn canonical_small_grant_and_panic_drop_actual_authority_before_work() {
        let f = Fixture::new();
        let current = f.epoch().counter();
        let mut prepared = f.prepared();
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::for_test(1, std::time::Duration::from_secs(1)),
            1,
            &mut |_, _| panic!("small callback")
        )
        .is_err());
        assert_eq!(f.epoch().counter(), current);
        f.unlocked();
        let mut prepared = f.prepared();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = with_prepared_canonical(
                &mut prepared,
                &mut MutationGrant::start(),
                0,
                &mut |_, _| panic!("fixture callback unwind"),
            );
        }));
        assert!(outcome.is_err());
        f.unlocked();
        assert_eq!(f.epoch().counter(), current + 1);
        // Bulk prepared work is still owned here, AFTER authority unwound.
        drop(prepared);
    }
    #[test]
    fn canonical_live_producer_is_nonblocking_and_preparation_does_not_spend_grant() {
        let f = Fixture::new();
        let mut prepared = f.prepared();
        let producer = ReferenceMutation::begin_in_domain(&f.domain).unwrap();
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
            0,
            &mut |_, _| panic!("live callback")
        )
        .is_err());
        drop(producer);
        f.unlocked();
        let mut prepared = f.prepared();
        // Outlive the production window, then start a fresh fixture grant.
        std::thread::sleep(std::time::Duration::from_millis(110));
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
            0,
            &mut |_, _| Ok(())
        )
        .is_ok());
    }
}

/// Fixed request-time source facts, separate from runtime target identity.
/// Equality binds the exact default pair, both recognized cache incarnations,
/// and the complete native DB/manifest stamps. It is no mutation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyConfirmationVersion {
    state: super::locks::FileIdentity,
    gate: super::locks::FileIdentity,
    nonce: [u8; 32],
    version: u64,
    job: [u8; 32],
    phase: super::maintenance_legacy_guard::LegacyPhase,
}
pub(super) fn legacy_confirmation_version_from_records(
    binding: &super::maintenance_legacy_guard::Binding,
    record: &super::maintenance_legacy_guard::Record,
) -> LegacyConfirmationVersion {
    LegacyConfirmationVersion {
        state: binding.state,
        gate: binding.gate,
        nonce: binding.nonce,
        version: record.version,
        job: record.job,
        phase: record.phase,
    }
}
fn legacy_confirmation_version(
    target: &MaintenanceTarget,
) -> Result<Option<LegacyConfirmationVersion>, AdmissionError> {
    use super::maintenance_legacy_guard as legacy;
    let root = RootedDirectory::open_readonly_root(
        &target.state_root,
        super::locks::path_identity(&target.state_root).map_err(|_| invalid(Problem::Changed))?,
    )?;
    let state = match root.open_readonly_file(legacy::STATE) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let gate = root.open_readonly_file(legacy::GATE)?;
    let observed = legacy::observe_files(&root, &gate, &state)?;
    Ok(Some(legacy_confirmation_version_from_records(
        &observed.binding,
        &observed.record,
    )))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyConfirmationSource {
    pub(crate) database: Option<FixedFileStamp>,
    pub(crate) manifest: Option<FixedFileStamp>,
    pub(crate) caches: [Option<super::locks::FileIdentity>; 2],
    pub(crate) version: Option<LegacyConfirmationVersion>,
    pair_binding: [u8; 32],
}
impl LegacyConfirmationSource {
    pub(crate) fn encode_checkpoint(&self, out: &mut [u8; 512]) {
        use sha2::{Digest, Sha256};
        out.fill(0);
        out[..8].copy_from_slice(b"CGSCON01");
        for (i, stamp) in [self.database, self.manifest].into_iter().enumerate() {
            if let Some(stamp) = stamp {
                out[8 + i] = 1;
                stamp.encode_checkpoint((&mut out[16 + i * 80..96 + i * 80]).try_into().unwrap());
            }
        }
        for (i, id) in self.caches.into_iter().enumerate() {
            if let Some(id) = id {
                out[10 + i] = 1;
                let (a, b) = id.fixed_words();
                out[176 + i * 16..184 + i * 16].copy_from_slice(&a.to_le_bytes());
                out[184 + i * 16..192 + i * 16].copy_from_slice(&b.to_le_bytes());
            }
        }
        out[208..240].copy_from_slice(&self.pair_binding);
        if let Some(version) = self.version {
            out[12] = 1;
            for (i, id) in [version.state, version.gate].into_iter().enumerate() {
                let (a, b) = id.fixed_words();
                out[240 + i * 16..248 + i * 16].copy_from_slice(&a.to_le_bytes());
                out[248 + i * 16..256 + i * 16].copy_from_slice(&b.to_le_bytes());
            }
            out[272..304].copy_from_slice(&version.nonce);
            out[304..312].copy_from_slice(&version.version.to_le_bytes());
            out[312..344].copy_from_slice(&version.job);
            out[344] = match version.phase {
                super::maintenance_legacy_guard::LegacyPhase::Active => 0,
                super::maintenance_legacy_guard::LegacyPhase::Preparing => 1,
                super::maintenance_legacy_guard::LegacyPhase::Retiring => 2,
                super::maintenance_legacy_guard::LegacyPhase::Removed => 3,
            };
        }
        let digest = Sha256::digest(&out[..480]);
        out[480..].copy_from_slice(&digest);
    }
    pub(crate) fn decode_checkpoint(input: &[u8; 512]) -> Result<Self, AdmissionError> {
        use sha2::{Digest, Sha256};
        if &input[..8] != b"CGSCON01"
            || input[8..13].iter().any(|v| *v > 1)
            || input[13..16]
                .iter()
                .chain(&input[345..480])
                .any(|v| *v != 0)
            || Sha256::digest(&input[..480]).as_slice() != &input[480..]
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let stamp = |i: usize| -> Result<Option<FixedFileStamp>, AdmissionError> {
            let bytes = &input[16 + i * 80..96 + i * 80];
            if input[8 + i] == 1 {
                Ok(Some(FixedFileStamp::decode_checkpoint(
                    bytes.try_into().unwrap(),
                )?))
            } else if bytes.iter().all(|v| *v == 0) {
                Ok(None)
            } else {
                Err(invalid(Problem::InvalidRecord))
            }
        };
        let id = |offset: usize| {
            super::locks::FileIdentity::from_fixed_words(
                u64::from_le_bytes(input[offset..offset + 8].try_into().unwrap()),
                u64::from_le_bytes(input[offset + 8..offset + 16].try_into().unwrap()),
            )
        };
        let cache = |i: usize| -> Result<Option<super::locks::FileIdentity>, AdmissionError> {
            if input[10 + i] == 1 {
                Ok(Some(id(176 + i * 16)))
            } else if input[176 + i * 16..192 + i * 16].iter().all(|v| *v == 0) {
                Ok(None)
            } else {
                Err(invalid(Problem::InvalidRecord))
            }
        };
        let version = if input[12] == 1 {
            Some(LegacyConfirmationVersion {
                state: id(240),
                gate: id(256),
                nonce: input[272..304].try_into().unwrap(),
                version: u64::from_le_bytes(input[304..312].try_into().unwrap()),
                job: input[312..344].try_into().unwrap(),
                phase: match input[344] {
                    0 => super::maintenance_legacy_guard::LegacyPhase::Active,
                    1 => super::maintenance_legacy_guard::LegacyPhase::Preparing,
                    2 => super::maintenance_legacy_guard::LegacyPhase::Retiring,
                    3 => super::maintenance_legacy_guard::LegacyPhase::Removed,
                    _ => return Err(invalid(Problem::InvalidRecord)),
                },
            })
        } else if input[240..345].iter().all(|v| *v == 0) {
            None
        } else {
            return Err(invalid(Problem::InvalidRecord));
        };
        Ok(Self {
            database: stamp(0)?,
            manifest: stamp(1)?,
            caches: [cache(0)?, cache(1)?],
            pair_binding: input[208..240].try_into().unwrap(),
            version,
        })
    }
}
/// Observational source pins prevent inode ABA while a true demand is queued.
/// No lock is held and this owner must be disposed outside execution authority.
pub(crate) struct PreparedLegacyConfirmationSource {
    entries: [Entry; 5],
    pins: [Option<std::fs::File>; 5],
    facts: LegacyConfirmationSource,
    _memory: MemoryReservation,
}
impl PreparedLegacyConfirmationSource {
    pub(crate) fn facts(&self) -> LegacyConfirmationSource {
        self.facts
    }
    /// Actual writer/native/cache exclusions belong to the caller. This only
    /// repeats bounded exact source observations; no hash, codec or allocation.
    pub(crate) fn recheck(
        &self,
        scratch: &mut FixedControlScratch,
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        for (index, entry) in self.entries.iter().enumerate() {
            grant.take_entry()?;
            match (
                open_fixed_control(&entry.path, entry.kind, false, scratch),
                entry.stamp,
                &self.pins[index],
            ) {
                (Ok(file), Some(stamp), Some(pin)) => {
                    if super::locks::file_identity(pin).map_err(|_| invalid(Problem::Changed))?
                        != stamp.identity()
                    {
                        return Err(invalid(Problem::Changed));
                    }
                    if index < 2 || index == 4 {
                        stamp.recheck(&file)?;
                    } else if FixedFileStamp::capture(&file, Kind::Directory)?.identity()
                        != stamp.identity()
                    {
                        return Err(invalid(Problem::Changed));
                    }
                }
                (Err(AdmissionError::Io(error)), None, None)
                    if error.kind() == std::io::ErrorKind::NotFound => {}
                (Err(error), _, _) => return Err(error),
                _ => return Err(invalid(Problem::Changed)),
            }
        }
        Ok(())
    }
}
fn legacy_confirmation_charge(target: &MaintenanceTarget) -> Result<usize, AdmissionError> {
    target
        .state_root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_mul(16)
        .and_then(|x| x.checked_add(16384))
        .and_then(|x| x.checked_add(std::mem::size_of::<PreparedLegacyConfirmationSource>()))
        .ok_or(AdmissionError::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))
}
fn legacy_confirmation_entries(
    target: &MaintenanceTarget,
    scratch: &mut FixedControlScratch,
) -> Result<[Entry; 5], AdmissionError> {
    let pair = super::inventory::default_direct(target);
    let cache = DirectLayout::new(&pair.database, &pair.manifest).artifact_root_path();
    Ok([
        Entry::prepare(pair.database, Kind::File, true, scratch)?,
        Entry::prepare(pair.manifest, Kind::File, true, scratch)?,
        Entry::prepare(
            target.state_root.join("artifacts"),
            Kind::Directory,
            true,
            scratch,
        )?,
        Entry::prepare(cache, Kind::Directory, true, scratch)?,
        Entry::prepare(
            target
                .state_root
                .join(std::str::from_utf8(super::maintenance_legacy_guard::STATE).unwrap()),
            Kind::File,
            true,
            scratch,
        )?,
    ])
}
fn legacy_confirmation_facts(
    entries: &[Entry; 5],
    version: Option<LegacyConfirmationVersion>,
) -> LegacyConfirmationSource {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"CGDEFAULTSOURCE01");
    for entry in entries {
        let path = entry.path.as_os_str().as_encoded_bytes();
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path);
    }
    LegacyConfirmationSource {
        database: entries[0].stamp,
        manifest: entries[1].stamp,
        caches: [
            entries[2].stamp.map(|s| s.identity()),
            entries[3].stamp.map(|s| s.identity()),
        ],
        version,
        pair_binding: hash.finalize().into(),
    }
}
/// Facts-only advisory preparation probes one payload at a time, retaining no
/// DB/manifest descriptors alongside the request's two existing source pins.
/// The first actual enrollment must recheck them under its real source guards.
#[inline(never)]
pub(crate) fn capture_legacy_confirmation_source(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<LegacyConfirmationSource, AdmissionError> {
    let _reservation = memory
        .reserve(legacy_confirmation_charge(target)?)
        .map_err(AdmissionError::Resource)?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    let entries = legacy_confirmation_entries(target, &mut scratch)?;
    let version = legacy_confirmation_version(target)?;
    Ok(legacy_confirmation_facts(&entries, version))
}
/// Noncreating request capture. Missing entries are explicit facts, never
/// approval to enroll a replacement which appears after the request.
#[inline(never)]
pub(crate) fn prepare_legacy_confirmation_source(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<PreparedLegacyConfirmationSource, AdmissionError> {
    let reservation = memory
        .reserve(legacy_confirmation_charge(target)?)
        .map_err(AdmissionError::Resource)?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    let entries = legacy_confirmation_entries(target, &mut scratch)?;
    let mut pins = [None, None, None, None, None];
    for (index, entry) in entries.iter().enumerate() {
        if let Some(stamp) = entry.stamp {
            let file = open_fixed_control(&entry.path, entry.kind, false, &mut scratch)?;
            if index < 2 || index == 4 {
                stamp.recheck(&file)?;
            } else if FixedFileStamp::capture(&file, Kind::Directory)?.identity()
                != stamp.identity()
            {
                return Err(invalid(Problem::Changed));
            }
            pins[index] = Some(file);
        }
    }
    let version = legacy_confirmation_version(target)?;
    let facts = legacy_confirmation_facts(&entries, version);
    let owner = PreparedLegacyConfirmationSource {
        entries,
        pins,
        facts,
        _memory: reservation,
    };
    owner.recheck(&mut scratch, &mut MutationGrant::start())?;
    Ok(owner)
}

/// Exact-incarnation confirmation witness. Retained non-authority descriptors
/// prevent root/control inode ABA even for legacy roots lacking a nonce. This
/// is preparation, never a lock or enrollment/deletion capability.
pub(crate) struct PreparedTargetBinding {
    digest: [u8; 32],
    root: Entry,
    _root_file: std::fs::File,
    coordination: Entry,
    _coordination_file: Option<std::fs::File>,
    binding: BoundConfiguration,
    repository: Entry,
    state: Entry,
    _repository_file: std::fs::File,
    _state_file: std::fs::File,
    scratch: FixedControlScratch,
    legacy: bool,
    _floor: MemoryReservation,
}
impl PreparedTargetBinding {
    pub(crate) fn duplicate(&self, memory: &CleanupMemory) -> Result<Self, AdmissionError> {
        let floor = memory
            .reserve(self._floor.remaining())
            .map_err(AdmissionError::Resource)?;
        let binding = self
            .binding
            .duplicate(memory)
            .map_err(AdmissionError::Resource)?;
        let scratch = FixedControlScratch::new(
            memory
                .reserve(FixedControlScratch::required_bytes())
                .map_err(AdmissionError::Resource)?,
        )?;
        Ok(Self {
            digest: self.digest,
            root: self.root.clone(),
            _root_file: self._root_file.try_clone()?,
            coordination: self.coordination.clone(),
            _coordination_file: self
                ._coordination_file
                .as_ref()
                .map(std::fs::File::try_clone)
                .transpose()?,
            binding,
            repository: self.repository.clone(),
            state: self.state.clone(),
            _repository_file: self._repository_file.try_clone()?,
            _state_file: self._state_file.try_clone()?,
            scratch,
            legacy: self.legacy,
            _floor: floor,
        })
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn is_legacy(&self) -> bool {
        self.legacy
    }
    pub(crate) fn is_current(&mut self) -> Result<bool, AdmissionError> {
        // Advisory fixed recheck; no mutation authority or user confirmation is
        // implied by this convenience. The real admission repeats these checks.
        self.recheck_identity(&mut || Ok(())).map(|_| true)
    }
    pub(super) fn recheck(&mut self, grant: &mut MutationGrant) -> Result<(), AdmissionError> {
        self.recheck_identity(&mut || {
            grant.take_entry()?;
            Ok(())
        })
    }
    fn recheck_identity(
        &mut self,
        charge: &mut impl FnMut() -> Result<(), AdmissionError>,
    ) -> Result<(), AdmissionError> {
        charge()?;
        let current =
            open_fixed_control(&self.root.path, Kind::Directory, false, &mut self.scratch)?;
        if FixedFileStamp::capture(&current, Kind::Directory)?.identity()
            != self.root.stamp.unwrap().identity()
        {
            return Err(invalid(Problem::Changed));
        }
        charge()?;
        check_entry_identity(
            &self.coordination.path,
            self.coordination.kind,
            self.coordination.stamp,
            &mut self.scratch,
        )?;
        charge()?;
        check_entry_identity(
            &self.binding.config_path,
            Kind::File,
            self.binding.config_stamp,
            &mut self.scratch,
        )?;
        for (entry, pin) in [
            (&self.repository, &self._repository_file),
            (&self.state, &self._state_file),
        ] {
            charge()?;
            let current =
                open_fixed_control(&entry.path, Kind::Directory, false, &mut self.scratch)?;
            let expected = entry
                .stamp
                .ok_or_else(|| invalid(Problem::Changed))?
                .identity();
            if FixedFileStamp::capture(&current, Kind::Directory)?.identity() != expected
                || super::locks::file_identity(pin).map_err(|_| invalid(Problem::Changed))?
                    != expected
            {
                return Err(invalid(Problem::Changed));
            }
        }
        Ok(())
    }
}
pub(crate) fn prepare_target_binding(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<PreparedTargetBinding, AdmissionError> {
    use sha2::{Digest, Sha256};
    let binding = project_binding(target, memory).map_err(projection)?;
    let floor = memory
        .reserve(128 * 1024 + std::mem::size_of::<PreparedTargetBinding>())
        .map_err(AdmissionError::Resource)?;
    let root_path = if let Some(pair) = &binding.direct {
        DirectLayout::new(&pair.database, &pair.manifest).artifact_root_path()
    } else {
        ManagedLayout::new(&binding.storage).artifacts_root()
    };
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    let root = Entry::prepare(root_path, Kind::Directory, false, &mut scratch)?;
    let root_file = open_fixed_control(&root.path, Kind::Directory, false, &mut scratch)?;
    root.stamp.unwrap().recheck(&root_file)?;
    let coordination = Entry::prepare(root.path.join(".use.lock"), Kind::File, true, &mut scratch)?;
    let coordination_file = if let Some(stamp) = coordination.stamp {
        let file = open_fixed_control(&coordination.path, Kind::File, false, &mut scratch)?;
        stamp.recheck(&file)?;
        Some(file)
    } else {
        None
    };
    let incarnation = coordination_file.as_ref().and_then(|file| {
        super::locks::read_coordination_incarnation(file)
            .ok()
            .flatten()
    });
    let legacy = incarnation.is_none();
    let mut nonce = incarnation.unwrap_or([0; 32]);
    if legacy {
        getrandom::fill(&mut nonce).map_err(|_| invalid(Problem::InvalidRecord))?;
    }
    let repository = Entry::prepare(
        target.repository_root.clone(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let state = Entry::prepare(
        target.state_root.clone(),
        Kind::Directory,
        false,
        &mut scratch,
    )?;
    let mut hash = Sha256::new();
    hash.update(b"CGTARGET02");
    hash.update([u8::from(legacy)]);
    hash.update(nonce);
    for entry in [&root, &coordination, &repository, &state] {
        hash.update(entry.path.as_os_str().as_encoded_bytes());
        if let Some(stamp) = entry.stamp {
            let (a, b) = stamp.identity().fixed_words();
            hash.update([1]);
            hash.update(a.to_le_bytes());
            hash.update(b.to_le_bytes());
        } else {
            hash.update([0]);
        }
    }
    hash.update(binding.config_path.as_os_str().as_encoded_bytes());
    if let Some(stamp) = binding.config_stamp {
        hash.update(stamp.binding_digest());
    }
    if let Some(pair) = &binding.direct {
        hash.update(pair.database.as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update(pair.manifest.as_os_str().as_encoded_bytes());
    } else {
        hash.update(binding.storage.as_os_str().as_encoded_bytes());
    }
    Ok(PreparedTargetBinding {
        digest: hash.finalize().into(),
        root,
        _root_file: root_file,
        coordination,
        _coordination_file: coordination_file,
        binding,
        _repository_file: open_fixed_control(
            &repository.path,
            Kind::Directory,
            false,
            &mut scratch,
        )?,
        _state_file: open_fixed_control(&state.path, Kind::Directory, false, &mut scratch)?,
        repository,
        state,
        scratch,
        legacy,
        _floor: floor,
    })
}

impl PreparedCanonical {
    /// Advisory origin facts for offslot receipt/journal preparation; this
    /// getter does not transfer any current namespace authority.
    pub(crate) fn origin_scope(&self) -> RootOriginBinding {
        self.origin
    }
}

pub(crate) struct PreparedPrivateMetadata {
    origin: RootOriginBinding,
    coordination_header: [u8; super::locks::COORDINATION_HEADER_BYTES],
    root: Entry,
    root_use: Entry,
    cohort: Entry,
    binding: Entry,
    selector: Entry,
    data: [Entry; 2],
    data_files: [std::fs::File; 2],
    stream: Entry,
    stream_file: std::fs::File,
    images: ReservedBytes,
    read: ReservedBytes,
    scratch: MemoryBox<FixedControlScratch>,
    committed: Option<super::maintenance_controls::CommitFact>,
    recovery: super::maintenance_controls::RecoveryStep,
    _epoch: MemoryBox<PreparedEpoch>,
    _binding: MemoryBox<BoundConfiguration>,
    _floor: MemoryReservation,
}
impl PreparedPrivateMetadata {
    pub(crate) fn origin_scope(&self) -> RootOriginBinding {
        self.origin
    }
    pub(crate) fn committed(&self) -> Option<super::maintenance_controls::CommitFact> {
        self.committed
    }
    pub(crate) fn recovery(&self) -> super::maintenance_controls::RecoveryStep {
        self.recovery
    }
    pub(crate) fn stream_len(&self) -> u64 {
        self.stream.stamp.unwrap().len()
    }
    pub(crate) fn payload(&self) -> Option<&[u8]> {
        let commit = self.committed?;
        let begin = commit.active * super::maintenance_controls::SLOT_BYTES;
        let slot = &self.images.as_slice()[begin..begin + super::maintenance_controls::SLOT_BYTES];
        let len = u64::from_le_bytes(slot[48..56].try_into().ok()?) as usize;
        slot.get(56..56 + len)
    }
}

/// Compatibility value API; shipping stream callers retain the charged owner.
#[inline(never)]
pub(crate) fn prepare_private_metadata(
    target: &MaintenanceTarget,
    origin: &RootOriginBinding,
    memory: &CleanupMemory,
) -> Result<PreparedPrivateMetadata, AdmissionError> {
    prepare_private_metadata_boxed(target, origin, memory).map(MemoryBox::into_inner)
}

// Isolate fallible by-value constructors from the retained metadata frame.
#[inline(never)]
fn private_metadata_binding(
    memory: &CleanupMemory,
    target: &MaintenanceTarget,
) -> Result<MemoryBox<BoundConfiguration>, AdmissionError> {
    let binding = project_binding(target, memory).map_err(projection)?;
    MemoryBox::new(binding, memory).map_err(AdmissionError::Resource)
}
#[inline(never)]
fn private_metadata_scratch(
    memory: &CleanupMemory,
) -> Result<MemoryBox<FixedControlScratch>, AdmissionError> {
    let scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    MemoryBox::new(scratch, memory).map_err(AdmissionError::Resource)
}
#[inline(never)]
fn private_metadata_epoch(
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
    read: &mut ReservedBytes,
    scratch: &mut FixedControlScratch,
) -> Result<MemoryBox<PreparedEpoch>, AdmissionError> {
    let epoch = PreparedEpoch::prepare_local(domain, memory, read, scratch)?;
    MemoryBox::new(epoch, memory).map_err(AdmissionError::Resource)
}

#[inline(never)]
pub(crate) fn prepare_private_metadata_boxed(
    target: &MaintenanceTarget,
    origin: &RootOriginBinding,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedPrivateMetadata>, AdmissionError> {
    use super::maintenance_controls as c;
    let binding = private_metadata_binding(memory, target)?;
    let floor = memory
        .reserve(256 * 1024 + std::mem::size_of::<PreparedPrivateMetadata>())
        .map_err(AdmissionError::Resource)?;
    let domain = if let Some(pair) = &binding.direct {
        ReferenceDomain::direct(&DirectLayout::new(&pair.database, &pair.manifest))
    } else {
        ReferenceDomain::managed(&ManagedLayout::new(&binding.storage))
    }
    .map_err(|_| invalid(Problem::InvalidRecord))?;
    let mut scratch = private_metadata_scratch(memory)?;
    let mut read = memory
        .reserve(4096)
        .map_err(AdmissionError::Resource)?
        .bytes(4096)
        .map_err(AdmissionError::Resource)?;
    let epoch = private_metadata_epoch(&domain, memory, &mut read, &mut scratch)?;
    let facts = epoch.root_facts();
    if facts
        != (
            origin.root,
            origin.coordination,
            origin.incarnation,
            origin.cohort,
        )
    {
        return Err(invalid(Problem::Changed));
    }
    let path = domain.artifact_root();
    let root = Entry::prepare(path.to_owned(), Kind::Directory, false, &mut scratch)?;
    let root_use = Entry::prepare(path.join(".use.lock"), Kind::File, false, &mut scratch)?;
    let cohort = Entry::prepare(
        path.join(".use-cohort.json"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    let binding_entry = Entry::prepare(
        path.join(".publication.binding"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    if binding_entry.stamp.unwrap().identity() != origin.publication_binding {
        return Err(invalid(Problem::Changed));
    }
    let mut file = open_fixed_control(&binding_entry.path, Kind::File, false, &mut scratch)?;
    let len =
        super::locks::read_fixed_control(&mut file, &binding_entry.stamp.unwrap(), &mut read)?;
    let publisher = c::PublisherBinding::decode(&read.as_slice()[..len])
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    if (
        publisher.root,
        publisher.coordination,
        publisher.incarnation,
    ) != (origin.root, origin.coordination, origin.incarnation)
    {
        return Err(invalid(Problem::Changed));
    }
    let selector = Entry::prepare(
        path.join(".publication.selector"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    let data = [
        Entry::prepare(
            path.join(".publication.data0"),
            Kind::File,
            false,
            &mut scratch,
        )?,
        Entry::prepare(
            path.join(".publication.data1"),
            Kind::File,
            false,
            &mut scratch,
        )?,
    ];
    let data_files = [
        open_fixed_control(&data[0].path, Kind::File, true, &mut scratch)?,
        open_fixed_control(&data[1].path, Kind::File, true, &mut scratch)?,
    ];
    if selector.stamp.unwrap().identity() != publisher.selector
        || data[0].stamp.unwrap().identity() != publisher.data[0]
        || data[1].stamp.unwrap().identity() != publisher.data[1]
    {
        return Err(invalid(Problem::Changed));
    }
    let total = 2 * c::SLOT_BYTES + c::SELECTOR_BYTES;
    let mut images = memory
        .reserve(total)
        .map_err(AdmissionError::Resource)?
        .bytes(total)
        .map_err(AdmissionError::Resource)?;
    for index in 0..2 {
        data[index].stamp.unwrap().recheck(&data_files[index])?;
        if data[index].stamp.unwrap().len() != c::SLOT_BYTES as u64 {
            return Err(invalid(Problem::InvalidRecord));
        }
        super::locks::read_fixed_range(
            &data_files[index],
            0,
            &mut images.as_mut_slice()[index * c::SLOT_BYTES..(index + 1) * c::SLOT_BYTES],
        )?;
        data[index].stamp.unwrap().recheck(&data_files[index])?;
    }
    let selector_file = open_fixed_control(&selector.path, Kind::File, false, &mut scratch)?;
    if selector.stamp.unwrap().len() != c::SELECTOR_BYTES as u64 {
        return Err(invalid(Problem::InvalidRecord));
    }
    super::locks::read_fixed_range(
        &selector_file,
        0,
        &mut images.as_mut_slice()[2 * c::SLOT_BYTES..],
    )?;
    selector.stamp.unwrap().recheck(&selector_file)?;
    let slots = [
        c::decode_slot(&images.as_slice()[..c::SLOT_BYTES], &origin.incarnation).ok(),
        c::decode_slot(
            &images.as_slice()[c::SLOT_BYTES..2 * c::SLOT_BYTES],
            &origin.incarnation,
        )
        .ok(),
    ];
    let selector_bytes = &images.as_slice()[2 * c::SLOT_BYTES..];
    let committed = c::decode_commit(&selector_bytes[..c::COMMIT_BYTES], &origin.incarnation).ok();
    let intent = c::decode_intent(&selector_bytes[c::COMMIT_BYTES..], &origin.incarnation).ok();
    let recovery = c::recovery_step(committed, intent, slots);
    if recovery == c::RecoveryStep::Unknown {
        return Err(invalid(Problem::InvalidRecord));
    }
    let stream = Entry::prepare(
        path.join(".publication.stream"),
        Kind::File,
        false,
        &mut scratch,
    )?;
    if Some(stream.stamp.unwrap().identity()) != publisher.stream {
        return Err(invalid(Problem::Changed));
    }
    let stream_file = open_fixed_control(&stream.path, Kind::File, true, &mut scratch)?;
    stream.stamp.unwrap().recheck(&stream_file)?;
    MemoryBox::new(
        PreparedPrivateMetadata {
            origin: *origin,
            coordination_header: super::locks::encode_coordination_incarnation(origin.incarnation),
            root,
            root_use,
            cohort,
            binding: binding_entry,
            selector,
            data,
            data_files,
            stream,
            stream_file,
            images,
            read,
            scratch,
            committed,
            recovery,
            _epoch: epoch,
            _binding: binding,
            _floor: floor,
        },
        memory,
    )
    .map_err(AdmissionError::Resource)
}

/// Only the prebound metadata stream/slots are reachable through this facade.
/// No payload directory/path/handle or destination writer authority is exposed.
pub(crate) struct PrivateMetadataMutation<'a> {
    origin: RootOriginBinding,
    selector: &'a FixedLockedFile,
    data: [&'a std::fs::File; 2],
    stream: &'a std::fs::File,
    stream_len: u64,
    read: &'a mut [u8],
    committed: Option<super::maintenance_controls::CommitFact>,
    remaining_effects: usize,
    usable: bool,
    recovery_pending: bool,
}
impl PrivateMetadataMutation<'_> {
    fn spend(&mut self, count: usize) -> Result<(), AdmissionError> {
        self.remaining_effects =
            self.remaining_effects
                .checked_sub(count)
                .ok_or(AdmissionError::Grant(
                    super::maintenance_budget::GrantExhausted::Entries,
                ))?;
        Ok(())
    }
    fn range(
        &mut self,
        file: &std::fs::File,
        offset: u64,
        bytes: &[u8],
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        if bytes.len() > self.read.len() {
            return Err(invalid(Problem::BufferTooSmall));
        }
        grant.check_time()?;
        super::locks::write_fixed_range(file, offset, bytes, grant)?;
        #[cfg(test)]
        METADATA_WRITE_CUT.with(|cut| {
            let mut cut = cut.borrow_mut();
            if let Some(remaining) = cut.as_mut() {
                *remaining -= 1;
                if *remaining == 0 {
                    *cut = None;
                    panic!("fixture durable metadata write cut");
                }
            }
        });
        grant.check_time()?;
        super::locks::read_fixed_range(file, offset, &mut self.read[..bytes.len()])?;
        if self.read[..bytes.len()] != *bytes {
            return Err(invalid(Problem::Changed));
        }
        grant.check_time()?;
        Ok(())
    }
    /// Exact bounded inert append. A durable S3 AppendIntent must already bind
    /// these bytes/offset/sourceE/G. Partial own prefixes can be rewritten only
    /// when they match; unknown tails are retained, never truncated/adopted.
    pub(crate) fn append_frame(
        &mut self,
        offset: u64,
        bytes: &[u8],
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        if !self.usable || bytes.is_empty() || bytes.len() > 4096 || bytes.len() > self.read.len() {
            return Err(invalid(Problem::BufferTooSmall));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        if self.stream_len < offset || self.stream_len > end {
            return Err(invalid(Problem::Changed));
        }
        let partial = (self.stream_len - offset) as usize;
        self.spend(if partial == 0 { 2 } else { 3 })?;
        if partial > 0 {
            grant.check_time()?;
            super::locks::read_fixed_range(self.stream, offset, &mut self.read[..partial])?;
            if self.read[..partial] != bytes[..partial] {
                return Err(invalid(Problem::Changed));
            }
        }
        self.usable = false;
        let stream = self.stream;
        self.range(stream, offset, bytes, grant)?;
        if stream.metadata()?.len() != end {
            return Err(invalid(Problem::Changed));
        }
        self.stream_len = end;
        self.usable = true;
        Ok(())
    }
}

pub(crate) fn with_private_metadata(
    prepared: &mut PreparedPrivateMetadata,
    grant: &mut MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(
        &mut PrivateMetadataMutation<'_>,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
) -> Result<(), AdmissionError> {
    with_private_metadata_inner(prepared, None, None, grant, effect_entries, false, apply)
}
/// Composition-only entry point: caller is inside an actual scoped-role
/// callback and included these effects in that role's atomic reservation.
/// Fixed existing-only metadata admission observations still spend previously
/// UNRESERVED entries. No canonical writer/state/reference admission is nested.
pub(crate) fn with_reserved_private_metadata(
    prepared: &mut PreparedPrivateMetadata,
    grant: &mut MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(
        &mut PrivateMetadataMutation<'_>,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
) -> Result<(), AdmissionError> {
    with_private_metadata_inner(prepared, None, None, grant, effect_entries, true, apply)
}
/// Compose with an already held actual Exclusive artifact-root lease. No
/// reference/writer admission is repeated and no namespace marker substitutes
/// for the borrowed kernel guard. Effects belong to the outer reservation.
pub(crate) fn with_reserved_private_metadata_owned_root(
    prepared: &mut PreparedPrivateMetadata,
    root_use: &FixedLockedFile,
    grant: &mut MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(
        &mut PrivateMetadataMutation<'_>,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
) -> Result<(), AdmissionError> {
    with_private_metadata_inner(
        prepared,
        Some(root_use),
        None,
        grant,
        effect_entries,
        true,
        apply,
    )
}
/// Compose only inside an actual canonical callback. Borrow both real
/// Exclusive guards, keeping all metadata identity/image/stream checks while
/// avoiding self-contention on the already held root and selector locks.
pub(crate) fn with_reserved_canonical_private_metadata(
    prepared: &mut PreparedPrivateMetadata,
    authority: &CanonicalMutation<'_>,
    grant: &mut MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(
        &mut PrivateMetadataMutation<'_>,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
) -> Result<(), AdmissionError> {
    if prepared.origin != authority.origin {
        return Err(invalid(Problem::Changed));
    }
    with_private_metadata_inner(
        prepared,
        Some(authority.root_use),
        Some(authority.selector),
        grant,
        effect_entries,
        true,
        apply,
    )
}
fn with_private_metadata_inner(
    prepared: &mut PreparedPrivateMetadata,
    borrowed_root: Option<&FixedLockedFile>,
    borrowed_selector: Option<&FixedLockedFile>,
    grant: &mut MutationGrant,
    effect_entries: usize,
    already_reserved: bool,
    apply: &mut impl FnMut(
        &mut PrivateMetadataMutation<'_>,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
) -> Result<(), AdmissionError> {
    grant.check()?;
    let acquired_root = if borrowed_root.is_none() {
        grant.take_entry()?;
        let root_file = open_fixed_control(
            &prepared.root_use.path,
            Kind::File,
            false,
            &mut prepared.scratch,
        )?;
        Some(
            FixedLockedFile::try_lock(root_file, LockMode::Shared)?
                .ok_or_else(|| invalid(Problem::Busy))?,
        )
    } else {
        None
    };
    let root_use = borrowed_root
        .or(acquired_root.as_ref())
        .ok_or_else(|| invalid(Problem::Busy))?;
    if borrowed_root.is_some() && root_use.mode() != LockMode::Exclusive {
        return Err(invalid(Problem::Busy));
    }
    prepared
        .root_use
        .stamp
        .ok_or_else(|| invalid(Problem::InvalidRecord))?
        .recheck(root_use.file())?;
    if !root_use.matches_fixed_header(&prepared.coordination_header)? {
        return Err(invalid(Problem::Changed));
    }
    let acquired_selector = if borrowed_selector.is_none() {
        Some(prepared.selector.lock(&mut prepared.scratch, grant)?)
    } else {
        None
    };
    let selector = borrowed_selector
        .or(acquired_selector.as_ref())
        .ok_or_else(|| invalid(Problem::Busy))?;
    let selector_stamp = prepared
        .selector
        .stamp
        .ok_or_else(|| invalid(Problem::InvalidRecord))?;
    if !selector.matches_fixed_current(&selector_stamp, LockMode::Exclusive)? {
        return Err(invalid(Problem::Changed));
    }
    selector_stamp.recheck(selector.file())?;
    for entry in [
        &prepared.root,
        &prepared.root_use,
        &prepared.cohort,
        &prepared.binding,
        &prepared.selector,
        &prepared.data[0],
        &prepared.data[1],
        &prepared.stream,
    ] {
        entry.check(&mut prepared.scratch, grant)?;
    }
    for index in 0..2 {
        prepared.data[index]
            .stamp
            .unwrap()
            .recheck(&prepared.data_files[index])?;
        grant.take_entry()?;
        let begin = index * super::maintenance_controls::SLOT_BYTES;
        let len = super::maintenance_controls::SLOT_BYTES;
        super::locks::read_fixed_range(
            &prepared.data_files[index],
            0,
            &mut prepared.read.as_mut_slice()[..len],
        )?;
        if prepared.read.as_slice()[..len] != prepared.images.as_slice()[begin..begin + len] {
            return Err(invalid(Problem::Changed));
        }
    }
    let len = super::maintenance_controls::SELECTOR_BYTES;
    grant.take_entry()?;
    super::locks::read_fixed_range(selector.file(), 0, &mut prepared.read.as_mut_slice()[..len])?;
    if prepared.read.as_slice()[..len]
        != prepared.images.as_slice()[2 * super::maintenance_controls::SLOT_BYTES..]
    {
        return Err(invalid(Problem::Changed));
    }
    prepared
        .stream
        .stamp
        .unwrap()
        .recheck(&prepared.stream_file)?;
    if !already_reserved {
        grant.take_entries(effect_entries)?;
    }
    {
        let mut mutation = PrivateMetadataMutation {
            origin: prepared.origin,
            selector,
            data: [&prepared.data_files[0], &prepared.data_files[1]],
            stream: &prepared.stream_file,
            stream_len: prepared.stream.stamp.unwrap().len(),
            read: prepared.read.as_mut_slice(),
            committed: prepared.committed,
            remaining_effects: effect_entries,
            usable: true,
            recovery_pending: !matches!(
                prepared.recovery,
                super::maintenance_controls::RecoveryStep::Committed(_)
            ),
        };
        apply(&mut mutation, grant)?;
        grant.check_time()?;
    }
    drop(acquired_selector);
    drop(acquired_root);
    Ok(())
}

pub(crate) struct PreparedControlUpdate {
    incarnation: [u8; 32],
    before: Option<super::maintenance_controls::CommitFact>,
    next: super::maintenance_controls::CommitFact,
    images: ReservedBytes,
    effects: usize,
    recovery: bool,
}
impl PreparedControlUpdate {
    pub(crate) fn effects(&self) -> usize {
        self.effects
    }
    #[cfg(test)]
    pub(crate) fn next(&self) -> super::maintenance_controls::CommitFact {
        self.next
    }
}
pub(crate) fn prepare_metadata_control(
    origin: &RootOriginBinding,
    before: super::maintenance_controls::CommitFact,
    payload: &[u8],
    memory: &CleanupMemory,
) -> Result<PreparedControlUpdate, AdmissionError> {
    use super::maintenance_controls as c;
    let size = c::SLOT_BYTES + c::INTENT_BYTES + c::COMMIT_BYTES;
    let mut images = memory
        .reserve(size + std::mem::size_of::<PreparedControlUpdate>())
        .map_err(AdmissionError::Resource)?
        .bytes(size)
        .map_err(AdmissionError::Resource)?;
    let (slot, rest) = images.as_mut_slice().split_at_mut(c::SLOT_BYTES);
    let (intent, commit) = rest.split_at_mut(c::INTENT_BYTES);
    let generation = before
        .generation
        .checked_add(1)
        .ok_or_else(|| invalid(Problem::EpochExhausted))?;
    let fact = c::encode_slot(slot, &origin.incarnation, generation, payload)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let intended = c::encode_intent(intent, &origin.incarnation, before, slot)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let next = c::CommitFact {
        generation,
        active: intended.target,
        slot_digest: fact.digest,
    };
    c::encode_commit(commit, &origin.incarnation, next)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    Ok(PreparedControlUpdate {
        incarnation: origin.incarnation,
        before: Some(before),
        next,
        images,
        effects: 6,
        recovery: false,
    })
}
pub(crate) fn prepare_metadata_recovery(
    prepared: &PreparedPrivateMetadata,
    memory: &CleanupMemory,
) -> Result<Option<PreparedControlUpdate>, AdmissionError> {
    use super::maintenance_controls as c;
    let (intent, effects) = match prepared.recovery {
        c::RecoveryStep::Committed(_) => return Ok(None),
        c::RecoveryStep::RewriteIntendedSlot(intent) => (intent, 4),
        c::RecoveryStep::CommitIntendedSlot(intent) => (intent, 2),
        c::RecoveryStep::Unknown => return Err(invalid(Problem::InvalidRecord)),
    };
    let size = c::SLOT_BYTES + c::INTENT_BYTES + c::COMMIT_BYTES;
    let mut images = memory
        .reserve(size + std::mem::size_of::<PreparedControlUpdate>())
        .map_err(AdmissionError::Resource)?
        .bytes(size)
        .map_err(AdmissionError::Resource)?;
    let source = &prepared.images.as_slice()[2 * c::SLOT_BYTES + c::COMMIT_BYTES..];
    let (slot, rest) = images.as_mut_slice().split_at_mut(c::SLOT_BYTES);
    let (intent_bytes, commit) = rest.split_at_mut(c::INTENT_BYTES);
    slot.copy_from_slice(&source[96..96 + c::SLOT_BYTES]);
    intent_bytes.copy_from_slice(source);
    let next = c::CommitFact {
        generation: intent.after,
        active: intent.target,
        slot_digest: intent.slot_digest,
    };
    c::encode_commit(commit, &prepared.origin.incarnation, next)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    Ok(Some(PreparedControlUpdate {
        incarnation: prepared.origin.incarnation,
        before: prepared.committed,
        next,
        images,
        effects,
        recovery: true,
    }))
}
impl PrivateMetadataMutation<'_> {
    pub(crate) fn publish_control(
        &mut self,
        update: &PreparedControlUpdate,
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        use super::maintenance_controls as c;
        if !self.usable
            || self.recovery_pending != update.recovery
            || self.origin.incarnation != update.incarnation
            || self.committed != update.before
        {
            return Err(invalid(Problem::Changed));
        }
        self.spend(update.effects)?;
        self.usable = false;
        let selector = self.selector.file();
        let data = self.data[update.next.active];
        let (slot, rest) = update.images.as_slice().split_at(c::SLOT_BYTES);
        let (intent, commit) = rest.split_at(c::INTENT_BYTES);
        if !update.recovery {
            self.range(selector, c::COMMIT_BYTES as u64, intent, grant)?;
        }
        if update.effects >= 4 {
            self.range(data, 0, slot, grant)?;
        }
        self.range(selector, 0, commit, grant)?;
        self.committed = Some(update.next);
        self.recovery_pending = false;
        self.usable = true;
        Ok(())
    }
}

impl PrivateMetadataMutation<'_> {
    /// S3 must first durably publish its verified RESET_INTENT after Done and
    /// verify the complete old prefix offslot. Only this same bound metadata
    /// stream can be truncated; no payload/namespace handle is available.
    pub(crate) fn reset_stream(
        &mut self,
        expected_len: u64,
        grant: &mut MutationGrant,
    ) -> Result<(), AdmissionError> {
        if !self.usable || self.recovery_pending || self.stream_len != expected_len {
            return Err(invalid(Problem::Changed));
        }
        self.spend(2)?;
        self.usable = false;
        grant.check_time()?;
        self.stream.set_len(0)?;
        grant.check_time()?;
        self.stream.sync_all()?;
        grant.check_time()?;
        if self.stream.metadata()?.len() != 0 {
            return Err(invalid(Problem::Changed));
        }
        self.stream_len = 0;
        self.usable = true;
        Ok(())
    }
}

#[cfg(test)]
thread_local! {pub(crate) static METADATA_WRITE_CUT:std::cell::RefCell<Option<usize>>=const {std::cell::RefCell::new(None)};}

/// Global invalidation component for independently proven scoped roles. It
/// grants NO generation/workspace/payload ownership itself. The role module
/// retains its actual fixed local leases and immutable retirement/source proof.
pub(crate) struct PreparedScopedInvalidation {
    epoch: PreparedEpoch,
    read: ReservedBytes,
    scratch: FixedControlScratch,
}
impl PreparedScopedInvalidation {
    pub(crate) fn root_facts(
        &self,
    ) -> (
        super::locks::FileIdentity,
        super::locks::FileIdentity,
        [u8; 32],
        super::locks::FileIdentity,
    ) {
        self.epoch.root_facts()
    }
}
pub(crate) fn prepare_scoped_invalidation(
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
) -> Result<PreparedScopedInvalidation, AdmissionError> {
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(AdmissionError::Resource)?,
    )?;
    let mut read = memory
        .reserve(2048)
        .map_err(AdmissionError::Resource)?
        .bytes(2048)
        .map_err(AdmissionError::Resource)?;
    let epoch = PreparedEpoch::prepare_local(domain, memory, &mut read, &mut scratch)?;
    Ok(PreparedScopedInvalidation {
        epoch,
        read,
        scratch,
    })
}
pub(crate) fn with_scoped_reference_invalidation(
    prepared: &mut PreparedScopedInvalidation,
    grant: &mut MutationGrant,
    effect_entries: usize,
    validate: &mut impl FnMut(
        &mut FixedControlScratch,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
    apply: &mut impl FnMut(EpochAdvance, &mut MutationGrant) -> Result<(), AdmissionError>,
) -> Result<EpochAdvance, AdmissionError> {
    prepared.epoch.with_local_validated(
        &mut prepared.read,
        &mut prepared.scratch,
        grant,
        effect_entries,
        validate,
        apply,
    )
}

/// Existing actual Exclusive root ownership is borrowed, never reacquired or
/// replaced by a marker. Error/unwind leaves the caller's real lease intact.
pub(crate) fn with_scoped_reference_invalidation_owned_root(
    prepared: &mut PreparedScopedInvalidation,
    root_use: &FixedLockedFile,
    grant: &mut MutationGrant,
    effect_entries: usize,
    validate: &mut impl FnMut(
        &mut FixedControlScratch,
        &mut MutationGrant,
    ) -> Result<(), AdmissionError>,
    apply: &mut impl FnMut(EpochAdvance, &mut MutationGrant) -> Result<(), AdmissionError>,
) -> Result<EpochAdvance, AdmissionError> {
    prepared.epoch.with_local_validated_owned_root(
        Some(root_use),
        &mut prepared.read,
        &mut prepared.scratch,
        grant,
        effect_entries,
        validate,
        apply,
    )
}

impl PreparedPrivateMetadata {
    /// Offslot bounded advisory read of this exact captured metadata stream.
    /// Callers reprepare after any write; no path/handle/authority is exported.
    pub(crate) fn read_stream_exact(
        &self,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), AdmissionError> {
        if out.len() > 4096
            || offset
                .checked_add(out.len() as u64)
                .is_none_or(|end| end > self.stream.stamp.unwrap().len())
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let stamp = self
            .stream
            .stamp
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        stamp.recheck(&self.stream_file)?;
        super::locks::read_fixed_range(&self.stream_file, offset, out)?;
        stamp.recheck(&self.stream_file)
    }
}
