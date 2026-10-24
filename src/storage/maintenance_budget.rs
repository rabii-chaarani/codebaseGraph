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

    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.account().used.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) fn peak(&self) -> usize {
        self.account().peak.load(Ordering::Acquire)
    }
    #[cfg(test)]
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
    #[cfg(test)]
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
            _reservation: self,
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
    _reservation: MemoryReservation,
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
    #[cfg(test)]
    pub(crate) fn reserved_bytes(&self) -> usize {
        self._reservation.remaining()
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

/// Fallible, exactly laid-out owned state. Its pointee drops before the shared
/// reservation is released. This keeps large prepared values out of repeatedly
/// copied Result/Option frames without using a second accounting pool.
pub(crate) struct MemoryBox<T> {
    pointer: NonNull<T>,
    _reservation: MemoryReservation,
}
// SAFETY: the allocation has one owner; borrowing follows ordinary T rules.
unsafe impl<T: Send> Send for MemoryBox<T> {}
unsafe impl<T: Sync> Sync for MemoryBox<T> {}
impl<T> MemoryBox<T> {
    /// Allocate charged storage without constructing a large value on stack.
    pub(crate) fn new_uninit(
        memory: &CleanupMemory,
    ) -> Result<MemoryBox<std::mem::MaybeUninit<T>>, ResourceLimit> {
        let layout = Layout::new::<T>();
        let reservation = memory.reserve(layout.size())?;
        let pointer = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: exact nonzero allocation, null checked before ownership.
            NonNull::new(unsafe { alloc(layout) }.cast::<std::mem::MaybeUninit<T>>())
                .ok_or(ResourceLimit::AllocationFailed)?
        };
        Ok(MemoryBox {
            pointer,
            _reservation: reservation,
        })
    }
    pub(crate) fn new(value: T, memory: &CleanupMemory) -> Result<Self, ResourceLimit> {
        let layout = Layout::new::<T>();
        let reservation = memory.reserve(layout.size())?;
        let pointer = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: exact nonzero T layout. Null is handled before writing.
            NonNull::new(unsafe { alloc(layout) }.cast::<T>())
                .ok_or(ResourceLimit::AllocationFailed)?
        };
        // SAFETY: correctly aligned unique storage, also valid for a ZST write.
        unsafe { pointer.as_ptr().write(value) };
        Ok(Self {
            pointer,
            _reservation: reservation,
        })
    }
}
impl<T> MemoryBox<T> {
    /// Move the actual owned value into its next owner without cloning handles.
    /// Call only off authority; a retained destination needs its own accounting.
    #[inline(never)]
    pub(crate) fn into_inner(self) -> T {
        let owner = std::mem::ManuallyDrop::new(self);
        // SAFETY: consuming the sole owner transfers the initialized T once;
        // ManuallyDrop prevents the old pointee destructor from running.
        let value = unsafe { owner.pointer.as_ptr().read() };
        let layout = Layout::new::<T>();
        if layout.size() != 0 {
            // SAFETY: the moved allocation uses its exact original layout.
            unsafe { dealloc(owner.pointer.as_ptr().cast(), layout) };
        }
        // SAFETY: this field has not been moved or dropped. Release it once,
        // after deallocation, while T now belongs exclusively to the caller.
        drop(unsafe { std::ptr::read(&owner._reservation) });
        value
    }
}
impl<T> MemoryBox<std::mem::MaybeUninit<T>> {
    /// # Safety
    /// Every field of T must be initialized with its valid unique owned value.
    /// No alias may retain ownership of a field transferred into this storage.
    pub(crate) unsafe fn assume_init(self) -> MemoryBox<T> {
        let owner = std::mem::ManuallyDrop::new(self);
        MemoryBox {
            pointer: owner.pointer.cast(),
            // SAFETY: consumes the sole reservation without dropping it twice.
            _reservation: unsafe { std::ptr::read(&owner._reservation) },
        }
    }
}
impl<T> std::ops::Deref for MemoryBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: initialized pointee remains alive for this owner's lifetime.
        unsafe { self.pointer.as_ref() }
    }
}
impl<T> std::ops::DerefMut for MemoryBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: unique owner and exclusive borrow prevent aliased mutation.
        unsafe { self.pointer.as_mut() }
    }
}
impl<T> Drop for MemoryBox<T> {
    fn drop(&mut self) {
        struct Allocation<T>(NonNull<T>);
        impl<T> Drop for Allocation<T> {
            fn drop(&mut self) {
                let layout = Layout::new::<T>();
                if layout.size() != 0 {
                    // SAFETY: original pointer/layout, freed exactly once.
                    unsafe { dealloc(self.0.as_ptr().cast(), layout) };
                }
            }
        }
        // The local guard deallocates on both normal completion and unwinding,
        // before the reservation field drops. T's destructor runs exactly once.
        let _allocation = Allocation(self.pointer);
        // SAFETY: this owner holds the live, uniquely owned initialized T.
        unsafe { std::ptr::drop_in_place(self.pointer.as_ptr()) };
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
        let elapsed = self.started.elapsed();
        if elapsed >= self.duration {
            #[cfg(test)]
            if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                eprintln!(
                    "cleanup grant expired: elapsed_us={} remaining_entries={}",
                    elapsed.as_micros(),
                    self.remaining
                );
            }
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
            #[cfg(test)]
            if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                eprintln!(
                    "cleanup grant entries exhausted: requested={count}, remaining={}",
                    self.remaining
                );
            }
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
    fn dc_memory_box_alignment_exact_charge_and_drop_order() {
        #[repr(align(256))]
        struct Aligned(u64);
        let memory = CleanupMemory::new(8192).unwrap();
        let before = memory.used();
        let mut value = MemoryBox::new(Aligned(7), &memory).unwrap();
        assert_eq!((&*value as *const Aligned as usize) % 256, 0);
        assert_eq!(memory.used() - before, std::mem::size_of::<Aligned>());
        value.0 = 9;
        assert_eq!(value.0, 9);
        drop(value);
        assert_eq!(memory.used(), before);
        struct Check<'a>(&'a CleanupMemory, usize);
        impl Drop for Check<'_> {
            fn drop(&mut self) {
                assert!(self.0.used() >= self.1 + std::mem::size_of::<Self>());
            }
        }
        drop(MemoryBox::new(Check(&memory, before), &memory).unwrap());
        assert_eq!(memory.used(), before);
    }
    #[test]
    fn dc_memory_box_fieldwise_large_initialization_is_charged_and_drops_once() {
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        let before = memory.used();
        let drops = std::cell::Cell::new(0);
        struct Chunk<'a> {
            bytes: [u8; 64 * 1024],
            drops: &'a std::cell::Cell<usize>,
        }
        impl Drop for Chunk<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }
        let mut uninit = MemoryBox::<Chunk<'_>>::new_uninit(&memory).unwrap();
        assert_eq!(memory.used() - before, std::mem::size_of::<Chunk<'_>>());
        let pointer = uninit.as_mut_ptr();
        // SAFETY: each field is initialized directly; no reference to invalid T
        // or temporary large aggregate is constructed before assume_init.
        unsafe {
            std::ptr::addr_of_mut!((*pointer).bytes)
                .cast::<u8>()
                .write_bytes(7, 64 * 1024);
            std::ptr::addr_of_mut!((*pointer).drops).write(&drops);
        }
        let initialized = unsafe { uninit.assume_init() };
        assert_eq!(initialized.bytes[65535], 7);
        assert_eq!(drops.get(), 0);
        drop(initialized);
        assert_eq!(drops.get(), 1);
        assert_eq!(memory.used(), before);
        drop(MemoryBox::<Chunk<'_>>::new_uninit(&memory).unwrap());
        assert_eq!(drops.get(), 1);
        assert_eq!(memory.used(), before);
    }
    #[test]
    fn dc_memory_box_into_inner_transfers_drop_once_and_releases_exact_charge() {
        let memory = CleanupMemory::new(4096).unwrap();
        let before = memory.used();
        let drops = std::cell::Cell::new(0);
        struct Owned<'a>(&'a std::cell::Cell<usize>);
        impl Drop for Owned<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let value = MemoryBox::new(Owned(&drops), &memory).unwrap();
        assert_eq!(memory.used(), before + std::mem::size_of::<Owned<'_>>());
        let actual = value.into_inner();
        assert_eq!(memory.used(), before);
        assert_eq!(drops.get(), 0);
        drop(actual);
        assert_eq!(drops.get(), 1);
        let zst = MemoryBox::new((), &memory).unwrap();
        let () = zst.into_inner();
        assert_eq!(memory.used(), before);
    }
    #[test]
    fn dc_memory_box_failed_reservation_and_panicking_pointee_release() {
        let memory = CleanupMemory::new(128).unwrap();
        let before = memory.used();
        assert!(matches!(
            MemoryBox::new([0u8; 256], &memory),
            Err(ResourceLimit::CeilingExceeded)
        ));
        assert_eq!(memory.used(), before);
        struct Panics(u8);
        impl Drop for Panics {
            fn drop(&mut self) {
                let _ = self.0;
                panic!("expected pointee panic");
            }
        }
        let owned = MemoryBox::new(Panics(1), &memory).unwrap();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(owned))).is_err());
        assert_eq!(memory.used(), before);
        let zero = MemoryBox::new((), &memory).unwrap();
        assert_eq!(memory.used(), before);
        drop(zero);
    }

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
        let mut zero = MutationGrant::for_test(0, Duration::from_secs(1));
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
