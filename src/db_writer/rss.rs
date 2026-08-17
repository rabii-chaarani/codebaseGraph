use std::io;

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
    // SAFETY: a successful `proc_pid_rusage` call initialized `info` above.
    Ok(unsafe { info.assume_init() }.ri_resident_size)
}

#[cfg(target_os = "linux")]
pub(crate) fn sample_process_rss(pid: u32) -> io::Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    let kib = status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "VmRSS is missing"))?;
    kib.checked_mul(1024)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "VmRSS overflowed u64"))
}

#[cfg(windows)]
pub(crate) fn sample_process_rss(pid: u32) -> io::Result<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
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
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>())
            .expect("PROCESS_MEMORY_COUNTERS size should fit u32"),
        ..Default::default()
    };
    // SAFETY: `counters` is correctly sized and writable for the duration of the call.
    let status = unsafe {
        GetProcessMemoryInfo(
            handle,
            &mut counters,
            u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>())
                .expect("PROCESS_MEMORY_COUNTERS size should fit u32"),
        )
    };
    // SAFETY: `handle` was returned by `OpenProcess` and has not been closed yet.
    unsafe { CloseHandle(handle) };
    if status == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(counters.WorkingSetSize)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "working set exceeds u64"))
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
    use super::sample_process_rss;

    #[test]
    fn sampler_reports_current_process_resident_memory() {
        let rss = sample_process_rss(std::process::id()).expect("current RSS should be readable");
        assert!(rss > 0);
    }
}
