use crate::api::context::{bind_repo_selector, resolve_runtime, RepositoryIdentity};
use crate::api::ExecutionContext;
use crate::api::{
    ApiError, CodebaseGraphApi, OperationInvocation, OperationResponse, RefreshServiceConfig,
    RepoSelector,
};
use crate::api::{RefreshCancellation, RefreshRuntime, RefreshState};
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::{DirectLayout, ManagedLayout};
use crate::storage::locks::{try_open_locked, CoordinatorLease, LockMode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(debug_assertions)]
mod fixture_trace {
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::net::{SocketAddr, TcpStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    const CAPACITY: usize = 32;
    const OUTPUT_LIMIT: usize = 2048;
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static RENDERED: AtomicBool = AtomicBool::new(false);
    static EVENTS: Mutex<Ring> = Mutex::new(Ring::new());
    static CORRELATION: Mutex<Correlation> = Mutex::new(Correlation::new());
    static CORRELATION_UNCERTAIN: AtomicBool = AtomicBool::new(false);
    static CORRELATION_LOSS: AtomicU64 = AtomicU64::new(0);
    static EVENT_LOSS: AtomicU64 = AtomicU64::new(0);
    static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    static CORRELATION_INFLIGHT: AtomicU64 = AtomicU64::new(0);
    const ACTIVE_LIMIT: usize = 64;
    const HISTORY_LIMIT: usize = 4096;

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Tuple(SocketAddr, SocketAddr);
    struct Entry {
        tuple: Tuple,
        sequence: u64,
        active: Arc<AtomicBool>,
        reused: bool,
    }
    struct Correlation {
        entries: Vec<Entry>,
    }
    impl Correlation {
        const fn new() -> Self {
            Self {
                entries: Vec::new(),
            }
        }
        fn register(&mut self, tuple: Tuple, sequence: u64) -> Option<ClientGuard> {
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.tuple == tuple) {
                entry.reused = true;
                return None;
            }
            if self.entries.len() >= HISTORY_LIMIT {
                return None;
            }
            if self
                .entries
                .iter()
                .filter(|entry| entry.active.load(Ordering::SeqCst))
                .count()
                >= ACTIVE_LIMIT
            {
                return None;
            }
            let active = Arc::new(AtomicBool::new(true));
            self.entries.push(Entry {
                tuple,
                sequence,
                active: Arc::clone(&active),
                reused: false,
            });
            Some(ClientGuard { active, sequence })
        }
        fn valid_sequence(&self, sequence: u64) -> bool {
            self.entries.iter().any(|entry| {
                entry.sequence == sequence && !entry.reused && entry.active.load(Ordering::SeqCst)
            })
        }
        fn lookup(&self, tuple: Tuple) -> u64 {
            self.entries
                .iter()
                .find(|entry| {
                    entry.tuple == tuple && !entry.reused && entry.active.load(Ordering::SeqCst)
                })
                .map_or(0, |entry| entry.sequence)
        }
    }
    pub(super) struct ClientGuard {
        active: Arc<AtomicBool>,
        sequence: u64,
    }
    impl ClientGuard {
        pub(super) fn sequence(&self) -> u64 {
            self.sequence
        }
    }
    impl Drop for ClientGuard {
        fn drop(&mut self) {
            self.active.store(false, Ordering::SeqCst);
            if CORRELATION_INFLIGHT.load(Ordering::SeqCst) != 0 {
                CORRELATION_UNCERTAIN.store(true, Ordering::SeqCst);
                loss();
            }
        }
    }
    fn loss() {
        CORRELATION_LOSS.fetch_add(1, Ordering::Relaxed);
    }
    fn next_sequence(counter: &AtomicU64) -> Option<u64> {
        counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()
            .filter(|n| *n != 0)
    }
    fn inspected_tuple(
        local: std::io::Result<SocketAddr>,
        remote: std::io::Result<SocketAddr>,
    ) -> Option<Tuple> {
        Some(Tuple(local.ok()?, remote.ok()?))
    }
    pub(super) fn register_client(stream: &TcpStream) -> Option<ClientGuard> {
        if !ENABLED.load(Ordering::Relaxed) {
            return None;
        }
        let tuple = match inspected_tuple(stream.local_addr(), stream.peer_addr()) {
            Some(tuple) => tuple,
            None => {
                CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                loss();
                return None;
            }
        };
        let sequence = match next_sequence(&NEXT_SEQUENCE) {
            Some(sequence) => sequence,
            None => {
                CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                loss();
                return None;
            }
        };
        if CORRELATION_UNCERTAIN.load(Ordering::Acquire) {
            loss();
            return None;
        }
        let result = match CORRELATION.try_lock() {
            Ok(mut table) => table.register(tuple, sequence),
            Err(_) => {
                CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                None
            }
        };
        if result.is_none() {
            CORRELATION_UNCERTAIN.store(true, Ordering::Release);
            loss();
        }
        result
    }
    pub(super) fn correlate_server(stream: &TcpStream) -> u64 {
        if !ENABLED.load(Ordering::Relaxed) {
            return 0;
        }
        let tuple = match inspected_tuple(stream.peer_addr(), stream.local_addr()) {
            Some(tuple) => tuple,
            None => {
                loss();
                return 0;
            }
        };
        if CORRELATION_UNCERTAIN.load(Ordering::Acquire) {
            loss();
            return 0;
        }
        let sequence = match CORRELATION.try_lock() {
            Ok(table) => table.lookup(tuple),
            Err(_) => {
                CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                0
            }
        };
        if sequence == 0 || CORRELATION_UNCERTAIN.load(Ordering::Acquire) {
            loss();
            0
        } else {
            sequence
        }
    }

