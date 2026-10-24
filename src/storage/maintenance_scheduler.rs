//! Joined advisory-driver foundation. This module performs no storage collection.
//! Drivers must reserve every dynamic allocation from the supplied lifetime account.
use super::maintenance_admission::{LegacyConfirmationSource, PreparedLegacyConfirmationSource};
use super::maintenance_budget::{
    CleanupMemory, MemoryBox, MemoryReservation, MutationGrant, ResourceLimit,
};
use std::alloc::Layout;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);
const THREAD_STACK: usize = 256 * 1024;
// Requested-layout allowance, not allocator slack/RSS. Rust 1.96 std/thread
// lifecycle.rs allocates Packet, rust_start and ThreadInit, and thread.rs owns
// the Thread handle. The native allocation probe observed <256 bytes beyond
// Shared/captures without a name; reserve 4 KiB for those fixed blocks and
// platform hooks/TLS scaffolding. Closure layout/padding is charged separately.
// Toolchain/platform qualification must rerun the instrumented boundary probe.
const THREAD_STARTUP_ALLOWANCE: usize = 4096;

/// Validated opaque target identity; paths/proofs belong to the accounted driver.
/// Equality must include the installation and physical destination incarnation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TargetBinding(pub(crate) [u8; 32]);

/// A borrowed, one-use operator demand captured only by the post-permit
/// mailbox gate. Neither a bool nor stored options can construct this token.
/// Source/version evidence remains in the separately rechecked preparation.
pub(crate) struct LegacyQuiescenceConfirmation<'a> {
    binding: &'a TargetBinding,
    generation: &'a u64,
    source: Option<LegacyConfirmationSource>,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl LegacyQuiescenceConfirmation<'_> {
    pub(crate) fn binding_digest(&self) -> [u8; 32] {
        self.binding.0
    }
    pub(crate) fn generation(&self) -> u64 {
        *self.generation
    }
    pub(crate) fn source(&self) -> Option<&LegacyConfirmationSource> {
        self.source.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriverError {
    ResourceLimit,
    Preparation,
    Mutation,
    Panicked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptOutcome {
    Complete,
    Pending,
    Deferred,
    Cancelled,
    Failed(DriverError),
}

/// Fixed-size control evidence, deliberately distinct from measured storage health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DriverSnapshot {
    pub(crate) binding: TargetBinding,
    pub(crate) generation: u64,
    pub(crate) requested: bool,
    pub(crate) pending: bool,
    pub(crate) running: bool,
    pub(crate) liveness: DriverLiveness,
    pub(crate) last_attempt: Option<AttemptRecord>,
    pub(crate) failure: Option<DriverError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttemptRecord {
    pub(crate) binding: TargetBinding,
    pub(crate) generation: u64,
    pub(crate) request_sequence: u64,
    pub(crate) triggers: u8,
    pub(crate) outcome: AttemptOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriverLiveness {
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestDisposition {
    Enqueued,
    UnboundConfirmation,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Startup = 1,
    Completion = 2,
    Health = 4,
    Continuation = 8,
}

/// Only this interface may acquire the existing actual dispatcher slot. Its permit
/// must release admission on Drop and cannot contain any advisory/bulk work.
pub(crate) trait MaintenanceAdmission: Send + Sync + 'static {
    type Permit;
    fn try_acquire(&self) -> Option<Self::Permit>;
    fn stop_admission(&self);

    // Integration fixtures exercise the post-disposal dispatcher seam;
    // shipping recovery remains in the actual producer/read entry points.
    #[cfg(test)]
    fn run_recovery(
        &self,
        _operation: &mut dyn FnMut() -> Result<(), DriverError>,
    ) -> Result<bool, DriverError> {
        Ok(false)
    }
}

/// Constructed on the dedicated thread; neither Driver nor Work need Send/Sync.
/// Work must own no storage authority, and Drop must perform no filesystem effects.
/// mutate only immutably borrows bulk Work; no bulk ownership may be moved into
/// the callback or destroyed there (including driver-held buffers).
/// Storage admission and all authority stay within
/// mutate, and only fixed, bounded hot frames may be consumed by inner callbacks.
pub(crate) trait MaintenanceDriver {
    type Work;
    fn desired_binding(&self) -> Option<TargetBinding> {
        None
    }
    /// Fixed status publication only, after all work/disposal/recovery ends.
    fn completed(&mut self, _outcome: AttemptOutcome) {}
    fn prepare(
        &mut self,
        binding: TargetBinding,
        cancellation: &Cancellation<'_>,
        memory: &CleanupMemory,
    ) -> Result<Option<Self::Work>, DriverError>;
    fn requires_legacy_confirmation(&self, _work: &Self::Work) -> bool {
        false
    }
    fn source_payloads_parked(&self) -> bool {
        false
    }
    fn mutate(
        &mut self,
        work: &Self::Work,
        grant: &mut MutationGrant,
        confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    ) -> Result<AttemptOutcome, DriverError>;
    /// One advisory validation/disposal continuation, after the real permit and
    /// every storage authority have been released. Bulk Work remains owned by
    /// the driver loop throughout this call and its eventual destruction.
    fn finish(
        &mut self,
        _work: &mut Self::Work,
        outcome: AttemptOutcome,
        _cancellation: &Cancellation<'_>,
        _memory: &CleanupMemory,
    ) -> Result<AttemptOutcome, DriverError> {
        Ok(outcome)
    }
    /// Follow-up producer recovery starts only after every prepared payload and
    /// its source handles have been disposed outside the collector permit.
    fn after_disposal(
        &mut self,
        outcome: AttemptOutcome,
        _cancellation: &Cancellation<'_>,
        _memory: &CleanupMemory,
    ) -> Result<AttemptOutcome, DriverError> {
        Ok(outcome)
    }
}

/// Pure coalescing clock state. New requests never slide an existing due time.
struct MaintenanceScheduler {
    interval: Duration,
    next_due: Duration,
    pending: u8,
    sequence: u64,
}
impl MaintenanceScheduler {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_due: Duration::ZERO,
            pending: Trigger::Startup as u8,
            sequence: 0,
        }
    }
    fn request(&mut self, trigger: Trigger) {
        self.pending |= trigger as u8;
        self.sequence = self.sequence.saturating_add(1);
    }
    fn begin(&mut self, now: Duration) -> Option<(u8, u64)> {
        if now < self.next_due {
            return None;
        }
        let triggers = std::mem::take(&mut self.pending);
        self.next_due = now.saturating_add(self.interval);
        Some((triggers, self.sequence))
    }
    fn complete(&mut self, now: Duration, outcome: AttemptOutcome) {
        // Cooldown starts after actual work/disposal, not after preparation start.
        self.next_due = now.saturating_add(self.interval);
        if outcome != AttemptOutcome::Complete {
            self.pending |= Trigger::Continuation as u8;
        }
    }
}

struct Control {
    binding: TargetBinding,
    generation: u64,
    confirmation: bool,
    confirmation_source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    claimed_source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    capturing_source: bool,
    source_payloads_parked: bool,
    stopping: bool,
    scheduler: MaintenanceScheduler,
    snapshot: DriverSnapshot,
}

struct Shared<A> {
    control: Mutex<Control>,
    wake: Condvar,
    admission: A,
    memory: CleanupMemory,
    // Retains runtime/control/closure/explicit stack layout charge through join
    // and all driver/work disposal. This is requested-layout accounting, not RSS.
    _reservation: MemoryReservation,
}

pub(crate) struct Cancellation<'a> {
    check: &'a dyn Fn() -> bool,
    legacy_interest: &'a dyn Fn() -> bool,
    legacy_metadata: &'a dyn Fn() -> Option<(u64, Option<LegacyConfirmationSource>)>,
}
impl Cancellation<'_> {
    pub(crate) fn is_cancelled(&self) -> bool {
        (self.check)()
    }
    /// Advisory scheduling interest only; this cannot authorize enrollment.
    pub(crate) fn legacy_confirmation_pending(&self) -> bool {
        (self.legacy_interest)()
    }
    /// Advisory bytes for off-permit preparation. Only the actual claimed
    /// non-Send token can authorize the matching mutation later.
    pub(crate) fn legacy_confirmation_metadata(
        &self,
    ) -> Option<(u64, Option<LegacyConfirmationSource>)> {
        (self.legacy_metadata)()
    }
}

