//! Untrusted mapping claims and their canonical identity.
mod identity;
mod input;
mod output;
mod source;
pub use identity::{
    mapping_facts_digest, source_extractor_identity, MappingFactsDigest, SourceExtractorDescriptor,
    SourceExtractorIdentity,
};
pub use input::*;
pub use output::{
    CheckStatus, CitationCheck, EvidenceCheck, EvidenceDiagnostic, EvidenceRecord,
    EvidenceValidation,
};

/// Stable error and diagnostic codes emitted by evidence validation.
///
/// `INVALID_*`, `CURRENT_REVISION_MISMATCH`, `SOURCE_NODE_MISMATCH`, and
/// `INFERENCE_VERSION_MISMATCH` identify invalid claims. `*_UNAVAILABLE`,
/// `EXTRACTOR_MISMATCH`, `HISTORY_UNQUALIFIED`, `PREDICATE_UNQUALIFIED`,
/// `OWNERSHIP_UNQUALIFIED`, `BUILD_CONTEXT_UNQUALIFIED`, `INSUFFICIENT_EVIDENCE`,
/// and `UNSUPPORTED_EVIDENCE`
/// identify facts or qualifications that remain unknown. `RESOURCE_LIMIT`,
/// `CANCELLED`, `DEADLINE_EXCEEDED`, and configuration errors stop the
/// operation without producing a complete successful result.
pub mod codes {
    pub const INVALID_INPUT: &str = "architecture_invalid_input";
    pub const INVALID_REFERENCE: &str = "invalid_reference";
    pub const INPUT_UNAVAILABLE: &str = "architecture_input_unavailable";
    pub const CONFIGURATION_REQUIRED: &str = "architecture_configuration_required";
    pub const RESOURCE_LIMIT: &str = "resource_limit";
    pub const CANCELLED: &str = "cancelled";
    pub const DEADLINE_EXCEEDED: &str = "deadline_exceeded";
    pub const INVALID_EVALUATED_TREE: &str = "invalid_evaluated_tree";
    pub const CURRENT_REVISION_MISMATCH: &str = "current_revision_mismatch";
    pub const INVALID_SOURCE_REFERENCE: &str = "invalid_source_reference";
    pub const SOURCE_NODE_MISMATCH: &str = "source_node_mismatch";
    pub const SOURCE_INPUT_UNAVAILABLE: &str = "source_input_unavailable";
    pub const EXTRACTOR_MISMATCH: &str = "extractor_mismatch";
    pub const RECONSTRUCTION_UNAVAILABLE: &str = "reconstruction_unavailable";
    pub const INVALID_RESPONSIBILITIES: &str = "invalid_responsibilities";
    pub const INVALID_RATIONALE: &str = "invalid_rationale";
    pub const INFERENCE_VERSION_MISMATCH: &str = "inference_version_mismatch";
    pub const INVALID_HISTORY: &str = "invalid_history";
    pub const INVALID_LINEAGE_ENDPOINTS: &str = "invalid_lineage_endpoints";
    pub const HISTORY_UNQUALIFIED: &str = "history_unqualified";
    pub const INVALID_SCOPE_ENDPOINTS: &str = "invalid_scope_endpoints";
    pub const SCOPE_UNAVAILABLE: &str = "scope_unavailable";
    pub const SCOPE_NOT_DIRECTLY_CONTAINS: &str = "scope_not_directly_contains";
    pub const PREDICATE_UNQUALIFIED: &str = "predicate_unqualified";
    pub const BUILD_CONTEXT_UNQUALIFIED: &str = "build_context_unqualified";
    pub const OWNERSHIP_UNQUALIFIED: &str = "ownership_unqualified";
    pub const INSUFFICIENT_EVIDENCE: &str = "insufficient_evidence";
    pub const UNSUPPORTED_EVIDENCE: &str = "unsupported_evidence";
    pub const INVALID_EVIDENCE_DETECTED: &str = "invalid_evidence_detected";
    pub const QUALIFICATION_UNKNOWN: &str = "qualification_unknown";
}

