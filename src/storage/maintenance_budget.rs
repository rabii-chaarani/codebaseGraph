//! Shared accounting for requested allocation layouts, not allocator slack/RSS.
//! These primitives carry no storage ownership or deletion authority.
use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{fence, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceLimit {
    ArithmeticOverflow,
    CeilingExceeded,
    AllocationFailed,
}

struct Account {
    references: AtomicUsize,
    limit: usize,
    used: AtomicUsize,
    peak: AtomicUsize,
}

/// One explicit, fallible, exactly laid-out shared allocation. Clones allocate
/// nothing; reservations retain this allocation until their last release.
pub(crate) struct CleanupMemory {
    account: NonNull<Account>,
}
// SAFETY: the pointer remains alive through atomic reference counting, and all
// mutable shared fields are atomic. The limit never changes after construction.
unsafe impl Send for CleanupMemory {}
unsafe impl Sync for CleanupMemory {}

impl CleanupMemory {
    pub(crate) fn new(limit: usize) -> Result<Self, ResourceLimit> {
        let layout = Layout::new::<Account>();
        if limit < layout.size() {
            return Err(ResourceLimit::CeilingExceeded);
        }
        // SAFETY: valid nonzero layout; null is handled before initialization.
        let account = NonNull::new(unsafe { alloc(layout) }.cast::<Account>())
            .ok_or(ResourceLimit::AllocationFailed)?;
        // SAFETY: this allocation is uniquely owned and large/aligned enough.
        unsafe {
            account.as_ptr().write(Account {
                references: AtomicUsize::new(1),
                limit,
                used: AtomicUsize::new(layout.size()),
                peak: AtomicUsize::new(layout.size()),
            });
        }
        Ok(Self { account })
    }

    fn account(&self) -> &Account {
        // SAFETY: every handle retains one live account reference.
        unsafe { self.account.as_ref() }
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Result<MemoryReservation, ResourceLimit> {
        let account = self.account();
        let mut old = account.used.load(Ordering::Relaxed);
        loop {
            let new = old
                .checked_add(bytes)
                .ok_or(ResourceLimit::ArithmeticOverflow)?;
            if new > account.limit {
                return Err(ResourceLimit::CeilingExceeded);
            }
            match account
                .used
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => {
                    account.peak.fetch_max(new, Ordering::Relaxed);
                    return Ok(MemoryReservation {
                        memory: self.clone(),
                        bytes,
                    });
                }
                Err(current) => old = current,
            }
        }
    }

    pub(crate) fn used(&self) -> usize {
        self.account().used.load(Ordering::Acquire)
    }
    pub(crate) fn peak(&self) -> usize {
        self.account().peak.load(Ordering::Acquire)
    }
    pub(crate) fn limit(&self) -> usize {
        self.account().limit
    }
}

impl Clone for CleanupMemory {
    fn clone(&self) -> Self {
        if self.account().references.fetch_add(1, Ordering::Relaxed) >= isize::MAX as usize {
            std::process::abort();
        }
        Self {
            account: self.account,
        }
    }
}
impl Drop for CleanupMemory {
    fn drop(&mut self) {
        if self.account().references.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            // SAFETY: the last reference owns the initialized allocation.
            unsafe {
                std::ptr::drop_in_place(self.account.as_ptr());
                dealloc(self.account.as_ptr().cast(), Layout::new::<Account>());
            }
        }
    }
}

