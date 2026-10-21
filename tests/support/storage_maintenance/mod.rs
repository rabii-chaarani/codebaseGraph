//! Isolated fixture inputs and independent filesystem observations for DC-S5.
//! This module is consumed once the production maintenance surfaces are available.

pub mod workload;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct FixtureRoot {
    path: PathBuf,
}

impl FixtureRoot {
    pub fn new(label: &str) -> io::Result<Self> {
        // create_dir, rather than create_dir_all, rejects an existing incarnation.
        for _ in 0..100 {
            let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "cgraph-s5-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate an isolated storage-maintenance fixture",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write_revision(&self, revision: usize) -> io::Result<()> {
        let directory = self.path.join("src");
        fs::create_dir_all(&directory)?;
        let stable_source = directory.join("stable.rs");
        if !stable_source.exists() {
            fs::write(&stable_source, "pub fn stable_helper() -> usize { 7 }\n")?;
        }
        fs::write(directory.join("lib.rs"), source_revision(revision))
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        // This type owns only the directory it exclusively created. Process
        // fixtures must independently terminate and reap children before drop.
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn source_revision(revision: usize) -> String {
    format!(
        "mod stable;\npub use stable::stable_helper;\n\
         pub fn revision_{revision}() -> usize {{ stable_helper() + {revision} }}\n"
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObservedKind {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
    Other,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ObservedEntry {
    pub kind: ObservedKind,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
    #[cfg(unix)]
    pub unix_mode: u32,
}

/// Exact observation for small fixture source/sentinel trees. It deliberately
/// does not follow symlinks or silently discard unreadable entries. Large graph
/// trees require a separate reachability/count oracle, rather than loading their
/// database contents through this helper.
pub fn observe_small_tree(root: &Path) -> io::Result<BTreeMap<PathBuf, ObservedEntry>> {
    fn visit(
        root: &Path,
        path: &Path,
        entries: &mut BTreeMap<PathBuf, ObservedEntry>,
    ) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        let kind = if metadata.file_type().is_symlink() {
            ObservedKind::Symlink(fs::read_link(path)?)
        } else if metadata.is_dir() {
            ObservedKind::Directory
        } else if metadata.is_file() {
            ObservedKind::File(fs::read(path)?)
        } else {
            ObservedKind::Other
        };
        let is_directory = matches!(kind, ObservedKind::Directory);
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        entries.insert(
            path.strip_prefix(root)
                .expect("visited paths remain lexically inside the fixture")
                .to_path_buf(),
            ObservedEntry {
                kind,
                modified: metadata.modified().ok(),
                readonly: metadata.permissions().readonly(),
                #[cfg(unix)]
                unix_mode: metadata.permissions().mode(),
            },
        );
        if is_directory {
            for entry in fs::read_dir(path)? {
                visit(root, &entry?.path(), entries)?;
            }
        }
        Ok(())
    }

    let mut entries = BTreeMap::new();
    match fs::symlink_metadata(root) {
        Ok(_) => visit(root, root, &mut entries)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(entries)
}
