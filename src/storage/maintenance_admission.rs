//! Bounded mutation admission. Prepared values own advisory facts/buffers only;
//! actual non-Send guards exist solely during a borrowed hot-frame callback.
use super::inventory::MaintenanceTarget;
use super::layout::{DirectLayout, ManagedLayout};
use super::locks::{
    open_fixed_control, FixedControlError, FixedControlKind as Kind,
    FixedControlProblem as Problem, FixedControlScratch, FixedFileStamp, FixedLockedFile, LockMode,
    RootedDirectory,
};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, MutationGrant, ReservedBytes};
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
    root: Entry,
    workers: Entry,
    phases: Entry,
    worker_state: Entry,
    direct: bool,
    read: ReservedBytes,
    scratch: FixedControlScratch,
    directory_scratch: [u8; 4096],
    _floor: MemoryReservation,
}
pub(crate) struct CanonicalMutation<'a> {
    directory: &'a RootedDirectory,
    advance: EpochAdvance,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl CanonicalMutation<'_> {
    pub(crate) fn directory(&self) -> &RootedDirectory {
        self.directory
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
    #[cfg(windows)]
    const PATH_BYTES: usize = 65536;
    #[cfg(not(windows))]
    const PATH_BYTES: usize = 4096;
    let floor = memory
        .reserve(32 * PATH_BYTES + std::mem::size_of::<PreparedCanonical>())
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
        root,
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
        root,
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
    let binding = project_binding(target, memory).map_err(projection)?;
    let domain = if let Some(pair) = &binding.direct {
        ReferenceDomain::direct(&DirectLayout::new(&pair.database, &pair.manifest))
    } else {
        ReferenceDomain::managed(&ManagedLayout::new(&binding.storage))
    }
    .map_err(|_| invalid(Problem::InvalidRecord))?;
    let expected = match ownership::observe_reference_epoch(&domain)
        .map_err(|_| invalid(Problem::InvalidRecord))?
    {
        ownership::EpochObservation::Verified(epoch) => epoch,
        _ => return Err(invalid(Problem::Busy)),
    };
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
    let prepared = PreparedEpoch::prepare(&domain, &expected, memory, &mut read, &mut scratch)?;
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(prepared.fingerprint());
    for identity in [binding.repository, binding.state] {
        let (a, b) = identity.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
    }
    hash.update(binding.config_path.as_os_str().as_encoded_bytes());
    match binding.config_stamp {
        Some(stamp) => {
            hash.update([1]);
            hash.update(stamp.binding_digest());
        }
        None => hash.update([0]),
    }
    Ok(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::super::layout::{create_fresh_reference_root, managed_generation_id};
    use super::super::locks::{open_locked, try_open_existing_locked};
    use super::super::maintenance_ownership::{
        initialize_fresh_reference_domain, observe_reference_epoch, EpochObservation,
        ReferenceMutation,
    };
    use super::*;
    use std::fs;
    struct Fixture {
        root: PathBuf,
        target: MaintenanceTarget,
        layout: ManagedLayout,
        domain: ReferenceDomain,
        memory: CleanupMemory,
    }
    impl Fixture {
        fn new() -> Self {
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
        fn epoch(&self) -> ReferenceEpoch {
            match observe_reference_epoch(&self.domain).unwrap() {
                EpochObservation::Verified(epoch) => epoch,
                other => panic!("{other:?}"),
            }
        }
        fn prepared(&self) -> PreparedCanonical {
            prepare_canonical(&self.target, &self.domain, &self.epoch(), &self.memory).unwrap()
        }
        fn unlocked(&self) {
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
                &mut MutationGrant::start(),
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
            &mut MutationGrant::start(),
            0,
            &mut |_, _| panic!("live callback")
        )
        .is_err());
        drop(producer);
        f.unlocked();
        let mut prepared = f.prepared();
        std::thread::sleep(std::time::Duration::from_millis(110));
        assert!(with_prepared_canonical(
            &mut prepared,
            &mut MutationGrant::start(),
            0,
            &mut |_, _| Ok(())
        )
        .is_ok());
    }
}
