//! Owned, immutable inputs for the architecture compiler.
use super::{policy::RunContext, CompileError};
use crate::architecture::{GitObjectId, GitRevision, RelativePath, Text};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ModelInput {
    kind: InputKind,
}
#[derive(Clone, Debug)]
enum InputKind {
    Pinned {
        revision: GitRevision,
        root: RelativePath,
    },
    Inline {
        root: RelativePath,
        files: BTreeMap<String, Vec<u8>>,
    },
}
impl ModelInput {
    pub fn pinned_root(revision: GitRevision, root: RelativePath) -> Result<Self, CompileError> {
        check_path(root.as_str()).map_err(|error| with_subject(error, root.as_str()))?;
        Ok(Self {
            kind: InputKind::Pinned { revision, root },
        })
    }
    pub fn inline_files(
        root: RelativePath,
        files: impl IntoIterator<Item = (RelativePath, Vec<u8>)>,
    ) -> Result<Self, CompileError> {
        check_path(root.as_str()).map_err(|error| with_subject(error, root.as_str()))?;
        let mut result = BTreeMap::new();
        for (path, bytes) in files {
            check_path(path.as_str()).map_err(|error| with_subject(error, path.as_str()))?;
            if result.insert(path.as_str().to_owned(), bytes).is_some() {
                return Err(with_subject(
                    CompileError::simple("architecture_invalid_input", "duplicate inline path"),
                    path.as_str(),
                ));
            }
        }
        if !result.contains_key(root.as_str()) {
            return Err(with_subject(
                CompileError::simple("architecture_input_unavailable", "root file missing"),
                root.as_str(),
            ));
        }
        Ok(Self {
            kind: InputKind::Inline {
                root,
                files: result,
            },
        })
    }
}
#[derive(Clone, Debug)]
pub struct RepositoryBinding {
    identity: Text,
    directory: PathBuf,
}
impl RepositoryBinding {
    pub fn new(identity: Text, directory: PathBuf) -> Result<Self, CompileError> {
        if !directory.is_absolute() {
            return Err(CompileError::simple(
                "architecture_configuration_required",
                "repository directory must be absolute",
            ));
        }
        Ok(Self {
            identity,
            directory,
        })
    }
    pub fn identity(&self) -> &Text {
        &self.identity
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}
#[derive(Clone, Debug)]
pub struct ClosureFile {
    path: RelativePath,
    bytes: Vec<u8>,
    blob: Option<GitObjectId>,
}
impl ClosureFile {
    pub fn path(&self) -> &RelativePath {
        &self.path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn blob(&self) -> Option<&GitObjectId> {
        self.blob.as_ref()
    }
}
#[derive(Clone, Debug)]
pub struct PinnedProvenance {
    revision: GitRevision,
    tree: GitObjectId,
}
impl PinnedProvenance {
    pub fn revision(&self) -> &GitRevision {
        &self.revision
    }
    pub fn tree(&self) -> &GitObjectId {
        &self.tree
    }
}

pub(super) fn with_subject(mut error: CompileError, path: &str) -> CompileError {
    for diagnostic in &mut error.diagnostics {
        if diagnostic.subject.is_none() {
            diagnostic.subject = Some(path.to_owned());
        }
    }
    error
}
fn check_path(path: &str) -> Result<(), CompileError> {
    RelativePath::new(path).map_err(|e| CompileError::simple("path_not_confined", e))?;
    if path.contains(['*', '?', '[', ']']) {
        return Err(CompileError::simple(
            "path_not_confined",
            "glob paths are unsupported",
        ));
    }
    Ok(())
}

/// Holds a coherent input while the caller parses the root and supplies its explicit lists.
pub(super) struct Reader<'a> {
    root: RelativePath,
    mode: ReaderMode<'a>,
    captured: BTreeMap<String, ClosureFile>,
    max_files: u64,
    max_file_bytes: u64,
    max_yaml_bytes: u64,
    total: u64,
    run: &'a RunContext,
}
enum ReaderMode<'a> {
    Inline(BTreeMap<String, Vec<u8>>),
    Git {
        binding: &'a RepositoryBinding,
        provenance: PinnedProvenance,
    },
}
impl<'a> Reader<'a> {
    pub(super) fn new(
        input: ModelInput,
        repository: Option<&'a RepositoryBinding>,
        policy: &super::ArchitectureExecutionPolicy,
        run: &'a RunContext,
    ) -> Result<Self, CompileError> {
        run.check()?;
        let (root, mode) = match input.kind {
            InputKind::Inline { root, files } => (root, ReaderMode::Inline(files)),
            InputKind::Pinned { revision, root } => {
                let binding = repository.ok_or_else(|| {
                    CompileError::simple(
                        "architecture_configuration_required",
                        "repository binding required",
                    )
                })?;
                if binding.identity != revision.repository {
                    return Err(CompileError::simple(
                        "architecture_configuration_required",
                        "repository identity mismatch",
                    ));
                }
                let tree = git::commit_tree(binding, &revision.commit, run, false)?;
                (
                    root,
                    ReaderMode::Git {
                        binding,
                        provenance: PinnedProvenance { revision, tree },
                    },
                )
            }
        };
        Ok(Self {
            root,
            mode,
            captured: BTreeMap::new(),
            max_files: policy.max_closure_files,
            max_file_bytes: policy.max_file_bytes,
            max_yaml_bytes: policy.max_yaml_bytes,
            total: 0,
            run,
        })
    }
    pub(super) fn root_path(&self) -> &RelativePath {
        &self.root
    }
    pub(super) fn root(&mut self) -> Result<&ClosureFile, CompileError> {
        let path = self.root.clone();
        self.read(path.clone())
            .map_err(|error| with_subject(error, path.as_str()))?;
        Ok(self
            .captured
            .get(self.root.as_str())
            .expect("root captured"))
    }
    fn read(&mut self, path: RelativePath) -> Result<(), CompileError> {
        self.run.check()?;
        if self.captured.len() as u64 >= self.max_files {
            return Err(CompileError::simple(
                "resource_limit",
                "closure file count exceeded",
            ));
        }
        let remaining = self
            .max_yaml_bytes
            .checked_sub(self.total)
            .ok_or_else(|| CompileError::simple("resource_limit", "closure byte count exceeded"))?;
        let limit = self.max_file_bytes.min(remaining);
        let (bytes, blob) = match &mut self.mode {
            ReaderMode::Inline(files) => {
                let bytes = files.remove(path.as_str()).ok_or_else(|| {
                    CompileError::simple("architecture_input_unavailable", "listed file missing")
                })?;
                if bytes.len() as u64 > limit {
                    return Err(CompileError::simple(
                        "resource_limit",
                        "file or closure bytes exceeded",
                    ));
                }
                (bytes, None)
            }
            ReaderMode::Git {
                binding,
                provenance,
            } => git::file(
                binding,
                &provenance.tree,
                path.as_str(),
                limit,
                self.run,
                false,
            )?,
        };
        self.total = self
            .total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| CompileError::simple("resource_limit", "closure byte count overflow"))?;
        self.captured
            .insert(path.as_str().to_owned(), ClosureFile { path, bytes, blob });
        Ok(())
    }
    pub(super) fn finish(
        mut self,
        paths: &[RelativePath],
    ) -> Result<(Vec<ClosureFile>, Option<PinnedProvenance>), CompileError> {
        if !self.captured.contains_key(self.root.as_str()) {
            return Err(CompileError::simple(
                "architecture_internal_error",
                "root must be captured first",
            ));
        }
        let directory = self
            .root
            .as_str()
            .rsplit_once('/')
            .map(|(d, _)| d)
            .unwrap_or("");
        let directory = directory.to_owned();
        let mut seen = BTreeSet::new();
        seen.insert(self.root.as_str().to_owned());
        for path in paths {
            self.run.check()?;
            let resolved = if directory.is_empty() {
                path.as_str().to_owned()
            } else {
                format!("{directory}/{}", path.as_str())
            };
            check_path(path.as_str()).map_err(|error| with_subject(error, &resolved))?;
            check_path(&resolved).map_err(|error| with_subject(error, &resolved))?;
            if !seen.insert(resolved.clone()) {
                return Err(with_subject(
                    CompileError::simple("architecture_invalid_input", "duplicate closure path"),
                    &resolved,
                ));
            }
            let valid = RelativePath::new(resolved.clone()).map_err(|e| {
                with_subject(CompileError::simple("path_not_confined", e), &resolved)
            })?;
            self.read(valid)
                .map_err(|error| with_subject(error, &resolved))?;
        }
        let provenance = match self.mode {
            ReaderMode::Inline(files) => {
                if !files.is_empty() {
                    return Err(with_subject(
                        CompileError::simple(
                            "architecture_invalid_input",
                            "unreferenced inline files",
                        ),
                        files.keys().next().expect("nonempty map"),
                    ));
                }
                None
            }
            ReaderMode::Git { provenance, .. } => Some(provenance),
        };
        Ok((self.captured.into_values().collect(), provenance))
    }
}

