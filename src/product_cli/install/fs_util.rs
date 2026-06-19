use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(in crate::product_cli) fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    fs::write(&tmp_path, text).map_err(|error| {
        format!(
            "failed to write temporary config {}: {error}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to replace config {}: {error}", path.display()))
}

pub(in crate::product_cli) fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(in crate::product_cli) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(in crate::product_cli) fn executable_in_path(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}

pub(in crate::product_cli) fn subprocess_error(completed: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&completed.stdout)
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(&completed.stderr)
        .trim()
        .to_string();
    let output = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let code = completed.status.code().unwrap_or(1);
    if output.is_empty() {
        format!("exit {code}")
    } else {
        format!("exit {code}: {output}")
    }
}
