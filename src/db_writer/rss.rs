use std::io;

/// Ask the Rust and platform allocators to return unused pages to the OS.
///
/// Materialization deliberately separates allocation-heavy Rust phases from
/// Ladybug database phases. Dropping the Rust collections is not sufficient on
/// every allocator: arenas may keep those free pages resident and make the next
/// isolated database child appear to exceed the combined memory budget. This
/// call is therefore made only at that coarse phase boundary, never on a hot
/// path.
pub(crate) fn release_unused_allocator_pages() -> u64 {
    // SAFETY: this is a process-wide mimalloc maintenance operation. It does
    // not invalidate live allocations and retains no caller-owned pointers.
    unsafe { libmimalloc_sys::mi_collect(true) };
    release_unused_platform_allocator_pages()
}

#[cfg(target_os = "macos")]
fn release_unused_platform_allocator_pages() -> u64 {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(
            zone: *mut libc::malloc_zone_t,
            goal: libc::size_t,
        ) -> libc::size_t;
    }

    // SAFETY: passing a null zone asks the system allocator to examine all
    // registered zones, and a zero goal requests maximal pressure relief. The
    // function does not retain the pointer and is thread-safe allocator API.
    unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) as u64 }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn release_unused_platform_allocator_pages() -> u64 {
    // SAFETY: `malloc_trim(0)` operates on the process allocator and retains no
    // caller-owned pointers. Its return value only reports whether memory was
    // released, not the number of bytes.
    u64::from(unsafe { libc::malloc_trim(0) } != 0)
}

#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn release_unused_platform_allocator_pages() -> u64 {
    0
}

#[cfg(target_os = "macos")]
pub(crate) fn sample_process_rss(pid: u32) -> io::Result<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: `info` points to writable storage for the exact RUSAGE_INFO_V2
    // structure requested, and `proc_pid_rusage` initializes it on success.
    let status = unsafe {
        libc::proc_pid_rusage(
            i32::try_from(pid)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID exceeds i32"))?,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    // `ri_phys_footprint` is the current physical charge attributed to this
    // process. Unlike raw resident size, it excludes clean shared mappings, so
    // adding parent and child values does not count the same Ladybug/runtime
    // pages twice.
    // SAFETY: a successful `proc_pid_rusage` call initialized `info` above.
    Ok(unsafe { info.assume_init() }.ri_phys_footprint)
}

#[cfg(target_os = "linux")]
pub(crate) fn sample_process_rss(pid: u32) -> io::Result<u64> {
    let proportional_path = format!("/proc/{pid}/smaps_rollup");
    let (contents, field) = match std::fs::read_to_string(&proportional_path) {
        Ok(contents) => (contents, "Pss:"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (
            std::fs::read_to_string(format!("/proc/{pid}/status"))?,
            "VmRSS:",
        ),
        Err(error) => return Err(error),
    };
    let kib = parse_kib_field(&contents, field)?;
    kib.checked_mul(1024)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "memory sample overflowed u64"))
}

#[cfg(target_os = "linux")]
fn parse_kib_field(contents: &str, field: &str) -> io::Result<u64> {
    contents
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("{field} is missing")))
}

#[cfg(windows)]
pub(crate) fn sample_process_rss(pid: u32) -> io::Result<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    // SAFETY: the handle is checked before use and closed on every path below.
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let counters_size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>())
        .expect("PROCESS_MEMORY_COUNTERS_EX size should fit u32");
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: counters_size,
        ..Default::default()
    };
    // SAFETY: `counters` is correctly sized and writable for the duration of the call.
    let status = unsafe {
        GetProcessMemoryInfo(
            handle,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters_size,
        )
    };
    // SAFETY: `handle` was returned by `OpenProcess` and has not been closed yet.
    unsafe { CloseHandle(handle) };
    if status == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(counters.PrivateUsage)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "private usage exceeds u64"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) fn sample_process_rss(_pid: u32) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "RSS sampling is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::{release_unused_allocator_pages, sample_process_rss};

    #[test]
    fn sampler_reports_current_process_memory_charge() {
        let rss = sample_process_rss(std::process::id()).expect("current RSS should be readable");
        assert!(rss > 0);
    }

    #[test]
    fn allocator_pressure_relief_is_safe_at_phase_boundaries() {
        let allocation = vec![0_u8; 1024 * 1024];
        assert_eq!(allocation.len(), 1024 * 1024);
        drop(allocation);

        let _released_bytes_or_status = release_unused_allocator_pages();
    }
}