mod git {
    use super::*;
    use std::io::{Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::Duration;

    fn command(binding: &RepositoryBinding) -> Command {
        let mut c = Command::new("git");
        c.env_clear();
        for key in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                c.env(key, value);
            }
        }
        c.env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .arg("--git-dir")
            .arg(&binding.directory)
            .arg("--no-replace-objects")
            .arg("--literal-pathspecs")
            .arg("-c")
            .arg("protocol.allow=never");
        c
    }
    fn unavailable(reason: impl Into<String>) -> CompileError {
        CompileError::simple("architecture_input_unavailable", reason)
    }
    fn integrity_failure(reason: impl Into<String>) -> CompileError {
        let mut error = unavailable(reason);
        error.object_integrity_failure = true;
        error
    }
    fn drain(mut source: impl Read, limit: usize) -> (Vec<u8>, bool) {
        let mut result = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut overflow = false;
        loop {
            match source.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = limit.saturating_sub(result.len());
                    let keep = n.min(remaining);
                    if result.try_reserve(keep).is_err() {
                        overflow = true;
                    } else {
                        result.extend_from_slice(&buffer[..keep]);
                    }
                    if keep < n {
                        overflow = true;
                    }
                }
                Err(_) => {
                    overflow = true;
                    break;
                }
            }
        }
        (result, overflow)
    }
    fn terminate(children: &mut [&mut Child]) {
        for child in children.iter_mut() {
            let _ = child.kill();
        }
        for child in children.iter_mut() {
            let _ = child.wait();
        }
    }
    // On a thread-creation error the already-created children and threads are
    // synchronously reaped before the operation is allowed to return.
    macro_rules! spawn_checked {
        ($job:expr, $children:expr, [$($started:ident),*]) => {
            match thread::Builder::new().spawn($job) {
                Ok(handle) => handle,
                Err(error) => {
                    terminate($children);
                    $(let _ = $started.join();)*
                    return Err(unavailable(error.to_string()));
                }
            }
        };
    }
    fn wait(children: &mut [&mut Child], run: &RunContext) -> Result<(), CompileError> {
        loop {
            if let Err(e) = run.check() {
                for child in children.iter_mut() {
                    let _ = child.kill();
                }
                for child in children.iter_mut() {
                    let _ = child.wait();
                }
                return Err(e);
            }
            let mut all = true;
            for child in children.iter_mut() {
                match child.try_wait() {
                    Ok(Some(_)) => {}
                    Ok(None) => all = false,
                    Err(e) => {
                        for c in children.iter_mut() {
                            let _ = c.kill();
                        }
                        for c in children.iter_mut() {
                            let _ = c.wait();
                        }
                        return Err(unavailable(e.to_string()));
                    }
                }
            }
            if all {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        for child in children.iter_mut() {
            if !child
                .wait()
                .map_err(|e| unavailable(e.to_string()))?
                .success()
            {
                return Err(integrity_failure("git object operation failed"));
            }
        }
        Ok(())
    }
    fn output(
        binding: &RepositoryBinding,
        args: &[&str],
        input: Option<Vec<u8>>,
        limit: usize,
        run: &RunContext,
    ) -> Result<Vec<u8>, CompileError> {
        run.check()?;
        let mut child = command(binding)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| unavailable(e.to_string()))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = spawn_checked!(move || drain(stdout, limit), &mut [&mut child], []);
        let err = spawn_checked!(move || drain(stderr, 0), &mut [&mut child], [out]);
        let writer = if let Some(bytes) = input {
            let mut stdin = child.stdin.take().unwrap();
            Some(spawn_checked!(
                move || stdin.write_all(&bytes),
                &mut [&mut child],
                [out, err]
            ))
        } else {
            None
        };
        let status = wait(&mut [&mut child], run);
        let (bytes, overflow) = out.join().map_err(|_| unavailable("git reader failed"))?;
        let _ = err.join().map_err(|_| unavailable("git reader failed"))?;
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        status?;
        if overflow {
            return Err(CompileError::simple(
                "resource_limit",
                "git output limit exceeded",
            ));
        }
        Ok(bytes)
    }
    fn verify_bytes(
        binding: &RepositoryBinding,
        typ: &str,
        expected: &str,
        bytes: &[u8],
        run: &RunContext,
    ) -> Result<(), CompileError> {
        let mut owned = Vec::new();
        owned
            .try_reserve(bytes.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        owned.extend_from_slice(bytes);
        let hash = output(
            binding,
            &["hash-object", "-t", typ, "--stdin"],
            Some(owned),
            128,
            run,
        )?;
        if hash.strip_suffix(b"\n") != Some(expected.as_bytes()) {
            return Err(integrity_failure("git object hash mismatch"));
        }
        Ok(())
    }
    struct TreeScan {
        target: Vec<u8>,
        hash_len: usize,
        phase: u8,
        mode: u32,
        digits: usize,
        name_len: usize,
        matches: bool,
        oid: Vec<u8>,
        selected: Option<(u32, Vec<u8>)>,
    }
    impl TreeScan {
        fn new(target: Vec<u8>, hash_len: usize) -> Self {
            Self {
                target,
                hash_len,
                phase: 0,
                mode: 0,
                digits: 0,
                name_len: 0,
                matches: true,
                oid: Vec::new(),
                selected: None,
            }
        }
        fn push(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            use std::io::{Error, ErrorKind};
            let bad = || Error::new(ErrorKind::InvalidData, "invalid tree object");
            for &byte in bytes {
                match self.phase {
                    0 => {
                        if byte == b' ' {
                            if self.digits == 0 {
                                return Err(bad());
                            }
                            self.phase = 1;
                        } else if (b'0'..=b'7').contains(&byte) && self.digits < 6 {
                            self.mode = self.mode * 8 + u32::from(byte - b'0');
                            self.digits += 1;
                        } else {
                            return Err(bad());
                        }
                    }
                    1 => {
                        if byte == 0 {
                            if self.name_len == 0 {
                                return Err(bad());
                            }
                            self.matches &= self.name_len == self.target.len();
                            self.phase = 2;
                        } else {
                            self.matches &= self.target.get(self.name_len) == Some(&byte);
                            self.name_len = self.name_len.checked_add(1).ok_or_else(bad)?;
                        }
                    }
                    2 => {
                        if self.matches {
                            self.oid.push(byte);
                        }
                        self.name_len = self.name_len.checked_add(1).ok_or_else(bad)?;
                        // name_len already contains the filename length.
                        if self.matches
                            && self.name_len.checked_sub(self.target.len()) == Some(self.hash_len)
                        {
                            if self.selected.is_some() {
                                return Err(bad());
                            }
                            self.selected = Some((self.mode, std::mem::take(&mut self.oid)));
                            self.phase = 0;
                            self.mode = 0;
                            self.digits = 0;
                            self.name_len = 0;
                            self.matches = true;
                        } else if !self.matches {
                            // For nonmatching names track object bytes separately.
                            self.oid.push(byte);
                            if self.oid.len() == self.hash_len {
                                self.oid.clear();
                                self.phase = 0;
                                self.mode = 0;
                                self.digits = 0;
                                self.name_len = 0;
                                self.matches = true;
                            }
                        }
                    }
                    _ => return Err(bad()),
                }
            }
            Ok(())
        }
        fn finish(self) -> std::io::Result<Option<(u32, Vec<u8>)>> {
            if self.phase != 0 || self.digits != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "truncated tree object",
                ));
            }
            Ok(self.selected)
        }
    }
    // Stream the entire immutable object through Git's object-format-aware hasher.
    // Only the fixed-length commit tree header is retained; unrelated tree entries
    // are never buffered or subjected to a YAML-file size limit.
    type VerifiedObject = (Vec<u8>, Option<(u32, Vec<u8>)>);
    fn verify_object(
        binding: &RepositoryBinding,
        typ: &str,
        id: &str,
        run: &RunContext,
        target: Option<&str>,
    ) -> Result<VerifiedObject, CompileError> {
        run.check()?;
        let mut cat = command(binding)
            .args(["cat-file", typ, id])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| unavailable(e.to_string()))?;
        let hash_result = command(binding)
            .args(["hash-object", "-t", typ, "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut hash = match hash_result {
            Ok(c) => c,
            Err(e) => {
                let _ = cat.kill();
                let _ = cat.wait();
                return Err(unavailable(e.to_string()));
            }
        };
        let mut source = cat.stdout.take().unwrap();
        let mut sink = hash.stdin.take().unwrap();
        let prefix_limit = if typ == "commit" { id.len() + 6 } else { 0 };
        let target = target.map(|s| s.as_bytes().to_vec());
        let hash_len = id.len() / 2;
        let transfer = spawn_checked!(
            move || {
                let mut scan = target.map(|target| TreeScan::new(target, hash_len));
                let mut prefix = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    let n = source.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    if let Some(scan) = &mut scan {
                        scan.push(&buffer[..n])?;
                    }
                    let take = n.min(prefix_limit.saturating_sub(prefix.len()));
                    prefix.extend_from_slice(&buffer[..take]);
                    sink.write_all(&buffer[..n])?;
                }
                let selected = match scan {
                    Some(scan) => scan.finish()?,
                    None => None,
                };
                Ok::<_, std::io::Error>((prefix, selected))
            },
            &mut [&mut cat, &mut hash],
            []
        );
        let a = cat.stderr.take().unwrap();
        let b = hash.stderr.take().unwrap();
        let c = hash.stdout.take().unwrap();
        let ta = spawn_checked!(move || drain(a, 0), &mut [&mut cat, &mut hash], [transfer]);
        let tb = spawn_checked!(
            move || drain(b, 0),
            &mut [&mut cat, &mut hash],
            [transfer, ta]
        );
        let tc = spawn_checked!(
            move || drain(c, 128),
            &mut [&mut cat, &mut hash],
            [transfer, ta, tb]
        );
        let status = wait(&mut [&mut cat, &mut hash], run);
        let prefix = transfer
            .join()
            .map_err(|_| unavailable("git transfer failed"))?;
        let _ = ta.join().map_err(|_| unavailable("git reader failed"))?;
        let _ = tb.join().map_err(|_| unavailable("git reader failed"))?;
        let (digest, co) = tc.join().map_err(|_| unavailable("git reader failed"))?;
        status?;
        let result = prefix.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                integrity_failure(e.to_string())
            } else {
                unavailable(e.to_string())
            }
        })?;
        if co || digest.strip_suffix(b"\n") != Some(id.as_bytes()) {
            return Err(integrity_failure("object hash mismatch"));
        }
        Ok(result)
    }
    fn verify_tree(
        binding: &RepositoryBinding,
        tree: &str,
        run: &RunContext,
    ) -> Result<(), CompileError> {
        verify_object(binding, "tree", tree, run, None).map(|_| ())
    }
    pub(super) fn commit_tree(
        binding: &RepositoryBinding,
        commit: &GitObjectId,
        run: &RunContext,
        strict: bool,
    ) -> Result<GitObjectId, CompileError> {
        let (prefix, _) = if strict {
            checked_object(binding, commit.as_str(), "commit", run, || {
                verify_object(binding, "commit", commit.as_str(), run, None)
            })?
        } else {
            verify_object(binding, "commit", commit.as_str(), run, None)?
        };
        let line = prefix
            .strip_suffix(b"\n")
            .and_then(|v| v.strip_prefix(b"tree "))
            .ok_or_else(|| integrity_failure("missing commit tree"))?;
        let tree =
            std::str::from_utf8(line).map_err(|_| integrity_failure("invalid commit tree"))?;
        let tree = GitObjectId::new(tree).map_err(integrity_failure)?;
        if tree.as_str().len() != commit.as_str().len() {
            return Err(integrity_failure("object format mismatch"));
        }
        if strict {
            checked_object(binding, tree.as_str(), "tree", run, || {
                verify_tree(binding, tree.as_str(), run)
            })?;
        } else {
            verify_tree(binding, tree.as_str(), run)?;
        }
        Ok(tree)
    }
    // Query existence separately without interpreting Git's diagnostics.
    fn object_exists(
        binding: &RepositoryBinding,
        id: &str,
        run: &RunContext,
    ) -> Result<bool, CompileError> {
        run.check()?;
        let mut child = command(binding)
            .args(["cat-file", "-e", id])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| unavailable(e.to_string()))?;
        loop {
            if let Err(error) = run.check() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return match status.code() {
                        Some(0) => Ok(true),
                        Some(1) => Ok(false),
                        _ => Err(unavailable("Git object existence query failed")),
                    }
                }
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(unavailable(error.to_string()));
                }
            }
        }
    }
    fn check_object_type(
        binding: &RepositoryBinding,
        id: &str,
        expected: &str,
        run: &RunContext,
    ) -> Result<(), CompileError> {
        let input = format!("{id}\n").into_bytes();
        let result = output(
            binding,
            &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
            Some(input),
            160,
            run,
        )
        .map_err(|mut error| {
            error.object_integrity_failure = false;
            error
        })?;
        if result == format!("{id} missing\n").as_bytes() {
            return if object_exists(binding, id, run)? {
                Err(CompileError::simple(
                    "invalid_reference",
                    "Git object is unreadable",
                ))
            } else {
                Err(unavailable("Git object missing"))
            };
        }
        if result != format!("{id} {expected}\n").as_bytes() {
            return Err(CompileError::simple(
                "invalid_reference",
                "Git object has wrong type or is unreadable",
            ));
        }
        Ok(())
    }
    // After an object was found, distinguish a concurrent disappearance from an
    // unreadable or malformed object stream without interpreting stderr text.
    fn checked_object<T>(
        binding: &RepositoryBinding,
        id: &str,
        expected: &str,
        run: &RunContext,
        action: impl FnOnce() -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        check_object_type(binding, id, expected, run)?;
        match action() {
            Err(error) if error.object_integrity_failure => {
                check_object_type(binding, id, expected, run)?;
                Err(CompileError::simple(
                    "invalid_reference",
                    "Git object stream failed verification",
                ))
            }
            result => result,
        }
    }
    pub(super) fn file(
        binding: &RepositoryBinding,
        root_tree: &GitObjectId,
        path: &str,
        limit: u64,
        run: &RunContext,
        strict: bool,
    ) -> Result<(Vec<u8>, Option<GitObjectId>), CompileError> {
        let mut tree = root_tree.clone();
        let mut components = path.split('/').peekable();
        while let Some(part) = components.next() {
            run.check()?;
            let (_, selected) = if strict {
                checked_object(binding, tree.as_str(), "tree", run, || {
                    verify_object(binding, "tree", tree.as_str(), run, Some(part))
                })?
            } else {
                verify_object(binding, "tree", tree.as_str(), run, Some(part))?
            };
            let (mode, raw_id) = selected.ok_or_else(|| unavailable("missing tree entry"))?;
            let mut encoded = String::new();
            encoded
                .try_reserve(raw_id.len() * 2)
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            const HEX: &[u8] = b"0123456789abcdef";
            for byte in raw_id {
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 15) as usize] as char);
            }
            let id = GitObjectId::new(encoded).map_err(unavailable)?;
            if components.peek().is_some() {
                if mode != 0o40000 {
                    return Err(CompileError::simple(
                        "path_not_confined",
                        "non-directory or unsafe tree entry",
                    ));
                }
                if strict {
                    checked_object(binding, id.as_str(), "tree", run, || {
                        verify_tree(binding, id.as_str(), run)
                    })?;
                } else {
                    verify_tree(binding, id.as_str(), run)?;
                }
                tree = id;
            } else {
                if !matches!(mode, 0o100644 | 0o100755) {
                    return Err(CompileError::simple(
                        "path_not_confined",
                        "nonregular file entry",
                    ));
                }
                let read_blob = || -> Result<Vec<u8>, CompileError> {
                    let size = output(binding, &["cat-file", "-s", id.as_str()], None, 64, run)?;
                    let size = std::str::from_utf8(&size)
                        .ok()
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .ok_or_else(|| integrity_failure("invalid blob size"))?;
                    if size > limit || size > usize::MAX as u64 {
                        return Err(CompileError::simple(
                            "resource_limit",
                            "blob size exceeds limit",
                        ));
                    }
                    let bytes = output(
                        binding,
                        &["cat-file", "blob", id.as_str()],
                        None,
                        size as usize,
                        run,
                    )?;
                    if bytes.len() as u64 != size {
                        return Err(integrity_failure("blob size mismatch"));
                    }
                    verify_bytes(binding, "blob", id.as_str(), &bytes, run)?;
                    Ok(bytes)
                };
                let bytes = if strict {
                    checked_object(binding, id.as_str(), "blob", run, read_blob)?
                } else {
                    read_blob()?
                };
                return Ok((bytes, Some(id)));
            }
        }
        Err(unavailable("empty path"))
    }
}