pub(crate) struct MemoryReservation {
    memory: CleanupMemory,
    bytes: usize,
}
impl MemoryReservation {
    pub(crate) fn split(&mut self, bytes: usize) -> Result<Self, ResourceLimit> {
        if bytes > self.bytes {
            return Err(ResourceLimit::CeilingExceeded);
        }
        self.bytes -= bytes;
        Ok(Self {
            memory: self.memory.clone(),
            bytes,
        })
    }
    pub(crate) fn remaining(&self) -> usize {
        self.bytes
    }
    pub(crate) fn bytes(self, len: usize) -> Result<ReservedBytes, ResourceLimit> {
        if len > self.bytes {
            return Err(ResourceLimit::CeilingExceeded);
        }
        let layout = Layout::array::<u8>(len).map_err(|_| ResourceLimit::ArithmeticOverflow)?;
        let pointer = if len == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: valid layout, checked charge already retained by self.
            NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(ResourceLimit::AllocationFailed)?
        };
        Ok(ReservedBytes {
            pointer,
            len,
            reservation: self,
        })
    }
}
impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.memory
            .account()
            .used
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub(crate) struct ReservedBytes {
    pointer: NonNull<u8>,
    len: usize,
    reservation: MemoryReservation,
}
// SAFETY: uniquely owned bytes; sharing permits immutable access only.
unsafe impl Send for ReservedBytes {}
unsafe impl Sync for ReservedBytes {}
impl ReservedBytes {
    pub(crate) fn as_slice(&self) -> &[u8] {
        // SAFETY: live initialized allocation, including valid empty slice.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.len) }
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: exclusive borrow guarantees unique access to the allocation.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
    }
    pub(crate) fn reserved_bytes(&self) -> usize {
        self.reservation.remaining()
    }
}
impl Drop for ReservedBytes {
    fn drop(&mut self) {
        if self.len != 0 {
            // SAFETY: original pointer/layout; reservation drops AFTER this body.
            unsafe {
                dealloc(
                    self.pointer.as_ptr(),
                    Layout::array::<u8>(self.len).expect("validated byte layout"),
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantExhausted {
    Entries,
    Time,
}

/// Cooperative limits. A syscall may exceed the deadline. No grant or authority
/// crosses threads, and no fresh grant may be substituted inside an admission.
pub(crate) struct MutationGrant {
    started: Instant,
    duration: Duration,
    remaining: usize,
    _same_thread: PhantomData<Rc<()>>,
}
impl MutationGrant {
    pub(crate) fn start() -> Self {
        Self::with_limits(100, Duration::from_millis(100))
    }
    #[cfg(test)]
    pub(crate) fn for_test(entries: usize, duration: Duration) -> Self {
        Self::with_limits(entries, duration)
    }
    fn with_limits(entries: usize, duration: Duration) -> Self {
        Self {
            started: Instant::now(),
            duration,
            remaining: entries,
            _same_thread: PhantomData,
        }
    }
    pub(crate) fn check(&self) -> Result<(), GrantExhausted> {
        self.check_time()?;
        if self.remaining == 0 {
            Err(GrantExhausted::Entries)
        } else {
            Ok(())
        }
    }
    /// Check before each effect whose entry charge was already reserved. Unlike
    /// check(), this permits performing the final successfully reserved entry.
    pub(crate) fn check_time(&self) -> Result<(), GrantExhausted> {
        if self.started.elapsed() >= self.duration {
            Err(GrantExhausted::Time)
        } else {
            Ok(())
        }
    }
    pub(crate) fn take_entry(&mut self) -> Result<(), GrantExhausted> {
        self.take_entries(1)
    }
    pub(crate) fn take_entries(&mut self, count: usize) -> Result<(), GrantExhausted> {
        self.check()?;
        if count > self.remaining {
            return Err(GrantExhausted::Entries);
        }
        self.remaining -= count;
        Ok(())
    }
    pub(crate) fn remaining_entries(&self) -> usize {
        self.remaining
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_memory_work_types_cross_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<CleanupMemory>();
        send_sync::<MemoryReservation>();
        send_sync::<ReservedBytes>();
    }

    #[test]
    fn dc_memory_checks_layout_overflow_and_duplicate_buffers() {
        assert!(matches!(
            CleanupMemory::new(0),
            Err(ResourceLimit::CeilingExceeded)
        ));
        let memory = CleanupMemory::new(usize::MAX).unwrap();
        assert_eq!(memory.limit(), usize::MAX);
        let floor = memory.used();
        assert!(matches!(
            memory.reserve(usize::MAX),
            Err(ResourceLimit::ArithmeticOverflow)
        ));
        let a = memory.reserve(100).unwrap().bytes(100).unwrap();
        let b = memory.reserve(100).unwrap().bytes(100).unwrap();
        assert_eq!(memory.used(), floor + 200);
        assert_eq!(a.reserved_bytes(), 100);
        drop(a);
        assert_eq!(memory.used(), floor + 100);
        drop(b);
        assert_eq!(memory.used(), floor);
        let memory = CleanupMemory::new(floor + 20).unwrap();
        let _a = memory.reserve(11).unwrap().bytes(11).unwrap();
        assert!(matches!(
            memory.reserve(10),
            Err(ResourceLimit::CeilingExceeded)
        ));
    }

    #[test]
    fn dc_memory_split_failure_and_account_lifetime() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let mut reservation = memory.reserve(20).unwrap();
        assert!(reservation.split(21).is_err());
        let child = reservation.split(8).unwrap();
        assert_eq!(reservation.remaining(), 12);
        assert_eq!(memory.used(), floor + 20);
        assert!(child.bytes(9).is_err());
        assert_eq!(memory.used(), floor + 12);
        let mut bytes = reservation.bytes(12).unwrap();
        bytes.as_mut_slice()[11] = 42;
        assert_eq!(bytes.as_slice()[11], 42);
        drop(memory);
        drop(bytes); // Buffer owns the last account reference.
    }

    #[test]
    fn dc_memory_simultaneous_reservations_share_one_ceiling() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let account = memory.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    let _bytes = account.reserve(1024).unwrap().bytes(1024).unwrap();
                    barrier.wait();
                    barrier.wait();
                });
            }
            barrier.wait();
            assert_eq!(memory.used(), floor + 2048);
            assert!(memory.reserve(4096 - floor - 2048 + 1).is_err());
            barrier.wait();
        });
        assert_eq!(memory.used(), floor);
        assert_eq!(memory.peak(), floor + 2048);
    }

    #[test]
    fn dc_grant_zero_entry101_deadline_and_atomic_transition() {
        let mut zero = MutationGrant::with_limits(0, Duration::from_secs(1));
        let mut effects = 0;
        if zero.take_entry().is_ok() {
            effects += 1;
        }
        assert_eq!(effects, 0);
        let mut grant = MutationGrant::start();
        grant.take_entries(99).unwrap();
        assert_eq!(grant.take_entries(2), Err(GrantExhausted::Entries));
        assert_eq!(grant.remaining_entries(), 1);
        grant.take_entry().unwrap();
        assert_eq!(grant.check_time(), Ok(()));
        assert_eq!(grant.take_entry(), Err(GrantExhausted::Entries));
        let mut expired = MutationGrant::with_limits(100, Duration::ZERO);
        assert_eq!(expired.take_entry(), Err(GrantExhausted::Time));
        assert_eq!(expired.remaining_entries(), 100);
    }
}
