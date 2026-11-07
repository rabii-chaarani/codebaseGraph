//! Immutable source bytes admitted through the compiler's verified Git reader.
use super::EvidenceError;
use crate::architecture::{
    compile::{evidence_support as support, ArchitectureExecutionPolicy, RepositoryBinding},
    Digest, GitObjectId, RelativePath, SourceProvenance, SourceRef,
};

struct CapturedFile<'a> {
    path: &'a RelativePath,
    digest: &'a Digest,
    bytes: Vec<u8>,
}
pub(super) struct Capture<'a> {
    binding: &'a RepositoryBinding,
    tree: GitObjectId,
    commit: &'a GitObjectId,
    run: &'a support::Run,
    max_file: u64,
    remaining: u64,
    files: Vec<CapturedFile<'a>>,
}
impl<'a> Capture<'a> {
    /// Resolve and verify the declared tree even when no files are cited.
    pub(super) fn begin(
        source: &'a SourceProvenance,
        binding: &'a RepositoryBinding,
        policy: &ArchitectureExecutionPolicy,
        payload_bytes: u64,
        run: &'a support::Run,
    ) -> Result<Self, EvidenceError> {
        run.check()?;
        if binding.identity() != &source.repository {
            return Err(EvidenceError::new(
                "architecture_input_unavailable",
                "repository binding does not match",
            ));
        }
        let tree = support::tree(binding, &source.evaluated_commit, run)?;
        if tree != source.evaluated_tree {
            return Err(EvidenceError::new(
                "invalid_reference",
                "evaluated tree mismatch",
            ));
        }
        let remaining = policy
            .max_evidence_bytes()
            .checked_sub(payload_bytes)
            .ok_or_else(|| EvidenceError::new("resource_limit", "evidence bytes exceeded"))?;
        Ok(Self {
            binding,
            tree,
            commit: &source.evaluated_commit,
            run,
            max_file: policy.max_file_bytes(),
            remaining,
            files: Vec::new(),
        })
    }
    pub(super) fn file(&mut self, reference: &'a SourceRef) -> Result<&[u8], EvidenceError> {
        self.run.check()?;
        if self.binding.identity() != &reference.repository {
            return Err(EvidenceError::new(
                "invalid_reference",
                "repository mismatch",
            ));
        }
        if self.commit != &reference.commit {
            return Err(EvidenceError::new("invalid_reference", "commit mismatch"));
        }
        let mut found = None;
        for (i, file) in self.files.iter().enumerate() {
            self.run.check()?;
            if file.path == &reference.path {
                if file.digest != &reference.file_digest {
                    return Err(EvidenceError::new(
                        "invalid_reference",
                        "file digest mismatch",
                    ));
                }
                found = Some(i);
                break;
            }
        }
        let index = if let Some(i) = found {
            i
        } else {
            let bytes = support::file(
                self.binding,
                &self.tree,
                &reference.path,
                self.max_file.min(self.remaining),
                self.run,
            )?;
            let digest = support::raw_digest(&bytes, self.run)?;
            if digest != reference.file_digest {
                return Err(EvidenceError::new(
                    "invalid_reference",
                    "file digest mismatch",
                ));
            }
            self.remaining = self
                .remaining
                .checked_sub(bytes.len() as u64)
                .ok_or_else(|| EvidenceError::new("resource_limit", "evidence bytes exceeded"))?;
            self.files
                .try_reserve(1)
                .map_err(|_| EvidenceError::new("resource_limit", "allocation failed"))?;
            self.files.push(CapturedFile {
                path: &reference.path,
                digest: &reference.file_digest,
                bytes,
            });
            self.files.len() - 1
        };
        let bytes = &self.files[index].bytes;
        if reference.byte_span.end() > bytes.len() as u64 {
            return Err(EvidenceError::new("invalid_reference", "span exceeds file"));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::{compile::ExecutionContext, ByteSpan, GitObjectId, Text};
    use sha2::{Digest as _, Sha256};
    use std::{
        path::Path,
        sync::{atomic::AtomicBool, Arc},
    };
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    fn policy(evidence: u64) -> ArchitectureExecutionPolicy {
        let keys = [
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
        let mut raw = serde_json::Map::new();
        for key in keys {
            raw.insert(key.into(), 100_000.into());
        }
        raw.insert("max_evidence_bytes".into(), evidence.into());
        ArchitectureExecutionPolicy::from_json(raw.into()).unwrap()
    }
    #[test]
    fn immutable_capture_verifies_tree_bytes_identity_and_budgets_both_formats() {
        for format in ["sha1", "sha256"] {
            let dir = std::env::temp_dir().join(format!(
                "cod23-source-{}-{}-{}",
                std::process::id(),
                format,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            git(&dir, &["init", "-q", &format!("--object-format={format}")]);
            git(&dir, &["config", "user.name", "test"]);
            git(&dir, &["config", "user.email", "test@example.invalid"]);
            std::fs::write(dir.join("file.rs"), b"original").unwrap();
            git(&dir, &["add", "file.rs"]);
            git(&dir, &["commit", "-qm", "fixture"]);
            let commit = GitObjectId::new(git(&dir, &["rev-parse", "HEAD"])).unwrap();
            let tree = GitObjectId::new(git(&dir, &["rev-parse", "HEAD^{tree}"])).unwrap();
            let d = Digest::new(format!("sha256:{:x}", Sha256::digest(b"original"))).unwrap();
            let source = SourceProvenance {
                repository: Text::new("repo").unwrap(),
                evaluated_commit: commit.clone(),
                evaluated_tree: tree,
                extractor_digest: d.clone(),
                build_context_digest: d.clone(),
            };
            let reference = SourceRef {
                repository: source.repository.clone(),
                commit,
                path: RelativePath::new("file.rs").unwrap(),
                node_kind: Text::new("Function").unwrap(),
                node_id: Text::new("id").unwrap(),
                symbol: Text::new("name").unwrap(),
                byte_span: ByteSpan::new(0, 8).unwrap(),
                file_digest: d,
            };
            let binding =
                RepositoryBinding::new(source.repository.clone(), dir.join(".git")).unwrap();
            let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
            let p = policy(12);
            let (_, run) = support::start(&p, None, &context).unwrap();
            std::fs::write(dir.join("file.rs"), b"dirty").unwrap();
            let mut capture = Capture::begin(&source, &binding, &p, 4, &run).unwrap();
            assert_eq!(capture.file(&reference).unwrap(), b"original");
            assert_eq!(capture.file(&reference).unwrap(), b"original");
            let mut bad = reference.clone();
            bad.byte_span = ByteSpan::new(0, 9).unwrap();
            assert_eq!(capture.file(&bad).unwrap_err().code(), "invalid_reference");
            let mut bad_digest = reference.clone();
            bad_digest.file_digest = Digest::new(format!("sha256:{}", "0".repeat(64))).unwrap();
            assert_eq!(
                capture.file(&bad_digest).unwrap_err().code(),
                "invalid_reference"
            );
            let mut wrong = source.clone();
            wrong.evaluated_tree =
                GitObjectId::new("0".repeat(wrong.evaluated_tree.as_str().len())).unwrap();
            assert_eq!(
                Capture::begin(&wrong, &binding, &p, 0, &run)
                    .err()
                    .unwrap()
                    .code(),
                "invalid_reference"
            );
            let mut missing = source.clone();
            missing.evaluated_commit =
                GitObjectId::new("0".repeat(missing.evaluated_commit.as_str().len())).unwrap();
            assert_eq!(
                Capture::begin(&missing, &binding, &p, 0, &run)
                    .err()
                    .unwrap()
                    .code(),
                "architecture_input_unavailable"
            );
            let p2 = policy(11);
            let (_, r2) = support::start(&p2, None, &context).unwrap();
            let mut under = Capture::begin(&source, &binding, &p2, 4, &r2).unwrap();
            assert_eq!(under.file(&reference).unwrap_err().code(), "resource_limit");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
