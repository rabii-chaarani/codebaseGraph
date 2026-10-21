#![allow(dead_code)]
#[path = "/private/tmp/cleanup-v2-s3-review-_098ctam/repo/src/storage/maintenance_budget.rs"]
mod maintenance_budget;
#[path = "/private/tmp/cleanup-v2-s3-review-_098ctam/repo/src/storage/maintenance_json.rs"]
mod maintenance_json;
#[path = "/private/tmp/cleanup-v2-s3-review-_098ctam/repo/src/storage/maintenance.rs"]
mod maintenance;

use maintenance_budget::{CleanupMemory, MutationGrant, ResourceLimit};
use maintenance_json::{JsonFailure, MaintenanceJsonReader, TokenKind};
use maintenance::{CursorAddress, MaintenanceCursor, WorkCategory};
use sha2::{Digest, Sha256};
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

struct ObservedAllocator;
static FAIL_NEXT: AtomicBool = AtomicBool::new(false);
static WATCH_MEMORY: AtomicPtr<CleanupMemory> = AtomicPtr::new(std::ptr::null_mut());
static WATCH_CHARGE: AtomicUsize = AtomicUsize::new(0);
static WATCH_BYTES: AtomicUsize = AtomicUsize::new(0);
static OBSERVED_ALLOC: AtomicUsize = AtomicUsize::new(0);
static OBSERVED_FREE: AtomicUsize = AtomicUsize::new(0);
static WRONG_CHARGE: AtomicBool = AtomicBool::new(false);
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if FAIL_NEXT.swap(false, Ordering::SeqCst) {
            return std::ptr::null_mut();
        }
        inspect_charge(layout, true);
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if FAIL_NEXT.swap(false, Ordering::SeqCst) {
            return std::ptr::null_mut();
        }
        inspect_charge(layout, true);
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        inspect_charge(layout, false);
        System.dealloc(pointer, layout)
    }
}
unsafe fn inspect_charge(layout: Layout, allocation: bool) {
    let memory = WATCH_MEMORY.load(Ordering::SeqCst);
    if !memory.is_null() && layout.size() == WATCH_BYTES.load(Ordering::SeqCst) {
        if (*memory).used() != WATCH_CHARGE.load(Ordering::SeqCst) {
            WRONG_CHARGE.store(true, Ordering::SeqCst);
        }
        if allocation { OBSERVED_ALLOC.fetch_add(1, Ordering::SeqCst); }
        else { OBSERVED_FREE.fetch_add(1, Ordering::SeqCst); }
    }
}
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

fn allocation_checks() {
    FAIL_NEXT.store(true, Ordering::SeqCst);
    assert!(matches!(CleanupMemory::new(4096), Err(ResourceLimit::AllocationFailed)));
    assert!(!FAIL_NEXT.load(Ordering::SeqCst));
    let mut memory = CleanupMemory::new(8192).unwrap();
    let floor = memory.used();
    FAIL_NEXT.store(true, Ordering::SeqCst);
    assert!(matches!(memory.reserve(513).unwrap().bytes(513), Err(ResourceLimit::AllocationFailed)));
    assert!(!FAIL_NEXT.load(Ordering::SeqCst));
    assert_eq!(memory.used(), floor);
    WATCH_MEMORY.store(&mut memory, Ordering::SeqCst);
    WATCH_BYTES.store(512, Ordering::SeqCst);
    WATCH_CHARGE.store(floor + 512, Ordering::SeqCst);
    let mut bytes = memory.reserve(512).unwrap().bytes(512).unwrap();
    assert!(bytes.as_slice().iter().all(|b| *b == 0));
    bytes.as_mut_slice().fill(42);
    assert!(bytes.as_slice().iter().all(|b| *b == 42));
    drop(bytes);
    WATCH_MEMORY.store(std::ptr::null_mut(), Ordering::SeqCst);
    assert_eq!(OBSERVED_ALLOC.load(Ordering::SeqCst), 1);
    assert_eq!(OBSERVED_FREE.load(Ordering::SeqCst), 1);
    assert!(!WRONG_CHARGE.load(Ordering::SeqCst));
    assert_eq!(memory.used(), floor);
    let zero = memory.reserve(0).unwrap().bytes(0).unwrap();
    assert!(zero.as_slice().is_empty());
    drop(zero);
    let memory = CleanupMemory::new(usize::MAX).unwrap();
    let floor = memory.used();
    assert!(matches!(memory.reserve(isize::MAX as usize + 1).unwrap().bytes(isize::MAX as usize + 1), Err(ResourceLimit::ArithmeticOverflow)));
    assert_eq!(memory.used(), floor);
    assert!(matches!(memory.reserve(usize::MAX), Err(ResourceLimit::ArithmeticOverflow)));
    let mut reservation = memory.reserve(100).unwrap();
    let zero = reservation.split(0).unwrap();
    let child = reservation.split(100).unwrap();
    assert_eq!(reservation.remaining(), 0);
    assert_eq!(child.remaining(), 100);
    drop(memory);
    drop(zero);
    drop(reservation);
    let buffer = child.bytes(100).unwrap();
    std::thread::spawn(move || { assert_eq!(buffer.as_slice().len(), 100); drop(buffer); }).join().unwrap();
}