/// Capture one independently selected immutable Git artifact using the compiler's
/// existing verified object reader and confinement rules.
pub(super) fn capture_artifact(
    binding: &RepositoryBinding,
    reference: &crate::architecture::ArtifactRef,
    limit: u64,
    run: &RunContext,
) -> Result<(GitObjectId, GitObjectId, Vec<u8>), CompileError> {
    let capture = || {
        RelativePath::new(reference.path.as_str())
            .map_err(|e| CompileError::simple("path_not_confined", e))?;
        let tree = git::commit_tree(binding, &reference.commit, run, true)?;
        let (bytes, object) = git::file(binding, &tree, reference.path.as_str(), limit, run, true)?;
        Ok((tree, object.expect("Git capture returns blob ID"), bytes))
    };
    capture().map_err(|mut error: CompileError| {
        if error.object_integrity_failure {
            error.code = "invalid_reference";
            for diagnostic in &mut error.diagnostics {
                diagnostic.code = "invalid_reference";
            }
        }
        with_subject(error, reference.path.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::super::{ArchitectureExecutionPolicy, ExecutionContext};
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};
    fn policy() -> ArchitectureExecutionPolicy {
        let fields = [
            "max_closure_files",
            "max_file_bytes",
            "max_yaml_bytes",
            "max_parse_depth",
            "max_authored_entities",
            "max_expanded_rule_pairs",
            "max_mapping_records",
            "max_citations_per_mapping",
            "max_rationale_bytes",
            "max_proposal_operations",
            "max_scenarios",
            "max_projected_vertices",
            "max_projected_edges",
            "max_traversal_steps",
            "max_evidence_bytes",
            "max_result_bytes",
            "max_evaluation_duration_ms",
        ];
        ArchitectureExecutionPolicy::from_json(serde_json::Value::Object(
            fields
                .into_iter()
                .map(|k| (k.into(), 100000.into()))
                .collect(),
        ))
        .unwrap()
    }
    fn path(s: &str) -> RelativePath {
        RelativePath::new(s).unwrap()
    }
    fn context(p: &ArchitectureExecutionPolicy) -> super::super::policy::RunContext {
        ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(p)
            .unwrap()
    }
    #[test]
    fn inline_exact_closure_and_paths() {
        let p = policy();
        let ctx = context(&p);
        let input = ModelInput::inline_files(
            path("dir/root"),
            [
                (path("dir/root"), b"root".to_vec()),
                (path("dir/a"), b"a".to_vec()),
            ],
        )
        .unwrap();
        let mut reader = Reader::new(input, None, &p, &ctx).unwrap();
        assert_eq!(reader.root().unwrap().bytes(), b"root");
        let (files, provenance) = reader.finish(&[path("a")]).unwrap();
        assert!(provenance.is_none());
        assert_eq!(
            files.iter().map(|f| f.path().as_str()).collect::<Vec<_>>(),
            vec!["dir/a", "dir/root"]
        );
        let input = ModelInput::inline_files(
            path("root"),
            [(path("root"), vec![]), (path("extra"), vec![])],
        )
        .unwrap();
        let mut reader = Reader::new(input, None, &p, &ctx).unwrap();
        reader.root().unwrap();
        assert!(reader.finish(&[]).is_err());
        assert!(ModelInput::inline_files(path("*.yaml"), [(path("*.yaml"), vec![])]).is_err());
        assert!(RelativePath::new("../escape").is_err());
    }
    fn git(repo: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    #[test]
    fn pinned_commit_is_immutable_and_unsafe_entries_reject() {
        let repo = std::env::temp_dir().join(format!(
            "cod21-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.name", "test"]);
        git(&repo, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(repo.join("root"), b"old").unwrap();
        std::fs::write(repo.join("abc:line\nname"), b"child").unwrap();
        std::fs::create_dir(repo.join("nested")).unwrap();
        std::fs::write(repo.join("nested/leaf"), b"nested").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("root", repo.join("link")).unwrap();
        git(&repo, &["add", "--all"]);
        git(&repo, &["commit", "-qm", "initial"]);
        let commit = git(&repo, &["rev-parse", "HEAD"]);
        std::fs::write(repo.join("root"), b"new").unwrap();
        git(&repo, &["add", "root"]);
        git(&repo, &["commit", "-qm", "change"]);
        let revision = GitRevision {
            repository: Text::new("repo").unwrap(),
            commit: GitObjectId::new(commit).unwrap(),
        };
        let binding =
            RepositoryBinding::new(Text::new("repo").unwrap(), repo.join(".git")).unwrap();
        let p = policy();
        let ctx = context(&p);
        let input = ModelInput::pinned_root(revision.clone(), path("root")).unwrap();
        let mut reader = Reader::new(input, Some(&binding), &p, &ctx).unwrap();
        assert_eq!(reader.root().unwrap().bytes(), b"old");
        let (files, provenance) = reader
            .finish(&[path("abc:line\nname"), path("nested/leaf")])
            .unwrap();
        assert_eq!(files.len(), 3);
        assert!(provenance.is_some());
        #[cfg(unix)]
        {
            let input = ModelInput::pinned_root(revision.clone(), path("link")).unwrap();
            let mut reader = Reader::new(input, Some(&binding), &p, &ctx).unwrap();
            assert_eq!(reader.root().unwrap_err().code(), "path_not_confined");
        }
        let blob = files
            .iter()
            .find(|f| f.path().as_str() == "root")
            .unwrap()
            .blob()
            .unwrap()
            .as_str();
        let object = repo.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
        let wrong_id = git(&repo, &["rev-parse", "HEAD:root"]);
        let wrong_object = repo
            .join(".git/objects")
            .join(&wrong_id[..2])
            .join(&wrong_id[2..]);
        let valid_wrong_bytes = std::fs::read(wrong_object).unwrap();
        std::fs::remove_file(&object).unwrap();
        std::fs::write(object, valid_wrong_bytes).unwrap();
        let input = ModelInput::pinned_root(revision, path("root")).unwrap();
        let mut reader = Reader::new(input, Some(&binding), &p, &ctx).unwrap();
        assert_eq!(
            reader.root().unwrap_err().code(),
            "architecture_input_unavailable"
        );
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn bounds_missing_objects_and_cancellation() {
        let mut p = policy();
        p.max_file_bytes = 3;
        let ctx = context(&p);
        let input =
            ModelInput::inline_files(path("root"), [(path("root"), b"four".to_vec())]).unwrap();
        let mut reader = Reader::new(input, None, &p, &ctx).unwrap();
        assert_eq!(reader.root().unwrap_err().code(), "resource_limit");
        let flag = Arc::new(AtomicBool::new(true));
        let cancelled = ExecutionContext::new(flag, None).start(&p).unwrap();
        let input = ModelInput::inline_files(path("root"), [(path("root"), vec![])]).unwrap();
        assert_eq!(
            Reader::new(input, None, &p, &cancelled)
                .err()
                .unwrap()
                .code(),
            "cancelled"
        );
        let repo = std::env::temp_dir().join(format!("cod21-missing-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        let binding =
            RepositoryBinding::new(Text::new("repo").unwrap(), repo.join(".git")).unwrap();
        let revision = GitRevision {
            repository: Text::new("repo").unwrap(),
            commit: GitObjectId::new("0".repeat(40)).unwrap(),
        };
        let input = ModelInput::pinned_root(revision, path("root")).unwrap();
        assert_eq!(
            Reader::new(input, Some(&binding), &p, &ctx)
                .err()
                .unwrap()
                .code(),
            "architecture_input_unavailable"
        );
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn sha256_repository_and_gitlink() {
        let repo = std::env::temp_dir().join(format!("cod21-sha256-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q", "--object-format=sha256"])
            .output()
            .unwrap();
        if !init.status.success() {
            std::fs::remove_dir_all(repo).unwrap();
            return;
        }
        git(&repo, &["config", "user.name", "test"]);
        git(&repo, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(repo.join("root"), b"root").unwrap();
        git(&repo, &["add", "root"]);
        git(&repo, &["commit", "-qm", "first"]);
        let first = git(&repo, &["rev-parse", "HEAD"]);
        assert_eq!(first.len(), 64);
        git(
            &repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000",
                &first,
                "module",
            ],
        );
        git(&repo, &["commit", "-qm", "gitlink"]);
        let revision = GitRevision {
            repository: Text::new("repo").unwrap(),
            commit: GitObjectId::new(git(&repo, &["rev-parse", "HEAD"])).unwrap(),
        };
        let binding =
            RepositoryBinding::new(Text::new("repo").unwrap(), repo.join(".git")).unwrap();
        let p = policy();
        let ctx = context(&p);
        let input = ModelInput::pinned_root(revision.clone(), path("root")).unwrap();
        let mut reader = Reader::new(input, Some(&binding), &p, &ctx).unwrap();
        assert_eq!(reader.root().unwrap().bytes(), b"root");
        assert_eq!(
            reader.finish(&[]).unwrap().1.unwrap().tree().as_str().len(),
            64
        );
        let input = ModelInput::pinned_root(revision, path("module")).unwrap();
        let mut reader = Reader::new(input, Some(&binding), &p, &ctx).unwrap();
        assert_eq!(reader.root().unwrap_err().code(), "path_not_confined");
        std::fs::remove_dir_all(repo).unwrap();
    }
}
