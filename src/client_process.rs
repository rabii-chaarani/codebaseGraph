use std::env;
#[cfg(windows)]
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A client executable selected from a pinned path rather than a name that
/// `Command` would have to search again at spawn time.
#[derive(Debug)]
pub(crate) struct ClientExecutable {
    path: PathBuf,
    #[cfg(windows)]
    kind: ExecutableKind,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutableKind {
    Direct,
    Batch,
}

impl ClientExecutable {
    /// Find a client executable using the current process PATH.
    ///
    /// Windows extensionless names are searched in PATH order, checking
    /// `.exe`, `.cmd`, then `.bat` in each directory. Other explicit Windows
    /// extensions are rejected instead of consulting PATHEXT.
    pub(crate) fn discover(program: &str) -> Result<Option<Self>, String> {
        let path_entries = env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default();
        let current_dir = env::current_dir()
            .map_err(|error| format!("could not resolve the current directory: {error}"))?;
        Self::discover_in_path(program, &path_entries, &current_dir)
    }

    /// Build a structured command using the already selected path.
    ///
    /// Arguments remain separate throughout native and batch execution. The
    /// standard library's Windows batch handling is deliberately used for
    /// `.cmd` and `.bat` files; this module does not construct shell command
    /// strings or use raw arguments.
    pub(crate) fn command(&self, args: &[String]) -> Result<Command, String> {
        for arg in args {
            if arg.contains('\0') {
                return Err("client command arguments cannot contain NUL".to_string());
            }
        }

        #[cfg(windows)]
        if self.kind == ExecutableKind::Batch {
            validate_batch_script_path(&self.path)?;
            if args
                .iter()
                .any(|arg| arg.contains('\r') || arg.contains('\n'))
            {
                return Err(
                    "Windows batch client arguments cannot contain carriage returns or line feeds"
                        .to_string(),
                );
            }
        }

        let mut command = Command::new(&self.path);
        command.args(args);
        Ok(command)
    }