fn concurrent_checks() {
    use std::sync::{Arc, Barrier};
    for _ in 0..16 {
        let discovery=CleanupMemory::new(4096).unwrap();
        let floor=discovery.used(); drop(discovery);
        let memory=CleanupMemory::new(floor+257).unwrap();
        let start=Arc::new(Barrier::new(3));
        let held=Arc::new(Barrier::new(3));
        let release=Arc::new(Barrier::new(3));
        let successes=AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let memory=memory.clone(); let start=start.clone(); let held=held.clone(); let release=release.clone(); let successes=&successes;
                scope.spawn(move || {
                    start.wait();
                    let reservation=memory.reserve(257).ok();
                    if reservation.is_some() { successes.fetch_add(1,Ordering::SeqCst); }
                    held.wait(); release.wait(); drop(reservation);
                });
            }
            start.wait(); held.wait();
            assert_eq!(successes.load(Ordering::SeqCst),1);
            assert_eq!(memory.used(),floor+257);
            release.wait();
        });
        assert_eq!(memory.used(),floor);
        assert_eq!(memory.peak(),floor+257);
    }
}

struct Chunked<'a> { bytes: &'a [u8], chunk: usize, interrupts: usize }
impl Read for Chunked<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.interrupts > 0 { self.interrupts -= 1; return Err(io::ErrorKind::Interrupted.into()); }
        let n = output.len().min(self.chunk).min(self.bytes.len());
        output[..n].copy_from_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        Ok(n)
    }
}
fn parsed(bytes: &[u8], chunk: usize) -> Result<([u8;32], [u8;32]), JsonFailure> {
    let memory = CleanupMemory::new(16384).unwrap();
    MaintenanceJsonReader::new(Chunked { bytes, chunk, interrupts: 2 }, &memory)?.finish()
}
fn check_tokens(bytes: &[u8], chunk: usize) {
    let memory = CleanupMemory::new(16384).unwrap();
    let mut reader = MaintenanceJsonReader::new(Chunked { bytes, chunk, interrupts: 1 }, &memory).unwrap();
    let mut last = 0;
    while let Some(token) = reader.next_token().unwrap() {
        assert!(token.start >= last && token.start < token.end && token.end <= bytes.len() as u64);
        last = token.end;
        let span = &bytes[token.start as usize..token.end as usize];
        match token.kind {
            TokenKind::String { decoded_len, decoded_sha256, .. } => {
                let value: String = serde_json::from_slice(span).unwrap();
                assert_eq!(decoded_len, value.len() as u64);
                assert_eq!(decoded_sha256, <[u8;32]>::from(Sha256::digest(value.as_bytes())));
            }
            TokenKind::ObjectStart => assert_eq!(span, b"{"),
            TokenKind::ObjectEnd => assert_eq!(span, b"}"),
            TokenKind::ArrayStart => assert_eq!(span, b"["),
            TokenKind::ArrayEnd => assert_eq!(span, b"]"),
            TokenKind::Bool(true) => assert_eq!(span, b"true"),
            TokenKind::Bool(false) => assert_eq!(span, b"false"),
            TokenKind::Null => assert_eq!(span, b"null"),
            TokenKind::Number => { serde_json::from_slice::<serde_json::Value>(span).unwrap(); }
        }
    }
    assert_eq!(reader.finish().unwrap().0, <[u8;32]>::from(Sha256::digest(bytes)));
}
fn json_checks() -> (usize, usize) {
    let seeds: &[&[u8]] = &[
        b"0", b"-0", b"1.25e-3", b"true", b"false", b"null", b"[]", b"{}",
        br#"{"z":[0,true,null,"space \n\t\u0000\u001f\uD83D\uDE00"],"a":{"x":3}}"#,
        br#"["\"\\\/\b\f\n\r\t","\u007f\u0080\u07ff\u0800\ud7ff\ue000\udbff\udfff"]"#,
        br#"{"x":1,"x":2}"#,
    ];
    let mut changed = 0;
    let mutations = b"[]{}:,\"\\0ntf.e-+ \n\r\t";
    for seed in seeds {
        assert!(serde_json::from_slice::<serde_json::Value>(seed).is_ok());
        for chunk in [1,2,3,7,4095,4096,4097] { check_tokens(seed, chunk); }
        for index in 0..=seed.len() {
            let mut candidates = Vec::new();
            if index < seed.len() {
                let mut missing=seed.to_vec(); missing.remove(index); candidates.push(missing);
                for byte in mutations { let mut swapped=seed.to_vec(); swapped[index]=*byte; candidates.push(swapped); }
            }
            for byte in mutations { let mut inserted=seed.to_vec(); inserted.insert(index,*byte); candidates.push(inserted); }
            for candidate in candidates {
                let expected=serde_json::from_slice::<serde_json::Value>(&candidate).is_ok();
                assert_eq!(parsed(&candidate, 3).is_ok(), expected, "candidate={candidate:?}");
                changed += 1;
            }
        }
    }
    let mut boundaries = 0;
    for prefix in [4092,4093,4094,4095,4096,4097] {
        for value in ["é", "😀", "\u{10ffff}", "\0\u{1f}\n"] {
            let mut input=vec![b' ';prefix]; input.extend(serde_json::to_vec(value).unwrap());
            for chunk in [1,2,3,7,4095,4096,4097] { check_tokens(&input,chunk); boundaries+=1; }
        }
    }
    for depth in 1..=130 {
        for kind in [0,1] {
            let input=if kind==0 { format!("{}0{}", "[".repeat(depth), "]".repeat(depth)) }
                else { format!("{}0{}", "{\"x\":".repeat(depth), "}".repeat(depth)) };
            assert_eq!(parsed(input.as_bytes(),1).is_ok(),serde_json::from_str::<serde_json::Value>(&input).is_ok(),"depth={depth},kind={kind}");
            boundaries+=1;
        }
    }
    (changed,boundaries)
}

