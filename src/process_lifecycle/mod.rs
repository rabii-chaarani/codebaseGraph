//! Private, capability-authenticated build containment. PIDs are diagnostic only.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;
const TICK: Duration = Duration::from_millis(25);
const DEADLINE: Duration = Duration::from_secs(5);
const HEADROOM: u64 = 16 * 1024 * 1024;
const MAX_FRAME: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum OutputMode {
    Pipe,
    Null,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SpawnSpec {
    pub program: PathBuf,
    #[serde(default)]
    pub guardian_executable: Option<PathBuf>,
    pub args: Vec<std::ffi::OsString>,
    pub workspace: PathBuf,
    pub stderr_path: PathBuf,
    pub stdout: OutputMode,
    pub memory_limit_bytes: Option<u64>,
    #[serde(default)]
    pub memory_phase: Option<String>,
    pub parent_rss_pid: Option<u32>,
}
pub(crate) use crate::storage::containment::{BuildDescriptorV2, ContainmentProof, ExitOutcome};
#[derive(Debug)]
pub(crate) enum RecoveryOutcome {
    Empty,
    Terminated(ExitOutcome),
    ContainmentUnknown(String),
}
#[derive(Serialize, Deserialize)]
struct Launch {
    spec: SpawnSpec,
    descriptor: BuildDescriptorV2,
}
#[derive(Serialize, Deserialize)]
struct Control {
    nonce: String,
    command: String,
}

// The owner lifetime pipe is dropped before this guard. Reaping happens on a
// dedicated thread so Drop remains bounded even when a guardian is stopped.
struct GuardianChild(Option<Child>);
impl std::ops::Deref for GuardianChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        self.0.as_ref().expect("guardian exists")
    }
}
impl std::ops::DerefMut for GuardianChild {
    fn deref_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("guardian exists")
    }
}
impl Drop for GuardianChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        }
    }
}