/// Clone owns only bounded control state. It never owns Runtime, API or refresh.
pub(crate) struct MaintenanceHandle<A> {
    shared: Arc<Shared<A>>,
}
impl<A> Clone for MaintenanceHandle<A> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}
/// A fixed reservation prevents two request threads from opening duplicate
/// source pins. The worker cannot begin another preparation until it is dropped.
pub(crate) struct SourceCapture<A> {
    shared: Arc<Shared<A>>,
}
impl<A> Drop for SourceCapture<A> {
    fn drop(&mut self) {
        self.shared
            .control
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .capturing_source = false;
        self.shared.wake.notify_all();
    }
}
impl<A> MaintenanceHandle<A> {
    pub(crate) fn begin_source_capture(&self) -> Option<SourceCapture<A>> {
        let previous = {
            let mut control = self
                .shared
                .control
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if control.stopping
                || control.snapshot.liveness == DriverLiveness::Failed
                || control.snapshot.running
                || control.source_payloads_parked
                || control.claimed_source.is_some()
                || control.capturing_source
            {
                return None;
            }
            control.capturing_source = true;
            control.confirmation = false;
            control.confirmation_source.take()
        };
        // This request owns no execution authority. Old pins are gone BEFORE
        // the replacement request opens its source pair.
        drop(previous);
        Some(SourceCapture {
            shared: Arc::clone(&self.shared),
        })
    }
    pub(crate) fn snapshot(&self) -> DriverSnapshot {
        self.shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
    }
    /// A transient owner may wait for its startup opportunity without polling
    /// or imposing a deadline on preparation, validation or disposal. The
    /// returned attempt is published only after bulk work has been dropped.
    /// Rebinding returns the new snapshot so its owner can discard old demand;
    /// stopping waits for actual worker disposal and terminal publication.
    pub(crate) fn wait_for_first_attempt(&self) -> DriverSnapshot {
        let mut control = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let binding = control.binding;
        let generation = control.generation;
        loop {
            if control.binding != binding
                || control.generation != generation
                || matches!(
                    control.snapshot.liveness,
                    DriverLiveness::Stopped | DriverLiveness::Failed
                )
                || control.snapshot.last_attempt.is_some_and(|attempt| {
                    attempt.binding == binding && attempt.generation == generation
                })
            {
                return control.snapshot;
            }
            control = self
                .shared
                .wake
                .wait(control)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Confirmation: busy/preparation-error retains the exact current demand;
    /// the post-permit gate takes it once, including callback error/panic or an
    /// expired grant. Rebind/stop/failure discard it. A new in-flight demand stays
    /// pending for a later gate. False never creates or clears confirmation.
    pub(crate) fn request(
        &self,
        trigger: Trigger,
        confirmation: Option<TargetBinding>,
    ) -> RequestDisposition {
        self.request_with_source(trigger, confirmation, None)
    }
    pub(crate) fn request_with_source(
        &self,
        trigger: Trigger,
        confirmation: Option<TargetBinding>,
        source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    ) -> RequestDisposition {
        let mut control = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if control.snapshot.liveness == DriverLiveness::Failed {
            return RequestDisposition::Failed;
        }
        if control.stopping {
            return RequestDisposition::Stopped;
        }
        if confirmation.is_some_and(|binding| binding != control.binding) {
            return RequestDisposition::UnboundConfirmation;
        }
        if confirmation.is_some() {
            control.confirmation_source = source;
        }
        control.confirmation |= confirmation.is_some();
        control.scheduler.request(trigger);
        control.snapshot.requested = true;
        control.snapshot.pending = true;
        self.shared.wake.notify_one();
        RequestDisposition::Enqueued
    }
    pub(crate) fn request_and_wait(
        &self,
        trigger: Trigger,
        confirmation: Option<TargetBinding>,
    ) -> DriverSnapshot {
        self.request_and_wait_with_source(trigger, confirmation, None)
    }
    pub(crate) fn request_and_wait_with_source(
        &self,
        trigger: Trigger,
        confirmation: Option<TargetBinding>,
        source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    ) -> DriverSnapshot {
        let mut control = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if control.stopping
            || control.snapshot.liveness == DriverLiveness::Failed
            || confirmation.is_some_and(|binding| binding != control.binding)
        {
            return control.snapshot;
        }
        if confirmation.is_some() {
            control.confirmation_source = source;
        }
        control.confirmation |= confirmation.is_some();
        control.scheduler.request(trigger);
        control.snapshot.requested = true;
        control.snapshot.pending = true;
        let binding = control.binding;
        let generation = control.generation;
        let sequence = control.scheduler.sequence;
        self.shared.wake.notify_all();
        loop {
            if control.binding != binding
                || control.generation != generation
                || matches!(
                    control.snapshot.liveness,
                    DriverLiveness::Stopped | DriverLiveness::Failed
                )
                || control.snapshot.last_attempt.is_some_and(|attempt| {
                    attempt.binding == binding
                        && attempt.generation == generation
                        && attempt.request_sequence >= sequence
                })
            {
                return control.snapshot;
            }
            control = self
                .shared
                .wake
                .wait(control)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    pub(crate) fn rebind(&self, binding: TargetBinding) -> RequestDisposition {
        let mut control = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if control.snapshot.liveness == DriverLiveness::Failed {
            return RequestDisposition::Failed;
        }
        if control.stopping {
            return RequestDisposition::Stopped;
        }
        if binding != control.binding {
            let Some(generation) = control.generation.checked_add(1) else {
                control.stopping = true;
                control.snapshot.liveness = DriverLiveness::Failed;
                self.shared.wake.notify_one();
                return RequestDisposition::Failed;
            };
            control.binding = binding;
            control.generation = generation;
            control.confirmation = false;
            control.confirmation_source = None;
            control.snapshot.binding = binding;
            control.snapshot.generation = generation;
            // Old evidence keeps its old binding/generation; never relabel it.
            control.scheduler.request(Trigger::Startup);
            control.snapshot.pending = true;
        }
        self.shared.wake.notify_one();
        RequestDisposition::Enqueued
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartError {
    ResourceLimit(ResourceLimit),
    SpawnFailed,
}

/// Unique owner of the join handle. Never cloned or captured by its own worker.
pub(crate) struct MaintenanceRuntime<A: MaintenanceAdmission> {
    handle: MaintenanceHandle<A>,
    thread: Option<JoinHandle<()>>,
}
impl<A: MaintenanceAdmission> MaintenanceRuntime<A> {
    /// The caller creates ONE account before any dynamic binding/factory data.
    /// Captured owned buffers must already retain reservations on this account;
    /// external infrastructure (dispatcher/fixture channels) is not cleanup data.
    /// Driver allocations must use this same account; it is never replaced.
    pub(crate) fn start<D, F>(
        memory: CleanupMemory,
        binding: TargetBinding,
        admission: A,
        factory: F,
    ) -> Result<Self, StartError>
    where
        D: MaintenanceDriver,
        F: FnOnce(&CleanupMemory) -> Result<D, DriverError> + Send + 'static,
    {
        Self::start_with_interval(memory, binding, admission, factory, DEFAULT_INTERVAL)
    }

    pub(crate) fn start_with_confirmation<D, F>(
        memory: CleanupMemory,
        binding: TargetBinding,
        admission: A,
        factory: F,
        confirmation: bool,
        source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    ) -> Result<Self, StartError>
    where
        D: MaintenanceDriver,
        F: FnOnce(&CleanupMemory) -> Result<D, DriverError> + Send + 'static,
    {
        Self::start_with_options(
            memory,
            binding,
            admission,
            factory,
            DEFAULT_INTERVAL,
            confirmation,
            source,
        )
    }

    fn start_with_interval<D, F>(
        memory: CleanupMemory,
        binding: TargetBinding,
        admission: A,
        factory: F,
        interval: Duration,
    ) -> Result<Self, StartError>
    where
        D: MaintenanceDriver,
        F: FnOnce(&CleanupMemory) -> Result<D, DriverError> + Send + 'static,
    {
        Self::start_with_options(memory, binding, admission, factory, interval, false, None)
    }

    fn start_with_options<D, F>(
        memory: CleanupMemory,
        binding: TargetBinding,
        admission: A,
        factory: F,
        interval: Duration,
        initial_confirmation: bool,
        initial_source: Option<MemoryBox<PreparedLegacyConfirmationSource>>,
    ) -> Result<Self, StartError>
    where
        D: MaintenanceDriver,
        F: FnOnce(&CleanupMemory) -> Result<D, DriverError> + Send + 'static,
    {
        // The account predates every control allocation. Fixed copy snapshots
        // avoid an uncharged protocol DTO clone on health's request thread.
        let arc_layout = Layout::new::<[usize; 2]>()
            .extend(Layout::new::<Shared<A>>())
            .map_err(|_| StartError::ResourceLimit(ResourceLimit::ArithmeticOverflow))?
            .0
            .pad_to_align();
        // Box the factory before spawn, so over-alignment cannot multiply
        // through std's by-value startup/catch frames on the bounded stack.
        // Charge that actual heap layout plus the fixed pointer capture layout.
        let factory_layout = Layout::new::<F>();
        let capture_layout = Layout::new::<Arc<Shared<A>>>()
            .extend(Layout::new::<Box<F>>())
            .map_err(|_| StartError::ResourceLimit(ResourceLimit::ArithmeticOverflow))?
            .0
            .pad_to_align();
        let charge = arc_layout
            .size()
            .checked_add(THREAD_STACK)
            .and_then(|n| n.checked_add(THREAD_STARTUP_ALLOWANCE))
            .and_then(|n| n.checked_add(factory_layout.size()))
            .and_then(|n| n.checked_add(capture_layout.size()))
            .and_then(|n| n.checked_add(capture_layout.align() - 1))
            .and_then(|n| n.checked_add(std::mem::size_of::<D>()))
            .and_then(|n| n.checked_add(std::mem::size_of::<Arc<Shared<A>>>()))
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>()))
            .ok_or(StartError::ResourceLimit(ResourceLimit::ArithmeticOverflow))?;
        let reservation = memory.reserve(charge).map_err(StartError::ResourceLimit)?;
        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                binding,
                generation: 0,
                confirmation: initial_confirmation,
                confirmation_source: initial_source,
                claimed_source: None,
                capturing_source: false,
                source_payloads_parked: false,
                stopping: false,
                scheduler: MaintenanceScheduler::new(interval),
                snapshot: DriverSnapshot {
                    binding,
                    generation: 0,
                    requested: false,
                    pending: true,
                    running: false,
                    liveness: DriverLiveness::Running,
                    last_attempt: None,
                    failure: None,
                },
            }),
            wake: Condvar::new(),
            admission,
            memory,
            _reservation: reservation,
        });
        let worker = Arc::clone(&shared);
        let factory = Box::new(factory);
        let thread = spawn_worker(move || {
            // Declared first: runs last during normal return or unwinding,
            // after driver/factory/work and caught panic-payload destruction.
            let _exit = WorkerExit { shared: &worker };
            let result = catch_unwind(AssertUnwindSafe(|| {
                match prepare_driver_boxed(factory, &worker.memory) {
                    Ok(mut driver) => {
                        let execution = catch_unwind(AssertUnwindSafe(|| {
                            run_driver(&worker, &mut *driver);
                        }));
                        if execution.is_err() {
                            fail_worker(&worker, DriverError::Panicked);
                        }
                        // Catch execution separately so driver destruction is
                        // not itself performed during an existing unwind.
                        drop(execution);
                        // Driver destruction can block or panic. Remain Stopping
                        // until it, factory captures and all Work are disposed.
                        drop(driver);
                    }
                    Err(error) => fail_worker(&worker, error),
                }
            }));
            if result.is_err() {
                fail_worker(&worker, DriverError::Panicked);
            }
            drop(result);
        })
        .map_err(|_| StartError::SpawnFailed)?;
        Ok(Self {
            handle: MaintenanceHandle { shared },
            thread: Some(thread),
        })
    }