    fn revalidate(sequence: u64) -> u64 {
        if sequence == 0 || CORRELATION_UNCERTAIN.load(Ordering::Acquire) {
            return 0;
        }
        let valid = match CORRELATION.try_lock() {
            Ok(table) => table.valid_sequence(sequence),
            Err(_) => {
                CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                false
            }
        };
        if valid && !CORRELATION_UNCERTAIN.load(Ordering::Acquire) {
            sequence
        } else {
            loss();
            0
        }
    }
    pub(super) fn record_corr(kind: Kind, sequence: u64) {
        if ENABLED.load(Ordering::Relaxed) {
            CORRELATION_INFLIGHT.fetch_add(1, Ordering::SeqCst);
            record(kind, revalidate(sequence));
            CORRELATION_INFLIGHT.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Kind {
        Empty,
        OwnerStart,
        OwnerStop,
        Accept,
        RegisterFailed,
        NonblockingFailed,
        ExecutorSpawnFailed,
        HandlerSpawnFailed,
        Dispatch,
        CloneFailed,
        Busy,
        WorkerStart,
        WorkerReply,
        WorkerPanic,
        WorkerAbandon,
        ResponseWritten,
        ResponseWriteFailed,
        ResponseSpawnFailed,
        CloseAll,
        ClientReadFailed,
        CorrReceive,
        CorrReceiveFailed,
        CorrAuthRejected,
        CorrPrepareRejected,
        CorrDispatch,
        CorrWorkerStart,
        CorrWorkerReply,
        CorrWorkerAbandon,
        CorrResponseWritten,
        CorrResponseFailed,
        CorrEarlyReply,
    }
    impl Kind {
        fn label(self) -> &'static str {
            match self {
                Self::Empty => "empty",
                Self::OwnerStart => "owner_start",
                Self::OwnerStop => "owner_stop",
                Self::Accept => "accept",
                Self::RegisterFailed => "register_failed",
                Self::NonblockingFailed => "nonblocking_failed",
                Self::ExecutorSpawnFailed => "executor_spawn_failed",
                Self::HandlerSpawnFailed => "handler_spawn_failed",
                Self::Dispatch => "dispatch",
                Self::CloneFailed => "clone_failed",
                Self::Busy => "busy",
                Self::WorkerStart => "worker_start",
                Self::WorkerReply => "worker_reply",
                Self::WorkerPanic => "worker_panic",
                Self::WorkerAbandon => "worker_abandon",
                Self::ResponseWritten => "response_written",
                Self::ResponseWriteFailed => "response_write_failed",
                Self::ResponseSpawnFailed => "response_spawn_failed",
                Self::CloseAll => "close_all",
                Self::ClientReadFailed => "client_read_failed",
                Self::CorrReceive => "corr_receive",
                Self::CorrReceiveFailed => "corr_receive_failed",
                Self::CorrAuthRejected => "corr_auth_rejected",
                Self::CorrPrepareRejected => "corr_prepare_rejected",
                Self::CorrDispatch => "corr_dispatch",
                Self::CorrWorkerStart => "corr_worker_start",
                Self::CorrWorkerReply => "corr_worker_reply",
                Self::CorrWorkerAbandon => "corr_worker_abandon",
                Self::CorrResponseWritten => "corr_response_written",
                Self::CorrResponseFailed => "corr_response_failed",
                Self::CorrEarlyReply => "corr_early_reply",
            }
        }
    }
    #[derive(Clone, Copy)]
    struct Event {
        kind: Kind,
        connection: u64,
    }
    const EMPTY: Event = Event {
        kind: Kind::Empty,
        connection: 0,
    };
    #[derive(Clone, Copy)]
    pub(super) struct Ring {
        entries: [Event; CAPACITY],
        next: usize,
        len: usize,
        overwritten: u64,
    }
    impl Ring {
        const fn new() -> Self {
            Self {
                entries: [EMPTY; CAPACITY],
                next: 0,
                len: 0,
                overwritten: 0,
            }
        }
        fn push(&mut self, kind: Kind, connection: u64) {
            if self.len == CAPACITY {
                self.overwritten = self.overwritten.saturating_add(1);
            }
            self.entries[self.next] = Event { kind, connection };
            self.next = (self.next + 1) % CAPACITY;
            self.len = (self.len + 1).min(CAPACITY);
        }
        fn render(&self, pid: u32, owner_pid_matches: bool) -> Buffer {
            let mut output = Buffer {
                bytes: [0; OUTPUT_LIMIT],
                len: 0,
            };
            let _ = writeln!(output, "CGR_COORD_TRACE v1 pid={pid} owner_pid_matches={owner_pid_matches} server_trace_may_be_incomplete=true");
            let start = (self.next + CAPACITY - self.len) % CAPACITY;
            let mut truncated = false;
            for offset in 0..self.len {
                let event = self.entries[(start + offset) % CAPACITY];
                let line = format!(
                    "CGR_COORD_TRACE {} {}\n",
                    event.kind.label(),
                    event.connection
                );
                if output.len + line.len() + 320 > OUTPUT_LIMIT {
                    truncated = true;
                    break;
                }
                let _ = output.write_str(&line);
            }
            let _ = writeln!(
                output,
                "CGR_COORD_TRACE event_loss {}",
                EVENT_LOSS.load(Ordering::Relaxed)
            );
            let _ = writeln!(
                output,
                "CGR_COORD_TRACE ring_overwrite {}",
                self.overwritten
            );
            let _ = writeln!(
                output,
                "CGR_COORD_TRACE correlation_loss {}",
                CORRELATION_LOSS.load(Ordering::Relaxed)
            );
            let _ = writeln!(
                output,
                "CGR_COORD_TRACE correlation_uncertain {}",
                u8::from(CORRELATION_UNCERTAIN.load(Ordering::Acquire))
            );
            let _ = writeln!(
                output,
                "CGR_COORD_TRACE render_truncated {}",
                u8::from(truncated)
            );
            output
        }
    }
    struct Buffer {
        bytes: [u8; OUTPUT_LIMIT],
        len: usize,
    }
    impl std::fmt::Write for Buffer {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            let remaining = self.bytes.len().saturating_sub(self.len);
            let count = remaining.min(value.len());
            self.bytes[self.len..self.len + count].copy_from_slice(&value.as_bytes()[..count]);
            self.len += count;
            Ok(())
        }
    }
    fn enabled_value(value: Option<&std::ffi::OsStr>) -> bool {
        value == Some(std::ffi::OsStr::new("1"))
    }
    pub(super) fn init() {
        if enabled_value(std::env::var_os("CODEBASE_GRAPH_TEST_COORDINATOR_TRACE").as_deref()) {
            ENABLED.store(true, Ordering::Relaxed);
        }
    }
    fn try_record(ring: &Mutex<Ring>, kind: Kind, connection: u64) -> bool {
        if let Ok(mut ring) = ring.try_lock() {
            ring.push(kind, connection);
            true
        } else {
            false
        }
    }
    fn claim_once(rendered: &AtomicBool) -> bool {
        !rendered.swap(true, Ordering::Relaxed)
    }
    pub(super) fn record(kind: Kind, connection: u64) {
        if ENABLED.load(Ordering::Relaxed) && !try_record(&EVENTS, kind, connection) {
            EVENT_LOSS.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn render_once(owner_pid_matches: bool) {
        if !ENABLED.load(Ordering::Relaxed) || !claim_once(&RENDERED) {
            return;
        }
        let snapshot = match EVENTS.try_lock() {
            Ok(ring) => *ring,
            Err(_) => {
                EVENT_LOSS.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let output = snapshot.render(std::process::id(), owner_pid_matches);
        // No request or response thread waits for diagnostic I/O. The explicitly
        // enabled fixture continuously drains stderr; failure to spawn is ignored.
        let _ = std::thread::Builder::new()
            .name("cgraph-fixture-trace".into())
            .spawn(move || {
                let _ = std::io::stderr().write_all(&output.bytes[..output.len]);
            });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn tuple(port: u16) -> Tuple {
            Tuple(([127, 0, 0, 1], port).into(), ([127, 0, 0, 1], 42).into())
        }

        #[test]
        fn correlation_failed_socket_inspection_is_unknown() {
            let error = || std::io::Error::other("synthetic address inspection failure");
            assert!(inspected_tuple(Err(error()), Ok(([127, 0, 0, 1], 1).into())).is_none());
            assert!(inspected_tuple(Ok(([127, 0, 0, 1], 1).into()), Err(error())).is_none());
        }

        #[test]
        fn correlation_render_truncates_only_complete_lines_and_keeps_loss_indicators() {
            let mut ring = Ring::new();
            for _ in 0..80 {
                ring.push(Kind::CorrResponseWritten, u64::MAX);
            }
            let output = ring.render(u32::MAX, false);
            assert!(output.len <= OUTPUT_LIMIT);
            let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
            assert!(text.ends_with('\n'));
            assert!(text.contains("CGR_COORD_TRACE ring_overwrite 48\n"));
            assert!(text.contains("CGR_COORD_TRACE render_truncated 1\n"));
            assert!(text
                .lines()
                .all(|line| line.starts_with("CGR_COORD_TRACE ")));
        }

        #[test]
        fn concurrent_guard_drop_marks_inflight_evidence_uncertain() {
            const CHILD: &str = "CGR_CORRELATION_INFLIGHT_CHILD";
            if std::env::var_os(CHILD).is_some() {
                let guard = CORRELATION.lock().unwrap().register(tuple(30), 5).unwrap();
                CORRELATION_INFLIGHT.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || drop(guard)).join().unwrap();
                assert!(CORRELATION_UNCERTAIN.load(Ordering::SeqCst));
                assert_eq!(revalidate(5), 0);
                CORRELATION_INFLIGHT.fetch_sub(1, Ordering::SeqCst);
                return;
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::concurrent_guard_drop_marks_inflight_evidence_uncertain"])
                .env(CHILD, "1").status().unwrap();
            assert!(status.success());
        }

        #[test]
        fn correlated_late_worker_and_response_phases_are_unknown_after_evidence_loss() {
            const CHILD: &str = "CGR_CORRELATION_LATE_PHASE_CHILD";
            if let Some(mode) = std::env::var_os(CHILD) {
                ENABLED.store(true, Ordering::Relaxed);
                let mut guard = Some(CORRELATION.lock().unwrap().register(tuple(33), 17).unwrap());
                if mode == "drop" {
                    drop(guard.take());
                } else if mode == "reuse" {
                    assert!(CORRELATION
                        .lock()
                        .unwrap()
                        .register(tuple(33), 18)
                        .is_none());
                } else {
                    CORRELATION_UNCERTAIN.store(true, Ordering::Release);
                }
                record_corr(Kind::CorrWorkerStart, 17);
                record_corr(Kind::CorrWorkerReply, 17);
                record_corr(Kind::CorrWorkerAbandon, 17);
                record_corr(Kind::CorrResponseWritten, 17);
                record_corr(Kind::CorrResponseFailed, 17);
                let snapshot = *EVENTS.lock().unwrap();
                let output = snapshot.render(1, false);
                let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
                for kind in [
                    "corr_worker_start",
                    "corr_worker_reply",
                    "corr_worker_abandon",
                    "corr_response_written",
                    "corr_response_failed",
                ] {
                    assert!(
                        text.contains(&format!("CGR_COORD_TRACE {kind} 0\n")),
                        "{text}"
                    );
                    assert!(
                        !text.contains(&format!("CGR_COORD_TRACE {kind} 17\n")),
                        "{text}"
                    );
                }
                return;
            }
            for mode in ["drop", "reuse", "uncertain"] {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "coordinator::fixture_trace::tests::correlated_late_worker_and_response_phases_are_unknown_after_evidence_loss", "--nocapture"])
                    .env(CHILD, mode).output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        #[test]
        fn correlation_history_exhaustion_never_reuses_lost_evidence() {
            const CHILD: &str = "CGR_CORRELATION_HISTORY_CHILD";
            if std::env::var_os(CHILD).is_some() {
                ENABLED.store(true, Ordering::Relaxed);
                {
                    let mut table = CORRELATION.lock().unwrap();
                    for index in 0..HISTORY_LIMIT {
                        drop(
                            table
                                .register(tuple(index as u16 + 100), index as u64 + 1)
                                .unwrap(),
                        );
                    }
                }
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                let (server, _) = listener.accept().unwrap();
                assert!(register_client(&client).is_none());
                assert!(CORRELATION_UNCERTAIN.load(Ordering::Acquire));
                assert_eq!(correlate_server(&server), 0);
                return;
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::correlation_history_exhaustion_never_reuses_lost_evidence"])
                .env(CHILD, "1").status().unwrap();
            assert!(status.success());
        }

        #[test]
        fn event_lock_loss_is_reported_without_waiting() {
            const CHILD: &str = "CGR_CORRELATION_EVENT_LOSS_CHILD";
            if std::env::var_os(CHILD).is_some() {
                ENABLED.store(true, Ordering::Relaxed);
                let held = EVENTS.lock().unwrap();
                record(Kind::CorrDispatch, 1);
                assert_eq!(EVENT_LOSS.load(Ordering::Relaxed), 1);
                drop(held);
                let snapshot = *EVENTS.lock().unwrap();
                let output = snapshot.render(1, false);
                let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
                assert!(text.contains("event_loss 1\n"));
                assert!(!text.contains("corr_dispatch 1\n"));
                return;
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::event_lock_loss_is_reported_without_waiting"])
                .env(CHILD, "1").status().unwrap();
            assert!(status.success());
        }

        #[test]
        fn correlation_contended_registration_sets_sticky_unknown() {
            const CHILD: &str = "CGR_CORRELATION_CONTENTION_CHILD";
            if std::env::var_os(CHILD).is_some() {
                ENABLED.store(true, Ordering::Relaxed);
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                let (server, _) = listener.accept().unwrap();
                let held = CORRELATION.lock().unwrap();
                assert!(register_client(&client).is_none());
                drop(held);
                assert!(CORRELATION_UNCERTAIN.load(Ordering::Acquire));
                assert_eq!(correlate_server(&server), 0);
                assert!(CORRELATION_LOSS.load(Ordering::Relaxed) > 0);
                return;
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::correlation_contended_registration_sets_sticky_unknown"])
                .env(CHILD, "1").status().unwrap();
            assert!(status.success());
        }

        #[test]
        fn correlation_sequence_wrap_and_unknown_are_rejected() {
            let counter = AtomicU64::new(u64::MAX - 1);
            assert_eq!(next_sequence(&counter), Some(u64::MAX - 1));
            assert_eq!(next_sequence(&counter), None);
            assert_eq!(next_sequence(&AtomicU64::new(0)), None);
        }

        #[test]
        fn correlation_drop_invalidates_without_taking_table_lock() {
            let table = Mutex::new(Correlation::new());
            let guard = table.lock().unwrap().register(tuple(77), 9).unwrap();
            let mut held = table.lock().unwrap();
            std::thread::spawn(move || drop(guard)).join().unwrap();
            assert_eq!(held.lookup(tuple(77)), 0);
            assert!(held.register(tuple(77), 10).is_none());
            assert_eq!(held.lookup(tuple(77)), 0);
            assert!(table.try_lock().is_err());
        }

        #[test]
        fn correlation_reuse_and_guard_invalidation_are_conservative() {
            let mut table = Correlation::new();
            let first = table.register(tuple(1), 1).unwrap();
            assert_eq!(table.lookup(tuple(1)), 1);
            drop(first);
            assert_eq!(table.lookup(tuple(1)), 0);
            assert!(table.register(tuple(1), 2).is_none());
            assert_eq!(table.lookup(tuple(1)), 0);
            let live = table.register(tuple(2), 3).unwrap();
            assert!(table.register(tuple(2), 4).is_none());
            assert_eq!(table.lookup(tuple(2)), 0);
            drop(live);
        }

        #[test]
        fn correlation_active_and_history_limits_do_not_evict_evidence() {
            let mut table = Correlation::new();
            let mut guards = Vec::new();
            for index in 0..ACTIVE_LIMIT {
                guards.push(
                    table
                        .register(tuple(index as u16 + 100), index as u64 + 1)
                        .unwrap(),
                );
            }
            assert!(table.register(tuple(999), 1000).is_none());
            assert_eq!(table.lookup(tuple(100)), 1);
            drop(guards);
            for index in ACTIVE_LIMIT..HISTORY_LIMIT {
                drop(
                    table
                        .register(tuple(index as u16 + 100), index as u64 + 1)
                        .unwrap(),
                );
            }
            assert!(table.register(tuple(9999), 10000).is_none());
        }

        #[test]
        fn correlation_real_sockets_match_only_during_guard_lifetime() {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (server, _) = listener.accept().unwrap();
            let tuple = Tuple(client.local_addr().unwrap(), client.peer_addr().unwrap());
            let reverse = Tuple(server.peer_addr().unwrap(), server.local_addr().unwrap());
            let mut table = Correlation::new();
            let guard = table.register(tuple, 37).unwrap();
            assert_eq!(table.lookup(reverse), 37);
            drop(guard);
            assert_eq!(table.lookup(reverse), 0);
        }

        #[test]
        fn actual_early_rejection_and_success_are_correlated() {
            const CHILD: &str = "CGR_CORRELATION_PHASE_CHILD";
            if let Some(mode) = std::env::var_os(CHILD) {
                use super::super::*;
                ENABLED.store(true, Ordering::Relaxed);
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let endpoint = listener.local_addr().unwrap();
                let state = CoordinatorState {
                    version: COORDINATOR_PROTOCOL_VERSION,
                    endpoint,
                    token: "a".repeat(64),
                    pid: std::process::id(),
                    supports_deadlines: true,
                };
                let mut server_state = state.clone();
                let reject = mode == "reject";
                let malformed = mode == "receive";
                let busy = mode == "busy";
                let stopping = mode == "stopping";
                if reject {
                    server_state.token = "b".repeat(64);
                }
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    let registry = Arc::new(ConnectionRegistry::default());
                    let permit = registry.register(&stream).unwrap();
                    let dispatcher = ExecutionDispatcher::default();
                    {
                        let mut dispatch_state = dispatcher.state.lock().unwrap();
                        dispatch_state.active = busy;
                        dispatch_state.stopping = stopping;
                    }
                    handle_connection_dispatch(
                        &mut stream,
                        Instant::now(),
                        &server_state,
                        &dispatcher,
                        permit,
                        Arc::new(Ok),
                    );
                    if !reject && !malformed && !busy && !stopping {
                        let job = dispatcher
                            .state
                            .lock()
                            .unwrap()
                            .job
                            .take()
                            .expect("one dispatched request");
                        let reply = CoordinatorReply::Success(OperationResponse::from_payload(
                            "health",
                            job.invocation.output_format,
                            serde_json::json!({"ok": true}),
                        ));
                        registry.send_response(job.stream, reply, job._connection);
                        registry.join_writers();
                    }
                });
                let command = CoordinatorCommand::Execute {
                    operation_id: "health".into(),
                    invocation: OperationInvocation {
                        repo: RepoSelector::default(),
                        arguments: serde_json::json!({"query": "SECRET_PHASE_PAYLOAD"}),
                        output_format: crate::api::OutputFormat::Typed,
                    },
                };
                use std::io::Write;
                let mut client = TcpStream::connect(endpoint).unwrap();
                let guard = register_client(&client).unwrap();
                if malformed {
                    client.write_all(b"not-json\n").unwrap();
                } else {
                    write_frame_until(
                        &mut client,
                        &CoordinatorRequest {
                            version: COORDINATOR_PROTOCOL_VERSION,
                            token: state.token.clone(),
                            timeout_ms: None,
                            command,
                        },
                        Instant::now() + std::time::Duration::from_secs(5),
                    )
                    .unwrap();
                }
                server.join().unwrap();
                let reply: CoordinatorReply = read_frame_until(
                    &mut client,
                    Instant::now() + std::time::Duration::from_secs(5),
                )
                .unwrap();
                if malformed || reject || busy || stopping {
                    let CoordinatorReply::Failure(error) = reply else {
                        panic!("expected rejection");
                    };
                    if busy || stopping {
                        assert_eq!(error.code, "graph_busy");
                        assert!(error.retryable);
                    }
                } else {
                    assert!(matches!(reply, CoordinatorReply::Success(_)));
                }
                drop(guard);
                let snapshot = *EVENTS.lock().unwrap();
                let output = snapshot.render(std::process::id(), true);
                let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
                if malformed {
                    assert!(text.contains("corr_receive_failed 1\n"));
                    assert!(text.contains("corr_early_reply 1\n"));
                    assert!(!text.contains("corr_dispatch 1\n"));
                } else if reject {
                    assert!(text.contains("corr_receive 1\n"));
                    assert!(text.contains("corr_auth_rejected 1\n"));
                    assert!(text.contains("corr_early_reply 1\n"));
                    assert!(!text.contains("corr_dispatch 1\n"));
                } else if busy || stopping {
                    assert!(text.contains("corr_receive 1\n"));
                    assert!(text.contains("corr_early_reply 1\n"));
                    assert!(!text.contains("corr_dispatch 1\n"));
                } else {
                    assert!(text.contains("corr_receive 1\n"));
                    assert!(text.contains("corr_dispatch 1\n"));
                    assert!(text.contains("corr_response_written 1\n"));
                }
                assert!(!text.contains("SECRET_PHASE_PAYLOAD"));
                assert!(!text.contains(&"a".repeat(64)));
                assert!(!text.contains(&endpoint.to_string()));
                return;
            }
            for mode in ["receive", "reject", "busy", "stopping", "success"] {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "coordinator::fixture_trace::tests::actual_early_rejection_and_success_are_correlated", "--nocapture"])
                    .env(CHILD, mode).output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        #[test]
        fn actual_worker_and_writer_drop_correlation_after_client_guard() {
            const CHILD: &str = "CGR_CORRELATION_ACTUAL_WORKER_CHILD";
            if std::env::var_os(CHILD).is_some() {
                use super::super::*;
                ENABLED.store(true, Ordering::Relaxed);
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                listener.set_nonblocking(true).unwrap();
                let endpoint = listener.local_addr().unwrap();
                let state = CoordinatorState {
                    version: COORDINATOR_PROTOCOL_VERSION,
                    endpoint,
                    token: "a".repeat(64),
                    pid: std::process::id(),
                    supports_deadlines: true,
                };
                let server_state = state.clone();
                let stop = Arc::new(AtomicBool::new(false));
                let server_stop = Arc::clone(&stop);
                let dispatcher = Arc::new(ExecutionDispatcher::default());
                let (started_tx, started_rx) = std::sync::mpsc::channel();
                let (release_tx, release_rx) = std::sync::mpsc::channel();
                let release_rx = Mutex::new(release_rx);
                let server = std::thread::spawn(move || {
                    serve_owner_with_executor(
                        listener,
                        &server_state,
                        &server_stop,
                        &dispatcher,
                        Ok,
                        move |operation_id, invocation, _| {
                            started_tx.send(()).unwrap();
                            release_rx
                                .lock()
                                .unwrap()
                                .recv_timeout(std::time::Duration::from_secs(5))
                                .unwrap();
                            Ok(OperationResponse::from_payload(
                                operation_id,
                                invocation.output_format,
                                serde_json::json!({"ok": true}),
                            ))
                        },
                    );
                });
                let mut client = TcpStream::connect(endpoint).unwrap();
                let guard = register_client(&client).unwrap();
                let request = CoordinatorRequest {
                    version: COORDINATOR_PROTOCOL_VERSION,
                    token: state.token,
                    timeout_ms: None,
                    command: CoordinatorCommand::Execute {
                        operation_id: "health".into(),
                        invocation: OperationInvocation {
                            repo: RepoSelector::default(),
                            arguments: serde_json::json!({}),
                            output_format: crate::api::OutputFormat::Typed,
                        },
                    },
                };
                write_frame_until(
                    &mut client,
                    &request,
                    Instant::now() + std::time::Duration::from_secs(5),
                )
                .unwrap();
                started_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                drop(guard);
                release_tx.send(()).unwrap();
                let reply: CoordinatorReply = read_frame_until(
                    &mut client,
                    Instant::now() + std::time::Duration::from_secs(5),
                )
                .unwrap();
                assert!(matches!(reply, CoordinatorReply::Success(_)));
                stop.store(true, Ordering::Release);
                server.join().unwrap();
                let snapshot = *EVENTS.lock().unwrap();
                let output = snapshot.render(std::process::id(), true);
                let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
                assert!(text.contains("corr_worker_reply 0\n"), "{text}");
                assert!(text.contains("corr_response_written 0\n"), "{text}");
                assert!(!text.contains("corr_worker_reply 1\n"), "{text}");
                assert!(!text.contains("corr_response_written 1\n"), "{text}");
                return;
            }
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::actual_worker_and_writer_drop_correlation_after_client_guard", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[test]
        fn actual_preaccept_close_is_not_misidentified_as_server_dispatch() {
            use std::io::{Read, Write};
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            client
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let tuple = Tuple(client.local_addr().unwrap(), client.peer_addr().unwrap());
            let mut table = Correlation::new();
            let guard = table.register(tuple, 11).unwrap();
            client.write_all(b"synthetic\n").unwrap();
            drop(listener);
            let mut byte = [0];
            assert!(!matches!(client.read(&mut byte), Ok(count) if count > 0));
            drop(guard);
            assert_eq!(table.lookup(tuple), 0);
        }

        #[test]
        fn actual_dispatched_disconnect_correlates_without_replay() {
            const CHILD: &str = "CGR_CORRELATION_SYNTHETIC_CHILD";
            const SENTINEL: &str = "CGR_CORRELATION_SECRET_SENTINEL";
            if std::env::var_os(CHILD).is_some() {
                use super::super::*;
                ENABLED.store(true, Ordering::Relaxed);
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let endpoint = listener.local_addr().unwrap();
                let state = CoordinatorState {
                    version: COORDINATOR_PROTOCOL_VERSION,
                    endpoint,
                    token: "a".repeat(64),
                    pid: std::process::id(),
                    supports_deadlines: true,
                };
                let server_state = state.clone();
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    let registry = Arc::new(ConnectionRegistry::default());
                    let permit = registry.register(&stream).unwrap();
                    let dispatcher = ExecutionDispatcher::default();
                    handle_connection_dispatch(
                        &mut stream,
                        Instant::now(),
                        &server_state,
                        &dispatcher,
                        permit,
                        Arc::new(Ok),
                    );
                    let job = dispatcher
                        .state
                        .lock()
                        .unwrap()
                        .job
                        .take()
                        .expect("exactly one request was dispatched");
                    let _ = job.stream.shutdown(std::net::Shutdown::Both);
                });
                let invocation = OperationInvocation {
                    repo: RepoSelector::default(),
                    arguments: serde_json::json!({"query": SENTINEL}),
                    output_format: crate::api::OutputFormat::Typed,
                };
                let command = CoordinatorCommand::Execute {
                    operation_id: "health".into(),
                    invocation,
                };
                let error =
                    send_to_state_with_context(&state, &command, ExecutionContext::default())
                        .unwrap_err();
                assert_eq!(error.code, "coordinator_transport_error");
                assert!(!error.retryable);
                server.join().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(100));
                return;
            }
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "coordinator::fixture_trace::tests::actual_dispatched_disconnect_correlates_without_replay", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8_lossy(&output.stderr);
            let id = text
                .lines()
                .find_map(|line| line.strip_prefix("CGR_COORD_TRACE client_read_failed "))
                .unwrap();
            assert_ne!(id, "0");
            assert!(text.contains(&format!("CGR_COORD_TRACE corr_receive {id}\n")));
            assert!(text.contains(&format!("CGR_COORD_TRACE corr_dispatch {id}\n")));
            assert!(!text.contains(SENTINEL));
            assert!(!text.contains(&"a".repeat(64)));
            assert!(output.stderr.len() <= OUTPUT_LIMIT);
        }

        #[test]
        fn fixture_trace_is_default_off_and_requires_exact_opt_in() {
            assert!(!enabled_value(None));
            assert!(!enabled_value(Some(std::ffi::OsStr::new("true"))));
            assert!(enabled_value(Some(std::ffi::OsStr::new("1"))));
        }

        #[test]
        fn fixture_trace_contention_drops_observation_and_render_is_one_shot() {
            let ring = Mutex::new(Ring::new());
            let held = ring.lock().unwrap();
            assert!(!try_record(&ring, Kind::Dispatch, 7));
            drop(held);
            assert!(try_record(&ring, Kind::Dispatch, 8));
            let rendered = AtomicBool::new(false);
            assert!(claim_once(&rendered));
            assert!(!claim_once(&rendered));
        }

        #[test]
        fn fixture_trace_ring_wraps_and_render_is_bounded_static_data() {
            let mut ring = Ring::new();
            for id in 0..80 {
                ring.push(Kind::Dispatch, id);
            }
            ring.push(Kind::ClientReadFailed, 0);
            let output = ring.render(17, false);
            assert!(output.len <= OUTPUT_LIMIT);
            let text = std::str::from_utf8(&output.bytes[..output.len]).unwrap();
            assert!(text.contains("owner_pid_matches=false"));
            assert!(text.contains("server_trace_may_be_incomplete=true"));
            assert!(text.contains("client_read_failed"));
            assert!(!text.contains("dispatch 0\n"));
            assert!(text.contains("ring_overwrite"));
            assert!(text.contains("render_truncated"));
            assert!(text
                .lines()
                .all(|line| line.starts_with("CGR_COORD_TRACE ")));
        }
    }
}

const COORDINATOR_PROTOCOL_VERSION: u64 = 1;
const COORDINATOR_AUTHENTICATION_FAILED: &str = "coordinator_authentication_failed";
const COORDINATOR_REQUEST_RECEIVE_FAILED: &str = "coordinator_request_receive_failed";
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const STREAM_IO_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const MONITOR_INTERVAL: Duration = Duration::from_secs(1);
const COORDINATOR_MAX_CONNECTIONS: usize = 32;
const COORDINATOR_POLL_INTERVAL: Duration = Duration::from_millis(10);
const COORDINATOR_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DEADLINE_MILLIS: u64 = 900;
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct CoordinatorApiConfig {
    selector: RepoSelector,
    refresh: Option<RefreshServiceConfig>,
    identity: Option<RepositoryIdentity>,
}

impl CoordinatorApiConfig {
    pub(crate) fn new(selector: RepoSelector, refresh: Option<RefreshServiceConfig>) -> Self {
        Self {
            selector,
            refresh,
            identity: None,
        }
    }

    fn build_api(
        &self,
        cancellation: RefreshCancellation,
    ) -> (CodebaseGraphApi, Option<RefreshRuntime>) {
        CodebaseGraphApi::for_coordinator_owner(self.selector.clone(), self.refresh, cancellation)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CoordinatorClient {
    inner: Arc<ClientInner>,
}

#[derive(Debug)]
struct ClientInner {
    control: CoordinatorControlPaths,
    api_config: CoordinatorApiConfig,
    route: Mutex<CoordinatorRoute>,
    monitor_stop: Arc<AtomicBool>,
    monitor: JoinedThread,
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        self.monitor_stop.store(true, Ordering::Release);
        // The final strong reference may be released by this monitor itself.
        // That case returns to monitor_route's immediate post-drop exit tail.
        // External callers always join; no shared mutex spans the wait.
        self.monitor.join();
    }
}

#[derive(Debug)]
enum JoinState {
    Empty,
    Running(JoinHandle<()>),
    // Preserve target identity even after another drainer takes the handle.
    Joining(thread::ThreadId),
    Joined,
}

#[derive(Debug)]
struct JoinedThread {
    state: Mutex<JoinState>,
    completed: Condvar,
}
impl JoinedThread {
    fn new(handle: Option<JoinHandle<()>>) -> Self {
        Self {
            state: Mutex::new(handle.map_or(JoinState::Empty, JoinState::Running)),
            completed: Condvar::new(),
        }
    }
    fn install(&self, handle: JoinHandle<()>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(matches!(*state, JoinState::Empty));
        *state = JoinState::Running(handle);
    }
    fn is_running(&self) -> bool {
        match &*self.state.lock().unwrap_or_else(|e| e.into_inner()) {
            JoinState::Running(handle) => !handle.is_finished(),
            JoinState::Joining(_) => true,
            _ => false,
        }
    }
    /// False means the target is the calling thread, never completed drain.
    fn join(&self) -> bool {
        let handle = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                match &*state {
                    JoinState::Empty | JoinState::Joined => return true,
                    JoinState::Running(handle) => {
                        let target = handle.thread().id();
                        if target == thread::current().id() {
                            return false;
                        }
                        let JoinState::Running(handle) =
                            std::mem::replace(&mut *state, JoinState::Joining(target))
                        else {
                            unreachable!()
                        };
                        break handle;
                    }
                    JoinState::Joining(target) if *target == thread::current().id() => {
                        return false
                    }
                    JoinState::Joining(_) => {
                        state = self
                            .completed
                            .wait(state)
                            .unwrap_or_else(|e| e.into_inner())
                    }
                }
            }
        };
        // First local, last to drop, including if a join panic payload panics.
        // Concurrent drainers cannot pass until all join-owned disposal ends.
        let _completion = JoinCompletion(self);
        let result = handle.join();
        drop(result);
        true
    }
}
struct JoinCompletion<'a>(&'a JoinedThread);
impl Drop for JoinCompletion<'_> {
    fn drop(&mut self) {
        *self.0.state.lock().unwrap_or_else(|e| e.into_inner()) = JoinState::Joined;
        self.0.completed.notify_all();
    }
}