pub(crate) struct PreparedChild {
    guardian: GuardianChild,
    owner: Option<ChildStdin>,
    descriptor: BuildDescriptorV2,
    outcome: Option<ExitOutcome>,
}
impl PreparedChild {
    pub fn prepare(mut spec: SpawnSpec) -> Result<Self, String> {
        // Recovery evidence must survive later process cwd changes. Program and
        // argument interpretation still uses the caller's inherited cwd/PATH.
        if spec.workspace.is_relative() || spec.stderr_path.is_relative() {
            let cwd = std::env::current_dir().map_err(err)?;
            if spec.workspace.is_relative() {
                spec.workspace = cwd.join(&spec.workspace);
            }
            if spec.stderr_path.is_relative() {
                spec.stderr_path = cwd.join(&spec.stderr_path);
            }
        }
        fs::create_dir_all(&spec.workspace).map_err(err)?;
        let build_id = random_id()?;
        let dir = spec.workspace.join(format!(".lifecycle-{build_id}"));
        platform::create_private_dir(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(err)?;
        }
        OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(dir.join("lease"))
            .map_err(err)?;
        let descriptor = BuildDescriptorV2 {
            version: 2,
            build_id,
            lifecycle_dir: dir.clone(),
            endpoint: String::new(),
            nonce: random_id()?,
            worker_pid: 0,
            guardian_pid: 0,
            proof: ContainmentProof::Pending,
        };
        write_json(
            &dir.join("launch.json"),
            &Launch {
                spec: spec.clone(),
                descriptor,
            },
        )?;
        let guardian_executable = match &spec.guardian_executable {
            Some(executable) => executable.clone(),
            None => std::env::current_exe().map_err(err)?,
        };
        let mut command = Command::new(guardian_executable);
        command
            .args(["__codebase_graph_internal", "build-guardian-v2"])
            .arg(dir.join("launch.json"))
            .stdin(Stdio::piped())
            .stdout(match spec.stdout {
                OutputMode::Pipe => Stdio::piped(),
                OutputMode::Null => Stdio::null(),
            })
            .stderr(Stdio::from(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&spec.stderr_path)
                    .map_err(err)?,
            ));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut guardian = GuardianChild(Some(command.spawn().map_err(err)?));
        let owner = guardian.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(15);
        let descriptor = loop {
            if let Ok(bytes) = fs::read(dir.join("ready.json")) {
                break serde_json::from_slice(&bytes).map_err(err)?;
            }
            if let Some(status) = guardian.try_wait().map_err(err)? {
                return Err(format!(
                    "build guardian failed before READY: {status}; inspect {}",
                    spec.stderr_path.display()
                ));
            }
            if Instant::now() >= deadline {
                drop(owner);
                return Err(format!(
                    "build guardian READY timed out; retain {}",
                    dir.display()
                ));
            }
            thread::sleep(TICK);
        };
        Ok(Self {
            guardian,
            owner,
            descriptor,
            outcome: None,
        })
    }
    pub fn descriptor(&self) -> &BuildDescriptorV2 {
        &self.descriptor
    }
    pub fn id(&self) -> u32 {
        self.descriptor.worker_pid
    }
    pub fn start(&mut self) -> Result<(), String> {
        control(&self.descriptor, "START")
    }
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.guardian.stdout.take()
    }
    pub fn try_wait(&mut self) -> Result<Option<ExitOutcome>, String> {
        if let Some(outcome) = &self.outcome {
            return Ok(self
                .guardian
                .try_wait()
                .map_err(err)?
                .map(|_| outcome.clone()));
        }
        match fs::read(self.descriptor.lifecycle_dir.join("receipt.json")) {
            Ok(bytes) => {
                let outcome: ExitOutcome = serde_json::from_slice(&bytes).map_err(err)?;
                self.outcome = Some(outcome.clone());
                Ok(self.guardian.try_wait().map_err(err)?.map(|_| outcome))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(status) = self.guardian.try_wait().map_err(err)? {
                    return Err(format!(
                        "guardian exited {status} without cleanup receipt; containment_unknown"
                    ));
                }
                Ok(None)
            }
            Err(e) => Err(err(e)),
        }
    }
    pub fn wait(&mut self) -> Result<ExitOutcome, String> {
        loop {
            if let Some(o) = self.try_wait()? {
                return Ok(o);
            }
            thread::sleep(TICK)
        }
    }
    pub fn terminate(&mut self, reason: &str) -> Result<ExitOutcome, String> {
        control(&self.descriptor, &format!("CANCEL {reason}"))?;
        let deadline = Instant::now() + DEADLINE + Duration::from_secs(1);
        loop {
            if let Some(o) = self.try_wait()? {
                return Ok(o);
            }
            if Instant::now() >= deadline {
                return Err("containment_unknown: guardian teardown deadline expired".into());
            }
            thread::sleep(TICK)
        }
    }
}
impl Drop for PreparedChild {
    fn drop(&mut self) {
        self.owner.take();
    }
}