    pub(crate) fn handle(&self) -> MaintenanceHandle<A> {
        self.handle.clone()
    }

    pub(crate) fn request_stop(&self) {
        // Dispatcher lock order never overlaps the mailbox. Stop winning either
        // actual admission or the later mailbox gate prevents new storage work.
        self.handle.shared.admission.stop_admission();
        let mut control = self
            .handle
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        control.stopping = true;
        control.confirmation = false;
        control.confirmation_source = None;
        if control.snapshot.liveness == DriverLiveness::Running {
            control.snapshot.liveness = DriverLiveness::Stopping;
        }
        self.handle.shared.wake.notify_all();
    }

    pub(crate) fn stop_and_join(&mut self) {
        self.request_stop();
        // No shared mutex (including a join-handle mutex) is held while joining.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl<A: MaintenanceAdmission> Drop for MaintenanceRuntime<A> {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[cfg(test)]
thread_local! {
    static INJECT_SPAWN_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn spawn_worker<F: FnOnce() + Send + 'static>(body: F) -> std::io::Result<JoinHandle<()>> {
    #[cfg(test)]
    if INJECT_SPAWN_FAILURE.with(|flag| flag.replace(false)) {
        // Exercise the same closure-disposal ownership as Builder::spawn's Err.
        drop(body);
        return Err(std::io::Error::other("fixture thread creation failure"));
    }
    thread::Builder::new().stack_size(THREAD_STACK).spawn(body)
}

// This finalizer owns no payload and performs only fixed control updates. In
// particular, dropping a caught panic payload may itself unwind past the outer
// catch; terminal publication must still happen without catching/retrying or
// leaking that payload. All bulk owners are declared/consumed after this guard.
struct WorkerExit<'a, A> {
    shared: &'a Shared<A>,
}
impl<A> Drop for WorkerExit<'_, A> {
    fn drop(&mut self) {
        let mut control = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        control.stopping = true;
        control.confirmation = false;
        control.confirmation_source = None;
        control.claimed_source = None;
        control.snapshot.running = false;
        if thread::panicking() {
            control.snapshot.failure = Some(DriverError::Panicked);
            control.snapshot.pending = true;
        }
        control.snapshot.liveness = if control.snapshot.failure.is_some() {
            DriverLiveness::Failed
        } else {
            DriverLiveness::Stopped
        };
        self.shared.wake.notify_all();
    }
}

fn fail_worker<A>(shared: &Shared<A>, error: DriverError) {
    let mut control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
    control.stopping = true;
    control.confirmation = false;
    control.confirmation_source = None;
    control.claimed_source = None;
    control.snapshot.running = false;
    control.snapshot.pending = true;
    // Failure and completed-attempt evidence are separate. A later prepare or
    // destructor panic cannot rewrite a prior attempt's binding/result.
    control.snapshot.liveness = DriverLiveness::Stopping;
    control.snapshot.failure = Some(error);
    shared.wake.notify_all();
}

/// The startup reservation already includes the exact D heap layout. Keep
/// construction on a separate frame so D is not copied above every work call.
#[inline(never)]
fn prepare_driver_boxed<D, F>(
    factory: Box<F>,
    memory: &CleanupMemory,
) -> Result<Box<D>, DriverError>
where
    D: MaintenanceDriver,
    F: FnOnce(&CleanupMemory) -> Result<D, DriverError>,
{
    factory(memory).map(Box::new)
}