#[derive(Debug, Default)]
struct CoordinatorRoute {
    state: Option<CoordinatorState>,
    owner: Option<Arc<CoordinatorOwner>>,
}

#[derive(Debug)]
struct CoordinatorOwner {
    stop: Arc<AtomicBool>,
    thread: JoinedThread,
    dispatcher: Arc<ExecutionDispatcher>,
    refresh_cancel: RefreshCancellation,
}

impl Drop for CoordinatorOwner {
    fn drop(&mut self) {
        self.stop_and_drain();
    }
}
impl CoordinatorOwner {
    fn is_running(&self) -> bool {
        self.thread.is_running()
    }
    fn stop_and_drain(&self) {
        self.stop.store(true, Ordering::Release);
        self.refresh_cancel.request_stop();
        self.dispatcher.stop_admission();
        assert!(
            self.thread.join(),
            "coordinator owner cannot drain its own serving thread"
        );
    }
}

#[derive(Clone, Debug)]
struct CoordinatorControlPaths {
    lock: PathBuf,
    state: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CoordinatorState {
    version: u64,
    endpoint: SocketAddr,
    token: String,
    pid: u32,
    #[serde(default)]
    supports_deadlines: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoordinatorRequest {
    version: u64,
    token: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    command: CoordinatorCommand,
}

#[derive(Debug)]
struct ExecutionJob {
    operation_id: String,
    invocation: OperationInvocation,
    context: ExecutionContext,
    stream: TcpStream,
    _connection: ConnectionPermit,
}

#[derive(Debug, Default)]
struct ExecutionDispatchState {
    job: Option<ExecutionJob>,
    active: bool,
    stopping: bool,
}

#[derive(Debug, Default)]
struct ExecutionDispatcher {
    state: Mutex<ExecutionDispatchState>,
    ready: Condvar,
    active_snapshot: AtomicBool,
}

impl ExecutionDispatcher {
    fn try_submit(&self, job: ExecutionJob) -> Result<(), Box<ExecutionJob>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stopping || state.active || state.job.is_some() {
            return Err(Box::new(job));
        }
        state.active = true;
        self.active_snapshot.store(true, Ordering::Release);
        state.job = Some(job);
        self.ready.notify_one();
        Ok(())
    }

    fn take(&self) -> Option<ExecutionJob> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(job) = state.job.take() {
                return Some(job);
            }
            if state.stopping {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn finish_one(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active = false;
        self.active_snapshot.store(false, Ordering::Release);
        self.ready.notify_all();
    }

    fn stop_admission(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.stopping = true;
        self.ready.notify_all();
    }

    fn active(&self) -> bool {
        self.active_snapshot.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
struct ConnectionRegistry {
    next_id: AtomicU64,
    streams: Mutex<std::collections::HashMap<u64, TcpStream>>,
    writers: Mutex<Vec<JoinHandle<()>>>,
}

impl ConnectionRegistry {
    fn register(self: &Arc<Self>, stream: &TcpStream) -> std::io::Result<ConnectionPermit> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut streams = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if streams.len() >= COORDINATOR_MAX_CONNECTIONS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "repository coordinator connection limit reached",
            ));
        }
        streams.insert(id, stream.try_clone()?);
        Ok(ConnectionPermit {
            registry: Arc::clone(self),
            id,
            #[cfg(debug_assertions)]
            trace_id: 0,
        })
    }

