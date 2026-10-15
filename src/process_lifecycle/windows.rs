//! Windows 10+ containment, atomically assigned at process creation.
use super::*;
use std::{
    ffi::{OsStr, OsString},
    mem::{size_of, size_of_val, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
        process::ExitStatusExt,
    },
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{CreateDirectoryW, CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL},
    System::{JobObjects::*, Threading::*},
};

use windows_sys::Win32::System::SystemServices::{JOB_OBJECT_QUERY, JOB_OBJECT_TERMINATE};

const CONTEXT: &str = "CODEBASE_GRAPH_WINDOWS_JOB_V2";
struct Handle(HANDLE);
impl Handle {
    fn checked(raw: HANDLE) -> Result<Self, String> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(last("native handle"))
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct SecurityDescriptor(*mut core::ffi::c_void);
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
impl SecurityDescriptor {
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}
fn owner_security() -> Result<SecurityDescriptor, String> {
    // OW is the owner-rights SID. P prevents inherited access grants.
    let sddl = wide(OsStr::new("D:P(A;;GA;;;OW)"))?;
    let mut raw = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut raw, null_mut())
    } == 0
    {
        return Err(last("private lifecycle security"));
    }
    Ok(SecurityDescriptor(raw))
}
/// Win32 creation APIs need the extended prefix for paths beyond MAX_PATH.
/// Make ordinary paths absolute before adding it, preserving UTF-16 losslessly.
fn filesystem_path(path: &Path) -> Result<Vec<u16>, String> {
    let absolute = std::path::absolute(path).map_err(err)?;
    let mut units: Vec<_> = absolute.as_os_str().encode_wide().collect();
    let prefix: Vec<_> = "\\\\?\\".encode_utf16().collect();
    if !units.starts_with(&prefix) {
        for unit in &mut units {
            if *unit == b'/' as u16 {
                *unit = b'\\' as u16;
            }
        }
        let mut extended = prefix;
        if units.starts_with(&[92, 92]) {
            extended.extend("UNC\\".encode_utf16());
            extended.extend_from_slice(&units[2..]);
        } else {
            extended.extend(units);
        }
        units = extended;
    }
    if units.contains(&0) {
        return Err("Windows path contains NUL".into());
    }
    units.push(0);
    Ok(units)
}
pub(super) fn create_private_dir(path: &Path) -> Result<(), String> {
    let path = filesystem_path(path)?;
    let security = owner_security()?;
    if unsafe { CreateDirectoryW(path.as_ptr(), &security.attributes()) } == 0 {
        return Err(last("create private lifecycle directory"));
    }
    Ok(())
}
pub(super) fn create_private_file(path: &Path) -> Result<File, String> {
    let path = filesystem_path(path)?;
    let security = owner_security()?;
    let handle = Handle::checked(unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            &security.attributes(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    })?;
    let file = unsafe { File::from_raw_handle(handle.0) };
    std::mem::forget(handle);
    Ok(file)
}
struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new() -> Result<Self, String> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(last("attribute list size"));
        }
        let mut result = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(result.ptr(), 2, 0, &mut bytes) } == 0 {
            return Err(last("attribute list"));
        }
        result.initialized = true;
        Ok(result)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
    fn set(&mut self, key: u32, value: *mut core::ffi::c_void, bytes: usize) -> Result<(), String> {
        if unsafe {
            UpdateProcThreadAttribute(
                self.ptr(),
                0,
                key as usize,
                value,
                bytes,
                null_mut(),
                null(),
            )
        } == 0
        {
            Err(last("process attribute"))
        } else {
            Ok(())
        }
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }
}
fn last(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}
fn wide(value: &OsStr) -> Result<Vec<u16>, String> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err("Windows string contains NUL".into());
    }
    value.push(0);
    Ok(value)
}
/// Microsoft CRT parsing rules applied to UTF-16, preserving unpaired surrogates.
fn quote(value: &OsStr) -> Result<Vec<u16>, String> {
    let units: Vec<_> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err("Windows argument contains NUL".into());
    }
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in units {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        result.push(unit);
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}
fn job_name(d: &BuildDescriptorV2) -> Result<String, String> {
    crate::storage::containment::windows_job_name(d)
}
fn environment(name: &str) -> Result<Vec<u16>, String> {
    let mut entries: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| !key.to_string_lossy().eq_ignore_ascii_case(CONTEXT))
        .collect();
    entries.push((CONTEXT.into(), name.into()));
    entries.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
    let mut block = Vec::new();
    for (key, value) in entries {
        let mut entry = key;
        entry.push("=");
        entry.push(value);
        block.extend(wide(&entry)?);
    }
    block.push(0);
    Ok(block)
}
fn duplicate(handle: HANDLE) -> Result<Handle, String> {
    let mut output = null_mut();
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut output,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(last("duplicate stdio"));
    }
    Handle::checked(output)
}
fn active(job: HANDLE) -> Result<u32, String> {
    let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            null_mut(),
        )
    } == 0
    {
        return Err(last("containment_unknown: query job membership"));
    }
    Ok(info.ActiveProcesses)
}
fn await_empty(job: HANDLE, deadline: Instant) -> Result<(), String> {
    loop {
        if active(job)? == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("containment_unknown: Windows job still has active processes".into());
        }
        thread::sleep(TICK);
    }
}