fn run_driver<A: MaintenanceAdmission, D: MaintenanceDriver>(shared: &Shared<A>, driver: &mut D) {
    let origin = Instant::now();
    loop {
        let (binding, generation, triggers, sequence) = {
            let mut control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if control.stopping {
                    control.snapshot.running = false;
                    return;
                }
                if control.capturing_source {
                    control = shared
                        .wake
                        .wait(control)
                        .unwrap_or_else(|error| error.into_inner());
                    continue;
                }
                let now = origin.elapsed();
                if let Some((triggers, sequence)) = control.scheduler.begin(now) {
                    control.snapshot.running = true;
                    break (control.binding, control.generation, triggers, sequence);
                }
                let wait = control.scheduler.next_due.saturating_sub(now);
                control = shared
                    .wake
                    .wait_timeout(control, wait)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        };
        let is_cancelled = || {
            let control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            control.stopping || control.generation != generation
        };
        let legacy_interest = || {
            let control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            !control.stopping
                && control.generation == generation
                && control.binding == binding
                && control.confirmation
        };
        let legacy_metadata = || {
            let control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            if !control.stopping
                && control.generation == generation
                && control.binding == binding
                && control.confirmation
            {
                Some((
                    generation,
                    control
                        .confirmation_source
                        .as_ref()
                        .map(|owner| owner.facts()),
                ))
            } else {
                None
            }
        };
        let cancellation = Cancellation {
            check: &is_cancelled,
            legacy_interest: &legacy_interest,
            legacy_metadata: &legacy_metadata,
        };
        let prepared_legacy_metadata = legacy_metadata();
        let prepared = driver.prepare(binding, &cancellation, &shared.memory);
        let mut panicked = false;
        let outcome = match prepared {
            Err(_) if cancellation.is_cancelled() => AttemptOutcome::Cancelled,
            Err(error) => AttemptOutcome::Failed(error),
            Ok(None) if cancellation.is_cancelled() => AttemptOutcome::Cancelled,
            Ok(None) => AttemptOutcome::Complete,
            Ok(Some(mut work)) => {
                // Bulk work lives OUTSIDE this scope and the caught callback.
                // Every success/error/unwind drops permit before work disposal.
                let requires_confirmation = driver.requires_legacy_confirmation(&work);
                let result = {
                    let mut grant = MutationGrant::start();
                    match shared.admission.try_acquire() {
                        None => Ok(Ok(AttemptOutcome::Deferred)),
                        Some(_permit) => {
                            let confirmation = {
                                let mut control =
                                    shared.control.lock().unwrap_or_else(|e| e.into_inner());
                                let current_legacy_metadata = control.confirmation.then(|| {
                                    (
                                        generation,
                                        control
                                            .confirmation_source
                                            .as_ref()
                                            .map(|owner| owner.facts()),
                                    )
                                });
                                if control.stopping
                                    || control.generation != generation
                                    || control.binding != binding
                                    || (requires_confirmation
                                        && control.confirmation
                                        && current_legacy_metadata != prepared_legacy_metadata)
                                {
                                    None
                                } else {
                                    let confirmed = requires_confirmation
                                        && control.claimed_source.is_none()
                                        && std::mem::take(&mut control.confirmation);
                                    let source = if confirmed {
                                        control.claimed_source = control.confirmation_source.take();
                                        control.claimed_source.as_ref().map(|owner| owner.facts())
                                    } else {
                                        None
                                    };
                                    Some((confirmed, source))
                                }
                            };
                            match confirmation {
                                None => Ok(Ok(AttemptOutcome::Deferred)),
                                Some((confirmed, source)) if grant.check().is_ok() => {
                                    let confirmation =
                                        confirmed.then_some(LegacyQuiescenceConfirmation {
                                            binding: &binding,
                                            generation: &generation,
                                            source,
                                            _same_thread: std::marker::PhantomData,
                                        });
                                    catch_unwind(AssertUnwindSafe(|| {
                                        driver.mutate(&work, &mut grant, confirmation.as_ref())
                                    }))
                                }
                                Some(_) => Ok(Ok(AttemptOutcome::Deferred)),
                            }
                        }
                    }
                };
                // No permit or S2 authority is live here, even on panic/Err.
                // The request's payload pins were parked in a separate fixed
                // slot through the callback; dispose them before source replay.
                let source = shared
                    .control
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .claimed_source
                    .take();
                drop(source);
                let outcome = match result {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(error)) => AttemptOutcome::Failed(error),
                    Err(_) => {
                        panicked = true;
                        AttemptOutcome::Failed(DriverError::Panicked)
                    }
                };
                let outcome = if panicked {
                    outcome
                } else {
                    match catch_unwind(AssertUnwindSafe(|| {
                        driver.finish(&mut work, outcome, &cancellation, &shared.memory)
                    })) {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(error)) => AttemptOutcome::Failed(error),
                        Err(_) => {
                            panicked = true;
                            AttemptOutcome::Failed(DriverError::Panicked)
                        }
                    }
                };
                drop(work);
                if panicked {
                    outcome
                } else {
                    match catch_unwind(AssertUnwindSafe(|| {
                        driver.after_disposal(outcome, &cancellation, &shared.memory)
                    })) {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(error)) => AttemptOutcome::Failed(error),
                        Err(_) => {
                            panicked = true;
                            AttemptOutcome::Failed(DriverError::Panicked)
                        }
                    }
                }
            }
        };
        driver.completed(outcome);
        let desired_binding = driver.desired_binding();
        let source_payloads_parked = driver.source_payloads_parked();
        {
            let mut control = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            control.snapshot.running = false;
            control.source_payloads_parked = source_payloads_parked;
            if control.generation == generation {
                control.snapshot.last_attempt = Some(AttemptRecord {
                    binding,
                    generation,
                    request_sequence: sequence,
                    triggers,
                    outcome,
                });
            }
            let completed_at = origin.elapsed();
            control.scheduler.complete(completed_at, outcome);
            if let Some(binding) = desired_binding
                .filter(|binding| control.generation == generation && *binding != control.binding)
            {
                if let Some(generation) = control.generation.checked_add(1) {
                    control.binding = binding;
                    control.generation = generation;
                    control.confirmation = false;
                    control.confirmation_source = None;
                    control.snapshot.binding = binding;
                    control.snapshot.generation = generation;
                    control.scheduler.request(Trigger::Startup);
                    // Resolving a placeholder binding completed preparation,
                    // not this new target's promised startup pass. Only this
                    // worker-owned handoff bypasses the old target's cooldown.
                    control.scheduler.next_due = completed_at;
                } else {
                    control.stopping = true;
                    control.snapshot.failure = Some(DriverError::Preparation);
                }
            }
            if control.confirmation {
                control.scheduler.pending |= Trigger::Continuation as u8;
            }
            control.snapshot.pending = control.scheduler.pending != 0;
            shared.wake.notify_all();
        }
        if panicked {
            fail_worker(shared, DriverError::Panicked);
            return;
        }
    }
}

