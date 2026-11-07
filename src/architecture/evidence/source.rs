//! Immutable source bytes admitted through the compiler's verified Git reader.
use super::EvidenceError;
use crate::architecture::{
    compile::{evidence_support as support, ArchitectureExecutionPolicy, RepositoryBinding},
    Digest, GitObjectId, RelativePath, SourceProvenance, SourceRef,
};

struct CapturedFile<'a> {
    path: &'a RelativePath,
    digest: Digest,
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
            self.remaining = self
                .remaining
                .checked_sub(bytes.len() as u64)
                .ok_or_else(|| EvidenceError::new("resource_limit", "evidence bytes exceeded"))?;
            self.files
                .try_reserve(1)
                .map_err(|_| EvidenceError::new("resource_limit", "allocation failed"))?;
            self.files.push(CapturedFile {
                path: &reference.path,
                digest,
                bytes,
            });
            self.files.len() - 1
        };
        if self.files[index].digest != reference.file_digest {
            return Err(EvidenceError::new(
                "invalid_reference",
                "file digest mismatch",
            ));
        }
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
            std::fs::write(dir.join("second.rs"), b"second!!").unwrap();
            git(&dir, &["add", "file.rs", "second.rs"]);
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
            let mut second = reference.clone();
            second.path = RelativePath::new("second.rs").unwrap();
            second.file_digest =
                Digest::new(format!("sha256:{:x}", Sha256::digest(b"second!!"))).unwrap();
            let mut wrong_first = reference.clone();
            wrong_first.file_digest = Digest::new(format!("sha256:{}", "0".repeat(64))).unwrap();
            let mut wrong_second = second.clone();
            wrong_second.file_digest = wrong_first.file_digest.clone();
            let exact_policy = policy(20);
            let (_, exact_run) = support::start(&exact_policy, None, &context).unwrap();
            let mut exact =
                Capture::begin(&source, &binding, &exact_policy, 4, &exact_run).unwrap();
            assert_eq!(
                exact.file(&wrong_first).unwrap_err().code(),
                "invalid_reference"
            );
            assert_eq!(
                exact.file(&wrong_first).unwrap_err().code(),
                "invalid_reference"
            );
            assert_eq!(exact.file(&reference).unwrap(), b"original");
            assert_eq!(
                exact.file(&wrong_second).unwrap_err().code(),
                "invalid_reference"
            );
            assert_eq!(exact.file(&second).unwrap(), b"second!!");
            let under_policy = policy(19);
            let (_, under_run) = support::start(&under_policy, None, &context).unwrap();
            let mut under =
                Capture::begin(&source, &binding, &under_policy, 4, &under_run).unwrap();
            assert_eq!(
                under.file(&wrong_first).unwrap_err().code(),
                "invalid_reference"
            );
            assert_eq!(
                under.file(&wrong_second).unwrap_err().code(),
                "resource_limit"
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

/// Reconstruct contextual graph facts from captured bytes only. No graph is written.
pub(super) fn reconstruct(
    bytes: &[u8],
    path: &RelativePath,
    repository: &crate::architecture::Text,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<crate::syntax_materializer::BuiltGraphRows, crate::extraction_control::ExtractionError>
{
    use crate::extraction_control::ExtractionError;
    control.check()?;
    if !path.as_str().ends_with(".rs") {
        return Err(ExtractionError::new(
            "reconstruction_unavailable",
            "unsupported source language",
        ));
    }
    std::str::from_utf8(bytes)
        .map_err(|_| ExtractionError::new("reconstruction_unavailable", "source is not UTF-8"))?;
    let profile = crate::profiles::built_in_profiles()
        .into_iter()
        .find(|profile| profile.language == "rust")
        .ok_or_else(|| {
            ExtractionError::new(
                "architecture_internal_error",
                "built-in Rust profile missing",
            )
        })?;
    let tree = crate::parser::parse_guarded_tree(bytes, &profile, control)?;
    let flat = crate::parser::normalize_guarded_rust(&tree, bytes, &profile, control)?;
    let meta = std::collections::BTreeMap::from([
        (
            "path".to_string(),
            crate::extraction_control::copy_text(path.as_str(), control)?,
        ),
        ("language".to_string(), "rust".to_string()),
        ("grammar_version".to_string(), profile.grammar_version),
        ("source_root".to_string(), String::new()),
        (
            "repository_label".to_string(),
            crate::extraction_control::copy_text(repository.as_str(), control)?,
        ),
    ]);
    crate::syntax_materializer::build_flat_syntax_rows(meta, &flat, control)
}

pub(super) fn exact_node<'a>(
    rows: &'a crate::syntax_materializer::BuiltGraphRows,
    reference: &SourceRef,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<
    Option<&'a crate::syntax_materializer::GraphNodeRow>,
    crate::extraction_control::ExtractionError,
> {
    for row in &rows.nodes {
        control.visit(0)?;
        if row.id == reference.node_id.as_str()
            && row.table == reference.node_kind.as_str()
            && row.qualified_name == reference.symbol.as_str()
            && row.path == reference.path.as_str()
            && row.byte_start.and_then(|v| u64::try_from(v).ok())
                == Some(reference.byte_span.start())
            && row.byte_end.and_then(|v| u64::try_from(v).ok()) == Some(reference.byte_span.end())
        {
            return Ok(Some(row));
        }
    }
    Ok(None)
}
pub(super) fn direct_scope_contains(
    rows: &crate::syntax_materializer::BuiltGraphRows,
    scope: &SourceRef,
    member: &SourceRef,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<bool, crate::extraction_control::ExtractionError> {
    if scope.node_kind.as_str() != "Scope"
        || exact_node(rows, scope, control)?.is_none()
        || exact_node(rows, member, control)?.is_none()
    {
        return Ok(false);
    }
    for edge in &rows.edges {
        control.visit(0)?;
        if edge.edge_type == "Contains"
            && edge.source_id == scope.node_id.as_str()
            && edge.target_id == member.node_id.as_str()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod reconstruction_tests {
    use super::*;
    use crate::{
        architecture::{ByteSpan, GitObjectId, Text},
        extraction_control::ExtractionControl,
    };
    use sha2::{Digest as _, Sha256};
    #[test]
    fn exact_independent_literal_node_scope_and_mismatches() {
        let bytes = b"fn hello() {}";
        let path = RelativePath::new("src/lib.rs").unwrap();
        let repo = Text::new("repo").unwrap();
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 100, 100_000, 100_000, 100_000);
        let rows = reconstruct(bytes, &path, &repo, &mut control).unwrap();
        let reference = SourceRef {
            repository: repo,
            commit: GitObjectId::new("1".repeat(40)).unwrap(),
            path,
            node_kind: Text::new("Function").unwrap(),
            node_id: Text::new("Function:216f9df7cf9055f46285").unwrap(),
            symbol: Text::new("src.lib.hello").unwrap(),
            byte_span: ByteSpan::new(0, 13).unwrap(),
            file_digest: Digest::new(format!("sha256:{:x}", Sha256::digest(bytes))).unwrap(),
        };
        assert!(exact_node(&rows, &reference, &mut control)
            .unwrap()
            .is_some());
        let mut scope = reference.clone();
        scope.node_kind = Text::new("Scope").unwrap();
        scope.node_id = Text::new("Scope:41046104907937f3c6e1").unwrap();
        scope.symbol = Text::new("src.lib.<scope>").unwrap();
        assert!(direct_scope_contains(&rows, &scope, &reference, &mut control).unwrap());
        let mut wrong = reference.clone();
        wrong.byte_span = ByteSpan::new(1, 13).unwrap();
        assert!(exact_node(&rows, &wrong, &mut control).unwrap().is_none());
        wrong = reference.clone();
        wrong.symbol = Text::new("hello").unwrap();
        assert!(exact_node(&rows, &wrong, &mut control).unwrap().is_none());
        wrong = reference.clone();
        wrong.node_kind = Text::new("Symbol").unwrap();
        assert!(exact_node(&rows, &wrong, &mut control).unwrap().is_none());
        assert!(!direct_scope_contains(&rows, &reference, &scope, &mut control).unwrap());
    }
}

#[cfg(test)]
mod deep_reconstruction_tests {
    use super::*;
    use crate::{architecture::Text, extraction_control::ExtractionControl};
    #[test]
    fn deep_flat_reconstruction_drops_without_recursive_owned_nodes() {
        let source = format!("fn f() {{ {} 0 {} }}", "(".repeat(500), ")".repeat(500));
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 2_000, 20_000_000, 100_000, 100_000);
        let rows = reconstruct(
            source.as_bytes(),
            &RelativePath::new("src/lib.rs").unwrap(),
            &Text::new("repo").unwrap(),
            &mut control,
        )
        .unwrap();
        assert!(!rows.nodes.is_empty());
        drop(rows);
    }
}

struct ReconstructedFile<'a> {
    path: &'a RelativePath,
    digest: &'a Digest,
    rows: crate::syntax_materializer::BuiltGraphRows,
}
/// Invocation-local contextual rows; source bytes are never installed or stored.
pub(super) struct ReconstructionCache<'a> {
    files: Vec<ReconstructedFile<'a>>,
}
impl<'a> ReconstructionCache<'a> {
    pub(super) fn new() -> Self {
        Self { files: Vec::new() }
    }
    pub(super) fn check_reference(
        &mut self,
        reference: &'a SourceRef,
        source: &SourceProvenance,
        capture: &mut Capture<'a>,
        extractor_matches: bool,
        control: &mut crate::extraction_control::ExtractionControl<'_>,
    ) -> Result<super::output::EvidenceCheck, super::EvidenceError> {
        use super::output::{CheckStatus, EvidenceCheck};
        fn check(status: CheckStatus, code: Option<&str>) -> EvidenceCheck {
            EvidenceCheck {
                status,
                reason_codes: code.into_iter().map(str::to_owned).collect(),
            }
        }
        control.visit(0)?;
        if reference.repository != source.repository || reference.commit != source.evaluated_commit
        {
            return Ok(check(
                CheckStatus::Invalid,
                Some("current_revision_mismatch"),
            ));
        }
        let bytes = match capture.file(reference) {
            Ok(bytes) => bytes,
            Err(error) => match error.code() {
                "architecture_input_unavailable" => {
                    return Ok(check(
                        CheckStatus::Unknown,
                        Some("source_input_unavailable"),
                    ))
                }
                "invalid_reference" | "path_not_confined" => {
                    return Ok(check(
                        CheckStatus::Invalid,
                        Some("invalid_source_reference"),
                    ))
                }
                _ => return Err(error),
            },
        };
        if !extractor_matches {
            return Ok(check(CheckStatus::Unknown, Some("extractor_mismatch")));
        }
        let mut existing = None;
        for (index, file) in self.files.iter().enumerate() {
            control.visit(0)?;
            if file.path == &reference.path && file.digest == &reference.file_digest {
                existing = Some(index);
                break;
            }
        }
        let index = if let Some(index) = existing {
            index
        } else {
            let rows = match reconstruct(bytes, &reference.path, &reference.repository, control) {
                Ok(rows) => rows,
                Err(error) if error.code == "reconstruction_unavailable" => {
                    return Ok(check(
                        CheckStatus::Unknown,
                        Some("reconstruction_unavailable"),
                    ))
                }
                Err(error) => return Err(error.into()),
            };
            self.files
                .try_reserve(1)
                .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
            self.files.push(ReconstructedFile {
                path: &reference.path,
                digest: &reference.file_digest,
                rows,
            });
            self.files.len() - 1
        };
        if exact_node(&self.files[index].rows, reference, control)?.is_some() {
            Ok(check(CheckStatus::Valid, None))
        } else {
            Ok(check(CheckStatus::Invalid, Some("source_node_mismatch")))
        }
    }
    pub(super) fn contains(
        &self,
        scope: &SourceRef,
        member: &SourceRef,
        control: &mut crate::extraction_control::ExtractionControl<'_>,
    ) -> Result<bool, super::EvidenceError> {
        for file in &self.files {
            control.visit(0)?;
            if file.path == &scope.path && file.digest == &scope.file_digest {
                return Ok(direct_scope_contains(&file.rows, scope, member, control)?);
            }
        }
        Ok(false)
    }
}