pub(super) struct Contained {
    job: Handle,
    process: Handle,
    thread: Option<Handle>,
    pid: u32,
    teardown_attempted: bool,
}
impl Contained {
    pub fn spawn(spec: &SpawnSpec, descriptor: &mut BuildDescriptorV2) -> Result<Self, String> {
        let name = job_name(descriptor)?;
        let name_wide = wide(OsStr::new(&name))?;
        let security = owner_security()?;
        let sa = security.attributes();
        unsafe {
            SetLastError(0);
        }
        let raw_job = unsafe { CreateJobObjectW(&sa, name_wide.as_ptr()) };
        let creation_error = unsafe { GetLastError() };
        let job = Handle::checked(raw_job)?;
        if creation_error == ERROR_ALREADY_EXISTS {
            return Err("Windows containment job already exists".into());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(last("job kill-on-close"));
        }
        descriptor.proof = ContainmentProof::WindowsJob { name: name.clone() };
        // Recovery must know the job identity before any contained child exists.
        write_json(&descriptor.lifecycle_dir.join("proof.json"), descriptor)?;
        let stdin = File::open("NUL").map_err(err)?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.stderr_path)
            .map_err(err)?;
        let stdin = duplicate(stdin.as_raw_handle())?;
        let stdout = duplicate(std::io::stdout().as_raw_handle())?;
        let stderr = duplicate(stderr.as_raw_handle())?;
        let mut handles = [stdin.0, stdout.0, stderr.0];
        let mut jobs = [job.0];
        let mut attributes = Attributes::new()?;
        attributes.set(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            handles.as_mut_ptr().cast(),
            size_of_val(&handles),
        )?;
        attributes.set(
            PROC_THREAD_ATTRIBUTE_JOB_LIST,
            jobs.as_mut_ptr().cast(),
            size_of_val(&jobs),
        )?;
        let application = wide(spec.program.as_os_str())?;
        let mut command = quote(spec.program.as_os_str())?;
        for arg in &spec.args {
            command.push(b' ' as u16);
            command.extend(quote(arg)?);
        }
        command.push(0);
        let mut env = environment(&name)?;
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin.0;
        startup.StartupInfo.hStdOutput = stdout.0;
        startup.StartupInfo.hStdError = stderr.0;
        startup.lpAttributeList = attributes.ptr();
        let mut pi: PROCESS_INFORMATION = unsafe { zeroed() };
        // JOB_LIST contains the process from its first instruction; suspension is only
        // the START gate, never an interval before assignment to containment.
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
                env.as_mut_ptr().cast(),
                null(),
                &startup.StartupInfo,
                &mut pi,
            )
        } == 0
        {
            return Err(last("create contained process"));
        }
        let contained = Self {
            job,
            process: Handle(pi.hProcess),
            thread: Some(Handle(pi.hThread)),
            pid: pi.dwProcessId,
            teardown_attempted: false,
        };
        let mut member = 0;
        if unsafe { IsProcessInJob(contained.process.0, contained.job.0, &mut member) } == 0 {
            return Err(last("verify contained process membership"));
        }
        if member == 0 {
            return Err("created process is not in its containment job".into());
        }
        Ok(contained)
    }
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn start(&mut self) -> Result<(), String> {
        if let Some(thread) = self.thread.as_ref() {
            let previous = unsafe { ResumeThread(thread.0) };
            if previous == u32::MAX {
                return Err(last("start contained process"));
            }
            if previous != 1 {
                return Err(format!(
                    "unexpected Windows start-gate suspension count: {previous}"
                ));
            }
        }
        self.thread.take();
        Ok(())
    }
    pub fn exited(&mut self) -> Result<bool, String> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(last("wait contained process")),
        }
    }
    pub fn teardown(&mut self, deadline: Instant) -> Result<std::process::ExitStatus, String> {
        self.teardown_attempted = true;
        if active(self.job.0)? > 0 && unsafe { TerminateJobObject(self.job.0, 1) } == 0 {
            return Err(last("terminate Windows job"));
        }
        await_empty(self.job.0, deadline)?;
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u32::MAX as u128 - 1) as u32;
        if unsafe { WaitForSingleObject(self.process.0, remaining) } != WAIT_OBJECT_0 {
            return Err(last("wait contained exit"));
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(last("contained exit code"));
        }
        Ok(std::process::ExitStatus::from_raw(code))
    }
}
impl Drop for Contained {
    fn drop(&mut self) {
        if !self.teardown_attempted {
            let _ = self.teardown(Instant::now() + DEADLINE);
        }
    }
}