fn cursor_checks() {
    let categories=[WorkCategory::Canonical,WorkCategory::TemporaryOrBackup,WorkCategory::Run,WorkCategory::Phase,WorkCategory::Generation,WorkCategory::LegacyNative,WorkCategory::LegacySnapshot,WorkCategory::DefaultDirect];
    let address=|n:u8| { let mut bytes=[0u8;64]; bytes[63]=n; CursorAddress(bytes) };
    let mut cursor=MaintenanceCursor::new([3;32]);
    let mut seen=[[false;4];8];
    for _ in 0..32 {
        cursor=MaintenanceCursor::decode(&cursor.encode(),[3;32]).unwrap();
        let mut selection=cursor.selection();
        for category in categories.iter().rev() { for n in (0..4).rev() { selection.observe(*category,address(n)); } }
        let (category,candidate)=selection.selected().unwrap();
        let index=category as usize; let n=candidate.0[63] as usize;
        assert!(!seen[index][n]); seen[index][n]=true;
        cursor.inspected(category,candidate);
    }
    assert!(seen.iter().flatten().all(|x|*x));
    let bytes=cursor.encode();
    for index in 0..bytes.len() { let mut invalid=bytes;invalid[index]^=1;assert!(MaintenanceCursor::decode(&invalid,[3;32]).is_err()); }
}
fn main() {
    allocation_checks();
    concurrent_checks();
    let (differentials,boundaries)=json_checks();
    cursor_checks();
    let mut grant=MutationGrant::start();
    grant.take_entries(100).unwrap();
    assert!(grant.check_time().is_ok());
    assert!(grant.take_entry().is_err());
    println!("{{\"allocation_failure_and_lifetime\":\"passed\",\"exclusive_ceiling_races\":16,\"json_mutation_differentials\":{differentials},\"json_boundaries\":{boundaries},\"cursor_restart_selections\":32,\"cursor_bit_corruptions\":593,\"entry_101\":\"rejected\"}}");
}