    fn discover_in_path(
        program: &str,
        path_entries: &[PathBuf],
        current_dir: &Path,
    ) -> Result<Option<Self>, String> {
        if program.contains('\0') {
            return Err("client executable name cannot contain NUL".to_string());
        }
        if program.is_empty() {
            return Err("client executable name cannot be empty".to_string());
        }

        #[cfg(windows)]
        {
            discover_windows(program, path_entries, current_dir)
        }
        #[cfg(not(windows))]
        {
            discover_non_windows(program, path_entries, current_dir)
        }
    }
}

fn absolute_path(path: &Path, current_dir: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if !current_dir.is_absolute() {
        return Err("current directory is not absolute; cannot pin client executable".to_string());
    }
    let absolute = current_dir.join(path);
    if !absolute.is_absolute() {
        return Err(format!(
            "client executable path cannot be pinned to an absolute path: {}",
            path.display()
        ));
    }
    Ok(absolute)
}

#[cfg(not(windows))]
fn discover_non_windows(
    program: &str,
    path_entries: &[PathBuf],
    current_dir: &Path,
) -> Result<Option<ClientExecutable>, String> {
    let requested = Path::new(program);
    if requested.is_absolute() || requested.components().count() > 1 {
        let path = absolute_path(requested, current_dir)?;
        if !path.is_file() {
            return Ok(None);
        }
        return Ok(Some(ClientExecutable { path }));
    }

    for directory in path_entries {
        let candidate = directory.join(requested);
        if candidate.is_file() {
            return Ok(Some(ClientExecutable {
                path: absolute_path(&candidate, current_dir)?,
            }));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn discover_windows(
    program: &str,
    path_entries: &[PathBuf],
    current_dir: &Path,
) -> Result<Option<ClientExecutable>, String> {
    if program.trim() != program || program.chars().any(char::is_control) {
        return Err(
            "client executable name contains whitespace padding or control characters".to_string(),
        );
    }
    if program.contains('"') {
        return Err("client executable paths must not include command-line quotes".to_string());
    }

    let requested = Path::new(program);
    let explicit_path =
        program.contains('/') || program.contains('\\') || has_windows_path_prefix(requested);

    if !explicit_path && program.chars().any(is_ambiguous_bare_program_char) {
        return Err(format!(
            "ambiguous client executable command `{program}`; pass a program name or an explicit path"
        ));
    }

    if has_windows_path_prefix(requested) && !requested.is_absolute() {
        return Err(format!(
            "drive-relative client executable path `{program}` is ambiguous"
        ));
    }
    if explicit_path
        && !requested.is_absolute()
        && requested
            .components()
            .any(|component| matches!(component, Component::RootDir))
    {
        return Err(format!(
            "root-relative client executable path `{program}` is ambiguous"
        ));
    }

    let (names, kinds): (Vec<PathBuf>, Vec<ExecutableKind>) = match windows_kind(requested) {
        Some(kind) => (vec![requested.to_path_buf()], vec![kind]),
        None if requested.extension().is_none() && !explicit_path => (
            ["exe", "cmd", "bat"]
                .into_iter()
                .map(|extension| PathBuf::from(format!("{program}.{extension}")))
                .collect(),
            vec![
                ExecutableKind::Direct,
                ExecutableKind::Batch,
                ExecutableKind::Batch,
            ],
        ),
        None if explicit_path => {
            return Err(format!(
                "explicit Windows client executable path `{program}` must end in .exe, .cmd, or .bat"
            ));
        }
        None => {
            return Err(format!(
                "unsupported Windows client executable suffix in `{program}`; supported suffixes are .exe, .cmd, and .bat"
            ));
        }
    };

    if explicit_path {
        let kind = kinds[0];
        let path = absolute_path(requested, current_dir)?;
        if path.is_file() {
            if kind == ExecutableKind::Batch {
                validate_batch_script_path(&path)?;
            }
            return Ok(Some(ClientExecutable { path, kind }));
        }
        return Ok(None);
    }

    // PATH ordering takes precedence over suffix preference: for each PATH
    // directory, try .exe, .cmd, then .bat before moving to the next entry.
    for directory in path_entries {
        for (name, kind) in names.iter().zip(&kinds) {
            let candidate = absolute_path(&directory.join(name), current_dir)?;
            if candidate.is_file() {
                if *kind == ExecutableKind::Batch {
                    validate_batch_script_path(&candidate)?;
                }
                return Ok(Some(ClientExecutable {
                    path: candidate,
                    kind: *kind,
                }));
            }
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn has_windows_path_prefix(path: &Path) -> bool {
    matches!(path.components().next(), Some(Component::Prefix(_)))
}

#[cfg(windows)]
fn is_ambiguous_bare_program_char(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | '&' | '|' | '<' | '>' | '^' | '%' | '!' | ';' | '(' | ')'
        )
}

#[cfg(windows)]
fn windows_kind(path: &Path) -> Option<ExecutableKind> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("exe") {
        Some(ExecutableKind::Direct)
    } else if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
        Some(ExecutableKind::Batch)
    } else {
        None
    }
}

#[cfg(windows)]
fn validate_batch_script_path(path: &Path) -> Result<(), String> {
    let path_text = path.as_os_str().to_string_lossy();
    if path_text.chars().any(char::is_control) {
        return Err(
            "Windows batch client script paths cannot contain control characters".to_string(),
        );
    }
    if path_text.contains('%') {
        return Err(
            "Windows batch client script paths cannot contain `%` expansion markers".to_string(),
        );
    }
    if path_text.contains('"') {
        return Err("Windows batch client script paths cannot contain quotes".to_string());
    }
    let normalized = path_text.replace('/', "\\");
    if normalized.starts_with("\\\\?\\") || normalized.starts_with("\\\\.\\") {
        return Err(
            "Windows batch client script paths cannot use verbatim or device prefixes".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let root = env::temp_dir();
            for _ in 0..100 {
                let number = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
                let candidate = root.join(format!("c55_{}_{}", std::process::id(), number));
                match fs::create_dir(&candidate) {
                    Ok(()) => return Self(candidate),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("could not create test directory: {error}"),
                }
            }
            panic!("could not allocate a unique test directory");
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::File::create(path).expect("create fixture file");
    }

    #[cfg(not(windows))]
    #[test]
    fn cod55_non_windows_discovery_keeps_exact_filename_semantics() {
        let temp = TestDirectory::new();
        let exact = temp.path().join("client");
        let suffixed = temp.path().join("client.cmd");
        touch(&suffixed);
        let current_dir = env::current_dir().expect("current directory");
        let path_entries = vec![temp.path().to_path_buf()];

        assert!(
            ClientExecutable::discover_in_path("client", &path_entries, &current_dir)
                .expect("exact discovery")
                .is_none()
        );
        touch(&exact);
        let selected = ClientExecutable::discover_in_path("client", &path_entries, &current_dir)
            .expect("exact discovery")
            .expect("exact filename selected");
        assert_eq!(selected.path, exact);
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_path_precedence_and_fixed_suffix_order() {
        let temp = TestDirectory::new();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        touch(&first.join("cod55-client.bat"));
        touch(&first.join("cod55-client.cmd"));
        touch(&second.join("cod55-client.exe"));
        let current_dir = env::current_dir().expect("current directory");
        let selected = ClientExecutable::discover_in_path(
            "cod55-client",
            &[first.clone(), second.clone()],
            &current_dir,
        )
        .expect("discovery")
        .expect("client found");
        assert!(selected.path.is_absolute());
        assert_eq!(selected.path, first.join("cod55-client.cmd"));
        assert_eq!(selected.kind, ExecutableKind::Batch);

        touch(&first.join("cod55-client.exe"));
        let selected = ClientExecutable::discover_in_path(
            "cod55-client",
            &[first.clone(), second],
            &current_dir,
        )
        .expect("discovery")
        .expect("client found");
        assert_eq!(selected.path, first.join("cod55-client.exe"));
        assert_eq!(selected.kind, ExecutableKind::Direct);
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_rejects_unsupported_suffixes_and_ambiguous_commands() {
        let temp = TestDirectory::new();
        let current_dir = env::current_dir().expect("current directory");
        let path_entries = vec![temp.path().to_path_buf()];
        assert!(absolute_path(Path::new(r"C:relative\client.exe"), &current_dir).is_err());
        touch(&temp.path().join("cod55-client.ps1"));

        assert!(ClientExecutable::discover_in_path(
            "cod55-client.ps1",
            &path_entries,
            &current_dir
        )
        .is_err());
        assert!(ClientExecutable::discover_in_path(
            "cod55-client && whoami",
            &path_entries,
            &current_dir
        )
        .is_err());
        assert!(
            ClientExecutable::discover_in_path("cod55-client", &[], &current_dir)
                .expect("fixed suffix lookup")
                .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_explicit_paths_are_supported_and_pinned() {
        let temp = TestDirectory::new();
        let current_dir = env::current_dir().expect("current directory");
        let executable = temp.path().join("client with spaces.exe");
        let batch = temp.path().join("client with spaces.CmD");
        touch(&executable);
        touch(&batch);

        let selected = ClientExecutable::discover_in_path(
            executable.to_str().expect("UTF-8 fixture path"),
            &[],
            &current_dir,
        )
        .expect("explicit executable path")
        .expect("executable found");
        assert!(selected.path.is_absolute());
        assert_eq!(selected.path, executable);
        assert_eq!(selected.kind, ExecutableKind::Direct);

        let selected = ClientExecutable::discover_in_path(
            batch.to_str().expect("UTF-8 fixture path"),
            &[],
            &current_dir,
        )
        .expect("explicit batch path")
        .expect("batch found");
        assert_eq!(selected.path, batch);
        assert_eq!(selected.kind, ExecutableKind::Batch);
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_rejects_unsafe_batch_paths_and_arguments_before_spawn() {
        let temp = TestDirectory::new();
        let current_dir = env::current_dir().expect("current directory");
        let unsafe_batch = temp.path().join("client%PATH%.cmd");
        touch(&unsafe_batch);
        let selected = ClientExecutable::discover_in_path(
            unsafe_batch.to_str().expect("UTF-8 fixture path"),
            &[],
            &current_dir,
        )
        .expect_err("unsafe script path is rejected during discovery");
        assert!(selected.contains("%"));

        let safe_batch = temp.path().join("client.cmd");
        touch(&safe_batch);
        let selected = ClientExecutable::discover_in_path(
            safe_batch.to_str().expect("UTF-8 fixture path"),
            &[],
            &current_dir,
        )
        .expect("safe batch path")
        .expect("batch found");
        assert!(selected.command(&["bad\nargument".to_string()]).is_err());
        assert!(selected.command(&["bad\rargument".to_string()]).is_err());
        assert!(selected.command(&["bad\0argument".to_string()]).is_err());

        let unsafe_verbatim = Path::new(r"\\?\C:\cod55\client.cmd");
        assert!(validate_batch_script_path(unsafe_verbatim).is_err());
    }

    #[cfg(windows)]
    fn compile_argv_capture_fixture(temp: &TestDirectory) -> PathBuf {
        let source = temp.path().join("capture.rs");
        let executable = temp.path().join("argv capture.exe");
        fs::write(
            &source,
            r#"
use std::env;
use std::fs::File;
use std::io::Write;

fn main() {
    let path = env::var_os("CODEBASEGRAPH_ARGV_CAPTURE").expect("capture path");
    let mut output = File::create(path).expect("create capture file");
    for argument in env::args().skip(1) {
        for byte in argument.as_bytes() {
            write!(output, "{byte:02x}").expect("write argument");
        }
        writeln!(output).expect("finish argument");
    }
}
"#,
        )
        .expect("write standalone fixture source");

        // PATH stays inherited and unmodified while rustc runs.
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("rustc must be available on the Windows test runner");
        assert!(
            output.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        executable
    }

    #[cfg(windows)]
    fn write_batch_shim(path: &Path, executable: &Path) {
        let script = format!(
            "@echo off\r\n\"{}\" %*\r\nexit /b %errorlevel%\r\n",
            executable.display()
        );
        fs::write(path, script).expect("write forwarding batch shim");
    }

    #[cfg(windows)]
    fn decode_hex_arguments(path: &Path) -> Vec<String> {
        let contents = fs::read_to_string(path).expect("captured argv");
        contents
            .lines()
            .map(|line| {
                assert_eq!(line.len() % 2, 0);
                let bytes = line
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        let byte_text = std::str::from_utf8(pair).expect("hex is ASCII");
                        u8::from_str_radix(byte_text, 16).expect("valid hex byte")
                    })
                    .collect::<Vec<_>>();
                String::from_utf8(bytes).expect("fixture arguments are UTF-8")
            })
            .collect()
    }

    #[cfg(windows)]
    fn run_capture(executable: &ClientExecutable, args: &[String], capture: &Path) -> Vec<String> {
        let mut command = executable
            .command(args)
            .expect("construct client invocation");
        command.env("CODEBASEGRAPH_ARGV_CAPTURE", capture);
        let output = command.output().expect("spawn pinned client executable");
        assert!(
            output.status.success(),
            "client failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        decode_hex_arguments(capture)
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_native_exe_preserves_arguments_without_batch_restrictions() {
        let temp = TestDirectory::new();
        let fixture = compile_argv_capture_fixture(&temp);
        let current_dir = env::current_dir().expect("current directory");
        let executable = ClientExecutable::discover_in_path(
            fixture.to_str().expect("UTF-8 fixture path"),
            &[],
            &current_dir,
        )
        .expect("native executable discovery")
        .expect("compiled executable found");
        let capture = temp.path().join("native-argv.txt");
        let args = vec![
            "with spaces".to_string(),
            String::new(),
            "trailing\\".to_string(),
            "quote\"inside".to_string(),
            "line\r\nbreak".to_string(),
            "& | < > ^ % !".to_string(),
        ];
        assert_eq!(run_capture(&executable, &args, &capture), args);
    }

    #[cfg(windows)]
    #[test]
    fn cod55_windows_batch_shims_forward_exact_argv_and_block_injection() {
        let temp = TestDirectory::new();
        let fixture = compile_argv_capture_fixture(&temp);
        let current_dir = env::current_dir().expect("current directory");
        let sentinel = temp.path().join("injection-sentinel.txt");
        let injection = format!("& echo CODE55_INJECTED > {}", sentinel.display());
        let args = vec![
            "with spaces".to_string(),
            String::new(),
            "trailing\\".to_string(),
            "quote\"inside".to_string(),
            "ampersand & pipe | angle < > caret ^ percent % bang !".to_string(),
            injection,
        ];

        for extension in ["cmd", "bat"] {
            let shim_path = temp
                .path()
                .join(format!("forwarder with spaces.{extension}"));
            write_batch_shim(&shim_path, &fixture);
            let shim = ClientExecutable::discover_in_path(
                shim_path.to_str().expect("UTF-8 shim path"),
                &[],
                &current_dir,
            )
            .expect("batch discovery")
            .expect("batch script found");
            let capture = temp.path().join(format!("argv-{extension}.txt"));
            assert_eq!(run_capture(&shim, &args, &capture), args);
            assert!(
                !sentinel.exists(),
                "batch arguments executed shell injection"
            );
        }
    }
}
