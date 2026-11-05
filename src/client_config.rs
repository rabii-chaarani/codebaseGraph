use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
struct ClientConfigEnvironment {
    home: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    openclaw_home: Option<PathBuf>,
}

impl ClientConfigEnvironment {
    fn from_process() -> Self {
        #[cfg(windows)]
        let home = None;
        #[cfg(not(windows))]
        let home = env::var_os("HOME").map(PathBuf::from);

        Self {
            home,
            user_profile: env::var_os("USERPROFILE").map(PathBuf::from),
            codex_home: env::var_os("CODEX_HOME").map(PathBuf::from),
            openclaw_home: env::var_os("OPENCLAW_HOME").map(PathBuf::from),
        }
    }
}

pub(crate) fn default_client_config_path(
    adapter: &str,
    scope: &str,
    repo_root: &Path,
) -> Result<PathBuf, String> {
    let environment = ClientConfigEnvironment::from_process();

    #[cfg(windows)]
    {
        resolve_client_config_path(
            adapter,
            scope,
            repo_root,
            &environment,
            true,
            native_user_profile_dir,
        )
    }

    #[cfg(not(windows))]
    {
        resolve_client_config_path(adapter, scope, repo_root, &environment, false, || {
            Err("native Windows user profile lookup is unavailable".to_string())
        })
    }
}

