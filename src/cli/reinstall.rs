use super::{
    format::reinstall_help,
    setup::{setup_payload, GraphStatePaths},
    watch::SetupOptions,
};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(in crate::cli) fn run_reinstall<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = SetupOptions::parse_with_help(args, "reinstall", reinstall_help())?;
    if options.help {
        writeln!(stdout, "{}", reinstall_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }

    let repo_root = options
        .repo_root
        .canonicalize()
        .map_err(|error| format!("failed to resolve repo root: {error}"))?;
    if repo_root
        .components()
        .any(|component| component.as_os_str() == ".codebaseGraph")
    {
        return Err(format!(
            "Repository root may not be inside a .codebaseGraph state directory: {}",
            repo_root.display()
        ));
    }

    let paths = GraphStatePaths::derive(&repo_root);
    let state = reinstall_state(&paths, options.dry_run)?;
    let install = if options.dry_run {
        setup_payload(&options)?
    } else {
        match setup_payload(&options) {
            Ok(payload) => {
                remove_backup(state.backup_path.as_deref())?;
                payload
            }
            Err(error) => {
                restore_backup(&paths.state_dir, state.backup_path.as_deref())?;
                return Err(error);
            }
        }
    };

    let output = json!({
        "ok": true,
        "repo_root": repo_root,
        "dry_run": options.dry_run,
        "state": state.payload,
        "install": install,
    });
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

struct ReinstallState {
    payload: serde_json::Value,
    backup_path: Option<PathBuf>,
}

fn reinstall_state(paths: &GraphStatePaths, dry_run: bool) -> Result<ReinstallState, String> {
    if !paths.state_dir.exists() {
        return Ok(ReinstallState {
            payload: json!({
                "action": "unchanged",
                "path": paths.state_dir,
                "backup_path": serde_json::Value::Null,
            }),
            backup_path: None,
        });
    }
    let backup_path = next_backup_path(&paths.state_dir)?;
    if dry_run {
        return Ok(ReinstallState {
            payload: json!({
                "action": "dry_run",
                "path": paths.state_dir,
                "backup_path": backup_path,
            }),
            backup_path: None,
        });
    }
    fs::rename(&paths.state_dir, &backup_path).map_err(|error| {
        format!(
            "failed to move existing graph state {} to {}: {error}",
            paths.state_dir.display(),
            backup_path.display()
        )
    })?;
    Ok(ReinstallState {
        payload: json!({
            "action": "backed_up",
            "path": paths.state_dir,
            "backup_path": backup_path,
        }),
        backup_path: Some(backup_path),
    })
}

fn next_backup_path(state_dir: &Path) -> Result<PathBuf, String> {
    let parent = state_dir.parent().unwrap_or_else(|| Path::new("."));
    let file_name = state_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(".codebaseGraph");
    for index in 0..1000 {
        let suffix = if index == 0 {
            "reinstall-backup".to_string()
        } else {
            format!("reinstall-backup-{index}")
        };
        let candidate = parent.join(format!("{file_name}.{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "failed to choose backup path for {}",
        state_dir.display()
    ))
}

fn remove_backup(path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    remove_path(path).map_err(|error| {
        format!(
            "failed to remove reinstall backup {} after successful setup: {error}",
            path.display()
        )
    })
}

fn restore_backup(state_dir: &Path, backup_path: Option<&Path>) -> Result<(), String> {
    let Some(backup_path) = backup_path else {
        if state_dir.exists() {
            remove_path(state_dir).map_err(|error| {
                format!(
                    "failed to remove partial graph state {} after setup failure: {error}",
                    state_dir.display()
                )
            })?;
        }
        return Ok(());
    };
    if state_dir.exists() {
        remove_path(state_dir).map_err(|error| {
            format!(
                "failed to remove partial graph state {} before restore: {error}",
                state_dir.display()
            )
        })?;
    }
    fs::rename(backup_path, state_dir).map_err(|error| {
        format!(
            "failed to restore graph state backup {} to {}: {error}",
            backup_path.display(),
            state_dir.display()
        )
    })
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