    fn close_all(&self) {
        #[cfg(debug_assertions)]
        fixture_trace::record(fixture_trace::Kind::CloseAll, 0);
        let streams = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for stream in streams.values() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn send_response(
        self: &Arc<Self>,
        mut stream: TcpStream,
        reply: CoordinatorReply,
        connection: ConnectionPermit,
    ) {
        #[cfg(debug_assertions)]
        let connection_id = connection.id;
        #[cfg(debug_assertions)]
        let trace_id = connection.trace_id;
        let writer = thread::Builder::new()
            .name("codebase-graph-coordinator-response".to_string())
            .spawn(move || {
                let result = write_frame_until(
                    &mut stream,
                    &reply,
                    Instant::now() + COORDINATOR_FRAME_TIMEOUT,
                );
                #[cfg(debug_assertions)]
                fixture_trace::record(
                    if result.is_ok() {
                        fixture_trace::Kind::ResponseWritten
                    } else {
                        fixture_trace::Kind::ResponseWriteFailed
                    },
                    connection_id,
                );
                #[cfg(debug_assertions)]
                fixture_trace::record_corr(
                    if result.is_ok() {
                        fixture_trace::Kind::CorrResponseWritten
                    } else {
                        fixture_trace::Kind::CorrResponseFailed
                    },
                    trace_id,
                );
                #[cfg(not(debug_assertions))]
                let _ = result;
                drop(connection);
            });
        #[cfg(debug_assertions)]
        if writer.is_err() {
            fixture_trace::record(fixture_trace::Kind::ResponseSpawnFailed, connection_id);
        }
        if let Ok(writer) = writer {
            let mut writers = self
                .writers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut index = 0;
            while index < writers.len() {
                if writers[index].is_finished() {
                    let finished = writers.swap_remove(index);
                    let _ = finished.join();
                } else {
                    index += 1;
                }
            }
            writers.push(writer);
        }
    }

    fn join_writers(&self) {
        let writers = std::mem::take(
            &mut *self
                .writers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        for writer in writers {
            let _ = writer.join();
        }
    }
}

#[derive(Debug)]
struct ConnectionPermit {
    registry: Arc<ConnectionRegistry>,
    id: u64,
    #[cfg(debug_assertions)]
    trace_id: u64,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.registry
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum CoordinatorCommand {
    Ping,
    Execute {
        operation_id: String,
        invocation: OperationInvocation,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
enum CoordinatorReply {
    Pong,
    Success(OperationResponse),
    Failure(ApiError),
}

impl CoordinatorClient {
    pub(crate) fn connect_or_start(mut config: CoordinatorApiConfig) -> Result<Self, String> {
        #[cfg(debug_assertions)]
        fixture_trace::init();
        config.selector = bind_repo_selector(&config.selector)?;
        config.identity = Some(RepositoryIdentity::capture(&config.selector)?);
        let control = coordinator_control_paths(&config.selector)?;
        let client = Self {
            inner: Arc::new(ClientInner {
                control,
                api_config: config,
                route: Mutex::new(CoordinatorRoute::default()),
                monitor_stop: Arc::new(AtomicBool::new(false)),
                monitor: JoinedThread::new(None),
            }),
        };
        client.refresh_route()?;
        client.start_monitor()?;
        Ok(client)
    }

    fn start_monitor(&self) -> Result<(), String> {
        let weak = Arc::downgrade(&self.inner);
        let stop = Arc::clone(&self.inner.monitor_stop);
        let monitor = thread::Builder::new()
            .name("codebase-graph-coordinator-monitor".to_string())
            .spawn(move || monitor_route(weak, stop))
            .map_err(|error| format!("failed to start coordinator monitor: {error}"))?;
        self.inner.monitor.install(monitor);
        Ok(())
    }

    pub(crate) fn execute_invocation(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
    ) -> Result<OperationResponse, ApiError> {
        self.execute_invocation_with_context(operation_id, invocation, ExecutionContext::default())
    }

    pub(crate) fn execute_invocation_with_context(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApiError> {
        let command = CoordinatorCommand::Execute {
            operation_id: operation_id.to_string(),
            invocation: invocation.clone(),
        };
        let reply = self.send_command_with_recovery(&command, context)?;
        match reply {
            CoordinatorReply::Success(response) => Ok(response),
            CoordinatorReply::Failure(error) => Err(error),
            CoordinatorReply::Pong => Err(coordinator_protocol_error(
                "coordinator returned pong for an operation request",
            )),
        }
    }

    fn refresh_route(&self) -> Result<(), String> {
        self.refresh_route_with_context(ExecutionContext::default())
            .map_err(|error| error.message)
    }

    fn refresh_route_with_context(&self, context: ExecutionContext) -> Result<(), ApiError> {
        let election_deadline = context.deadline.map_or_else(
            || Instant::now() + ELECTION_TIMEOUT,
            |deadline| deadline.min(Instant::now() + ELECTION_TIMEOUT),
        );
        loop {
            context.remaining()?;
            if let Some(lease) = try_open_locked(&self.inner.control.lock, LockMode::Exclusive)
                .map_err(|error| {
                    ApiError::new("coordinator_unavailable", error.to_string()).retryable(true)
                })?
            {
                let (state, owner) = start_owner(
                    self.inner.control.clone(),
                    lease,
                    self.inner.api_config.clone(),
                )
                .map_err(|error| ApiError::new("coordinator_unavailable", error).retryable(true))?;
                context.remaining()?;
                let old_owner = {
                    let mut route = self.lock_route(context)?;
                    route.state = Some(state);
                    route.owner.replace(Arc::new(owner))
                };
                drop(old_owner);
                return Ok(());
            }
            if let Ok(state) = read_coordinator_state(&self.inner.control.state) {
                if ping_state_with_context(&state, context).is_ok() {
                    let mut route = self.lock_route(context)?;
                    let same_endpoint = route
                        .state
                        .as_ref()
                        .is_some_and(|current| current.endpoint == state.endpoint);
                    let old_owner = if same_endpoint {
                        None
                    } else {
                        route.owner.take()
                    };
                    route.state = Some(state);
                    drop(route);
                    drop(old_owner);
                    return Ok(());
                }
            }
            if Instant::now() >= election_deadline {
                if context.deadline.is_some() && Instant::now() >= context.deadline.unwrap() {
                    return Err(ExecutionContext::expired_error());
                }
                return Err(ApiError::new(
                    "coordinator_unavailable",
                    format!(
                        "timed out waiting for repository coordinator at {}",
                        self.inner.control.state.display()
                    ),
                )
                .retryable(true));
            }
            thread::sleep(
                COORDINATOR_POLL_INTERVAL
                    .min(election_deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn lock_route(
        &self,
        context: ExecutionContext,
    ) -> Result<MutexGuard<'_, CoordinatorRoute>, ApiError> {
        if context.deadline.is_none() {
            return self.inner.route.lock().map_err(|_| {
                ApiError::new(
                    "coordinator_unavailable",
                    "coordinator route lock is poisoned",
                )
                .retryable(true)
            });
        }
        loop {
            context.remaining()?;
            match self.inner.route.try_lock() {
                Ok(route) => return Ok(route),
                Err(std::sync::TryLockError::Poisoned(error)) => {
                    return Err(ApiError::new(
                        "coordinator_unavailable",
                        format!("coordinator route lock is poisoned: {error}"),
                    )
                    .retryable(true));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(COORDINATOR_POLL_INTERVAL);
                }
            }
        }
    }

    fn send_command(
        &self,
        command: &CoordinatorCommand,
        context: ExecutionContext,
    ) -> Result<CoordinatorReply, ApiError> {
        let state = self.lock_route(context)?.state.clone().ok_or_else(|| {
            ApiError::new(
                "coordinator_unavailable",
                "repository coordinator route is unavailable",
            )
            .retryable(true)
        })?;
        if context.deadline.is_some() && !state.supports_deadlines {
            return Err(ApiError::new(
                "coordinator_deadline_unsupported",
                "the repository coordinator owner does not support bounded graph execution",
            ));
        }
        send_to_state(&state, command, context)
    }

    fn send_command_with_recovery(
        &self,
        command: &CoordinatorCommand,
        context: ExecutionContext,
    ) -> Result<CoordinatorReply, ApiError> {
        let deadline = context
            .deadline
            .unwrap_or_else(|| Instant::now() + COMMAND_RETRY_TIMEOUT);
        loop {
            context.remaining()?;
            match self.send_command(command, context) {
                Ok(reply) if reply_is_safe_to_retry(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(if context.deadline.is_some() {
                            ExecutionContext::expired_error()
                        } else {
                            ApiError::new(
                                "coordinator_unavailable",
                                format!("repository coordinator kept rejecting the request before dispatch: {reply:?}"),
                            )
                            .retryable(true)
                        });
                    }
                    thread::sleep(
                        COORDINATOR_POLL_INTERVAL
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                Ok(reply) if reply_requires_route_refresh(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(if context.deadline.is_some() {
                            ExecutionContext::expired_error()
                        } else {
                            ApiError::new(
                                "coordinator_unavailable",
                                format!("repository coordinator route stayed stale: {reply:?}"),
                            )
                            .retryable(true)
                        });
                    }
                    if context.deadline.is_some() {
                        return Err(ApiError::new(
                            "coordinator_route_stale",
                            "repository coordinator rejected the request; route recovery continues in the background",
                        )
                        .retryable(true));
                    }
                    self.refresh_route_with_context(context)?;
                }
                Ok(reply) => return Ok(reply),
                // A transport failure after connect or request transmission is
                // ambiguous: the owner may already be executing the request.
                // Never replay an operation without an explicit pre-dispatch reply.
                Err(error) => return Err(error),
            }
        }
    }

    /// Returns the local executor state, or `None` when this process is a follower
    /// or cannot take a nonblocking route snapshot.
    pub(crate) fn active_operation(&self) -> Option<bool> {
        self.inner
            .route
            .try_lock()
            .ok()
            .and_then(|route| route.owner.as_ref().map(|owner| owner.dispatcher.active()))
    }

    pub(crate) fn drain_owned_operation(&self) {
        self.inner.monitor_stop.store(true, Ordering::Release);
        assert!(
            self.inner.monitor.join(),
            "an external monitor drain cannot join itself"
        );
        let owner = loop {
            match self.inner.route.try_lock() {
                Ok(route) => break route.owner.clone(),
                Err(std::sync::TryLockError::Poisoned(error)) => {
                    break error.into_inner().owner.clone()
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(COORDINATOR_POLL_INTERVAL)
                }
            }
        };
        if let Some(owner) = owner {
            owner.stop_and_drain();
        }
    }

    #[cfg(test)]
    fn is_owner(&self) -> bool {
        self.inner
            .route
            .lock()
            .map(|route| route.owner.is_some())
            .unwrap_or(false)
    }

    #[cfg(test)]
    fn endpoint(&self) -> Option<SocketAddr> {
        self.inner
            .route
            .lock()
            .ok()
            .and_then(|route| route.state.as_ref().map(|state| state.endpoint))
    }

    #[cfg(test)]
    fn ping(&self) -> Result<(), String> {
        match self
            .send_command_with_recovery(&CoordinatorCommand::Ping, ExecutionContext::default())
            .map_err(|error| error.message)?
        {
            CoordinatorReply::Pong => Ok(()),
            reply => Err(format!(
                "repository coordinator returned a non-pong reply: {reply:?}"
            )),
        }
    }
}

fn monitor_route(weak: Weak<ClientInner>, stop: Arc<AtomicBool>) {
    monitor_route_with_probe(weak, stop, |_| {});
}

enum MonitorPhase {
    BeforeWait,
    Observed,
}
fn monitor_route_with_probe<P: FnMut(MonitorPhase)>(
    weak: Weak<ClientInner>,
    stop: Arc<AtomicBool>,
    mut probe: P,
) {
    loop {
        if stop.load(Ordering::Acquire) || weak.strong_count() == 0 {
            return;
        }
        probe(MonitorPhase::BeforeWait);
        thread::sleep(MONITOR_INTERVAL);
        {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if stop.load(Ordering::Acquire) {
                return;
            }
            let (state, owner) = inner
                .route
                .lock()
                .ok()
                .map(|route| (route.state.clone(), route.owner.clone()))
                .unwrap_or((None, None));
            let reachable = match owner {
                Some(owner) => owner.is_running(),
                None => state
                    .as_ref()
                    .is_some_and(|state| ping_state(state).is_ok()),
            };
            if !reachable {
                let client = CoordinatorClient {
                    inner: Arc::clone(&inner),
                };
                let _ = client.refresh_route();
            }
            probe(MonitorPhase::Observed);
        } // All iteration strong references, including a temporary client, end.
          // A final self-drop can set stop during that destruction. Check the
          // independent token/Weak only AFTER it, before another sleep or work.
        if stop.load(Ordering::Acquire) || weak.strong_count() == 0 {
            return;
        }
    }
}

fn start_owner(
    control: CoordinatorControlPaths,
    lease: CoordinatorLease,
    config: CoordinatorApiConfig,
) -> Result<(CoordinatorState, CoordinatorOwner), String> {
    start_owner_with_probe(control, lease, config, |stage| {
        if let OwnerStartup::RefreshStarted(state) = stage {
            drop(state);
        }
    })
}

enum OwnerStartup {
    BeforeRefresh,
    RefreshStarted(Option<Arc<RefreshState>>),
}

fn start_owner_with_probe<P>(
    control: CoordinatorControlPaths,
    lease: CoordinatorLease,
    config: CoordinatorApiConfig,
    mut started: P,
) -> Result<(CoordinatorState, CoordinatorOwner), String>
where
    P: FnMut(OwnerStartup) + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to bind repository coordinator: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure repository coordinator: {error}"))?;
    let endpoint = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect repository coordinator address: {error}"))?;
    let state = CoordinatorState {
        version: COORDINATOR_PROTOCOL_VERSION,
        endpoint,
        token: coordinator_token(endpoint),
        pid: std::process::id(),
        supports_deadlines: true,
    };
    write_json_atomically(&control.state, &state).map_err(|error| error.to_string())?;
    restrict_state_permissions(&control.state)?;

    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let refresh_cancel = RefreshCancellation::default();
    let server_refresh_cancel = refresh_cancel.clone();
    let server_state = state.clone();
    let state_path = control.state.clone();
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let server_dispatcher = Arc::clone(&dispatcher);
    let thread = thread::Builder::new()
        .name("codebase-graph-coordinator".to_string())
        .spawn(move || {
            started(OwnerStartup::BeforeRefresh);
            let (api, mut refresh_runtime) = config.build_api(server_refresh_cancel);
            started(OwnerStartup::RefreshStarted(
                refresh_runtime.as_ref().map(RefreshRuntime::state),
            ));
            let identity = config
                .identity
                .as_ref()
                .expect("coordinator identity is captured before owner startup")
                .clone();
            let owner_selector = config.selector.clone();
            serve_owner(
                listener,
                &api,
                &owner_selector,
                &identity,
                &server_state,
                &server_stop,
                &server_dispatcher,
            );
            if let Some(runtime) = refresh_runtime.as_mut() {
                runtime.stop_and_join();
            }
            remove_owned_state(&state_path, &server_state.token);
            drop(lease);
        })
        .map_err(|error| format!("failed to start repository coordinator: {error}"))?;
    Ok((
        state,
        CoordinatorOwner {
            stop,
            thread: JoinedThread::new(Some(thread)),
            dispatcher,
            refresh_cancel,
        },
    ))
}

fn serve_owner(
    listener: TcpListener,
    api: &CodebaseGraphApi,
    selector: &RepoSelector,
    identity: &RepositoryIdentity,
    state: &CoordinatorState,
    stop: &AtomicBool,
    dispatcher: &Arc<ExecutionDispatcher>,
) {
    let api = api.clone();
    let selector = selector.clone();
    let identity = identity.clone();
    let owner_api = api.clone();
    serve_owner_with_executor(
        listener,
        state,
        stop,
        dispatcher,
        move |mut invocation| {
            identity
                .validate()
                .map_err(|error| ApiError::new("repository_identity_changed", error))?;
            invocation.repo = selector.clone();
            Ok(invocation)
        },
        move |operation_id, invocation, context| {
            context.remaining()?;
            owner_api.execute_invocation(operation_id, invocation)
        },
    );
}

fn serve_owner_with_executor<P, E>(
    listener: TcpListener,
    state: &CoordinatorState,
    stop: &AtomicBool,
    dispatcher: &Arc<ExecutionDispatcher>,
    prepare: P,
    execute: E,
) where
    P: Fn(OperationInvocation) -> Result<OperationInvocation, ApiError> + Send + Sync + 'static,
    E: Fn(&str, &OperationInvocation, ExecutionContext) -> Result<OperationResponse, ApiError>
        + Send
        + Sync
        + 'static,
{
    #[cfg(debug_assertions)]
    fixture_trace::record(fixture_trace::Kind::OwnerStart, 0);
    let dispatcher = Arc::clone(dispatcher);
    let worker_dispatcher = Arc::clone(&dispatcher);
    let worker_state = state.clone();
    let prepare = Arc::new(prepare);
    let execute = Arc::new(execute);
    let worker_execute = Arc::clone(&execute);
    let worker = spawn_execution_worker(move || {
        while let Some(job) = worker_dispatcher.take() {
            #[cfg(debug_assertions)]
            fixture_trace::record(fixture_trace::Kind::WorkerStart, job._connection.id);
            #[cfg(debug_assertions)]
            fixture_trace::record_corr(
                fixture_trace::Kind::CorrWorkerStart,
                job._connection.trace_id,
            );
            let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if job.context.remaining().is_err() {
                    None
                } else {
                    match worker_execute(&job.operation_id, &job.invocation, job.context) {
                        Ok(mut response) if job.context.remaining().is_ok() => {
                            attach_coordinator_status(&mut response, &worker_state);
                            Some(CoordinatorReply::Success(response))
                        }
                        Ok(_) if job.context.remaining().is_err() => None,
                        Ok(_) => None,
                        Err(_) if job.context.remaining().is_err() => None,
                        Err(error) => Some(CoordinatorReply::Failure(error)),
                    }
                }
            }));
            let reply = match reply {
                Ok(reply) => reply,
                Err(payload) => {
                    #[cfg(debug_assertions)]
                    fixture_trace::record(fixture_trace::Kind::WorkerPanic, job._connection.id);
                    drop(payload);
                    None
                }
            };
            worker_dispatcher.finish_one();
            if let Some(reply) = reply {
                #[cfg(debug_assertions)]
                fixture_trace::record(fixture_trace::Kind::WorkerReply, job._connection.id);
                #[cfg(debug_assertions)]
                fixture_trace::record_corr(
                    fixture_trace::Kind::CorrWorkerReply,
                    job._connection.trace_id,
                );
                let registry = Arc::clone(&job._connection.registry);
                registry.send_response(job.stream, reply, job._connection);
            } else {
                #[cfg(debug_assertions)]
                fixture_trace::record(fixture_trace::Kind::WorkerAbandon, job._connection.id);
                #[cfg(debug_assertions)]
                fixture_trace::record_corr(
                    fixture_trace::Kind::CorrWorkerAbandon,
                    job._connection.trace_id,
                );
                let _ = job.stream.shutdown(std::net::Shutdown::Both);
                drop(job._connection);
            }
        }
    });

    let Ok(worker) = worker else {
        #[cfg(debug_assertions)]
        fixture_trace::record(fixture_trace::Kind::ExecutorSpawnFailed, 0);
        dispatcher.stop_admission();
        return;
    };
    let registry = Arc::new(ConnectionRegistry::default());
    let mut handlers = Vec::new();
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let accepted_at = Instant::now();
                if stream.set_nonblocking(true).is_err() {
                    #[cfg(debug_assertions)]
                    fixture_trace::record(fixture_trace::Kind::NonblockingFailed, 0);
                    continue;
                }
                let Ok(connection) = registry.register(&stream) else {
                    #[cfg(debug_assertions)]
                    fixture_trace::record(fixture_trace::Kind::RegisterFailed, 0);
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                };
                #[cfg(debug_assertions)]
                let connection_id = connection.id;
                #[cfg(debug_assertions)]
                fixture_trace::record(fixture_trace::Kind::Accept, connection_id);
                let server_state = state.clone();
                let server_dispatcher = Arc::clone(&dispatcher);
                let server_prepare = Arc::clone(&prepare);
                if let Ok(handler) = thread::Builder::new()
                    .name("codebase-graph-coordinator-client".to_string())
                    .spawn(move || {
                        handle_connection_dispatch(
                            &mut stream,
                            accepted_at,
                            &server_state,
                            &server_dispatcher,
                            connection,
                            server_prepare,
                        );
                    })
                {
                    handlers.push(handler);
                } else {
                    #[cfg(debug_assertions)]
                    fixture_trace::record(fixture_trace::Kind::HandlerSpawnFailed, connection_id);
                }
                let mut index = 0;
                while index < handlers.len() {
                    if handlers[index].is_finished() {
                        let handler = handlers.swap_remove(index);
                        let _ = handler.join();
                    } else {
                        index += 1;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(COORDINATOR_POLL_INTERVAL);
            }
            Err(_) => thread::sleep(COORDINATOR_POLL_INTERVAL),
        }
    }
    #[cfg(debug_assertions)]
    fixture_trace::record(fixture_trace::Kind::OwnerStop, 0);
    dispatcher.stop_admission();
    registry.close_all();
    for handler in handlers {
        let _ = handler.join();
    }
    let _ = worker.join();
    registry.close_all();
    registry.join_writers();
}

#[cfg(test)]
thread_local! {
    static INJECT_EXECUTOR_SPAWN_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn spawn_execution_worker<F: FnOnce() + Send + 'static>(
    body: F,
) -> std::io::Result<JoinHandle<()>> {
    #[cfg(test)]
    if INJECT_EXECUTOR_SPAWN_FAILURE.with(|flag| flag.replace(false)) {
        drop(body);
        return Err(std::io::Error::other("fixture executor spawn failure"));
    }
    thread::Builder::new()
        .name("codebase-graph-coordinator-executor".into())
        .spawn(body)
}

fn handle_connection_dispatch<P>(
    stream: &mut TcpStream,
    accepted_at: Instant,
    state: &CoordinatorState,
    dispatcher: &ExecutionDispatcher,
    connection: ConnectionPermit,
    prepare: Arc<P>,
) where
    P: Fn(OperationInvocation) -> Result<OperationInvocation, ApiError> + Send + Sync + 'static,
{
    #[cfg(debug_assertions)]
    let mut connection = connection;
    macro_rules! early_reply {
        ($reply:expr) => {{
            let result =
                write_frame_until(stream, $reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
            #[cfg(debug_assertions)]
            fixture_trace::record_corr(
                if result.is_ok() {
                    fixture_trace::Kind::CorrEarlyReply
                } else {
                    fixture_trace::Kind::CorrResponseFailed
                },
                connection.trace_id,
            );
            let _ = result;
        }};
    }
    let request_deadline = accepted_at + COORDINATOR_FRAME_TIMEOUT;
    let received = read_frame_until(stream, request_deadline);
    #[cfg(debug_assertions)]
    {
        connection.trace_id = fixture_trace::correlate_server(stream);
        fixture_trace::record_corr(
            if received.is_ok() {
                fixture_trace::Kind::CorrReceive
            } else {
                fixture_trace::Kind::CorrReceiveFailed
            },
            connection.trace_id,
        );
    }
    let request: CoordinatorRequest = match received {
        Ok(request) => request,
        Err(error) => {
            let reply = coordinator_request_receive_failure(error);
            early_reply!(&reply);
            return;
        }
    };
    if request.version != COORDINATOR_PROTOCOL_VERSION || request.token != state.token {
        #[cfg(debug_assertions)]
        fixture_trace::record_corr(fixture_trace::Kind::CorrAuthRejected, connection.trace_id);
        let reply = CoordinatorReply::Failure(ApiError::new(
            COORDINATOR_AUTHENTICATION_FAILED,
            "repository coordinator protocol or token is invalid",
        ));
        early_reply!(&reply);
        return;
    }
    match request.command {
        CoordinatorCommand::Ping => {
            early_reply!(&CoordinatorReply::Pong);
        }
        CoordinatorCommand::Execute {
            operation_id,
            invocation,
        } => {
            if request.timeout_ms.is_some() && !matches!(operation_id.as_str(), "health" | "search")
            {
                let reply = CoordinatorReply::Failure(ApiError::new(
                    "invalid_coordinator_deadline",
                    "bounded coordinator execution is only supported for health and search",
                ));
                early_reply!(&reply);
                return;
            }
            let context = match coordinator_execution_context(request.timeout_ms, accepted_at) {
                Ok(context) => context,
                Err(error) => {
                    let reply = CoordinatorReply::Failure(error);
                    early_reply!(&reply);
                    return;
                }
            };
            let invocation = match prepare(invocation) {
                Ok(invocation) => invocation,
                Err(error) => {
                    #[cfg(debug_assertions)]
                    fixture_trace::record_corr(
                        fixture_trace::Kind::CorrPrepareRejected,
                        connection.trace_id,
                    );
                    let reply = CoordinatorReply::Failure(error);
                    early_reply!(&reply);
                    return;
                }
            };
            if context.remaining().is_err() {
                let reply = CoordinatorReply::Failure(ExecutionContext::expired_error());
                early_reply!(&reply);
                return;
            }
            let Ok(job_stream) = stream.try_clone() else {
                #[cfg(debug_assertions)]
                fixture_trace::record(fixture_trace::Kind::CloneFailed, connection.id);
                return;
            };
            #[cfg(debug_assertions)]
            fixture_trace::record(fixture_trace::Kind::Dispatch, connection.id);
            #[cfg(debug_assertions)]
            let trace_id = connection.trace_id;
            let job = ExecutionJob {
                operation_id,
                invocation,
                context,
                stream: job_stream,
                _connection: connection,
            };
            let submission = dispatcher.try_submit(job);
            #[cfg(debug_assertions)]
            if submission.is_ok() {
                fixture_trace::record_corr(fixture_trace::Kind::CorrDispatch, trace_id);
            }
            if let Err(job) = submission {
                #[cfg(debug_assertions)]
                fixture_trace::record(fixture_trace::Kind::Busy, job._connection.id);
                let reply = CoordinatorReply::Failure(
                    ApiError::new(
                        "graph_busy",
                        "repository graph executor is already handling another operation",
                    )
                    .retryable(true),
                );
                let result =
                    write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                #[cfg(debug_assertions)]
                fixture_trace::record_corr(
                    if result.is_ok() {
                        fixture_trace::Kind::CorrEarlyReply
                    } else {
                        fixture_trace::Kind::CorrResponseFailed
                    },
                    job._connection.trace_id,
                );
                let _ = result;
                drop(job);
            }
        }
    }
}

fn coordinator_execution_context(
    timeout_ms: Option<u64>,
    accepted_at: Instant,
) -> Result<ExecutionContext, ApiError> {
    let Some(timeout_ms) = timeout_ms else {
        return Ok(ExecutionContext::default());
    };
    if timeout_ms == 0 {
        return Err(ApiError::new(
            "invalid_coordinator_deadline",
            "coordinator timeout_ms must be greater than zero",
        ));
    }
    let timeout = Duration::from_millis(timeout_ms.min(MAX_DEADLINE_MILLIS));
    Ok(ExecutionContext::with_timeout(accepted_at, timeout))
}

#[cfg(test)]
fn receive_request(stream: &mut TcpStream) -> Result<CoordinatorRequest, CoordinatorReply> {
    read_frame_until(stream, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
        .map_err(coordinator_request_receive_failure)
}

fn coordinator_request_receive_failure(error: String) -> CoordinatorReply {
    CoordinatorReply::Failure(
        ApiError::new(COORDINATOR_REQUEST_RECEIVE_FAILED, error).retryable(true),
    )
}

fn attach_coordinator_status(response: &mut OperationResponse, state: &CoordinatorState) {
    if response.operation != "health" {
        return;
    }
    let status = serde_json::json!({
        "role": "owner",
        "pid": state.pid,
        "endpoint": state.endpoint.to_string(),
    });
    if let Some(structured) = response.payload.get_mut("structured") {
        if let Some(object) = structured.as_object_mut() {
            object.insert("coordinator".to_string(), status);
        }
    } else if let Some(object) = response.payload.as_object_mut() {
        object.insert("coordinator".to_string(), status);
    }
}

fn send_to_state(
    state: &CoordinatorState,
    command: &CoordinatorCommand,
    context: ExecutionContext,
) -> Result<CoordinatorReply, ApiError> {
    send_to_state_with_context(state, command, context)
}

fn send_to_state_with_context(
    state: &CoordinatorState,
    command: &CoordinatorCommand,
    context: ExecutionContext,
) -> Result<CoordinatorReply, ApiError> {
    validate_state(state).map_err(|error| ApiError::new("coordinator_protocol_error", error))?;
    context.remaining()?;
    let ping = matches!(command, CoordinatorCommand::Ping);
    let connect_deadline = context.deadline.unwrap_or_else(|| {
        Instant::now()
            + if ping {
                STREAM_IO_TIMEOUT
            } else {
                CONNECT_TIMEOUT
            }
    });
    let connect_budget =
        CONNECT_TIMEOUT.min(connect_deadline.saturating_duration_since(Instant::now()));
    if connect_budget.is_zero() {
        return Err(ExecutionContext::expired_error());
    }
    let mut stream =
        TcpStream::connect_timeout(&state.endpoint, connect_budget).map_err(|error| {
            if context
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                ExecutionContext::expired_error()
            } else {
                ApiError::new(
                    "coordinator_unavailable",
                    format!("failed to connect to repository coordinator: {error}"),
                )
                .retryable(true)
            }
        })?;
    stream.set_nonblocking(true).map_err(|error| {
        ApiError::new(
            "coordinator_unavailable",
            format!("failed to configure coordinator stream: {error}"),
        )
        .retryable(true)
    })?;
    #[cfg(debug_assertions)]
    let trace_guard = (state.pid == std::process::id())
        .then(|| fixture_trace::register_client(&stream))
        .flatten();
    #[cfg(debug_assertions)]
    let trace_id = trace_guard
        .as_ref()
        .map_or(0, fixture_trace::ClientGuard::sequence);
    let timeout_ms = context
        .remaining()?
        .map(|remaining| remaining.as_nanos().saturating_add(999_999) / 1_000_000)
        .map(|millis| millis.clamp(1, u128::from(MAX_DEADLINE_MILLIS)) as u64);
    let coordinator_deadline =
        timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
    let execution_deadline = context
        .deadline
        .into_iter()
        .chain(coordinator_deadline)
        .min();
    let write_deadline = execution_deadline
        .unwrap_or_else(|| Instant::now() + COORDINATOR_FRAME_TIMEOUT)
        .min(Instant::now() + COORDINATOR_FRAME_TIMEOUT);
    write_frame_until(
        &mut stream,
        &CoordinatorRequest {
            version: COORDINATOR_PROTOCOL_VERSION,
            token: state.token.clone(),
            timeout_ms,
            command: clone_command(command),
        },
        write_deadline,
    )
    .map_err(|error| {
        if execution_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            ExecutionContext::expired_error()
        } else {
            ApiError::new("coordinator_transport_error", error).retryable(ping)
        }
    })?;
    let read_deadline =
        execution_deadline.or_else(|| ping.then_some(Instant::now() + STREAM_IO_TIMEOUT));
    read_frame_with_policy(&mut stream, read_deadline, read_deadline.is_none()).map_err(|error| {
        let expired = execution_deadline.is_some_and(|deadline| Instant::now() >= deadline);
        let classified = if expired {
            ExecutionContext::expired_error()
        } else {
            ApiError::new("coordinator_transport_error", error).retryable(ping)
        };
        #[cfg(debug_assertions)]
        if !ping && !expired {
            fixture_trace::record_corr(fixture_trace::Kind::ClientReadFailed, trace_id);
            fixture_trace::render_once(state.pid == std::process::id());
        }
        classified
    })
}

fn ping_state(state: &CoordinatorState) -> Result<(), String> {
    ping_state_with_context(state, ExecutionContext::default())
}

fn ping_state_with_context(
    state: &CoordinatorState,
    context: ExecutionContext,
) -> Result<(), String> {
    match send_to_state_with_context(state, &CoordinatorCommand::Ping, context)
        .map_err(|error| error.message)?
    {
        CoordinatorReply::Pong => Ok(()),
        _ => Err("repository coordinator did not answer ping".to_string()),
    }
}

fn reply_requires_route_refresh(reply: &CoordinatorReply) -> bool {
    matches!(
        reply,
        CoordinatorReply::Failure(error) if error.code == COORDINATOR_AUTHENTICATION_FAILED
    )
}

fn reply_is_safe_to_retry(reply: &CoordinatorReply) -> bool {
    matches!(
        reply,
        CoordinatorReply::Failure(error)
            if error.code == COORDINATOR_REQUEST_RECEIVE_FAILED && error.retryable
    )
}

fn clone_command(command: &CoordinatorCommand) -> CoordinatorCommand {
    match command {
        CoordinatorCommand::Ping => CoordinatorCommand::Ping,
        CoordinatorCommand::Execute {
            operation_id,
            invocation,
        } => CoordinatorCommand::Execute {
            operation_id: operation_id.clone(),
            invocation: invocation.clone(),
        },
    }
}

fn coordinator_control_paths(selector: &RepoSelector) -> Result<CoordinatorControlPaths, String> {
    let runtime = resolve_runtime(selector)?;
    if let Some(storage_root) = runtime.storage_root.as_ref() {
        let layout = ManagedLayout::new(storage_root);
        return Ok(CoordinatorControlPaths {
            lock: layout.coordinator_lock_path(),
            state: layout.coordinator_state_path(),
        });
    }
    let layout = DirectLayout::new(&runtime.db_path, &runtime.manifest_path);
    Ok(CoordinatorControlPaths {
        lock: layout.coordinator_lock_path(),
        state: layout.coordinator_state_path(),
    })
}

fn read_coordinator_state(path: &Path) -> Result<CoordinatorState, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect repository coordinator state: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "repository coordinator state must be a real file: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read repository coordinator state: {error}"))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("repository coordinator state is too large".to_string());
    }
    let state: CoordinatorState = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse repository coordinator state: {error}"))?;
    validate_state(&state)?;
    Ok(state)
}