fn resolve_client_config_path<F>(
    adapter: &str,
    scope: &str,
    repo_root: &Path,
    environment: &ClientConfigEnvironment,
    windows: bool,
    native_profile: F,
) -> Result<PathBuf, String>
where
    F: FnOnce() -> Result<PathBuf, String>,
{
    if let Some(path) = repository_config_path(adapter, scope, repo_root) {
        return Ok(path);
    }

    // These existing client-specific overrides intentionally keep their original spelling and
    // relative-path behavior. Resolve them before asking Windows for a user profile.
    match adapter {
        "codex" if scope == "user" => {
            if let Some(home) = &environment.codex_home {
                return Ok(home.join("config.toml"));
            }
        }
        "openclaw" => {
            if let Some(home) = &environment.openclaw_home {
                return Ok(home.join("mcp.json5"));
            }
        }
        _ => {}
    }

    let home = if windows {
        resolve_windows_profile_directory(environment.user_profile.clone(), native_profile)?
    } else {
        environment
            .home
            .clone()
            .unwrap_or_else(|| PathBuf::from("."))
    };

    Ok(match adapter {
        "codex" => home.join(".codex/config.toml"),
        "claude" => home.join(".claude.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => home.join(".openclaw/mcp.json5"),
        // Generic and unrecognized adapters retain the legacy shared-config fallback.
        _ => home.join(".config/mcp/mcp.json"),
    })
}

fn repository_config_path(adapter: &str, scope: &str, repo_root: &Path) -> Option<PathBuf> {
    match adapter {
        "codex" if scope != "user" => Some(repo_root.join(".codex/config.toml")),
        "claude" if scope == "project" => Some(repo_root.join(".mcp.json")),
        "claude-project" => Some(repo_root.join(".mcp.json")),
        "github-copilot" => Some(repo_root.join(".vscode/mcp.json")),
        "generic" if scope == "project" => Some(repo_root.join(".mcp.json")),
        _ => None,
    }
}

fn resolve_windows_profile_directory<F>(
    user_profile: Option<PathBuf>,
    native_profile: F,
) -> Result<PathBuf, String>
where
    F: FnOnce() -> Result<PathBuf, String>,
{
    if let Some(path) = user_profile
        .as_ref()
        .filter(|path| is_absolute_directory_path(path))
    {
        return Ok(path.clone());
    }

    let native_path = native_profile().map_err(|error| {
        if let Some(path) = &user_profile {
            format!(
                "USERPROFILE is not a valid absolute Windows directory ({}), and native profile lookup failed: {error}",
                path.display()
            )
        } else {
            format!("native Windows user profile lookup failed: {error}")
        }
    })?;

    if is_absolute_directory_path(&native_path) {
        Ok(native_path)
    } else {
        Err(format!(
            "native Windows user profile is not a valid absolute directory: {}",
            native_path.display()
        ))
    }
}

fn is_absolute_directory_path(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.is_absolute() && path.is_dir()
}

#[cfg(windows)]
fn native_user_profile_dir() -> Result<PathBuf, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;

    struct TokenHandle(HANDLE);

    impl Drop for TokenHandle {
        fn drop(&mut self) {
            // SAFETY: OpenProcessToken created this owned handle and Drop runs exactly once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    let mut raw_token: HANDLE = ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle, and raw_token is writable.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) };
    if opened == 0 {
        return Err(format!(
            "OpenProcessToken(TOKEN_QUERY) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let token = TokenHandle(raw_token);

    let mut required_chars = 0_u32;
    // SAFETY: token is live; the API accepts a null output buffer for its sizing call, and the
    // required_chars pointer is writable.
    let first_call =
        unsafe { GetUserProfileDirectoryW(token.0, ptr::null_mut(), &mut required_chars) };
    if required_chars == 0 {
        let detail = if first_call == 0 {
            std::io::Error::last_os_error().to_string()
        } else {
            "the API returned an empty profile path".to_string()
        };
        return Err(format!(
            "GetUserProfileDirectoryW sizing call failed: {detail}"
        ));
    }

    let mut buffer = vec![0_u16; required_chars as usize];
    let mut buffer_chars = required_chars;
    // SAFETY: token is live and buffer is writable for buffer_chars UTF-16 units; the length
    // pointer is writable for the API's result.
    let succeeded =
        unsafe { GetUserProfileDirectoryW(token.0, buffer.as_mut_ptr(), &mut buffer_chars) };
    if succeeded == 0 {
        return Err(format!(
            "GetUserProfileDirectoryW failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let nul = buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| "GetUserProfileDirectoryW returned an unterminated path".to_string())?;
    let path = PathBuf::from(OsString::from_wide(&buffer[..nul]));
    if is_absolute_directory_path(&path) {
        Ok(path)
    } else {
        Err(format!(
            "GetUserProfileDirectoryW returned an invalid absolute profile directory: {}",
            path.display()
        ))
    }
}

pub(crate) fn configure_command(
    command: &mut Command,
    client: &str,
    target_path: &Path,
) -> Result<bool, String> {
    #[cfg(windows)]
    {
        configure_windows_command(command, client, target_path, env::current_dir)
    }

    #[cfg(not(windows))]
    {
        let _ = (command, client, target_path);
        Ok(true)
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy)]
enum ClientHomeVariable {
    Codex,
    OpenClaw,
    Claude,
    ClaudeProject,
}

#[cfg(any(windows, test))]
fn configure_windows_command<F>(
    command: &mut Command,
    client: &str,
    target_path: &Path,
    current_dir: F,
) -> Result<bool, String>
where
    F: FnOnce() -> std::io::Result<PathBuf>,
{
    let Some(home_variable) = client_home_variable(client, target_path) else {
        return Ok(false);
    };

    // Call this before the caller changes Command::current_dir. Relative selected targets are
    // projected against the original process directory while preserving their path spelling.
    let absolute_target = absolute_target_path(target_path, current_dir)?;
    let parent = absolute_target.parent().ok_or_else(|| {
        format!(
            "selected Windows client config target has no parent directory: {}",
            absolute_target.display()
        )
    })?;
    if !parent.is_absolute() {
        return Err(format!(
            "selected Windows client config parent is not absolute: {}",
            parent.display()
        ));
    }

    match home_variable {
        ClientHomeVariable::Codex => {
            command.env("CODEX_HOME", parent);
        }
        ClientHomeVariable::OpenClaw => {
            command.env("OPENCLAW_HOME", parent);
        }
        ClientHomeVariable::Claude => {
            command.env("HOME", parent);
            command.env("USERPROFILE", parent);
        }
        ClientHomeVariable::ClaudeProject => {
            command.current_dir(parent);
        }
    }
    Ok(true)
}

#[cfg(any(windows, test))]
fn client_home_variable(client: &str, target_path: &Path) -> Option<ClientHomeVariable> {
    let filename = target_path.file_name()?.to_str()?;
    match client {
        "codex" if filename.eq_ignore_ascii_case("config.toml") => Some(ClientHomeVariable::Codex),
        "openclaw" if filename.eq_ignore_ascii_case("mcp.json5") => {
            Some(ClientHomeVariable::OpenClaw)
        }
        "claude" if filename.eq_ignore_ascii_case(".claude.json") => {
            Some(ClientHomeVariable::Claude)
        }
        "claude" | "claude-project" if filename.eq_ignore_ascii_case(".mcp.json") => {
            Some(ClientHomeVariable::ClaudeProject)
        }
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn absolute_target_path<F>(target_path: &Path, current_dir: F) -> Result<PathBuf, String>
where
    F: FnOnce() -> std::io::Result<PathBuf>,
{
    if target_path.is_absolute() {
        return Ok(target_path.to_path_buf());
    }

    let current_dir = current_dir().map_err(|error| {
        format!("failed to read the original current directory for Windows config target: {error}")
    })?;
    if !current_dir.is_absolute() {
        return Err(format!(
            "original current directory is not absolute: {}",
            current_dir.display()
        ));
    }
    Ok(current_dir.join(target_path))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(not(windows))]
    use super::configure_command;
    use super::{configure_windows_command, resolve_client_config_path, ClientConfigEnvironment};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "cod55-client-config-{}-{}-{label}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn env_value(command: &Command, name: &str) -> Option<std::ffi::OsString> {
        command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new(name))
            .and_then(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
    }

    #[test]
    fn repository_targets_and_client_overrides_do_not_resolve_a_profile() {
        let repo = Path::new("/workspace/repo");
        let environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/decoy/home")),
            codex_home: Some(PathBuf::from("relative/codex-home")),
            openclaw_home: Some(PathBuf::from("relative/openclaw-home")),
            ..ClientConfigEnvironment::default()
        };

        let codex_project =
            resolve_client_config_path("codex", "project", repo, &environment, true, || {
                panic!("repository target should not resolve a user profile")
            })
            .unwrap();
        assert_eq!(codex_project, repo.join(".codex/config.toml"));

        let claude_project =
            resolve_client_config_path("claude", "project", repo, &environment, true, || {
                panic!("repository target should not resolve a user profile")
            })
            .unwrap();
        assert_eq!(claude_project, repo.join(".mcp.json"));

        let codex_override =
            resolve_client_config_path("codex", "user", repo, &environment, true, || {
                panic!("Codex override should not resolve a user profile")
            })
            .unwrap();
        assert_eq!(
            codex_override,
            PathBuf::from("relative/codex-home/config.toml")
        );

        let openclaw_override =
            resolve_client_config_path("openclaw", "user", repo, &environment, true, || {
                panic!("OpenClaw override should not resolve a user profile")
            })
            .unwrap();
        assert_eq!(
            openclaw_override,
            PathBuf::from("relative/openclaw-home/mcp.json5")
        );
    }

    #[test]
    fn windows_uses_userprofile_and_ignores_home() {
        let repo = Path::new("/workspace/repo");
        let user_profile = TestDir::new("user-profile");
        let environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/decoy/home")),
            user_profile: Some(user_profile.path().to_path_buf()),
            ..ClientConfigEnvironment::default()
        };

        let path = resolve_client_config_path("claude", "user", repo, &environment, true, || {
            panic!("valid USERPROFILE should avoid native lookup")
        })
        .unwrap();

        assert_eq!(path, user_profile.path().join(".claude.json"));
    }

    #[test]
    fn windows_uses_native_profile_after_invalid_userprofile() {
        let repo = Path::new("/workspace/repo");
        let native_profile = TestDir::new("native-profile");
        let environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/decoy/home")),
            user_profile: Some(PathBuf::from("relative/profile")),
            ..ClientConfigEnvironment::default()
        };

        let path = resolve_client_config_path("hermes", "user", repo, &environment, true, || {
            Ok(native_profile.path().to_path_buf())
        })
        .unwrap();

        assert_eq!(path, native_profile.path().join(".hermes/config.yaml"));
    }

    #[test]
    fn windows_fails_when_profile_is_unavailable_or_not_absolute() {
        let repo = Path::new("/workspace/repo");
        let invalid_environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/decoy/home")),
            user_profile: Some(PathBuf::from("relative/profile")),
            ..ClientConfigEnvironment::default()
        };

        let unavailable =
            resolve_client_config_path("claude", "user", repo, &invalid_environment, true, || {
                Err("access denied".to_string())
            })
            .unwrap_err();
        assert!(unavailable.contains("USERPROFILE"));
        assert!(unavailable.contains("access denied"));

        let missing_environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/decoy/home")),
            ..ClientConfigEnvironment::default()
        };
        let relative_native =
            resolve_client_config_path("claude", "user", repo, &missing_environment, true, || {
                Ok(PathBuf::from("relative/native-profile"))
            })
            .unwrap_err();
        assert!(relative_native.contains("not a valid absolute directory"));
    }

    #[test]
    fn windows_uses_native_profile_when_userprofile_is_an_absolute_file() {
        let repo = Path::new("/workspace/repo");
        let user_profile_file = TestDir::new("profile-file-parent");
        let invalid_user_profile = user_profile_file.path().join("not-a-directory");
        fs::write(&invalid_user_profile, "file").unwrap();
        let native_profile = TestDir::new("native-profile-for-file-fallback");
        let environment = ClientConfigEnvironment {
            user_profile: Some(invalid_user_profile),
            ..ClientConfigEnvironment::default()
        };

        let path = resolve_client_config_path("claude", "user", repo, &environment, true, || {
            Ok(native_profile.path().to_path_buf())
        })
        .unwrap();

        assert_eq!(path, native_profile.path().join(".claude.json"));
    }

    #[test]
    fn non_windows_keeps_home_default_behavior() {
        let repo = Path::new("/workspace/repo");
        let environment = ClientConfigEnvironment {
            home: Some(PathBuf::from("/home/example")),
            user_profile: Some(PathBuf::from("/windows/decoy")),
            ..ClientConfigEnvironment::default()
        };

        let path = resolve_client_config_path("claude", "user", repo, &environment, false, || {
            panic!("non-Windows resolution must not call the Windows profile lookup")
        })
        .unwrap();
        assert_eq!(path, PathBuf::from("/home/example/.claude.json"));
    }

    #[test]
    fn windows_command_projects_relative_targets_before_child_cwd_changes() {
        let root = TestDir::new("command-projection");
        let original_dir = root.path().join("original");
        let openclaw_dir = root.path().join("openclaw");
        let claude_dir = root.path().join("claude");
        let mut codex = Command::new("fixture");
        assert!(configure_windows_command(
            &mut codex,
            "codex",
            Path::new("selected/config.toml"),
            || Ok(original_dir.clone()),
        )
        .unwrap());
        assert_eq!(
            env_value(&codex, "CODEX_HOME"),
            Some(original_dir.join("selected").into_os_string())
        );
        assert!(env_value(&codex, "HOME").is_none());

        let mut openclaw = Command::new("fixture");
        assert!(configure_windows_command(
            &mut openclaw,
            "openclaw",
            &openclaw_dir.join("mcp.json5"),
            || panic!("absolute target should not need the process CWD"),
        )
        .unwrap());
        assert_eq!(
            env_value(&openclaw, "OPENCLAW_HOME"),
            Some(openclaw_dir.into_os_string())
        );

        let mut claude = Command::new("fixture");
        assert!(configure_windows_command(
            &mut claude,
            "claude",
            &claude_dir.join(".claude.json"),
            || panic!("absolute target should not need the process CWD"),
        )
        .unwrap());
        assert_eq!(
            env_value(&claude, "HOME"),
            env_value(&claude, "USERPROFILE")
        );
        assert_eq!(
            env_value(&claude, "HOME"),
            Some(claude_dir.into_os_string())
        );
    }

    #[test]
    fn unsupported_windows_config_files_are_skipped_without_environment_changes() {
        let mut command = Command::new("fixture");
        command.env("PRESERVE_ME", "yes");

        let supported = configure_windows_command(
            &mut command,
            "codex",
            Path::new("/chosen/arbitrary.toml"),
            || panic!("unsupported targets should be rejected before reading the CWD"),
        )
        .unwrap();

        assert!(!supported);
        assert_eq!(env_value(&command, "PRESERVE_ME"), Some("yes".into()));
        assert!(env_value(&command, "CODEX_HOME").is_none());
    }

    #[test]
    fn windows_claude_project_visibility_uses_the_selected_project_directory() {
        let root = TestDir::new("claude-project-command");
        let target = root.path().join(".mcp.json");
        for client in ["claude", "claude-project"] {
            let mut command = Command::new("fixture");
            assert!(
                configure_windows_command(&mut command, client, &target, || {
                    panic!("absolute target must not read the caller CWD")
                })
                .unwrap()
            );
            assert_eq!(command.get_current_dir(), Some(root.path()));
            assert_eq!(command.get_envs().count(), 0);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_command_configuration_does_not_change_child_environment() {
        let mut command = Command::new("fixture");

        assert!(
            configure_command(&mut command, "codex", Path::new("/arbitrary/selected.toml"))
                .unwrap()
        );
        assert_eq!(command.get_envs().count(), 0);
    }
}