/// Integration-test adapter: preparation stays offslot on the joined worker;
/// the callback receives a token only after real enqueue, admission and claim.
/// Callers qualifying dispatcher behavior supply the actual ExecutionDispatcher.
#[cfg(test)]
pub(crate) fn with_claimed_legacy_confirmation<A, W, P, C>(
    source_target: Option<&super::inventory::MaintenanceTarget>,
    admission: A,
    prepare: P,
    callback: C,
) -> DriverSnapshot
where
    A: MaintenanceAdmission,
    W: 'static,
    P: FnMut(
            &CleanupMemory,
            Option<(u64, Option<LegacyConfirmationSource>)>,
            Option<W>,
        ) -> Result<(TargetBinding, W), DriverError>
        + Send
        + 'static,
    C: for<'claim> FnMut(
            &W,
            Option<&LegacyQuiescenceConfirmation<'claim>>,
            &mut MutationGrant,
        ) -> Result<AttemptOutcome, DriverError>
        + Send
        + 'static,
{
    struct ConfirmedDriver<W, P, C> {
        prepare: P,
        callback: C,
        binding: Option<TargetBinding>,
        parked: Option<W>,
        continuing: bool,
        complete: bool,
    }
    impl<W, P, C> MaintenanceDriver for ConfirmedDriver<W, P, C>
    where
        P: FnMut(
            &CleanupMemory,
            Option<(u64, Option<LegacyConfirmationSource>)>,
            Option<W>,
        ) -> Result<(TargetBinding, W), DriverError>,
        C: for<'claim> FnMut(
            &W,
            Option<&LegacyQuiescenceConfirmation<'claim>>,
            &mut MutationGrant,
        ) -> Result<AttemptOutcome, DriverError>,
    {
        type Work = Option<W>;
        fn desired_binding(&self) -> Option<TargetBinding> {
            self.binding
        }
        fn prepare(
            &mut self,
            binding: TargetBinding,
            cancellation: &Cancellation<'_>,
            memory: &CleanupMemory,
        ) -> Result<Option<Self::Work>, DriverError> {
            if self.complete {
                return Ok(None);
            }
            let request = cancellation.legacy_confirmation_metadata();
            let (actual, work) = (self.prepare)(memory, request, self.parked.take())?;
            self.binding = Some(actual);
            // The first observation establishes the genuine target binding.
            // No absent advisory request can be promoted into a claimed token.
            if actual != binding || (request.is_none() && !self.continuing) {
                drop(work);
                return Ok(None);
            }
            Ok(Some(Some(work)))
        }
        fn requires_legacy_confirmation(&self, _: &Self::Work) -> bool {
            !self.continuing
        }
        fn mutate(
            &mut self,
            work: &Self::Work,
            grant: &mut MutationGrant,
            confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            if !self.continuing && confirmation.is_none() {
                return Ok(AttemptOutcome::Deferred);
            }
            (self.callback)(
                work.as_ref().ok_or(DriverError::Preparation)?,
                confirmation,
                grant,
            )
        }
        fn finish(
            &mut self,
            work: &mut Self::Work,
            outcome: AttemptOutcome,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<AttemptOutcome, DriverError> {
            if matches!(outcome, AttemptOutcome::Pending | AttemptOutcome::Deferred) {
                self.parked = work.take();
            }
            if outcome == AttemptOutcome::Pending {
                self.continuing = true;
            }
            self.complete = outcome == AttemptOutcome::Complete;
            Ok(outcome)
        }
    }
    let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
    let source = source_target.map(|target| {
        let source =
            super::maintenance_admission::prepare_legacy_confirmation_source(target, &memory)
                .unwrap();
        MemoryBox::new(source, &memory).unwrap()
    });
    let mut runtime = MaintenanceRuntime::start_with_interval(
        memory,
        TargetBinding([0; 32]),
        admission,
        move |_| {
            Ok(ConfirmedDriver::<W, P, C> {
                prepare,
                callback,
                binding: None,
                parked: None,
                continuing: false,
                complete: false,
            })
        },
        Duration::from_millis(1),
    )
    .unwrap();
    let handle = runtime.handle();
    let prepared = handle.wait_for_first_attempt();
    let mut attempted =
        handle.request_and_wait_with_source(Trigger::Health, Some(prepared.binding), source);
    // These are fresh dispatcher callbacks, with no replacement TRUE request.
    // The real accepted source capsule alone authorizes each continuation.
    for _ in 0..64 {
        if attempted.last_attempt.is_some_and(|attempt| {
            attempt.binding == attempted.binding
                && attempt.generation == attempted.generation
                && matches!(
                    attempt.outcome,
                    AttemptOutcome::Failed(_) | AttemptOutcome::Cancelled
                )
        }) || (attempted.last_attempt.is_some_and(|attempt| {
            attempt.binding == attempted.binding
                && attempt.generation == attempted.generation
                && attempt.outcome == AttemptOutcome::Complete
        }) && !attempted.pending)
        {
            break;
        }
        attempted = handle.request_and_wait(Trigger::Continuation, None);
    }
    runtime.stop_and_join();
    attempted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_confirmation_keeps_request_time_source_after_pair_replacement() {
        let root = std::env::temp_dir().join(format!(
            "confirmation-source-version-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = root.join(".codebaseGraph");
        std::fs::create_dir_all(&state).unwrap();
        let target = super::super::inventory::MaintenanceTarget {
            repository_root: root.clone(),
            state_root: state.clone(),
            storage_root: None,
            direct_pair: None,
            config_path: None,
        };
        let pair = super::super::inventory::default_direct(&target);
        std::fs::write(&pair.database, b"old database incarnation").unwrap();
        std::fs::write(&pair.manifest, b"old manifest incarnation").unwrap();
        let request_target = target.clone();
        let current_target = target.clone();
        let prepare_pair = pair.clone();
        let snapshot = with_claimed_legacy_confirmation(
            Some(&request_target),
            Arc::new(crate::coordinator::ExecutionDispatcher::default()),
            move |memory,_,_:Option<super::super::maintenance_admission::LegacyConfirmationSource>| {
                // A real new pair is published AFTER the operator source pins
                // were captured, before that queued demand can be claimed.
                let database = prepare_pair.database.with_extension("replacement");
                let manifest = prepare_pair.manifest.with_extension("replacement");
                std::fs::write(&database, b"fresh database incarnation").unwrap();
                std::fs::write(&manifest, b"fresh manifest incarnation").unwrap();
                std::fs::rename(database, &prepare_pair.database).unwrap();
                std::fs::rename(manifest, &prepare_pair.manifest).unwrap();
                let current=super::super::maintenance_admission::capture_legacy_confirmation_source(&current_target,memory).unwrap();
                Ok((TargetBinding([73;32]),current))
            },
            move |current,token,_| {
                let token=token.expect("the dispatcher must claim the actual queued request");
                assert!(token.source().is_some());assert_ne!(token.source(),Some(current));
                Ok(AttemptOutcome::Complete)
            },
        );
        assert_eq!(
            snapshot.last_attempt.unwrap().outcome,
            AttemptOutcome::Complete
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dc_scheduler_sustained_requests_do_not_slide_due_or_lose_inflight_demand() {
        let mut scheduler = MaintenanceScheduler::new(DEFAULT_INTERVAL);
        assert_eq!(scheduler.begin(Duration::ZERO), Some((1, 0)));
        for second in 1..=60 {
            scheduler.request(Trigger::Health);
            if second < 60 {
                assert!(scheduler.begin(Duration::from_secs(second)).is_none());
            }
        }
        let (bits, sequence) = scheduler.begin(Duration::from_secs(60)).unwrap();
        assert_eq!(bits, Trigger::Health as u8);
        assert_eq!(sequence, 60);
        // A request during preparation survives completion of this attempt.
        scheduler.request(Trigger::Completion);
        scheduler.complete(Duration::from_secs(61), AttemptOutcome::Complete);
        assert!(scheduler.begin(Duration::from_secs(120)).is_none());
        assert_eq!(scheduler.begin(Duration::from_secs(121)), Some((2, 61)));
    }

    #[test]
    fn dc_scheduler_idle_busy_error_and_continuation_are_paced() {
        let mut scheduler = MaintenanceScheduler::new(DEFAULT_INTERVAL);
        for (step, outcome) in [
            AttemptOutcome::Complete,
            AttemptOutcome::Deferred,
            AttemptOutcome::Failed(DriverError::Preparation),
            AttemptOutcome::Pending,
        ]
        .into_iter()
        .enumerate()
        {
            let now = Duration::from_secs(step as u64 * 60);
            assert!(scheduler.begin(now).is_some());
            scheduler.complete(now, outcome);
            assert!(scheduler.begin(now + Duration::from_secs(59)).is_none());
        }
        assert_eq!(
            scheduler.begin(Duration::from_secs(240)).unwrap().0,
            Trigger::Continuation as u8
        );
    }

    struct NoAdmission;
    impl MaintenanceAdmission for NoAdmission {
        type Permit = ();
        fn try_acquire(&self) -> Option<()> {
            Some(())
        }
        fn stop_admission(&self) {}
    }
    struct StartupBindingDriver {
        desired: TargetBinding,
        prepares: usize,
        callbacks: std::sync::mpsc::Sender<(TargetBinding, usize)>,
    }
    impl MaintenanceDriver for StartupBindingDriver {
        type Work = TargetBinding;
        fn desired_binding(&self) -> Option<TargetBinding> {
            Some(self.desired)
        }
        fn prepare(
            &mut self,
            binding: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<TargetBinding>, DriverError> {
            self.prepares += 1;
            if self.prepares == 1 {
                self.desired = TargetBinding([82; 32]);
                return Ok(None);
            }
            Ok(Some(binding))
        }
        fn mutate(
            &mut self,
            work: &TargetBinding,
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            self.callbacks.send((*work, self.prepares)).unwrap();
            Ok(AttemptOutcome::Complete)
        }
    }

    #[test]
    fn dc_driver_resolved_startup_binding_runs_actual_dispatcher_without_default_cooldown() {
        let (callbacks, received) = std::sync::mpsc::channel();
        // Use the actual 60-second runtime interval and dispatcher. The first
        // callback resolves binding only; the second must enter real admission
        // promptly, while ordinary subsequent Health requests remain paced.
        let mut runtime = MaintenanceRuntime::start(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([0; 32]),
            Arc::new(crate::coordinator::ExecutionDispatcher::default()),
            move |_| {
                Ok(StartupBindingDriver {
                    desired: TargetBinding([0; 32]),
                    prepares: 0,
                    callbacks,
                })
            },
        )
        .unwrap();
        let callback = received.recv_timeout(Duration::from_secs(3));
        let health = if callback.is_ok() {
            runtime.handle().request(Trigger::Health, None);
            Some(received.recv_timeout(Duration::from_millis(100)))
        } else {
            None
        };
        // Stop before asserting so a failed handoff never leaves a worker
        // sleeping through the default cooldown or blocks cleanup on failure.
        runtime.stop_and_join();
        assert_eq!(callback.unwrap(), (TargetBinding([82; 32]), 2));
        assert!(matches!(
            health,
            Some(Err(std::sync::mpsc::RecvTimeoutError::Timeout))
        ));
    }

    struct PausedDriver {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
        _retained: MemoryReservation,
    }
    impl MaintenanceDriver for PausedDriver {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            cancellation: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
            assert!(cancellation.is_cancelled());
            Ok(None)
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }

    #[test]
    fn dc_driver_one_account_covers_rebind_snapshots_and_retained_handle_lifetime() {
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let floor = memory.used();
        let (entered, enter_rx) = std::sync::mpsc::channel();
        let (release_tx, release) = std::sync::mpsc::channel();
        let mut runtime = MaintenanceRuntime::start(
            memory.clone(),
            TargetBinding([1; 32]),
            NoAdmission,
            move |memory| {
                Ok(PausedDriver {
                    entered,
                    release,
                    _retained: memory.reserve(512).unwrap(),
                })
            },
        )
        .unwrap();
        enter_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let handle = runtime.handle();
        let used = memory.used();
        for value in 2..=100 {
            let clone = handle.clone();
            clone.rebind(TargetBinding([value; 32]));
            clone.request(Trigger::Health, None);
            assert_eq!(clone.snapshot().binding, TargetBinding([value; 32]));
            assert_eq!(memory.used(), used); // fixed-copy control, no DTO allocations
        }
        runtime.request_stop();
        release_tx.send(()).unwrap();
        runtime.stop_and_join();
        assert_eq!(memory.used(), used - 512); // driver disposed, mailbox still alive
        drop(runtime);
        assert!(memory.used() > floor); // a handle does not escape its account
        drop(handle);
        assert_eq!(memory.used(), floor);
        assert!(memory.peak() <= memory.limit());
    }

    struct CompletionWork {
        disposing: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl Drop for CompletionWork {
        fn drop(&mut self) {
            self.disposing.send(()).unwrap();
            self.release.recv().unwrap();
        }
    }
    struct CompletionDriver(Option<CompletionWork>);
    impl MaintenanceDriver for CompletionDriver {
        type Work = CompletionWork;
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<CompletionWork>, DriverError> {
            Ok(self.0.take())
        }
        fn mutate(
            &mut self,
            _: &CompletionWork,
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            Ok(AttemptOutcome::Complete)
        }
    }

    #[test]
    fn dc_driver_transient_wait_observes_completed_disposal_and_factory_failure() {
        let (disposing, disposing_rx) = std::sync::mpsc::channel();
        let (release_tx, release) = std::sync::mpsc::channel();
        let mut runtime = MaintenanceRuntime::start(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            NoAdmission,
            move |_| {
                Ok(CompletionDriver(Some(CompletionWork {
                    disposing,
                    release,
                })))
            },
        )
        .unwrap();
        disposing_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let handle = runtime.handle();
        assert!(handle.snapshot().last_attempt.is_none());
        let waiter = thread::spawn(move || handle.wait_for_first_attempt());
        release_tx.send(()).unwrap();
        let snapshot = waiter.join().unwrap();
        assert_eq!(
            snapshot.last_attempt.unwrap().outcome,
            AttemptOutcome::Complete
        );
        assert!(!snapshot.running);
        runtime.stop_and_join();

        let mut failed = MaintenanceRuntime::start::<CompletionDriver, _>(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([2; 32]),
            NoAdmission,
            |_| Err(DriverError::Panicked),
        )
        .unwrap();
        let snapshot = failed.handle().wait_for_first_attempt();
        assert_eq!(snapshot.liveness, DriverLiveness::Failed);
        assert_eq!(snapshot.failure, Some(DriverError::Panicked));
        assert!(snapshot.last_attempt.is_none());
        failed.stop_and_join();
    }

    struct ConfirmDriver {
        prepared: std::sync::mpsc::Sender<()>,
        prepare_release: std::sync::mpsc::Receiver<()>,
        mutated: std::sync::mpsc::Sender<bool>,
        mutate_release: std::sync::mpsc::Receiver<()>,
    }
    impl MaintenanceDriver for ConfirmDriver {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            self.prepared.send(()).unwrap();
            self.prepare_release
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(Some(()))
        }
        fn requires_legacy_confirmation(&self, _: &()) -> bool {
            true
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            if let Some(confirmation) = confirmation {
                assert_eq!(confirmation.binding_digest(), [1; 32]);
                assert_eq!(confirmation.generation(), 0);
            }
            self.mutated.send(confirmation.is_some()).unwrap();
            self.mutate_release
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(AttemptOutcome::Complete)
        }
    }

    #[test]
    fn dc_driver_confirmation_arriving_inflight_survives_prior_completion() {
        let (prepared, prepare_rx) = std::sync::mpsc::channel();
        let (prepare_tx, prepare_release) = std::sync::mpsc::channel();
        let (mutated, mutate_rx) = std::sync::mpsc::channel();
        let (mutate_tx, mutate_release) = std::sync::mpsc::channel();
        let mut runtime = MaintenanceRuntime::start_with_interval(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            NoAdmission,
            move |_| {
                Ok(ConfirmDriver {
                    prepared,
                    prepare_release,
                    mutated,
                    mutate_release,
                })
            },
            Duration::from_millis(1),
        )
        .unwrap();
        let handle = runtime.handle();
        prepare_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.request(Trigger::Health, Some(TargetBinding([1; 32])));
        prepare_tx.send(()).unwrap();
        // A new confirmation arrived after preparation captured its source
        // version. Preserve it for fresh preparation instead of claiming stale
        // work; the deferred callback must not consume that demand.
        prepare_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            handle.snapshot().last_attempt.unwrap().outcome,
            AttemptOutcome::Deferred
        );
        assert!(handle.snapshot().pending);
        assert!(matches!(
            mutate_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        prepare_tx.send(()).unwrap();
        assert!(mutate_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        // Gate already took the first demand. New true belongs to the next gate.
        handle.request(Trigger::Health, Some(TargetBinding([1; 32])));
        handle.request(Trigger::Health, None);
        mutate_tx.send(()).unwrap();
        prepare_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(handle.snapshot().pending);
        prepare_tx.send(()).unwrap();
        assert!(mutate_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        runtime.request_stop();
        mutate_tx.send(()).unwrap();
        runtime.stop_and_join();
        assert_eq!(
            handle.request(Trigger::Health, Some(TargetBinding([1; 32]))),
            RequestDisposition::Stopped
        );
    }
}

#[cfg(test)]
mod spawn_failure_test {
    use super::*;
    struct Admission;
    impl MaintenanceAdmission for Admission {
        type Permit = ();
        fn try_acquire(&self) -> Option<()> {
            unreachable!()
        }
        fn stop_admission(&self) {}
    }
    struct NeverCreated;
    impl MaintenanceDriver for NeverCreated {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            unreachable!()
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }
    #[test]
    fn dc_driver_spawn_failure_releases_all_startup_reservations_without_factory() {
        let memory = CleanupMemory::new(1024 * 1024).unwrap();
        let floor = memory.used();
        INJECT_SPAWN_FAILURE.with(|flag| flag.set(true));
        let result = MaintenanceRuntime::start::<NeverCreated, _>(
            memory.clone(),
            TargetBinding([1; 32]),
            Admission,
            |_| unreachable!("spawn failed before factory"),
        );
        assert!(matches!(result, Err(StartError::SpawnFailed)));
        assert_eq!(memory.used(), floor);
    }
}

#[cfg(test)]
mod independent_review_probes {
    use super::*;
    use std::sync::mpsc;
    struct Admission;
    impl MaintenanceAdmission for Admission {
        type Permit = ();
        fn try_acquire(&self) -> Option<()> {
            Some(())
        }
        fn stop_admission(&self) {}
    }
    struct PreparePanic {
        attempts: usize,
        second: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl MaintenanceDriver for PreparePanic {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            self.attempts += 1;
            if self.attempts == 1 {
                return Ok(None);
            }
            self.second.send(()).unwrap();
            self.release.recv().unwrap();
            panic!("independent prepare failure");
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }
    #[test]
    fn independent_outer_panic_preserves_prior_success() {
        let (second, second_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let mut runtime = MaintenanceRuntime::start_with_interval(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            Admission,
            move |_| {
                Ok(PreparePanic {
                    attempts: 0,
                    second,
                    release: release_rx,
                })
            },
            Duration::from_millis(1),
        )
        .unwrap();
        let handle = runtime.handle();
        second_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = handle.snapshot();
        assert_eq!(
            before.last_attempt.unwrap().outcome,
            AttemptOutcome::Complete
        );
        handle.rebind(TargetBinding([2; 32]));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while handle.snapshot().liveness != DriverLiveness::Failed {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let after = handle.snapshot();
        runtime.stop_and_join();
        println!(
            "prior_success_before={before:?}\nafter_different_generation_prepare_panic={after:?}"
        );
        assert_eq!(after.last_attempt.unwrap().binding, TargetBinding([1; 32]));
        assert_eq!(
            after.last_attempt.unwrap().outcome,
            AttemptOutcome::Complete
        );
        assert_eq!(before.last_attempt, after.last_attempt);
    }
    struct SlowDriverDrop {
        prepared: mpsc::Sender<()>,
        prepare_release: mpsc::Receiver<()>,
        drop_enter: mpsc::Sender<()>,
        drop_release: mpsc::Receiver<()>,
    }
    impl MaintenanceDriver for SlowDriverDrop {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            self.prepared.send(()).unwrap();
            self.prepare_release.recv().unwrap();
            Ok(None)
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }
    impl Drop for SlowDriverDrop {
        fn drop(&mut self) {
            self.drop_enter.send(()).unwrap();
            self.drop_release.recv().unwrap();
        }
    }
    #[test]
    fn independent_stopping_lasts_until_driver_disposal_finishes() {
        let (prepared, prepared_rx) = mpsc::channel();
        let (prepare_release, prepare_rx) = mpsc::channel();
        let (drop_enter, drop_enter_rx) = mpsc::channel();
        let (drop_release, drop_release_rx) = mpsc::channel();
        let mut runtime = MaintenanceRuntime::start(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            Admission,
            move |_| {
                Ok(SlowDriverDrop {
                    prepared,
                    prepare_release: prepare_rx,
                    drop_enter,
                    drop_release: drop_release_rx,
                })
            },
        )
        .unwrap();
        let handle = runtime.handle();
        prepared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        runtime.request_stop();
        prepare_release.send(()).unwrap();
        drop_enter_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let observed = handle.snapshot();
        let (joined, joined_rx) = mpsc::channel();
        let joiner = thread::spawn(move || {
            runtime.stop_and_join();
            joined.send(()).unwrap();
        });
        assert!(joined_rx.try_recv().is_err());
        println!("during_blocked_driver_drop={observed:?}; join_is_pending=true");
        drop_release.send(()).unwrap();
        joined_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        joiner.join().unwrap();
        assert_eq!(observed.liveness, DriverLiveness::Stopping);
    }

    struct DropPanic;
    impl MaintenanceDriver for DropPanic {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            Ok(None)
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }
    impl Drop for DropPanic {
        fn drop(&mut self) {
            panic!("fixture driver destructor");
        }
    }

    #[test]
    fn dc_driver_destructor_panic_preserves_success_and_runtime_drop_can_join_while_unwinding() {
        let runtime = MaintenanceRuntime::start(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            Admission,
            |_| Ok(DropPanic),
        )
        .unwrap();
        let handle = runtime.handle();
        let deadline = Instant::now() + Duration::from_secs(5);
        while handle.snapshot().last_attempt.is_none() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let successful = handle.snapshot().last_attempt;
        let unwind = catch_unwind(AssertUnwindSafe(move || {
            let _runtime = runtime;
            panic!("fixture caller unwinds through Runtime::drop");
        }));
        assert!(unwind.is_err());
        assert_eq!(handle.snapshot().liveness, DriverLiveness::Failed);
        assert_eq!(handle.snapshot().failure, Some(DriverError::Panicked));
        assert_eq!(handle.snapshot().last_attempt, successful);
        assert_eq!(successful.unwrap().outcome, AttemptOutcome::Complete);
    }

    #[test]
    fn dc_driver_factory_panic_records_failure_without_inventing_attempt() {
        let mut runtime = MaintenanceRuntime::start::<DropPanic, _>(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            Admission,
            |_| panic!("fixture factory before preparation"),
        )
        .unwrap();
        let handle = runtime.handle();
        let deadline = Instant::now() + Duration::from_secs(5);
        while handle.snapshot().liveness != DriverLiveness::Failed {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        runtime.stop_and_join();
        assert_eq!(handle.snapshot().last_attempt, None);
        assert_eq!(handle.snapshot().failure, Some(DriverError::Panicked));
    }
}

#[cfg(test)]
pub(crate) mod allocation_boundary_test {
    // Execute the actual integrated scheduler in an isolated child of this
    // very test binary, with the same allocator and source-ticket types. This
    // keeps unrelated parallel tests outside the allocation observation.
    #[test]
    fn dc_driver_real_requested_allocations_fit_tight_ceiling_and_aligned_factory() {
        const CHILD: &str = "CODEBASE_GRAPH_ALLOCATION_PROBE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            run_probe();
            return;
        }
        let binary = std::env::current_exe().unwrap();
        let output =
            std::env::temp_dir().join(format!("cleanup-driver-allocation-{}", std::process::id()));
        std::fs::create_dir(&output).unwrap();
        std::fs::write(
            output.join("binary-path.txt"),
            binary.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
        let run = std::process::Command::new(&binary)
            .args([
                "--exact",
                "storage::maintenance_scheduler::allocation_boundary_test::dc_driver_real_requested_allocations_fit_tight_ceiling_and_aligned_factory",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        std::fs::write(output.join("probe.stdout"), &run.stdout).unwrap();
        std::fs::write(output.join("probe.stderr"), &run.stderr).unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
        assert_eq!(
            stdout
                .matches("one-byte-under rejected before allocation")
                .count(),
            2,
            "{stdout}"
        );
        println!("allocation probe: {}\n{stdout}", output.display());
    }
    use super::*;
    use std::alloc::{GlobalAlloc, Layout};
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    #[derive(Copy, Clone)]
    struct Entry {
        ptr: usize,
        size: usize,
        live: bool,
    }
    const EMPTY: Entry = Entry {
        ptr: 0,
        size: 0,
        live: false,
    };
    struct Record(UnsafeCell<[Entry; 128]>);
    unsafe impl Sync for Record {}
    static RECORD: Record = Record(UnsafeCell::new([EMPTY; 128]));
    static LOCK: AtomicBool = AtomicBool::new(false);
    static TRACK: AtomicBool = AtomicBool::new(false);
    static LIVE: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    static PREPARED: AtomicBool = AtomicBool::new(false);
    fn lock() {
        while LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
    }
    unsafe fn note_alloc(p: *mut u8, l: Layout) {
        if p.is_null() || !TRACK.load(Ordering::Relaxed) {
            return;
        }
        lock();
        let i = COUNT.fetch_add(1, Ordering::Relaxed);
        if i < 128 {
            (*RECORD.0.get())[i] = Entry {
                ptr: p as usize,
                size: l.size(),
                live: true,
            };
        }
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        LOCK.store(false, Ordering::Release);
    }
    unsafe fn note_dealloc(p: *mut u8) {
        if !TRACK.load(Ordering::Relaxed) && LIVE.load(Ordering::Relaxed) == 0 {
            return;
        }
        lock();
        for e in (*RECORD.0.get()).iter_mut() {
            if e.live && e.ptr == p as usize {
                e.live = false;
                LIVE.fetch_sub(e.size, Ordering::Relaxed);
                break;
            }
        }
        LOCK.store(false, Ordering::Release);
    }
    pub(crate) struct Allocator;
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let p = mimalloc::MiMalloc.alloc(l);
            note_alloc(p, l);
            p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            let p = mimalloc::MiMalloc.alloc_zeroed(l);
            note_alloc(p, l);
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            note_dealloc(p);
            mimalloc::MiMalloc.dealloc(p, l);
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            let q = mimalloc::MiMalloc.realloc(p, l, n);
            if !q.is_null() {
                note_dealloc(p);
                note_alloc(q, Layout::from_size_align_unchecked(n, l.align()));
            }
            q
        }
    }
    struct Admission;
    impl MaintenanceAdmission for Admission {
        type Permit = ();
        fn try_acquire(&self) -> Option<()> {
            Some(())
        }
        fn stop_admission(&self) {}
    }
    struct Driver;
    impl MaintenanceDriver for Driver {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            PREPARED.store(true, Ordering::Release);
            Ok(None)
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }

    fn reset() {
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
        lock();
        unsafe {
            *RECORD.0.get() = [EMPTY; 128];
        }
        LOCK.store(false, Ordering::Release);
        COUNT.store(0, Ordering::Relaxed);
        PEAK.store(0, Ordering::Relaxed);
        PREPARED.store(false, Ordering::Release);
    }
    fn measure<F>(limit: usize, factory: F) -> (usize, usize, usize)
    where
        F: Copy + FnOnce(&CleanupMemory) -> Result<Driver, DriverError> + Send + 'static,
    {
        reset();
        let memory = CleanupMemory::new(limit).unwrap();
        let floor = memory.used();
        TRACK.store(true, Ordering::Relaxed);
        let mut runtime =
            MaintenanceRuntime::start(memory.clone(), TargetBinding([1; 32]), Admission, factory)
                .unwrap();
        while !PREPARED.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let charge = memory.used() - floor;
        runtime.stop_and_join();
        drop(runtime);
        TRACK.store(false, Ordering::Relaxed);
        let peak = PEAK.load(Ordering::Relaxed);
        assert!(
            COUNT.load(Ordering::Relaxed) < 128,
            "allocation recorder overflow"
        );
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
        assert!(
            peak + 262144 <= charge,
            "unreserved requested allocation: heap={peak}, stack=262144, charge={charge}"
        );
        (floor, charge, peak)
    }
    fn check<F>(factory: F)
    where
        F: Copy + FnOnce(&CleanupMemory) -> Result<Driver, DriverError> + Send + 'static,
    {
        let (floor, charge, peak) = measure(1024 * 1024, factory);
        let (_, tight, tight_peak) = measure(floor + charge, factory);
        assert_eq!(tight, charge);
        reset();
        let memory = CleanupMemory::new(floor + charge - 1).unwrap();
        TRACK.store(true, Ordering::Relaxed);
        let rejected =
            MaintenanceRuntime::start(memory.clone(), TargetBinding([1; 32]), Admission, factory);
        TRACK.store(false, Ordering::Relaxed);
        assert!(matches!(rejected, Err(StartError::ResourceLimit(_))));
        assert_eq!(
            COUNT.load(Ordering::Relaxed),
            0,
            "tight rejection allocated before reserve"
        );
        assert_eq!(memory.used(), floor);
        println!("charge={charge} heap_peak={peak} tight_peak={tight_peak} stack=262144; exact ceiling passed, one-byte-under rejected before allocation");
    }
    #[derive(Clone, Copy)]
    #[repr(align(4096))]
    struct Aligned([u8; 1]);
    impl Aligned {
        fn value(self) -> u8 {
            self.0[0]
        }
    }
    fn run_probe() {
        let _caller_thread = std::thread::current();
        check(|_| Ok(Driver));
        let captured = Aligned([7]);
        check(move |_| {
            assert_eq!(captured.value(), 7);
            Ok(Driver)
        });
    }
}

#[cfg(test)]
mod panic_payload_probe {
    use super::*;
    struct Admission;
    impl MaintenanceAdmission for Admission {
        type Permit = ();
        fn try_acquire(&self) -> Option<()> {
            Some(())
        }
        fn stop_admission(&self) {}
    }
    struct Driver;
    impl MaintenanceDriver for Driver {
        type Work = ();
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            _: &CleanupMemory,
        ) -> Result<Option<()>, DriverError> {
            unreachable!()
        }
        fn mutate(
            &mut self,
            _: &(),
            _: &mut MutationGrant,
            _: Option<&LegacyQuiescenceConfirmation<'_>>,
        ) -> Result<AttemptOutcome, DriverError> {
            unreachable!()
        }
    }
    struct Payload;
    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("independent payload destructor panic");
        }
    }
    #[test]
    fn independent_panicking_factory_payload_publishes_terminal_failure() {
        let mut runtime = MaintenanceRuntime::start::<Driver, _>(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            Admission,
            |_| std::panic::panic_any(Payload),
        )
        .unwrap();
        let handle = runtime.handle();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !runtime.thread.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        runtime.stop_and_join();
        let observed = handle.snapshot();
        println!("after_joined_panicking_panic_payload={observed:?}");
        assert_eq!(observed.failure, Some(DriverError::Panicked));
        assert_eq!(observed.liveness, DriverLiveness::Failed);
    }
}
