use super::*;
use std::{
    ffi::CString,
    io,
    os::{
        fd::{AsRawFd, BorrowedFd, FromRawFd},
        unix::{
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
            process::CommandExt,
        },
    },
    sync::Once,
};
const WATCH_ENV: &str = "CODEBASE_GRAPH_GUARDIAN_WATCH_FD_V2";
const GROUP_ENV: &str = "CODEBASE_GRAPH_GROUP_V2";
const FD_ENV: &str = "CODEBASE_GRAPH_CONTAINMENT_FD_V2";
const PROOF_ENV: &str = "CODEBASE_GRAPH_CONTAINMENT_PROOF_V2";
static WATCHDOG: Once = Once::new();
pub(super) struct Contained {
    child: Child,
    gate: Option<ChildStdin>,
    fifo: File,
    writer: Option<File>,
    proof: BuildDescriptorV2,
    _lifetime_writer: File,
    teardown_attempted: bool,
}
impl Contained {
    pub fn spawn(spec: &SpawnSpec, d: &mut BuildDescriptorV2) -> Result<Self, String> {
        let path = d.lifecycle_dir.join("writers.fifo");
        let cpath = CString::new(path.as_os_str().as_encoded_bytes()).map_err(err)?;
        if unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) } != 0 {
            return Err(err(io::Error::last_os_error()));
        }
        let fifo = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(&path)
            .map_err(err)?;
        let metadata = fifo.metadata().map_err(err)?;
        d.proof = ContainmentProof::UnixFifo {
            path: path.clone(),
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        let writer = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(&path)
            .map_err(err)?;
        let mut pipe = [-1; 2];
        if unsafe { libc::pipe(pipe.as_mut_ptr()) } != 0 {
            return Err(err(io::Error::last_os_error()));
        }
        let lifetime_reader = unsafe { File::from_raw_fd(pipe[0]) };
        let lifetime_writer = unsafe { File::from_raw_fd(pipe[1]) };
        for fd in pipe {
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(err(io::Error::last_os_error()));
            }
        }
        let watch_fd = lifetime_reader.as_raw_fd();
        let spec_path = d.lifecycle_dir.join("exec.json");
        write_json(
            &spec_path,
            &Launch {
                spec: spec.clone(),
                descriptor: d.clone(),
            },
        )?;
        let mut command = Command::new(std::env::current_exe().map_err(err)?);
        command
            .args(["__codebase_graph_internal", "contained-exec-v2"])
            .arg(&spec_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        command
            .env(FD_ENV, writer.as_raw_fd().to_string())
            .env(PROOF_ENV, &spec_path)
            .env(WATCH_ENV, watch_fd.to_string());
        let fd = writer.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0
                    || libc::fcntl(watch_fd, libc::F_SETFD, 0) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        write_json(&d.lifecycle_dir.join("proof.json"), d)?;
        let mut child = command.spawn().map_err(err)?;
        let gate = child.stdin.take();
        let mut contained = Self {
            child,
            gate,
            fifo,
            writer: Some(writer),
            proof: d.clone(),
            _lifetime_writer: lifetime_writer,
            teardown_attempted: false,
        };
        drop(lifetime_reader);
        let deadline = Instant::now() + DEADLINE;
        while !d.lifecycle_dir.join("inherited.ack").exists() {
            if contained.exited()? || Instant::now() >= deadline {
                let _ = contained.teardown(Instant::now() + DEADLINE);
                return Err(
                    "contained worker failed inheritance acknowledgement before START".into(),
                );
            }
            thread::sleep(TICK);
        }
        contained.writer.take();
        Ok(contained)
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn start(&mut self) -> Result<(), String> {
        if let Some(mut gate) = self.gate.take() {
            gate.write_all(b"S").map_err(err)?;
        }
        Ok(())
    }
    pub fn exited(&mut self) -> Result<bool, String> {
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id() as libc::id_t,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if rc != 0 {
            return Err(format!(
                "waitid for owned worker {}: {}",
                self.child.id(),
                io::Error::last_os_error()
            ));
        }
        let info = unsafe { info.assume_init() };
        #[cfg(target_os = "linux")]
        let pid = unsafe { info.si_pid() };
        #[cfg(not(target_os = "linux"))]
        let pid = info.si_pid;
        Ok(pid != 0)
    }
    pub fn teardown(&mut self, deadline: Instant) -> Result<std::process::ExitStatus, String> {
        self.teardown_attempted = true;
        self.gate.take();
        self.writer.take();
        // This direct child has deliberately not been reaped: its process group ID
        // cannot be recycled until the writer proof says all descendants are gone.
        loop {
            // A normally exited group can contain only its unreaped leader.
            // Darwin may reject signalling that group, although EOF already
            // proves that no descendant retains a writer.
            if fifo_empty(&mut self.fifo)? && self.exited()? {
                return self.child.wait().map_err(err);
            }
            if unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) } < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() != Some(libc::ESRCH) {
                    if fifo_empty(&mut self.fifo)? && self.exited()? {
                        return self.child.wait().map_err(err);
                    }
                    return Err(format!("killpg for owned worker {}: {e}", self.child.id()));
                }
            }
            if fifo_empty(&mut self.fifo)? && self.exited()? {
                return self.child.wait().map_err(err);
            }
            if Instant::now() >= deadline {
                return Err(format!("writers remain for {}", self.proof.build_id));
            }
            thread::sleep(TICK);
        }
    }
}
impl Drop for Contained {
    fn drop(&mut self) {
        if !self.teardown_attempted {
            let _ = self.teardown(Instant::now() + DEADLINE);
        }
    }
}
fn fifo_empty(fifo: &mut File) -> Result<bool, String> {
    let mut byte = [0];
    match fifo.read(&mut byte) {
        Ok(0) => Ok(true),
        Ok(_) => Err("containment FIFO unexpectedly contains bytes".into()),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
        Err(e) => Err(err(e)),
    }
}
pub(super) fn recover_empty(d: &BuildDescriptorV2) -> Result<bool, String> {
    crate::storage::containment::probe_descriptor(d)
}
fn inherited_proof() -> Result<Option<(i32, Launch)>, String> {
    let Some(raw) = std::env::var_os(FD_ENV) else {
        return Ok(None);
    };
    let fd: i32 = raw
        .to_str()
        .ok_or("invalid containment fd")?
        .parse()
        .map_err(err)?;
    let path = std::env::var_os(PROOF_ENV).ok_or("missing containment proof")?;
    let launch: Launch = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
    let ContainmentProof::UnixFifo { device, inode, .. } = launch.descriptor.proof else {
        return Err("invalid inherited FIFO proof".into());
    };
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || flags & libc::O_ACCMODE != libc::O_WRONLY {
        return Err("inherited containment fd is not a writer".into());
    }
    let metadata = File::from(
        unsafe { BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()
            .map_err(err)?,
    )
    .metadata()
    .map_err(err)?;
    if !metadata.file_type().is_fifo() || metadata.dev() != device || metadata.ino() != inode {
        return Err("inherited containment writer identity mismatch".into());
    }
    Ok(Some((fd, launch)))
}
pub(super) fn validate_inherited() -> Result<bool, String> {
    let Some((_, _launch)) = inherited_proof()? else {
        return Ok(false);
    };
    let watch_fd: i32 = std::env::var(WATCH_ENV)
        .map_err(err)?
        .parse()
        .map_err(err)?;
    let expected_group: i32 = std::env::var(GROUP_ENV)
        .map_err(err)?
        .parse()
        .map_err(err)?;
    if unsafe { libc::getpgrp() } != expected_group {
        return Err("inherited process group mismatch".into());
    }
    let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
    if unsafe { libc::fstat(watch_fd, stat.as_mut_ptr()) } != 0 {
        return Err("guardian lifetime pipe is closed".into());
    }
    WATCHDOG.call_once(move || {
        thread::spawn(move || {
            let mut byte = [0u8];
            loop {
                let result = unsafe { libc::read(watch_fd, byte.as_mut_ptr().cast(), 1) };
                if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if result <= 0 {
                    if unsafe { libc::getpgrp() } == expected_group {
                        unsafe { libc::kill(-expected_group, libc::SIGKILL) };
                    }
                    break;
                }
            }
        });
    });
    Ok(true)
}
pub(super) fn configure_inherited_command(command: &mut Command) -> Result<(), String> {
    if let Some((fd, _)) = inherited_proof()? {
        let watch_fd: i32 = std::env::var(WATCH_ENV)
            .map_err(err)?
            .parse()
            .map_err(err)?;
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0
                    || libc::fcntl(watch_fd, libc::F_SETFD, 0) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    Ok(())
}
pub(super) fn run_exec(path: &Path) -> Result<(), String> {
    let launch: Launch = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
    if inherited_proof()?.is_none() {
        return Err("contained exec has no inherited writer".into());
    }
    if unsafe { libc::getpgrp() } != unsafe { libc::getpid() } {
        return Err("contained exec is not its anchored group leader".into());
    }
    write_json(
        &launch.descriptor.lifecycle_dir.join("inherited.ack"),
        &true,
    )?;
    let mut gate = [0];
    std::io::stdin().read_exact(&mut gate).map_err(err)?;
    if gate != *b"S" {
        return Err("invalid contained START gate".into());
    }
    std::env::set_var(GROUP_ENV, unsafe { libc::getpgrp() }.to_string());
    let mut command = Command::new(&launch.spec.program);
    command.args(&launch.spec.args).stdin(Stdio::null());
    configure_inherited_command(&mut command)?;
    Err(err(command.exec()))
}

pub(super) fn create_private_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path).map_err(err)
}
pub(super) fn create_private_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_proof_never_establishes_emptiness() {
        let d = BuildDescriptorV2 {
            version: 2,
            build_id: "missing".into(),
            lifecycle_dir: PathBuf::new(),
            endpoint: String::new(),
            nonce: String::new(),
            worker_pid: 1,
            guardian_pid: 1,
            proof: ContainmentProof::UnixFifo {
                path: PathBuf::from("/definitely-missing-codebase-graph-fifo"),
                device: 0,
                inode: 0,
            },
        };
        assert!(!recover_empty(&d).unwrap());
    }
    #[test]
    fn recovery_rejects_regular_files_symlinks_and_replaced_fifo_identity() {
        let dir = std::env::temp_dir().join(format!("graph-fifo-proof-{}", random_id().unwrap()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("proof");
        fs::write(&path, b"").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let mut d = BuildDescriptorV2 {
            version: 2,
            build_id: "test".into(),
            lifecycle_dir: dir.clone(),
            endpoint: String::new(),
            nonce: String::new(),
            worker_pid: 1,
            guardian_pid: 1,
            proof: ContainmentProof::UnixFifo {
                path: path.clone(),
                device: metadata.dev(),
                inode: metadata.ino(),
            },
        };
        assert!(!recover_empty(&d).unwrap());
        fs::remove_file(&path).unwrap();
        let fifo = dir.join("fifo");
        let c = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        std::os::unix::fs::symlink(&fifo, &path).unwrap();
        let metadata = fs::metadata(&fifo).unwrap();
        d.proof = ContainmentProof::UnixFifo {
            path: path.clone(),
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        assert!(!recover_empty(&d).unwrap());
        d.proof = ContainmentProof::UnixFifo {
            path: fifo.clone(),
            device: metadata.dev(),
            inode: metadata.ino().wrapping_add(1),
        };
        assert!(!recover_empty(&d).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn proof_without_ready_retains_live_writers_then_allows_empty_recovery() {
        let dir =
            std::env::temp_dir().join(format!("graph-startup-proof-{}", random_id().unwrap()));
        let lifecycle = dir.join(".lifecycle-test");
        fs::create_dir_all(&lifecycle).unwrap();
        let path = lifecycle.join("writers.fifo");
        let c = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let reader = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let writer = OpenOptions::new().write(true).open(&path).unwrap();
        let metadata = reader.metadata().unwrap();
        let d = BuildDescriptorV2 {
            version: 2,
            build_id: "startup".into(),
            lifecycle_dir: lifecycle.clone(),
            endpoint: String::new(),
            nonce: String::new(),
            worker_pid: 1,
            guardian_pid: 1,
            proof: ContainmentProof::UnixFifo {
                path: path.clone(),
                device: metadata.dev(),
                inode: metadata.ino(),
            },
        };
        OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .unwrap();
        write_json(&lifecycle.join("proof.json"), &d).unwrap();
        assert!(!cleanup_if_inactive(&dir).unwrap());
        drop(writer);
        assert!(cleanup_if_inactive(&dir).unwrap());
        drop(reader);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fifo_writer_controls_empty_proof() {
        let dir = std::env::temp_dir().join(format!("graph-fifo-{}", random_id().unwrap()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("proof");
        let c = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let mut reader = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let writer = OpenOptions::new().write(true).open(&path).unwrap();
        assert!(!fifo_empty(&mut reader).unwrap());
        drop(writer);
        assert!(fifo_empty(&mut reader).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
