//! Exact upgraded-protocol coverage for a Direct root born through the genuine
//! first-creation hook. It never authorizes adopting an older destination.
use super::*;
use crate::storage::locks::{
    FixedControlError as Error, FixedControlKind as Kind, FixedControlProblem as Problem,
    FixedFileStamp, FixedLockedFile, RootedDirectory,
};
use crate::storage::maintenance_budget::{
    CleanupMemory, MemoryBox, MemoryReservation, MutationGrant,
};
fn bad() -> Error {
    Error::Invalid(Problem::Changed)
}
fn native(e: NativeError) -> Error {
    match e {
        NativeError::Io(e) => e.into(),
        _ => bad(),
    }
}
pub(crate) struct PreparedAutomaticDirect {
    destination: FileIdentity,
    artifact: FileIdentity,
    writer: FileIdentity,
    use_lock: FileIdentity,
    header: [u8; 72],
    epoch_stamp: FixedFileStamp,
    cohort_stamp: FixedFileStamp,
    epoch: [u8; REFERENCE_EPOCH_SIZE],
    read: [u8; REFERENCE_EPOCH_SIZE],
    _floor: MemoryReservation,
}
/// Facts alone cannot create coverage: observe_reference_epoch validates the
/// actual factory-born fixed epoch, coordination incarnation and physical root.
pub(crate) fn prepare_automatic_direct(
    layout: &DirectLayout,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<PreparedAutomaticDirect>>, Error> {
    let floor = memory
        .reserve(std::mem::size_of::<PreparedAutomaticDirect>() + 16384)
        .map_err(Error::Resource)?;
    let domain = ReferenceDomain::direct(layout).map_err(native)?;
    let epoch = match observe_reference_epoch(&domain).map_err(native)? {
        EpochObservation::Verified(epoch) => epoch,
        EpochObservation::Pending(_) => return Ok(None),
    };
    if epoch.record.scope.kind != 2 || !epoch.record.covered {
        return Ok(None);
    }
    let root = RootedDirectory::open_readonly_root(&domain.artifact_root, epoch.record.root)?;
    let cohort_file = root.open_readonly_file(b".use-cohort.json")?;
    let cohort_stamp = FixedFileStamp::capture(&cohort_file, Kind::File)?;
    if cohort_stamp.len() > 1024 {
        return Err(bad());
    }
    let mut body = [0; 1024];
    super::super::locks::read_fixed_range(
        &cohort_file,
        0,
        &mut body[..cohort_stamp.len() as usize],
    )?;
    let cohort: ArtifactCohortRecord =
        serde_json::from_slice(&body[..cohort_stamp.len() as usize]).map_err(|_| bad())?;
    if cohort.origin != CohortOrigin::GuardedCreation
        || cohort_stamp.identity() != epoch.record.cohort
        || cohort.root != epoch.record.root
        || cohort.coordination != epoch.record.use_lock
        || cohort.coordination_incarnation != epoch.record.use_incarnation
    {
        return Ok(None);
    }
    cohort_stamp.recheck(&cohort_file)?;
    let file = root.open_readonly_file(REFERENCE_EPOCH_FILE.as_bytes())?;
    let stamp = FixedFileStamp::capture(&file, Kind::File)?;
    if stamp.identity() != epoch.file || stamp.len() != REFERENCE_EPOCH_SIZE as u64 {
        return Err(bad());
    }
    let mut image = [0; REFERENCE_EPOCH_SIZE];
    super::super::locks::read_fixed_range(&file, 0, &mut image)?;
    stamp.recheck(&file)?;
    if image != encode_reference_epoch(&epoch.record).map_err(native)? {
        return Err(bad());
    }
    MemoryBox::new(
        PreparedAutomaticDirect {
            destination: epoch.record.scope.destination,
            artifact: epoch.record.root,
            writer: epoch.record.scope.writer,
            use_lock: epoch.record.use_lock,
            header: super::super::locks::encode_coordination_incarnation(
                epoch.record.use_incarnation,
            ),
            epoch_stamp: stamp,
            cohort_stamp,
            epoch: image,
            read: [0; REFERENCE_EPOCH_SIZE],
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(Error::Resource)
}
impl PreparedAutomaticDirect {
    /// Caller retains its actual writer, native family/resource and root-use
    /// exclusion. This only compares already prepared bytes and named identities.
    pub(crate) fn verify(
        &mut self,
        destination: &RootedDirectory,
        artifact: &RootedDirectory,
        writer: &FixedLockedFile,
        root_use: &FixedLockedFile,
        grant: &mut MutationGrant,
    ) -> Result<(), Error> {
        grant.check()?;
        if destination.identity() != self.destination
            || artifact.identity() != self.artifact
            || writer.mode() != LockMode::Exclusive
            || root_use.mode() != LockMode::Exclusive
            || super::super::locks::file_identity(writer.file()).map_err(native)? != self.writer
            || super::super::locks::file_identity(root_use.file()).map_err(native)? != self.use_lock
            || !root_use.matches_fixed_header(&self.header)?
        {
            return Err(bad());
        }
        grant.take_entries(2)?;
        let cohort = artifact.open_readonly_file(b".use-cohort.json")?;
        self.cohort_stamp.recheck(&cohort)?;
        let epoch = artifact.open_readonly_file(REFERENCE_EPOCH_FILE.as_bytes())?;
        self.epoch_stamp.recheck(&epoch)?;
        super::super::locks::read_fixed_range(&epoch, 0, &mut self.read)?;
        if self.read != self.epoch {
            return Err(bad());
        }
        grant.check_time()?;
        Ok(())
    }
}
