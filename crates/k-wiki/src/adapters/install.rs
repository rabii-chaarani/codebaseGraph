use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed { destination: PathBuf, on_path: bool },
    AlreadyInstalled { destination: PathBuf, on_path: bool },
}

pub fn default_bin_dir() -> Result<PathBuf, String> {
    if let Some(directory) = env::var_os("K_WIKI_BIN_DIR") {
        return Ok(PathBuf::from(directory));
    }

    let home =
        env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or_else(|| {
            "set --bin-dir or K_WIKI_BIN_DIR because a home directory is unavailable".to_string()
        })?;
    Ok(PathBuf::from(home).join(".local").join("bin"))
}

pub fn install_binary(
    source: &Path,
    bin_dir: &Path,
    force: bool,
) -> Result<InstallOutcome, String> {
    let source = source
        .canonicalize()
        .map_err(|_| "the active k-wiki executable could not be located".to_string())?;
    if !source.is_file() {
        return Err("the active k-wiki executable is not a file".to_string());
    }

    fs::create_dir_all(bin_dir)
        .map_err(|_| "the installation directory could not be created".to_string())?;
    let bin_dir = bin_dir
        .canonicalize()
        .map_err(|_| "the installation directory could not be opened".to_string())?;
    let destination = bin_dir.join(binary_name());
    let on_path = directory_is_on_path(&bin_dir);

    if destination.exists() {
        if destination
            .canonicalize()
            .ok()
            .is_some_and(|existing| existing == source)
        {
            return Ok(InstallOutcome::AlreadyInstalled {
                destination,
                on_path,
            });
        }
        if !force {
            return Err(format!(
                "k-wiki already exists at {}; rerun with --force to replace it",
                destination.display()
            ));
        }
    }

    let temporary = temporary_path(&bin_dir);
    copy_file(&source, &temporary)?;
    replace_destination(&temporary, &destination, force)?;

    Ok(InstallOutcome::Installed {
        destination,
        on_path,
    })
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "k-wiki.exe"
    } else {
        "k-wiki"
    }
}

fn copy_file(source: &Path, temporary: &Path) -> Result<(), String> {
    let result = (|| -> io::Result<()> {
        let mut input = fs::File::open(source)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)?;
        io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        fs::set_permissions(temporary, source.metadata()?.permissions())?;
        Ok(())
    })();
    result.map_err(|_| "k-wiki could not be copied to the installation directory".to_string())
}

fn replace_destination(temporary: &Path, destination: &Path, force: bool) -> Result<(), String> {
    #[cfg(windows)]
    if force && destination.exists() {
        fs::remove_file(destination)
            .map_err(|_| "the existing k-wiki installation could not be replaced".to_string())?;
    }

    match fs::rename(temporary, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(temporary);
            Err(if force {
                "the existing k-wiki installation could not be replaced".to_string()
            } else {
                "k-wiki could not be installed without replacing an existing file".to_string()
            })
        }
    }
}

fn temporary_path(bin_dir: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    bin_dir.join(format!(".k-wiki-{}-{unique:x}.tmp", std::process::id()))
}

fn directory_is_on_path(directory: &Path) -> bool {
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths)
                .any(|entry| entry.canonicalize().ok().as_deref() == Some(directory))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{binary_name, install_binary, InstallOutcome};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn install_binary_creates_the_destination_and_requires_force_to_replace_it() {
        let root = unique_temp_dir("install");
        let source = root.join("source");
        let bin_dir = root.join("bin");
        fs::write(&source, "new binary").expect("write source binary");

        let outcome = install_binary(&source, &bin_dir, false).expect("install binary");
        let destination = bin_dir.join(binary_name());
        assert!(matches!(outcome, InstallOutcome::Installed { .. }));
        assert_eq!(
            fs::read(&destination).expect("read installed binary"),
            b"new binary"
        );

        fs::write(&destination, "existing binary").expect("write existing binary");
        let error = install_binary(&source, &bin_dir, false).expect_err("reject existing binary");
        assert!(error.contains("--force"));
        assert_eq!(
            fs::read(&destination).expect("read unchanged binary"),
            b"existing binary"
        );

        install_binary(&source, &bin_dir, true).expect("force replacement");
        assert_eq!(
            fs::read(&destination).expect("read replaced binary"),
            b"new binary"
        );

        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn install_binary_is_idempotent_when_the_source_is_already_installed() {
        let root = unique_temp_dir("idempotent");
        let bin_dir = root.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin directory");
        let destination = bin_dir.join(binary_name());
        fs::write(&destination, "installed binary").expect("write installed binary");

        let outcome =
            install_binary(&destination, &bin_dir, false).expect("detect installed binary");
        assert!(matches!(outcome, InstallOutcome::AlreadyInstalled { .. }));

        fs::remove_dir_all(root).expect("remove temp root");
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("k_wiki_{prefix}_{}_{}", std::process::id(), unique));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }
}