/// An incomplete evidence operation. No successful validation is carried here.
#[derive(Debug)]
pub struct EvidenceError {
    code: &'static str,
    message: String,
    compiler_error: Option<super::compile::CompileError>,
    diagnostics: Vec<EvidenceDiagnostic>,
    known_invalid_evidence: bool,
    known_unknown: bool,
}
impl EvidenceError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            compiler_error: None,
            diagnostics: Vec::new(),
            known_invalid_evidence: false,
            known_unknown: false,
        }
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn compiler_diagnostics(&self) -> &[super::compile::CompileDiagnostic] {
        self.compiler_error
            .as_ref()
            .map_or(&[], |error| error.diagnostics())
    }
    pub fn diagnostics(&self) -> &[EvidenceDiagnostic] {
        &self.diagnostics
    }
    pub fn has_known_invalid_evidence(&self) -> bool {
        self.known_invalid_evidence
    }
    pub fn has_known_unknown(&self) -> bool {
        self.known_unknown
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for EvidenceError {}
impl From<super::compile::CompileError> for EvidenceError {
    fn from(value: super::compile::CompileError) -> Self {
        Self {
            code: value.code(),
            message: value.to_string(),
            compiler_error: Some(value),
            diagnostics: Vec::new(),
            known_invalid_evidence: false,
            known_unknown: false,
        }
    }
}

impl From<crate::extraction_control::ExtractionError> for EvidenceError {
    fn from(value: crate::extraction_control::ExtractionError) -> Self {
        Self::new(value.code, value.message)
    }
}

/// Validate contextual evidence without making an ownership or acceptance decision.
fn retain_known_diagnostics(error: &mut EvidenceError) {
    let needed = usize::from(error.known_invalid_evidence) + usize::from(error.known_unknown);
    if error.diagnostics.try_reserve(needed).is_err() {
        return;
    }
    if error.known_invalid_evidence {
        error.diagnostics.push(EvidenceDiagnostic {
            code: "invalid_evidence_detected".into(),
            mapping_index: None,
            citation_id: None,
            subject: None,
            message: "Invalid evidence was established before the operation stopped".into(),
        });
    }
    if error.known_unknown {
        error.diagnostics.push(EvidenceDiagnostic {
            code: "qualification_unknown".into(),
            mapping_index: None,
            citation_id: None,
            subject: None,
            message: "Evidence qualification remains unknown".into(),
        });
    }
}

pub fn validate_evidence(
    model: &super::compile::CompiledModel,
    source: &super::SourceProvenance,
    mappings: &[MappingSubmission],
    repositories: &[super::compile::RepositoryBinding],
    context: &super::compile::ExecutionContext,
) -> Result<EvidenceValidation, EvidenceError> {
    use super::compile::evidence_support as support;
    let (policy, run) = support::start(model.effective_policy(), None, context)?;
    let mut checkpoint = || {
        run.check().map_err(|error| {
            crate::extraction_control::ExtractionError::new(
                error.code(),
                "evidence operation interrupted",
            )
        })
    };
    let mut control = crate::extraction_control::ExtractionControl::new(
        &mut checkpoint,
        policy.max_parse_depth(),
        policy.max_traversal_steps(),
        policy.max_projected_vertices(),
        policy.max_projected_edges(),
    );
    let mut diagnostics = Vec::new();
    let mut known_invalid = false;
    let result = output::validate_in_run(
        model,
        source,
        mappings,
        repositories,
        &run,
        &mut control,
        output::ValidationProgress {
            diagnostics: &mut diagnostics,
            known_invalid: &mut known_invalid,
        },
    );
    let mut result = match result {
        Ok(value) => value,
        Err(mut error) => {
            error.diagnostics = diagnostics;
            error.known_invalid_evidence = known_invalid;
            error.known_unknown = true;
            retain_known_diagnostics(&mut error);
            return Err(error);
        }
    };
    match output::canonical_bytes(&result, &run, &mut control) {
        Ok(bytes) => {
            result.canonical_json = bytes;
            Ok(result)
        }
        Err(mut error) => {
            error.diagnostics = std::mem::take(&mut result.diagnostics);
            error.known_invalid_evidence = result.has_invalid_evidence;
            error.known_unknown = result.has_unknown;
            retain_known_diagnostics(&mut error);
            Err(error)
        }
    }
}
