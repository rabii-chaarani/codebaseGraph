//! Pure advisory selection and cursor codec. No filesystem effects, admission,
//! provenance proofs, retirement API or deletion authority live here.
use sha2::{Digest, Sha256};

const CATEGORIES: usize = 8;
const ADDRESS_BYTES: usize = 128;
const BODY_BYTES: usize = 8 + 32 + 1 + CATEGORIES * (1 + ADDRESS_BYTES);
pub(crate) const CURSOR_BYTES: usize = BODY_BYTES + 32;
const MAGIC: &[u8; 8] = b"CGMC0002";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WorkCategory {
    Canonical,
    TemporaryOrBackup,
    Run,
    Phase,
    Generation,
    LegacyNative,
    LegacySnapshot,
    DefaultDirect,
}
impl WorkCategory {
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Canonical,
            1 => Self::TemporaryOrBackup,
            2 => Self::Run,
            3 => Self::Phase,
            4 => Self::Generation,
            5 => Self::LegacyNative,
            6 => Self::LegacySnapshot,
            7 => Self::DefaultDirect,
            _ => unreachable!("validated category"),
        }
    }
}

/// Caller supplies a lossless fixed-width logical address, not a truncated or
/// hashed path. This module neither resolves it nor certifies its association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CursorAddress(pub(crate) [u8; ADDRESS_BYTES]);
impl CursorAddress {
    pub(crate) fn from_short(short: [u8; 64]) -> Self {
        let mut address = [0; ADDRESS_BYTES];
        address[..64].copy_from_slice(&short);
        Self(address)
    }
    pub(crate) fn as_short(self) -> Option<[u8; 64]> {
        if self.0[64..].iter().any(|byte| *byte != 0) {
            return None;
        }
        let mut short = [0; 64];
        short.copy_from_slice(&self.0[..64]);
        Some(short)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorFailure {
    Corrupt,
    DifferentInstallation,
}

pub(crate) struct MaintenanceCursor {
    installation: [u8; 32],
    next_category: u8,
    after: [Option<CursorAddress>; CATEGORIES],
}
impl MaintenanceCursor {
    pub(crate) fn new(installation: [u8; 32]) -> Self {
        Self {
            installation,
            next_category: 0,
            after: [None; CATEGORIES],
        }
    }
    pub(crate) fn next_category(&self) -> WorkCategory {
        WorkCategory::from_index(self.next_category as usize)
    }
    pub(crate) fn advance_empty(&mut self, category: WorkCategory) {
        if self.next_category() == category {
            self.next_category = ((category as usize + 1) % CATEGORIES) as u8;
        }
    }
    pub(crate) fn after(&self, category: WorkCategory) -> Option<CursorAddress> {
        self.after[category as usize]
    }

    pub(crate) fn encode(&self) -> [u8; CURSOR_BYTES] {
        let mut bytes = [0u8; CURSOR_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..40].copy_from_slice(&self.installation);
        bytes[40] = self.next_category;
        for (index, address) in self.after.iter().enumerate() {
            if let Some(address) = address {
                let offset = 41 + index * (1 + ADDRESS_BYTES);
                bytes[offset] = 1;
                bytes[offset + 1..offset + 1 + ADDRESS_BYTES].copy_from_slice(&address.0);
            }
        }
        let hash = Sha256::digest(&bytes[..BODY_BYTES]);
        bytes[BODY_BYTES..].copy_from_slice(&hash);
        bytes
    }
    pub(crate) fn decode(
        bytes: &[u8; CURSOR_BYTES],
        installation: [u8; 32],
    ) -> Result<Self, CursorFailure> {
        if &bytes[..8] != MAGIC
            || bytes[40] as usize >= CATEGORIES
            || bytes[BODY_BYTES..] != Sha256::digest(&bytes[..BODY_BYTES])[..]
        {
            return Err(CursorFailure::Corrupt);
        }
        if bytes[8..40] != installation {
            return Err(CursorFailure::DifferentInstallation);
        }
        let mut result = Self::new(installation);
        result.next_category = bytes[40];
        for index in 0..CATEGORIES {
            let offset = 41 + index * (1 + ADDRESS_BYTES);
            match bytes[offset] {
                0 if bytes[offset + 1..offset + 1 + ADDRESS_BYTES]
                    .iter()
                    .all(|byte| *byte == 0) => {}
                1 => {
                    let mut address = [0u8; ADDRESS_BYTES];
                    address.copy_from_slice(&bytes[offset + 1..offset + 1 + ADDRESS_BYTES]);
                    result.after[index] = Some(CursorAddress(address));
                }
                _ => return Err(CursorFailure::Corrupt),
            }
        }
        Ok(result)
    }
    pub(crate) fn selection(&self) -> FairSelection {
        FairSelection {
            next_category: self.next_category,
            after: self.after,
            later: [None; CATEGORIES],
            wrap: [None; CATEGORIES],
        }
    }
    /// Advance for inspected blocked/unknown entries as well as eligible ones.
    /// Persistence is a separately budgeted caller operation, never performed
    /// by this facts-only module.
    pub(crate) fn inspected(&mut self, category: WorkCategory, address: CursorAddress) {
        self.after[category as usize] = Some(address);
        self.next_category = ((category as usize + 1) % CATEGORIES) as u8;
    }
}

pub(crate) struct FairSelection {
    next_category: u8,
    after: [Option<CursorAddress>; CATEGORIES],
    later: [Option<CursorAddress>; CATEGORIES],
    wrap: [Option<CursorAddress>; CATEGORIES],
}
impl FairSelection {
    pub(crate) fn observe(&mut self, category: WorkCategory, address: CursorAddress) {
        let index = category as usize;
        let selected = if self.after[index].is_none_or(|after| address > after) {
            &mut self.later[index]
        } else {
            &mut self.wrap[index]
        };
        if selected.is_none_or(|existing| address < existing) {
            *selected = Some(address);
        }
    }
    pub(crate) fn selected(&self) -> Option<(WorkCategory, CursorAddress)> {
        for delta in 0..CATEGORIES {
            let index = (self.next_category as usize + delta) % CATEGORIES;
            if let Some(address) = self.later[index].or(self.wrap[index]) {
                return Some((WorkCategory::from_index(index), address));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_confirmation_ingress_does_not_pin_legacy_source_or_block_recovery() {
        use super::super::inventory::DirectPair;
        use super::super::layout::DirectLayout;
        use super::super::maintenance_admission::{
            prepare_canonical, prepare_metadata_control, prepare_private_metadata, tests::Fixture,
            with_private_metadata, METADATA_WRITE_CUT,
        };
        use super::super::maintenance_ownership::{self, EpochObservation, ReferenceDomain};
        #[derive(Clone)]
        struct Admission;
        impl MaintenanceAdmission for Admission {
            type Permit = ();
            fn try_acquire(&self) -> Option<()> {
                Some(())
            }
            fn stop_admission(&self) {}
        }
        let mut f = Fixture::new();
        // These real legacy payloads would be pinned by request capture even
        // though the selected destination is an unrelated modern Direct pair.
        let legacy = super::super::inventory::default_direct(&f.target);
        std::fs::write(&legacy.database, b"legacy database").unwrap();
        std::fs::write(&legacy.manifest, b"legacy manifest").unwrap();
        let pair = DirectPair {
            database: f.target.state_root.join("direct").join("graph.ldb"),
            manifest: f.target.state_root.join("direct").join("manifest.json"),
        };
        let layout = DirectLayout::new(&pair.database, &pair.manifest);
        let store = super::super::direct::DirectStore::new(layout.clone()).unwrap();
        drop(store.begin_write().unwrap());
        f.target.direct_pair = Some(pair);
        let domain = ReferenceDomain::direct(&layout).unwrap();
        let EpochObservation::Verified(epoch) =
            maintenance_ownership::observe_reference_epoch(&domain).unwrap()
        else {
            panic!("fresh Direct destination must have a tracked epoch");
        };
        let origin = prepare_canonical(&f.target, &domain, &epoch, &f.memory)
            .unwrap()
            .origin_scope();
        let interrupt = || {
            let mut metadata = prepare_private_metadata(&f.target, &origin, &f.memory).unwrap();
            let mut cursor = MaintenanceCursor::new(origin.incarnation);
            cursor.advance_empty(WorkCategory::Canonical);
            let control = cursor.encode();
            let update = prepare_metadata_control(
                &origin,
                metadata.committed().unwrap(),
                &control,
                &f.memory,
            )
            .unwrap();
            METADATA_WRITE_CUT.with(|value| *value.borrow_mut() = Some(1));
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = with_private_metadata(
                    &mut metadata,
                    &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                    update.effects(),
                    &mut |mutation, grant| mutation.publish_control(&update, grant),
                );
            }))
            .is_err());
            update.next()
        };
        let initial_commit = interrupt();
        let target = AccountedMaintenanceTarget::copy_from(&f.target, &f.memory).unwrap();
        assert!(!target.accepts_legacy_confirmation());
        let (mut runtime, client) =
            start_controller(target, f.memory.clone(), Admission, true).unwrap();
        client.wait_for_first_attempt();
        assert_eq!(
            prepare_private_metadata(&f.target, &origin, &f.memory)
                .unwrap()
                .committed(),
            Some(initial_commit)
        );
        let next_commit = interrupt();
        assert_eq!(
            client.request(Trigger::Health, true),
            RequestDisposition::Enqueued
        );
        assert_eq!(
            client.request(Trigger::Health, false),
            RequestDisposition::Enqueued
        );
        assert!(client.confirm_and_wait());
        assert_eq!(
            prepare_private_metadata(&f.target, &origin, &f.memory)
                .unwrap()
                .committed(),
            Some(next_commit)
        );
        runtime.stop_and_join();
        assert_eq!(std::fs::read(&legacy.database).unwrap(), b"legacy database");
        assert_eq!(std::fs::read(&legacy.manifest).unwrap(), b"legacy manifest");
    }

    fn address(number: u64) -> CursorAddress {
        let mut bytes = [0; ADDRESS_BYTES];
        bytes[..8].copy_from_slice(&number.to_be_bytes());
        CursorAddress(bytes)
    }
    #[test]
    fn dc_cursor_restart_blocked_first_and_category_rotation() {
        let mut cursor = MaintenanceCursor::new([42; 32]);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..510 {
            cursor = MaintenanceCursor::decode(&cursor.encode(), [42; 32]).unwrap();
            let mut selection = cursor.selection();
            // Reverse order falsifies dependence on directory enumeration order.
            for n in (0..250).rev() {
                selection.observe(WorkCategory::Canonical, address(n));
            }
            selection.observe(WorkCategory::Run, address(999));
            let (category, candidate) = selection.selected().unwrap();
            cursor.inspected(category, candidate); // candidate 0 remains blocked.
            if category == WorkCategory::Canonical {
                seen.insert(candidate);
            }
        }
        // Alternating categories prevents the large canonical namespace from
        // starving even one blocked run, while canonical resumes after blocked0.
        assert_eq!(seen.len(), 250);
        assert!(seen.contains(&address(249)));
    }
    #[test]
    fn dc_cursor_wrap_and_all_categories() {
        let mut cursor = MaintenanceCursor::new([1; 32]);
        for _ in 0..2 {
            for expected in 0..CATEGORIES {
                let mut selection = cursor.selection();
                for index in (0..CATEGORIES).rev() {
                    selection.observe(WorkCategory::from_index(index), address(1));
                }
                let (category, selected) = selection.selected().unwrap();
                assert_eq!(category as usize, expected);
                cursor.inspected(category, selected);
            }
        }
        assert!(MaintenanceCursor::new([1; 32])
            .selection()
            .selected()
            .is_none());
    }
    #[test]
    fn dc_cursor_full_generation_address_and_short_adapter() {
        assert_eq!(CURSOR_BYTES, 1105);
        let short = [7; 64];
        assert_eq!(CursorAddress::from_short(short).as_short(), Some(short));
        let long = CursorAddress([b'g'; ADDRESS_BYTES]);
        assert_eq!(long.as_short(), None);
        let mut cursor = MaintenanceCursor::new([3; 32]);
        cursor.inspected(WorkCategory::Generation, long);
        let decoded = MaintenanceCursor::decode(&cursor.encode(), [3; 32]).unwrap();
        assert_eq!(decoded.after(WorkCategory::Generation), Some(long));
        let mut old_version = cursor.encode();
        old_version[..8].copy_from_slice(b"CGMC0001");
        let hash = Sha256::digest(&old_version[..BODY_BYTES]);
        old_version[BODY_BYTES..].copy_from_slice(&hash);
        assert!(matches!(
            MaintenanceCursor::decode(&old_version, [3; 32]),
            Err(CursorFailure::Corrupt)
        ));
    }
    #[test]
    fn dc_measurement_foreign_json_is_unknown_not_retained_coordination() {
        let root = std::env::temp_dir().join(format!(
            "maintenance-unknown-json-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("caller-owned.json");
        let bytes = b"unregistered caller bytes";
        std::fs::write(&path, bytes).unwrap();
        let target = MaintenanceTarget {
            repository_root: root.clone(),
            state_root: root.clone(),
            storage_root: None,
            direct_pair: None,
            config_path: None,
        };
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let (_, _, complete, _, unknown, deferred) =
            observe_bytes_for_test(&target, &memory, &|| false).unwrap();
        assert_eq!(unknown.logical_bytes, Some(bytes.len() as u64));
        assert_eq!(deferred.logical_bytes, None);
        assert!(!complete);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dc_measurement_untraversed_subtree_has_unknown_sizes() {
        let root = std::env::temp_dir().join(format!(
            "maintenance-unknown-subtree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("foreign-directory")).unwrap();
        let payload = root.join("foreign-directory/untouched.bin");
        std::fs::write(&payload, b"unregistered subtree bytes").unwrap();
        let target = MaintenanceTarget {
            repository_root: root.clone(),
            state_root: root.clone(),
            storage_root: None,
            direct_pair: None,
            config_path: None,
        };
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let observation = measurement::observe(&target, &memory, &|| false).unwrap();
        assert!(!observation.complete);
        assert_eq!(observation.total.logical, None);
        assert_eq!(observation.total.allocated, None);
        assert_eq!(observation.unknown.logical, None);
        assert_eq!(
            std::fs::read(payload).unwrap(),
            b"unregistered subtree bytes"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dc_cursor_codec_corruption_and_recreated_root() {
        let cursor = MaintenanceCursor::new([3; 32]);
        let mut bytes = cursor.encode();
        assert!(matches!(
            MaintenanceCursor::decode(&bytes, [4; 32]),
            Err(CursorFailure::DifferentInstallation)
        ));
        bytes[100] ^= 1;
        assert!(matches!(
            MaintenanceCursor::decode(&bytes, [3; 32]),
            Err(CursorFailure::Corrupt)
        ));
    }
}

// Runtime orchestration keeps only fixed reports in its mailbox. Candidate
// payloads stay on the joined advisory thread and never cross the real permit.
use super::inventory::{measurement, MaintenanceTarget};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, MutationGrant, ResourceLimit};
use super::maintenance_candidate::{
    advance_candidate, finish_candidate, prepare_candidate_with_state, CandidateAdvance,
    CandidateError, CandidatePreparationState, CandidateReport, CandidateStatus, PreparedCandidate,
};
use super::maintenance_scheduler::{
    AttemptOutcome, Cancellation, DriverError, MaintenanceDriver, TargetBinding,
};
use std::alloc::Layout;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// The fixed report is independent of the last driver attempt: preparation can
/// fail before any mutation is attempted, without relabelling older evidence.
#[derive(Clone, Copy)]
pub(crate) struct ControllerObservation {
    pub(crate) report: CandidateReport,
    binding: Option<TargetBinding>,
    legacy_binding: bool,
    measurement: Option<measurement::Measurement>,
    pub(crate) attempt_report: Option<CandidateReport>,
    mutation_attempted_entries: Option<usize>,
    pub(crate) observed_at_unix_ms: Option<u64>,
    pub(crate) attempt_started_at_unix_ms: Option<u64>,
    pub(crate) attempt_finished_at_unix_ms: Option<u64>,
    pub(crate) error: Option<CandidateError>,
}
struct ObservationState {
    current: Mutex<ControllerObservation>,
    _reservation: MemoryReservation,
    _emergency_memory: MemoryReservation,
    emergency_busy: Mutex<bool>,
    emergency_wake: Condvar,
}
#[derive(Clone)]
pub(crate) struct MaintenanceObserver {
    state: Arc<ObservationState>,
}
impl std::fmt::Debug for MaintenanceObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MaintenanceObserver")
            .finish_non_exhaustive()
    }
}
impl MaintenanceObserver {
    pub(crate) fn new(memory: &CleanupMemory) -> Result<Self, ResourceLimit> {
        let layout = Layout::new::<[usize; 2]>()
            .extend(Layout::new::<ObservationState>())
            .map_err(|_| ResourceLimit::ArithmeticOverflow)?
            .0
            .pad_to_align();
        let reservation = memory.reserve(layout.size())?;
        let emergency_memory = memory.reserve(PROJECTION_BYTES)?;
        Ok(Self {
            state: Arc::new(ObservationState {
                current: Mutex::new(ControllerObservation {
                    report: CandidateReport::default(),
                    binding: None,
                    legacy_binding: false,
                    measurement: None,
                    attempt_report: None,
                    mutation_attempted_entries: None,
                    observed_at_unix_ms: None,
                    attempt_started_at_unix_ms: None,
                    attempt_finished_at_unix_ms: None,
                    error: None,
                }),
                _reservation: reservation,
                _emergency_memory: emergency_memory,
                emergency_busy: Mutex::new(false),
                emergency_wake: Condvar::new(),
            }),
        })
    }
    pub(crate) fn snapshot(&self) -> ControllerObservation {
        *self
            .state
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
    fn update(&self, update: impl FnOnce(&mut ControllerObservation)) {
        update(
            &mut self
                .state
                .current
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
    }
}

/// Reservation is established before cloning the selected paths. The original
/// selector/runtime is caller infrastructure; this owned copy is cleanup data.
pub(crate) struct AccountedMaintenanceTarget {
    target: MaintenanceTarget,
    _reservation: MemoryReservation,
    _arc_reservation: Option<MemoryReservation>,
}
impl AccountedMaintenanceTarget {
    pub(crate) fn copy_from(
        target: &MaintenanceTarget,
        memory: &CleanupMemory,
    ) -> Result<Self, ResourceLimit> {
        let mut bytes = std::mem::size_of::<MaintenanceTarget>();
        for path in [
            Some(target.repository_root.as_path()),
            Some(target.state_root.as_path()),
            target.storage_root.as_deref(),
            target.config_path.as_deref(),
            target
                .direct_pair
                .as_ref()
                .map(|pair| pair.database.as_path()),
            target
                .direct_pair
                .as_ref()
                .map(|pair| pair.manifest.as_path()),
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes
                .checked_add(path.as_os_str().len())
                .ok_or(ResourceLimit::ArithmeticOverflow)?;
        }
        let reservation = memory.reserve(bytes)?;
        Ok(Self {
            target: target.clone(),
            _reservation: reservation,
            _arc_reservation: None,
        })
    }
    pub(crate) fn from_paths(
        repository: &std::path::Path,
        state: &std::path::Path,
        storage: Option<&std::path::Path>,
        direct: Option<(&std::path::Path, &std::path::Path)>,
        config: Option<&std::path::Path>,
        memory: &CleanupMemory,
    ) -> Result<Self, ResourceLimit> {
        let mut bytes = std::mem::size_of::<MaintenanceTarget>();
        for path in [
            Some(repository),
            Some(state),
            storage,
            config,
            direct.map(|pair| pair.0),
            direct.map(|pair| pair.1),
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes
                .checked_add(path.as_os_str().len())
                .ok_or(ResourceLimit::ArithmeticOverflow)?;
        }
        let reservation = memory.reserve(bytes)?;
        Ok(Self {
            target: MaintenanceTarget {
                repository_root: repository.to_path_buf(),
                state_root: state.to_path_buf(),
                storage_root: storage.map(std::path::Path::to_path_buf),
                direct_pair: direct.map(|(database, manifest)| super::inventory::DirectPair {
                    database: database.to_path_buf(),
                    manifest: manifest.to_path_buf(),
                }),
                config_path: config.map(std::path::Path::to_path_buf),
            },
            _reservation: reservation,
            _arc_reservation: None,
        })
    }

    pub(crate) fn target(&self) -> &MaintenanceTarget {
        &self.target
    }
    fn accepts_legacy_confirmation(&self) -> bool {
        self.target.direct_pair.is_none()
    }
}

pub(crate) struct MaintenanceController<A> {
    target: AccountedMaintenanceTarget,
    observer: MaintenanceObserver,
    last_advance: Option<CandidateAdvance>,
    _admission: A,
    preparation: CandidatePreparationState,
    binding: Option<
        super::maintenance_budget::MemoryBox<super::maintenance_admission::PreparedTargetBinding>,
    >,
}
#[inline(never)]
fn prepare_controller_binding(
    target: &super::inventory::MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<
    super::maintenance_budget::MemoryBox<super::maintenance_admission::PreparedTargetBinding>,
    super::locks::FixedControlError,
> {
    let binding = super::maintenance_admission::prepare_target_binding(target, memory)?;
    super::maintenance_budget::MemoryBox::new(binding, memory)
        .map_err(super::locks::FixedControlError::Resource)
}

impl<A> MaintenanceController<A> {
    pub(crate) fn new(
        target: AccountedMaintenanceTarget,
        observer: MaintenanceObserver,
        admission: A,
    ) -> Self {
        Self {
            target,
            observer,
            last_advance: None,
            _admission: admission,
            preparation: CandidatePreparationState::default(),
            binding: None,
        }
    }
    fn failed(&self, error: CandidateError) -> DriverError {
        self.observer.update(|observation| {
            observation.error = Some(error);
            observation.report.pending = true;
            observation.report.observations_complete = false;
        });
        match error {
            CandidateError::ResourceLimit => DriverError::ResourceLimit,
            _ => DriverError::Preparation,
        }
    }
}
impl<A: MaintenanceAdmission> MaintenanceDriver for MaintenanceController<A> {
    type Work = PreparedCandidate;
    fn completed(&mut self, _outcome: AttemptOutcome) {
        self.observer.update(|observation| {
            observation.attempt_finished_at_unix_ms = Some(controller_unix_ms());
        });
    }
    fn desired_binding(&self) -> Option<TargetBinding> {
        self.binding
            .as_ref()
            .map(|binding| TargetBinding(binding.digest()))
    }
    fn prepare(
        &mut self,
        binding: TargetBinding,
        cancellation: &Cancellation<'_>,
        memory: &CleanupMemory,
    ) -> Result<Option<Self::Work>, DriverError> {
        self.last_advance = None;
        if cancellation.is_cancelled() {
            self.preparation.clear();
            return Err(self.failed(CandidateError::Cancelled));
        }
        self.observer.update(|observation| {
            observation.attempt_started_at_unix_ms = Some(controller_unix_ms());
            observation.attempt_finished_at_unix_ms = None;
            observation.attempt_report = None;
            observation.mutation_attempted_entries = None;
            observation.error = None;
        });
        let current = self
            .binding
            .as_mut()
            .is_some_and(|binding| binding.is_current().unwrap_or(false));
        if !current {
            self.preparation.clear();
            self.binding = None;
            let witness = prepare_controller_binding(self.target.target(), memory)
                .map_err(|_| self.failed(CandidateError::Stale))?;
            self.binding = Some(witness);
            self.observer.update(|observation| {
                observation.binding = self.desired_binding();
                observation.legacy_binding = self
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.is_legacy());
                observation.measurement = None;
                observation.observed_at_unix_ms = None;
                observation.report = CandidateReport::default();
            });
        }
        if self.desired_binding() != Some(binding) {
            return Err(self.failed(CandidateError::Stale));
        }
        let measurement = measurement::observe(self.target.target(), memory, &|| {
            cancellation.is_cancelled()
        })
        .map_err(|_| self.failed(CandidateError::ResourceLimit))?;
        self.observer.update(|observation| {
            observation.measurement = Some(measurement);
            observation.observed_at_unix_ms = Some(controller_unix_ms());
        });
        self.preparation
            .set_legacy_interest(cancellation.legacy_confirmation_pending());
        self.preparation
            .set_legacy_confirmation_metadata(cancellation.legacy_confirmation_metadata());
        let mut report = CandidateReport::default();
        let prepared = prepare_candidate_with_state(
            self.target.target(),
            self.binding
                .as_ref()
                .expect("retained current target witness"),
            &mut self.preparation,
            memory,
            &|| cancellation.is_cancelled(),
            &mut report,
        )
        .map_err(|error| self.failed(error))?;
        self.observer.update(|observation| {
            observation.report = report;
            observation.attempt_report = Some(report);
        });
        Ok(prepared)
    }
    fn requires_legacy_confirmation(&self, work: &Self::Work) -> bool {
        work.requires_legacy_confirmation()
    }
    fn source_payloads_parked(&self) -> bool {
        self.preparation.has_parked_payloads()
    }
    fn mutate(
        &mut self,
        work: &Self::Work,
        grant: &mut MutationGrant,
        confirmation: Option<
            &crate::storage::maintenance_scheduler::LegacyQuiescenceConfirmation<'_>,
        >,
    ) -> Result<AttemptOutcome, DriverError> {
        let starting_entries = grant.remaining_entries();
        let result = advance_candidate(work.borrow_hot_frame(), grant, confirmation);
        self.observer.update(|observation| {
            observation.mutation_attempted_entries =
                Some(starting_entries.saturating_sub(grant.remaining_entries()));
        });
        let advance = result.map_err(|error| {
            let failure = self.failed(error);
            if failure == DriverError::ResourceLimit {
                failure
            } else {
                DriverError::Mutation
            }
        })?;
        self.last_advance = Some(advance);
        self.observer.update(|observation| {
            observation.attempt_report = Some(advance.report);
        });
        Ok(candidate_outcome(advance.status))
    }
    fn finish(
        &mut self,
        work: &mut Self::Work,
        outcome: AttemptOutcome,
        cancellation: &Cancellation<'_>,
        _: &CleanupMemory,
    ) -> Result<AttemptOutcome, DriverError> {
        let Some(advance) = self.last_advance.take() else {
            return Ok(outcome);
        };
        let report = finish_candidate(work, advance, &mut self.preparation, &|| {
            cancellation.is_cancelled()
        })
        .map_err(|error| self.failed(error))?;
        self.observer.update(|observation| {
            observation.report = report;
            observation.attempt_report = Some(report);
        });
        if cancellation.is_cancelled() {
            return Ok(AttemptOutcome::Cancelled);
        }
        if report.pending {
            Ok(AttemptOutcome::Pending)
        } else {
            Ok(candidate_outcome(advance.status))
        }
    }
}
fn candidate_outcome(status: CandidateStatus) -> AttemptOutcome {
    match status {
        CandidateStatus::Complete => AttemptOutcome::Complete,
        CandidateStatus::Pending | CandidateStatus::NeedsDisposal => AttemptOutcome::Pending,
        CandidateStatus::Deferred => AttemptOutcome::Deferred,
    }
}
fn controller_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

use super::maintenance_scheduler::{
    DriverLiveness, MaintenanceAdmission, MaintenanceHandle, MaintenanceRuntime,
    RequestDisposition, Trigger,
};
use crate::protocol::{MaintenanceAttempt, MaintenanceAttemptStatus, MaintenanceSummary};

/// Cleanup has its own aggregate requested-layout ceiling, below the normal
/// native worker budget. It is shared by target copies, reports and candidates.
pub(crate) const MAINTENANCE_MEMORY_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct MaintenanceClient<A> {
    handle: MaintenanceHandle<A>,
    observer: MaintenanceObserver,
    memory: CleanupMemory,
    target: Arc<AccountedMaintenanceTarget>,
}
impl<A> Clone for MaintenanceClient<A> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            observer: self.observer.clone(),
            memory: self.memory.clone(),
            target: Arc::clone(&self.target),
        }
    }
}
impl<A> std::fmt::Debug for MaintenanceClient<A> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MaintenanceClient")
            .finish_non_exhaustive()
    }
}
impl<A> MaintenanceClient<A> {
    pub(crate) fn request(&self, trigger: Trigger, confirm: bool) -> RequestDisposition {
        let confirm = confirm && self.target.accepts_legacy_confirmation();
        let Some(binding) = self.observer.snapshot().binding else {
            return if confirm {
                RequestDisposition::UnboundConfirmation
            } else {
                self.handle.request(trigger, None)
            };
        };
        let _capture = if confirm {
            match self.handle.begin_source_capture() {
                Some(capture) => Some(capture),
                None => return RequestDisposition::UnboundConfirmation,
            }
        } else {
            None
        };
        let source = if confirm {
            let source = match super::maintenance_admission::prepare_legacy_confirmation_source(
                self.target.target(),
                &self.memory,
            ) {
                Ok(source) => source,
                Err(_) => return RequestDisposition::UnboundConfirmation,
            };
            match super::maintenance_budget::MemoryBox::new(source, &self.memory) {
                Ok(source) => Some(source),
                Err(_) => return RequestDisposition::UnboundConfirmation,
            }
        } else {
            None
        };
        self.handle.rebind(binding);
        self.handle
            .request_with_source(trigger, confirm.then_some(binding), source)
    }
    pub(crate) fn wait_for_first_attempt(&self) {
        loop {
            let snapshot = self.handle.wait_for_first_attempt();
            if matches!(
                snapshot.liveness,
                DriverLiveness::Stopped | DriverLiveness::Failed
            ) || snapshot.last_attempt.is_some_and(|attempt| {
                attempt.binding == snapshot.binding && attempt.generation == snapshot.generation
            }) {
                return;
            }
            // Initial legacy witness acquisition changes the placeholder
            // binding. That preparation-only handoff is not the transient
            // owner's promised advisory pass for the now-bound destination.
        }
    }
    pub(crate) fn confirm_and_wait(&self) -> bool {
        // A placeholder-generation handoff is not the bound target's startup
        // pass. Wait for that pass and its payload disposal before reserving
        // request-time source pins; ordinary busy/parked checks still apply.
        self.wait_for_first_attempt();
        if !self.target.accepts_legacy_confirmation() {
            let binding = self.observer.snapshot().binding;
            let snapshot = self.handle.request_and_wait(Trigger::Health, None);
            return binding == Some(snapshot.binding)
                && !matches!(
                    snapshot.liveness,
                    DriverLiveness::Failed | DriverLiveness::Stopped
                );
        }
        if let Some(binding) = self.observer.snapshot().binding {
            let Some(capture) = self.handle.begin_source_capture() else {
                return false;
            };
            let source = match super::maintenance_admission::prepare_legacy_confirmation_source(
                self.target.target(),
                &self.memory,
            )
            .and_then(|source| {
                super::maintenance_budget::MemoryBox::new(source, &self.memory)
                    .map_err(super::locks::FixedControlError::Resource)
            }) {
                Ok(source) => source,
                Err(_) => return false,
            };
            self.handle.rebind(binding);
            let disposition =
                self.handle
                    .request_with_source(Trigger::Health, Some(binding), Some(source));
            drop(capture);
            if disposition != RequestDisposition::Enqueued {
                return false;
            }
            let snapshot = self.handle.request_and_wait(Trigger::Health, None);
            return snapshot.binding == binding
                && !matches!(snapshot.liveness, DriverLiveness::Failed);
        }
        false
    }
    pub(crate) fn projection(&self) -> MaintenanceProjection {
        match self.memory.reserve(PROJECTION_BYTES) {
            Ok(reservation) => MaintenanceProjection {
                summary: self.build_summary(),
                _reservation: Some(reservation),
                _emergency: None,
            },
            Err(_) => {
                // Real coordinator execution is serialized. The parked fallback
                // also serializes unusual internal direct Core clones, retaining
                // one startup-precharged minimal projection through handoff.
                let mut busy = self
                    .observer
                    .state
                    .emergency_busy
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                while *busy {
                    busy = self
                        .observer
                        .state
                        .emergency_wake
                        .wait(busy)
                        .unwrap_or_else(|error| error.into_inner());
                }
                *busy = true;
                drop(busy);
                let emergency = EmergencyProjection {
                    state: Arc::clone(&self.observer.state),
                };
                let mut summary = MaintenanceSummary {
                    requested: self.handle.snapshot().requested,
                    ..MaintenanceSummary::default()
                };
                summary
                    .reasons
                    .push("maintenance_projection_resource_limit".into());
                summary
                    .errors
                    .push("maintenance memory ceiling reached while reporting".into());
                MaintenanceProjection {
                    summary,
                    _reservation: None,
                    _emergency: Some(emergency),
                }
            }
        }
    }

    fn build_summary(&self) -> MaintenanceSummary {
        let driver = self.handle.snapshot();
        let observation = self.observer.snapshot();
        let mut summary = MaintenanceSummary {
            requested: driver.requested,
            running: driver.running,
            pending: driver.pending
                || observation.report.pending
                || observation.report.unknown
                || observation.observed_at_unix_ms.is_none()
                || observation.error.is_some(),
            observed_at_unix_ms: observation.observed_at_unix_ms,
            observation_age_ms: observation
                .observed_at_unix_ms
                .map(|at| controller_unix_ms().saturating_sub(at)),
            observation_complete: observation.observed_at_unix_ms.is_some()
                && observation.report.observations_complete
                && observation.report.checked_categories == u8::MAX,
            measurement_complete: observation
                .measurement
                .is_some_and(|measurement| measurement.complete),
            ..MaintenanceSummary::default()
        };
        if let Some(measurement) = observation.measurement {
            let bucket = |value: measurement::Bucket| crate::protocol::MaintenanceByteBucket {
                logical_bytes: value.logical,
                allocated_bytes: value.allocated,
            };
            summary.bytes.total = bucket(measurement.total);
            summary.bytes.retained = bucket(measurement.retained);
            summary.bytes.deferred = bucket(measurement.deferred);
            summary.bytes.unknown = bucket(measurement.unknown);
            summary.bytes.by_category = crate::protocol::MaintenanceCategoryBytes {
                active_graph: bucket(measurement.categories[0]),
                active_cache: bucket(measurement.categories[1]),
                leased_generation: bucket(measurement.categories[2]),
                live_run: bucket(measurement.categories[3]),
                recovery_evidence: bucket(measurement.categories[4]),
                artifact_temporary: bucket(measurement.categories[5]),
                legacy_staging: bucket(measurement.categories[6]),
                retired_direct: bucket(measurement.categories[7]),
                coordination: bucket(measurement.categories[8]),
                unknown: bucket(measurement.categories[9]),
            };
        }
        summary.reasons.clear();
        if observation.legacy_binding {
            summary.reasons.push("legacy_quiescence_required".into());
        }
        if let Some(measurement) = observation.measurement {
            if !measurement.complete {
                summary
                    .reasons
                    .push("maintenance_measurement_partial_lower_bounds".into());
            }
            if !measurement.references_complete {
                summary
                    .reasons
                    .push("active_cache_references_incomplete".into());
            }
            if measurement.resource_limited {
                summary
                    .reasons
                    .push("maintenance_measurement_resource_limit".into());
            }
            if measurement.allocation_unknown {
                summary.reasons.push("allocated_bytes_unavailable".into());
            }
            if measurement.unknown_entries {
                summary.reasons.push("unknown_entries_retained".into());
            }
        }
        if observation.observed_at_unix_ms.is_none() {
            summary.reasons.push("maintenance_not_observed".into());
        }
        for bit in 0..64 {
            if observation.report.reasons & (1u64 << bit) != 0 {
                if let Some(reason) = super::maintenance_candidate::candidate_reason(bit) {
                    summary.reasons.push(reason.into());
                }
            }
        }
        if observation.report.deferred {
            summary
                .reasons
                .push("maintenance_candidate_deferred".into());
        }
        if observation.report.unknown {
            summary.reasons.push("maintenance_candidate_unknown".into());
        }
        if !summary.observation_complete {
            summary
                .reasons
                .push("maintenance_observation_incomplete".into());
        }
        if let Some(error) = observation.error {
            summary.errors.push(
                match error {
                    CandidateError::ResourceLimit => "maintenance memory ceiling reached",
                    CandidateError::Io(_) => "maintenance filesystem observation failed",
                    CandidateError::Stale => "maintenance target changed during preparation",
                    CandidateError::InvalidEvidence => "maintenance evidence is invalid or unknown",
                    CandidateError::Cancelled => "maintenance cancelled",
                }
                .into(),
            );
        }
        if let Some(failure) = driver.failure {
            summary
                .errors
                .push(format!("maintenance worker failed: {failure:?}"));
        }
        if driver.liveness == DriverLiveness::Stopped {
            summary.reasons.push("maintenance_stopped".into());
        }
        if let Some(started) = observation.attempt_started_at_unix_ms {
            let finished = observation.attempt_finished_at_unix_ms;
            summary.last_attempt = Some(MaintenanceAttempt {
                started_at_unix_ms: started,
                finished_at_unix_ms: finished,
                status: finished.map(|_| {
                    if observation.error.is_some() {
                        MaintenanceAttemptStatus::Invalid
                    } else if observation.report.deferred {
                        MaintenanceAttemptStatus::Deferred
                    } else if observation.report.pending
                        || !observation.report.observations_complete
                    {
                        MaintenanceAttemptStatus::Partial
                    } else {
                        MaintenanceAttemptStatus::Complete
                    }
                }),
                attempted_entries: finished.and(observation.mutation_attempted_entries),
                deleted_entries: finished
                    .and(observation.attempt_report)
                    .and_then(|report| usize::try_from(report.reclaimed_entries).ok()),
                reclaimed_allocated_bytes: finished
                    .and(observation.attempt_report)
                    .and_then(|report| report.reclaimed_allocated_bytes),
                continuation_phase: None,
            });
        }
        summary
    }
}

// Fixed source bounds: <=80 static reasons (each <=64 bytes), <=2 fixed-code
// errors (each <=128 bytes), 16 byte buckets and one fixed attempt. 256 KiB
// conservatively covers the DTO, Value/BTree nodes, structured wrapper and block
// rendering copies simultaneously. Normal graph/application payload is caller
// output, not maintenance working memory. The guard survives private formatting
// and serialization and ends at the public owned-response handoff.
const PROJECTION_BYTES: usize = 256 * 1024;
struct EmergencyProjection {
    state: Arc<ObservationState>,
}
impl Drop for EmergencyProjection {
    fn drop(&mut self) {
        *self
            .state
            .emergency_busy
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = false;
        self.state.emergency_wake.notify_all();
    }
}
pub(crate) struct MaintenanceProjection {
    pub(crate) summary: MaintenanceSummary,
    _reservation: Option<MemoryReservation>,
    _emergency: Option<EmergencyProjection>,
}
impl MaintenanceProjection {
    pub(crate) fn unavailable(error: DriverError) -> Self {
        let memory = CleanupMemory::new(MAINTENANCE_MEMORY_BYTES).ok();
        let reservation = memory
            .as_ref()
            .and_then(|memory| memory.reserve(PROJECTION_BYTES).ok());
        let mut summary = MaintenanceSummary::default();
        summary
            .reasons
            .push("maintenance_initialization_failed".into());
        summary
            .errors
            .push(format!("maintenance unavailable: {error:?}"));
        Self {
            summary,
            _reservation: reservation,
            _emergency: None,
        }
    }
}

pub(crate) fn start_controller<A: MaintenanceAdmission + Clone>(
    target: AccountedMaintenanceTarget,
    memory: CleanupMemory,
    admission: A,
    confirmation: bool,
) -> Result<(MaintenanceRuntime<A>, MaintenanceClient<A>), DriverError> {
    let binding =
        super::maintenance_admission::observe_maintenance_binding(target.target(), &memory).ok();
    let initial_source = if confirmation
        && target.accepts_legacy_confirmation()
        && binding.is_some()
    {
        super::maintenance_admission::prepare_legacy_confirmation_source(target.target(), &memory)
            .ok()
            .and_then(|source| super::maintenance_budget::MemoryBox::new(source, &memory).ok())
    } else {
        None
    };
    let initial_confirmation = initial_source.is_some();
    let binding = binding.unwrap_or([0; 32]);
    let observer = MaintenanceObserver::new(&memory).map_err(|_| DriverError::ResourceLimit)?;
    let worker_target = AccountedMaintenanceTarget::copy_from(target.target(), &memory)
        .map_err(|_| DriverError::ResourceLimit)?;
    let client_target = target;
    let arc_layout = Layout::new::<[usize; 2]>()
        .extend(Layout::new::<AccountedMaintenanceTarget>())
        .map_err(|_| DriverError::ResourceLimit)?
        .0
        .pad_to_align();
    // Retain Arc's control words together with this target's owned buffers.
    let mut client_target = client_target;
    let arc_reservation = memory
        .reserve(arc_layout.size())
        .map_err(|_| DriverError::ResourceLimit)?;
    client_target._arc_reservation = Some(arc_reservation);
    let target = Arc::new(client_target);
    let worker_observer = observer.clone();
    let worker_admission = admission.clone();
    let factory = move |_: &CleanupMemory| {
        Ok(MaintenanceController::new(
            worker_target,
            worker_observer,
            worker_admission,
        ))
    };
    let runtime = if initial_confirmation {
        MaintenanceRuntime::start_with_confirmation(
            memory.clone(),
            TargetBinding(binding),
            admission,
            factory,
            true,
            initial_source,
        )
    } else {
        MaintenanceRuntime::start(memory.clone(), TargetBinding(binding), admission, factory)
    }
    .map_err(|_| DriverError::ResourceLimit)?;
    let client = MaintenanceClient {
        handle: runtime.handle(),
        observer,
        memory,
        target,
    };
    Ok((runtime, client))
}

#[cfg(test)]
pub(crate) fn observe_bytes_for_test(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<
    (
        crate::protocol::MaintenanceByteBucket,
        crate::protocol::MaintenanceByteBucket,
        bool,
        bool,
        crate::protocol::MaintenanceByteBucket,
        crate::protocol::MaintenanceByteBucket,
    ),
    ResourceLimit,
> {
    let observation = measurement::observe(target, memory, cancel)?;
    let bucket = |value: measurement::Bucket| crate::protocol::MaintenanceByteBucket {
        logical_bytes: value.logical,
        allocated_bytes: value.allocated,
    };
    Ok((
        bucket(observation.categories[0]),
        bucket(observation.categories[1]),
        observation.complete,
        observation.references_complete,
        bucket(observation.unknown),
        bucket(observation.deferred),
    ))
}