pub(crate) fn configure_inherited_command(command: &mut Command) -> Result<(), String> {
    platform::configure_inherited_command(command)
}
pub(crate) fn inherited_containment() -> Result<bool, String> {
    platform::validate_inherited()
}
pub(crate) fn validate_inherited_containment() -> Result<(), String> {
    if platform::validate_inherited()? {
        Ok(())
    } else {
        Err("protected helper requires v2 build containment; restart the daemon".into())
    }
}
pub(crate) fn recover_descriptor(d: &BuildDescriptorV2) -> Result<RecoveryOutcome, String> {
    if d.version != 2 {
        return Ok(RecoveryOutcome::ContainmentUnknown(
            "unsupported containment descriptor version; restart daemon".into(),
        ));
    }
    if let Ok(bytes) = fs::read(d.lifecycle_dir.join("receipt.json")) {
        let o: ExitOutcome = serde_json::from_slice(&bytes).map_err(err)?;
        if o.containment_empty {
            return Ok(RecoveryOutcome::Terminated(o));
        }
    }
    if platform::recover_empty(d)? {
        return Ok(RecoveryOutcome::Empty);
    }
    if control(d, "CANCEL recovery").is_err() {
        return Ok(RecoveryOutcome::ContainmentUnknown(
            "guardian unavailable and containment is not proven empty".into(),
        ));
    }
    let deadline = Instant::now() + DEADLINE + Duration::from_secs(1);
    loop {
        if let Ok(bytes) = fs::read(d.lifecycle_dir.join("receipt.json")) {
            let o: ExitOutcome = serde_json::from_slice(&bytes).map_err(err)?;
            return Ok(if o.containment_empty {
                RecoveryOutcome::Terminated(o)
            } else {
                RecoveryOutcome::ContainmentUnknown(o.to_string())
            });
        }
        if Instant::now() >= deadline {
            return Ok(RecoveryOutcome::ContainmentUnknown(
                "recovery teardown deadline expired".into(),
            ));
        }
        thread::sleep(TICK)
    }
}
/// Checks all lifecycle leases, including workspaces lacking global worker state.
/// The caller owns removal of the workspace and result files.
pub(crate) fn cleanup_if_inactive(workspace: &Path) -> Result<bool, String> {
    crate::storage::containment::probe_registry(workspace)
}
pub(crate) fn run_internal(args: &[String]) -> Result<Option<()>, String> {
    match args.first().map(String::as_str) {
        Some("build-guardian-v2") => {
            if args.len() != 2 {
                return Err("guardian v2 requires exactly one launch path".into());
            }
            run_guardian(Path::new(&args[1]))?;
            Ok(Some(()))
        }
        Some("contained-exec-v2") => {
            if args.len() != 2 {
                return Err("contained exec v2 requires exactly one launch path".into());
            }
            platform::run_exec(Path::new(&args[1]))?;
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}
fn run_guardian(path: &Path) -> Result<(), String> {
    let launch: Launch = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
    let mut d = launch.descriptor;
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(d.lifecycle_dir.join("lease"))
        .map_err(err)?;
    lease.lock_exclusive().map_err(err)?;
    if d.lifecycle_dir.join("retired").exists()
        || d.lifecycle_dir.join("receipt.json").exists()
        || !path.is_file()
    {
        return Err("build startup was retired before containment; no worker was spawned".into());
    }
    let owner_gone = Arc::new(AtomicBool::new(false));
    let monitor = owner_gone.clone();
    thread::spawn(move || {
        let mut byte = [0];
        loop {
            match std::io::stdin().read(&mut byte) {
                Ok(0) | Err(_) => {
                    monitor.store(true, Ordering::Release);
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").map_err(err)?;
    listener.set_nonblocking(true).map_err(err)?;
    d.endpoint = listener.local_addr().map_err(err)?.to_string();
    d.guardian_pid = std::process::id();
    let mut child = platform::Contained::spawn(&launch.spec, &mut d)?;
    d.worker_pid = child.id();
    let mut started = false;
    let mut high_water = 0;
    let mut budget_reason = None;
    let mut supervision_error = before_ready_barrier(&d, &owner_gone).err();
    let mut cancelled = owner_gone.load(Ordering::Acquire);
    if !cancelled && supervision_error.is_none() {
        if let Err(e) = write_json(&d.lifecycle_dir.join("ready.json"), &d) {
            supervision_error = Some(e);
        }
    }
    loop {
        if supervision_error.is_some() || cancelled {
            break;
        }
        if owner_gone.load(Ordering::Acquire) {
            cancelled = true;
            break;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                match read_control_frame(&mut stream)
                    .and_then(|b| serde_json::from_slice::<Control>(&b).map_err(err))
                {
                    Ok(c) if c.nonce == d.nonce => {
                        if c.command == "START" {
                            if !started {
                                match child.start() {
                                    Ok(()) => started = true,
                                    Err(e) => supervision_error = Some(e),
                                }
                            }
                            let _ = stream.write_all(b"OK\n");
                        } else if c.command.starts_with("CANCEL ") {
                            cancelled = true;
                            let _ = stream.write_all(b"OK\n");
                        } else if c.command == "PING" {
                            let _ = stream.write_all(b"OK\n");
                        } else {
                            let _ = stream.write_all(b"ERR\n");
                        }
                    }
                    _ => {
                        let _ = stream.write_all(b"ERR\n");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => supervision_error = Some(err(e)),
        }
        if cancelled || supervision_error.is_some() {
            break;
        }
        match child.exited() {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => {
                supervision_error = Some(e);
                break;
            }
        }
        if started {
            if let Ok(rss) = crate::db_writer::sample_process_rss(child.id()) {
                let parent_rss = launch
                    .spec
                    .parent_rss_pid
                    .and_then(|pid| crate::db_writer::sample_process_rss(pid).ok())
                    .unwrap_or(0);
                let observed = rss.saturating_add(parent_rss);
                high_water = high_water.max(observed);
                if launch
                    .spec
                    .memory_limit_bytes
                    .is_some_and(|limit| observed > limit.saturating_sub(HEADROOM))
                {
                    let mut failure = crate::error::MemoryBudgetExceeded::new(
                        launch
                            .spec
                            .memory_phase
                            .as_deref()
                            .unwrap_or("materialization_worker"),
                        launch.spec.memory_limit_bytes.unwrap(),
                        observed,
                        observed,
                    );
                    if launch.spec.parent_rss_pid.is_some() {
                        failure = failure.with_process_rss(parent_rss, rss);
                    }
                    budget_reason =
                        Some(crate::error::NativeError::MemoryBudgetExceeded(failure).to_string());
                    break;
                }
            }
        }
        thread::sleep(TICK);
    }
    let result = child.teardown(Instant::now() + DEADLINE);
    let mut outcome = ExitOutcome {
        success: false,
        code: None,
        signal: None,
        high_water_bytes: high_water,
        budget_reason,
        cancelled,
        containment_empty: result.is_ok(),
        supervision_error,
    };
    match result {
        Ok(status) => {
            outcome.code = status.code();
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                outcome.signal = status.signal();
            }
            outcome.success = status.success()
                && !cancelled
                && outcome.budget_reason.is_none()
                && outcome.supervision_error.is_none();
        }
        Err(e) => {
            outcome.supervision_error = Some(format!("containment_unknown: {e}"));
            eprintln!("containment_unknown: {e}");
        }
    }
    write_json(&d.lifecycle_dir.join("receipt.json"), &outcome)?;
    drop(lease);
    Ok(())
}
fn control(d: &BuildDescriptorV2, command: &str) -> Result<(), String> {
    let addr: std::net::SocketAddr = d.endpoint.parse().map_err(err)?;
    if !addr.ip().is_loopback() {
        return Err("guardian endpoint must be loopback".into());
    }
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).map_err(err)?;
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(err)?;
    stream
        .set_write_timeout(Some(Duration::from_millis(250)))
        .map_err(err)?;
    let mut payload = serde_json::to_vec(&Control {
        nonce: d.nonce.clone(),
        command: command.into(),
    })
    .map_err(err)?;
    if payload.len() >= MAX_FRAME {
        return Err("guardian control request too large".into());
    }
    payload.push(b'\n');
    stream.write_all(&payload).map_err(err)?;
    if read_socket_frame(&mut stream, Instant::now() + Duration::from_millis(250))? == b"OK" {
        Ok(())
    } else {
        Err("guardian rejected control capability or command".into())
    }
}
fn read_control_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    read_control_frame_with_timeout(stream, Duration::from_millis(100))
}
fn read_control_frame_with_timeout(
    stream: &mut TcpStream,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    stream.set_nonblocking(false).map_err(err)?;
    read_socket_frame(stream, Instant::now() + timeout)
}
fn read_socket_frame(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut frame = Vec::new();
    let mut byte = [0u8];
    while frame.len() < MAX_FRAME {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("guardian frame deadline expired")?;
        if remaining.is_zero() {
            return Err("guardian frame deadline expired".into());
        }
        stream.set_read_timeout(Some(remaining)).map_err(err)?;
        match stream.read(&mut byte) {
            Ok(0) => return Err("guardian control stream closed before frame".into()),
            Ok(_) => {
                if byte[0] == b'\n' {
                    return Ok(frame);
                }
                frame.push(byte[0]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err("guardian frame exceeds limit".into())
}
fn before_ready_barrier(d: &BuildDescriptorV2, owner_gone: &AtomicBool) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        if let Some(root) = std::env::var_os("CODEBASE_GRAPH_TEST_GUARDIAN_READY_BARRIER_DIR") {
            let root = PathBuf::from(root);
            fs::create_dir_all(&root).map_err(err)?;
            write_private_json(&root.join("before-ready.json"), d)?;
            let deadline = Instant::now() + Duration::from_secs(30);
            while !root.join("CONTINUE").exists() {
                if owner_gone.load(Ordering::Acquire) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err("debug guardian READY barrier timed out".into());
                }
                thread::sleep(TICK)
            }
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (d, owner_gone);
    Ok(())
}
#[cfg(test)]
fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut frame = Vec::new();
    let mut b = [0];
    while frame.len() < MAX_FRAME {
        reader.read_exact(&mut b).map_err(err)?;
        if b[0] == b'\n' {
            return Ok(frame);
        }
        frame.push(b[0]);
    }
    Err("guardian frame exceeds limit".into())
}
fn random_id() -> Result<String, String> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(err)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
fn err(e: impl fmt::Display) -> String {
    e.to_string()
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    write_private_json(path, value)
}
/// Capability-bearing JSON must be private from the instant its file exists.
pub(crate) fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", random_id()?));
    let mut file = platform::create_private_file(&temp)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(value).map_err(err)?)
            .map_err(err)?;
        file.sync_all().map_err(err)?;
        drop(file);
        fs::rename(&temp, path).map_err(err)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_nonblocking_socket_is_normalized_for_control_frames() {
        #[cfg(unix)]
        fn is_nonblocking(socket: &TcpStream) -> bool {
            use std::os::fd::AsRawFd;
            let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
            assert_ne!(flags, -1);
            flags & libc::O_NONBLOCK != 0
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(b"ready\n").unwrap();
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nonblocking(true).unwrap();
        #[cfg(unix)]
        assert!(is_nonblocking(&socket));
        // Inspect the actual mode conversion instead of racing a delayed writer
        // against the production deadline. Negative tests retain that deadline.
        assert_eq!(
            read_control_frame_with_timeout(&mut socket, Duration::from_secs(2)).unwrap(),
            b"ready"
        );
        #[cfg(unix)]
        assert!(!is_nonblocking(&socket));
    }
    #[test]
    fn slow_trickle_cannot_extend_control_frame_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            for _ in 0..40 {
                thread::sleep(Duration::from_millis(15));
                if socket.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let (mut socket, _) = listener.accept().unwrap();
        let began = Instant::now();
        assert!(read_control_frame(&mut socket).is_err());
        assert!(began.elapsed() < Duration::from_millis(250));
        drop(socket);
        client.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn capability_json_is_private_at_creation_and_after_replacement() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("graph-private-json-{}", random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let path = root.join("state.json");
        write_private_json(&path, &serde_json::json!({"nonce":"secret"})).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        write_private_json(&path, &serde_json::json!({"nonce":"replacement"})).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_protocol_rejects_unterminated_control_frame() {
        let mut reader = std::io::Cursor::new(vec![b'x'; MAX_FRAME + 1]);
        assert!(read_frame(&mut reader)
            .unwrap_err()
            .contains("exceeds limit"));
    }
    #[test]
    fn retired_startup_rejects_late_guardian_before_child_creation() {
        let dir = std::env::temp_dir().join(format!("graph-retired-{}", random_id().unwrap()));
        let lifecycle = dir.join(".lifecycle-test");
        fs::create_dir_all(&lifecycle).unwrap();
        OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .unwrap();
        let descriptor = BuildDescriptorV2 {
            version: 2,
            build_id: "late".into(),
            lifecycle_dir: lifecycle.clone(),
            endpoint: String::new(),
            nonce: random_id().unwrap(),
            worker_pid: 0,
            guardian_pid: 0,
            proof: ContainmentProof::Pending,
        };
        let launch = Launch {
            descriptor,
            spec: SpawnSpec {
                program: std::env::current_exe().unwrap(),
                guardian_executable: None,
                args: Vec::new(),
                workspace: dir.clone(),
                stderr_path: dir.join("stderr"),
                stdout: OutputMode::Null,
                memory_limit_bytes: None,
                memory_phase: None,
                parent_rss_pid: None,
            },
        };
        let path = lifecycle.join("launch.json");
        write_json(&path, &launch).unwrap();
        assert!(cleanup_if_inactive(&dir).unwrap());
        assert!(run_guardian(&path).unwrap_err().contains("retired"));
        assert!(!lifecycle.join("proof.json").exists());
        assert!(!lifecycle.join("ready.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn workspace_without_global_state_remains_protected_by_guardian_lease() {
        let dir = std::env::temp_dir().join(format!("graph-lifecycle-{}", random_id().unwrap()));
        let lifecycle = dir.join(".lifecycle-test");
        fs::create_dir_all(&lifecycle).unwrap();
        let lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .unwrap();
        lease.lock_exclusive().unwrap();
        assert!(!cleanup_if_inactive(&dir).unwrap());
        drop(lease);
        // No proof was ever persisted, so retirement under the lease prevents
        // any late guardian from spawning after cleanup.
        assert!(cleanup_if_inactive(&dir).unwrap());
        assert!(lifecycle.join("retired").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