pub(super) fn recover_empty(descriptor: &BuildDescriptorV2) -> Result<bool, String> {
    let expected = job_name(descriptor)?;
    match &descriptor.proof {
        ContainmentProof::WindowsJob { name } if *name == expected => {}
        _ => return Err("containment_unknown: Windows job proof mismatch".into()),
    }
    let name = wide(OsStr::new(&expected))?;
    let raw = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE, 0, name.as_ptr()) };
    if raw.is_null() {
        let code = unsafe { GetLastError() };
        return if code == ERROR_FILE_NOT_FOUND {
            Ok(true)
        } else {
            Err(format!(
                "containment_unknown: OpenJobObjectW failed ({code})"
            ))
        };
    }
    let job = Handle(raw);
    if active(job.0)? == 0 {
        return Ok(true);
    }
    // This temporary handle must never keep an abandoned job alive.
    if unsafe { TerminateJobObject(job.0, 1) } == 0 {
        return Err(last("containment_unknown: recover Windows job"));
    }
    await_empty(job.0, Instant::now() + DEADLINE)?;
    Ok(true)
}
pub(super) fn validate_inherited() -> Result<bool, String> {
    let Some(name) = std::env::var_os(CONTEXT) else {
        return Ok(false);
    };
    let Some(name) = name.to_str() else {
        return Err("invalid inherited job context".into());
    };
    let Some(identity) = name.strip_prefix("Global\\codebase-graph-v2-") else {
        return Err("invalid inherited job context".into());
    };
    if identity.len() != 64 || !identity.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid inherited job context".into());
    }
    let mut member = 0;
    if unsafe { IsProcessInJob(GetCurrentProcess(), null_mut(), &mut member) } == 0 {
        return Err(last("inherited job membership"));
    }
    if member == 0 {
        return Err("inherited Windows job membership is absent".into());
    }
    // NULL queries our assigned job; opening a named handle here would keep it alive.
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    if unsafe {
        QueryInformationJobObject(
            null_mut(),
            JobObjectExtendedLimitInformation,
            (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            null_mut(),
        )
    } == 0
    {
        return Err(last("inherited job limits"));
    }
    let flags = limits.BasicLimitInformation.LimitFlags;
    if flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0
        && flags & (JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK) == 0
    {
        Ok(true)
    } else {
        Err("inherited Windows job limits do not enforce containment".into())
    }
}
pub(super) fn configure_inherited_command(_command: &mut Command) -> Result<(), String> {
    if validate_inherited()? {
        Ok(())
    } else {
        Err("protected phase requires Windows v2 containment".into())
    }
}
pub(super) fn run_exec(_path: &Path) -> Result<(), String> {
    Err(
        "contained-exec-v2 is Unix-only; Windows guardians create the target directly in its job"
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_file_is_exclusive_noninherited_and_has_protected_dacl() {
        use windows_sys::Win32::Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            GetSecurityDescriptorControl, DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
        };
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cbg-private-{}-{unique}", std::process::id()));
        create_private_dir(&dir).unwrap();
        let path = dir.join("capability.json");
        let mut file = create_private_file(&path).unwrap();
        std::io::Write::write_all(&mut file, b"private capability").unwrap();
        assert!(create_private_file(&path).is_err());
        let mut flags = 0;
        assert_ne!(
            unsafe { GetHandleInformation(file.as_raw_handle(), &mut flags) },
            0
        );
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        let mut raw = null_mut();
        assert_eq!(
            unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    &mut raw,
                )
            },
            ERROR_SUCCESS
        );
        let security = SecurityDescriptor(raw);
        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            unsafe { GetSecurityDescriptorControl(security.0, &mut control, &mut revision) },
            0
        );
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        drop(file);
        assert_eq!(std::fs::read(&path).unwrap(), b"private capability");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn filesystem_paths_support_unc_long_paths_and_utf16() {
        let unc = filesystem_path(Path::new(r"\\server\share\日本語")).unwrap();
        assert_eq!(
            unc,
            r"\\?\UNC\server\share\日本語"
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>()
        );
        let long = format!(r"C:\{}\file", "x".repeat(300));
        let path = filesystem_path(Path::new(&long)).unwrap();
        assert_eq!(
            path,
            format!(r"\\?\{long}")
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>()
        );
        use std::os::windows::ffi::OsStringExt;
        let verbatim = OsString::from_wide(&[92, 92, 63, 92, 67, 58, 92, 0xd800]);
        assert_eq!(
            filesystem_path(Path::new(&verbatim)).unwrap(),
            vec![92, 92, 63, 92, 67, 58, 92, 0xd800, 0]
        );
    }
    #[test]
    fn quoting_preserves_empty_quotes_and_trailing_backslashes() {
        for (input, expected) in [
            ("", "\"\""),
            ("hello world", "\"hello world\""),
            ("a\"b", "\"a\\\"b\""),
            ("x\\", "\"x\\\\\""),
            ("日本語", "\"日本語\""),
        ] {
            assert_eq!(
                quote(OsStr::new(input)).unwrap(),
                expected.encode_utf16().collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn quoting_preserves_unpaired_surrogates_and_rejects_nul() {
        use std::os::windows::ffi::OsStringExt;
        assert_eq!(
            quote(&OsString::from_wide(&[0xd800])).unwrap(),
            vec![34, 0xd800, 34]
        );
        assert!(quote(&OsString::from_wide(&[0])).is_err());
    }
}