fn validate_state(state: &CoordinatorState) -> Result<(), String> {
    if state.version != COORDINATOR_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported repository coordinator protocol version {}",
            state.version
        ));
    }
    if !state.endpoint.ip().is_loopback() || state.token.len() != 64 {
        return Err("repository coordinator state is invalid".to_string());
    }
    Ok(())
}

fn coordinator_token(endpoint: SocketAddr) -> String {
    let sequence = TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut digest = Sha256::new();
    digest.update(std::process::id().to_le_bytes());
    digest.update(timestamp.to_le_bytes());
    digest.update(sequence.to_le_bytes());
    digest.update(endpoint.to_string().as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
fn write_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<(), String> {
    write_frame_until(stream, value, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
}

fn write_frame_until<T: Serialize>(
    stream: &mut TcpStream,
    value: &T,
    deadline: Instant,
) -> Result<(), String> {
    let mut payload = Vec::new();
    payload
        .try_reserve(4096)
        .map_err(|_| "repository coordinator frame allocation failed".to_string())?;
    serde_json::to_writer(&mut payload, value)
        .map_err(|error| format!("failed to encode repository coordinator frame: {error}"))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(format!(
            "repository coordinator frame exceeds {MAX_FRAME_BYTES} bytes"
        ));
    }
    payload.push(b'\n');
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure coordinator stream: {error}"))?;
    let mut offset = 0;
    while offset < payload.len() {
        if Instant::now() >= deadline {
            return Err("timed out writing repository coordinator frame".to_string());
        }
        match stream.write(&payload[offset..]) {
            Ok(0) => return Err("repository coordinator stream closed while writing".to_string()),
            Ok(written) => offset += written,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(
                    COORDINATOR_POLL_INTERVAL
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(format!(
                    "failed to write repository coordinator frame: {error}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> Result<T, String> {
    read_frame_until(stream, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
}

fn read_frame_until<T: DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<T, String> {
    read_frame_with_policy(stream, Some(deadline), false)
}

fn read_frame_with_policy<T: DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Option<Instant>,
    start_timeout_after_first_byte: bool,
) -> Result<T, String> {
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure coordinator stream: {error}"))?;
    let mut payload = Vec::new();
    payload
        .try_reserve(4096)
        .map_err(|_| "repository coordinator frame allocation failed".to_string())?;
    let mut buffer = [0_u8; 8192];
    let mut deadline = deadline;
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("timed out reading repository coordinator frame".to_string());
        }
        match stream.read(&mut buffer) {
            Ok(0) if payload.is_empty() => {
                return Err("repository coordinator frame is missing".to_string());
            }
            Ok(0) => return Err("repository coordinator frame ended before newline".to_string()),
            Ok(read) => {
                if start_timeout_after_first_byte && deadline.is_none() {
                    deadline = Some(Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                }
                let frame_end = buffer[..read].iter().position(|byte| *byte == b'\n');
                let bytes_to_append = frame_end.unwrap_or(read);
                if payload.len().saturating_add(bytes_to_append) > MAX_FRAME_BYTES {
                    return Err(format!(
                        "repository coordinator frame exceeds {MAX_FRAME_BYTES} bytes"
                    ));
                }
                payload.extend_from_slice(&buffer[..bytes_to_append]);
                if frame_end.is_some() {
                    return serde_json::from_slice(&payload).map_err(|error| {
                        format!("failed to decode repository coordinator frame: {error}")
                    });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let pause = deadline
                    .map(|deadline| {
                        COORDINATOR_POLL_INTERVAL
                            .min(deadline.saturating_duration_since(Instant::now()))
                    })
                    .unwrap_or(COORDINATOR_POLL_INTERVAL);
                if !pause.is_zero() {
                    thread::sleep(pause);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(format!(
                    "failed to read repository coordinator frame: {error}"
                ))
            }
        }
    }
}

fn coordinator_protocol_error(message: impl Into<String>) -> ApiError {
    ApiError::new("coordinator_protocol_error", message.into())
}

fn remove_owned_state(path: &Path, token: &str) {
    if read_coordinator_state(path)
        .ok()
        .is_some_and(|state| state.token == token)
    {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
fn restrict_state_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to restrict repository coordinator state: {error}"))
}

#[cfg(not(unix))]
fn restrict_state_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "coordinator/tests.rs"]
mod tests;
