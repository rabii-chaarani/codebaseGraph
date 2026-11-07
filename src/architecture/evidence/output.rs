//! Logical evidence results. Checks report evidence quality, not ownership.
use super::{MappingFactsDigest, MappingSubmission, SourceExtractorIdentity};
use crate::architecture::{compile::ArchitectureExecutionPolicy, Digest, Id, SourceProvenance};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Valid,
    Invalid,
    Unknown,
}
#[derive(Clone, Debug, Serialize)]
pub struct EvidenceCheck {
    pub(super) status: CheckStatus,
    pub(super) reason_codes: Vec<String>,
}
impl EvidenceCheck {
    pub fn status(&self) -> CheckStatus {
        self.status
    }
    pub fn reason_codes(&self) -> &[String] {
        &self.reason_codes
    }
}
#[derive(Debug, Serialize)]
pub struct CitationCheck {
    pub(super) id: Id,
    pub(super) source_checks: Vec<EvidenceCheck>,
    pub(super) assertion_check: EvidenceCheck,
}
impl CitationCheck {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn source_checks(&self) -> &[EvidenceCheck] {
        &self.source_checks
    }
    pub fn assertion_check(&self) -> &EvidenceCheck {
        &self.assertion_check
    }
}
#[derive(Debug, Serialize)]
pub struct EvidenceRecord {
    pub(super) submission: MappingSubmission,
    pub(super) competing_components: Vec<Id>,
    pub(super) source_check: EvidenceCheck,
    pub(super) responsibility_check: EvidenceCheck,
    pub(super) rationale_check: EvidenceCheck,
    pub(super) inference_check: EvidenceCheck,
    pub(super) history_check: EvidenceCheck,
    pub(super) citations: Vec<CitationCheck>,
    pub(super) ownership_qualification: &'static str,
    pub(super) reason_codes: Vec<String>,
}
impl EvidenceRecord {
    pub fn submission(&self) -> &MappingSubmission {
        &self.submission
    }
    pub fn competing_components(&self) -> &[Id] {
        &self.competing_components
    }
    pub fn source_check(&self) -> &EvidenceCheck {
        &self.source_check
    }
    pub fn responsibility_check(&self) -> &EvidenceCheck {
        &self.responsibility_check
    }
    pub fn rationale_check(&self) -> &EvidenceCheck {
        &self.rationale_check
    }
    pub fn inference_check(&self) -> &EvidenceCheck {
        &self.inference_check
    }
    pub fn history_check(&self) -> &EvidenceCheck {
        &self.history_check
    }
    pub fn citations(&self) -> &[CitationCheck] {
        &self.citations
    }
    pub fn ownership_qualification(&self) -> &str {
        self.ownership_qualification
    }
    pub fn reason_codes(&self) -> &[String] {
        &self.reason_codes
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct EvidenceDiagnostic {
    pub(super) code: String,
    pub(super) mapping_index: Option<u64>,
    pub(super) citation_id: Option<Id>,
    pub(super) subject: Option<String>,
    pub(super) message: String,
}
impl EvidenceDiagnostic {
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn mapping_index(&self) -> Option<u64> {
        self.mapping_index
    }
    pub fn citation_id(&self) -> Option<&Id> {
        self.citation_id.as_ref()
    }
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
#[derive(Debug, Serialize)]
pub struct EvidenceValidation {
    pub(super) evidence_version: &'static str,
    pub(super) mapping_facts_digest: MappingFactsDigest,
    pub(super) source: SourceProvenance,
    pub(super) model_id: Id,
    pub(super) model_compile_input_digest: Digest,
    pub(super) extractor_identity: SourceExtractorIdentity,
    pub(super) effective_policy: ArchitectureExecutionPolicy,
    pub(super) records: Vec<EvidenceRecord>,
    pub(super) diagnostics: Vec<EvidenceDiagnostic>,
    pub(super) has_invalid_evidence: bool,
    pub(super) has_unknown: bool,
    #[serde(skip)]
    pub(super) canonical_json: Vec<u8>,
}
impl EvidenceValidation {
    pub fn evidence_version(&self) -> &str {
        self.evidence_version
    }
    pub fn mapping_facts_digest(&self) -> &MappingFactsDigest {
        &self.mapping_facts_digest
    }
    pub fn source(&self) -> &SourceProvenance {
        &self.source
    }
    pub fn model_id(&self) -> &Id {
        &self.model_id
    }
    pub fn model_compile_input_digest(&self) -> &Digest {
        &self.model_compile_input_digest
    }
    pub fn extractor_identity(&self) -> &SourceExtractorIdentity {
        &self.extractor_identity
    }
    pub fn effective_policy(&self) -> &ArchitectureExecutionPolicy {
        &self.effective_policy
    }
    pub fn records(&self) -> &[EvidenceRecord] {
        &self.records
    }
    pub fn diagnostics(&self) -> &[EvidenceDiagnostic] {
        &self.diagnostics
    }
    pub fn has_invalid_evidence(&self) -> bool {
        self.has_invalid_evidence
    }
    pub fn has_unknown(&self) -> bool {
        self.has_unknown
    }
    pub fn canonical_json(&self) -> &[u8] {
        &self.canonical_json
    }
}

/// Evaluate model references and attribution without asserting ownership.
pub(super) fn model_checks(
    model: &crate::architecture::compile::CompiledModel,
    submission: &MappingSubmission,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
    known_invalid: &mut bool,
    diagnostics: &mut Vec<EvidenceDiagnostic>,
    index: u64,
) -> Result<
    (
        Vec<Id>,
        EvidenceCheck,
        EvidenceCheck,
        EvidenceCheck,
        EvidenceCheck,
    ),
    super::EvidenceError,
> {
    use crate::extraction_control::{copy_text, sort_by};
    fn check(status: CheckStatus, reason: Option<&str>) -> EvidenceCheck {
        EvidenceCheck {
            status,
            reason_codes: reason.into_iter().map(str::to_owned).collect(),
        }
    }
    let mut competitors = Vec::new();
    let mut owner = None;
    for document in model.architecture() {
        control.visit(0)?;
        for component in &document.components {
            control.visit(0)?;
            if component.id == submission.proposed_component {
                owner = Some(component);
            } else {
                competitors.try_reserve(1).map_err(|_| {
                    super::EvidenceError::new("resource_limit", "allocation failed")
                })?;
                competitors.push(
                    Id::new(copy_text(component.id.as_str(), control)?)
                        .map_err(|e| super::EvidenceError::new("architecture_internal_error", e))?,
                );
            }
        }
    }
    if owner.is_none() || submission.responsibility_ids.is_empty() {
        *known_invalid = true;
    }
    sort_by(&mut competitors, control, |a, b| a.as_str().cmp(b.as_str()))?;
    let mut responsibilities_valid = owner.is_some() && !submission.responsibility_ids.is_empty();
    if let Some(owner) = owner {
        for id in &submission.responsibility_ids {
            control.visit(0)?;
            let mut declared = false;
            for responsibility in &owner.responsibilities {
                control.visit(0)?;
                if responsibility.id == *id {
                    declared = true;
                    break;
                }
            }
            if !declared {
                *known_invalid = true;
            }
            responsibilities_valid &= declared;
        }
    }
    if !responsibilities_valid {
        *known_invalid = true;
    }
    let responsibility_check = if responsibilities_valid {
        check(CheckStatus::Valid, None)
    } else {
        check(CheckStatus::Invalid, Some("invalid_responsibilities"))
    };
    record_diagnostic(
        diagnostics,
        &responsibility_check,
        Some(index),
        None,
        control,
    )?;
    let mut rationale_valid = owner.is_some()
        && submission.rationale.responsibilities.len() == submission.responsibility_ids.len()
        && submission.rationale.competing_components.len() == competitors.len();
    if !rationale_valid {
        *known_invalid = true;
    }
    if rationale_valid {
        for (explanation, id) in submission
            .rationale
            .responsibilities
            .iter()
            .zip(&submission.responsibility_ids)
        {
            control.visit(0)?;
            if explanation.responsibility_id != *id {
                rationale_valid = false;
                *known_invalid = true;
            }
        }
        for (explanation, id) in submission
            .rationale
            .competing_components
            .iter()
            .zip(&competitors)
        {
            control.visit(0)?;
            if explanation.component != *id {
                rationale_valid = false;
                *known_invalid = true;
            }
        }
    }
    if !rationale_valid {
        *known_invalid = true;
    }
    let rationale_check = if rationale_valid {
        check(CheckStatus::Valid, None)
    } else {
        check(CheckStatus::Invalid, Some("invalid_rationale"))
    };
    record_diagnostic(diagnostics, &rationale_check, Some(index), None, control)?;
    if submission.inference_provenance.rule_version != submission.discriminator {
        *known_invalid = true;
    }
    let inference_check =
        if submission.inference_provenance.rule_version == submission.discriminator {
            check(CheckStatus::Valid, None)
        } else {
            check(CheckStatus::Invalid, Some("inference_version_mismatch"))
        };
    record_diagnostic(diagnostics, &inference_check, Some(index), None, control)?;
    let history_check = match (&submission.change_kind, &submission.previous_mapping) {
        (super::ChangeKind::Initial, None) => check(CheckStatus::Valid, None),
        (super::ChangeKind::Initial, Some(_)) | (_, None) => {
            check(CheckStatus::Invalid, Some("invalid_history"))
        }
        (_, Some(previous)) => {
            if let Some(lineage) = &previous.lineage {
                if lineage.previous != previous.source_ref
                    || lineage.current != submission.source_ref
                {
                    check(CheckStatus::Invalid, Some("invalid_lineage_endpoints"))
                } else {
                    check(CheckStatus::Unknown, Some("history_unqualified"))
                }
            } else {
                check(CheckStatus::Unknown, Some("history_unqualified"))
            }
        }
    };
    if history_check.status == CheckStatus::Invalid {
        *known_invalid = true;
    }
    record_diagnostic(diagnostics, &history_check, Some(index), None, control)?;
    Ok((
        competitors,
        responsibility_check,
        rationale_check,
        inference_check,
        history_check,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        architecture::{
            compile::{compile, ExecutionContext, ModelInput},
            RelativePath,
        },
        extraction_control::ExtractionControl,
    };
    use std::sync::{atomic::AtomicBool, Arc};
    #[test]
    fn model_checks_require_exact_responsibilities_competitors_and_history() {
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
        let policy = ArchitectureExecutionPolicy::from_json(serde_json::Value::Object(
            fields
                .into_iter()
                .map(|v| (v.into(), 100_000.into()))
                .collect(),
        ))
        .unwrap();
        let root =
            b"schema_version: '0.1'\nmodel_id: model\narchitecture: [arch]\nconstraints: []\n";
        let arch=br#"{"schema_version":"0.1","systems":[{"id":"sys","name":"System","responsibilities":[{"id":"sys-task","statement":"System task"}]}],"containers":[{"id":"container","name":"Container","responsibilities":[{"id":"container-task","statement":"Container task"}],"system":"sys","kind":"application"}],"components":[{"id":"alpha","name":"Alpha","responsibilities":[{"id":"task","statement":"Task"}],"container":"container"},{"id":"beta","name":"Beta","responsibilities":[{"id":"other","statement":"Other"}],"container":"container"}],"relationships":[]}"#;
        let p = |s| RelativePath::new(s).unwrap();
        let input = ModelInput::inline_files(
            p("root"),
            [(p("root"), root.to_vec()), (p("arch"), arch.to_vec())],
        )
        .unwrap();
        let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
        let model = compile(input, None, &policy, None, &context).unwrap();
        let digest = format!("sha256:{}", "a".repeat(64));
        let source = serde_json::json!({"repository":"repo","commit":"1".repeat(40),"path":"src/lib.rs","node_kind":"Function","node_id":"node","symbol":"name","byte_span":{"start":0,"end":1},"file_digest":digest});
        let raw = serde_json::json!({"source_ref":source,"proposed_component":"alpha","responsibility_ids":["task"],"structural_citations":[],"rationale":{"responsibilities":[{"responsibility_id":"task","explanation":"reason"}],"competing_components":[{"component":"beta","explanation":"reason"}]},"inference_provenance":{"agent_id":"agent","model_id":"model","input_digest":digest,"rule_version":"responsibility-evidence/0.1","result_digest":digest},"previous_mapping":null,"change_kind":"initial","discriminator":"responsibility-evidence/0.1"});
        let mut submission: MappingSubmission = serde_json::from_value(raw).unwrap();
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 1000, 100_000, 1000, 1000);
        let (competitors, r, rationale, inference, history) = model_checks(
            &model,
            &submission,
            &mut control,
            &mut false,
            &mut Vec::new(),
            0,
        )
        .unwrap();
        assert_eq!(competitors[0].as_str(), "beta");
        assert_eq!(r.status(), CheckStatus::Valid);
        assert_eq!(rationale.status(), CheckStatus::Valid);
        assert_eq!(inference.status(), CheckStatus::Valid);
        assert_eq!(history.status(), CheckStatus::Valid);
        submission.rationale.competing_components.clear();
        submission.inference_provenance.rule_version =
            crate::architecture::Text::new("other").unwrap();
        submission.change_kind = super::super::ChangeKind::Retained;
        let (_, _, rationale, inference, history) = model_checks(
            &model,
            &submission,
            &mut control,
            &mut false,
            &mut Vec::new(),
            0,
        )
        .unwrap();
        assert_eq!(rationale.status(), CheckStatus::Invalid);
        assert_eq!(inference.status(), CheckStatus::Invalid);
        assert_eq!(history.status(), CheckStatus::Invalid);
    }
}

fn status_check(status: CheckStatus, code: Option<&'static str>) -> EvidenceCheck {
    EvidenceCheck {
        status,
        reason_codes: code.into_iter().map(str::to_owned).collect(),
    }
}
fn record_diagnostic(
    diagnostics: &mut Vec<EvidenceDiagnostic>,
    check: &EvidenceCheck,
    index: Option<u64>,
    citation: Option<&Id>,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<(), super::EvidenceError> {
    for code in &check.reason_codes {
        control.visit(0)?;
        diagnostics
            .try_reserve(1)
            .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
        let citation_id = if let Some(id) = citation {
            Some(
                Id::new(crate::extraction_control::copy_text(id.as_str(), control)?)
                    .map_err(|e| super::EvidenceError::new("architecture_internal_error", e))?,
            )
        } else {
            None
        };
        diagnostics.push(EvidenceDiagnostic {
            code: crate::extraction_control::copy_text(code, control)?,
            mapping_index: index,
            citation_id,
            subject: None,
            message: crate::extraction_control::copy_text(code, control)?,
        });
    }
    Ok(())
}
fn observe_check(
    check: &EvidenceCheck,
    index: Option<u64>,
    citation: Option<&Id>,
    diagnostics: &mut Vec<EvidenceDiagnostic>,
    known_invalid: &mut bool,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<(), super::EvidenceError> {
    if check.status == CheckStatus::Invalid {
        *known_invalid = true;
    }
    record_diagnostic(diagnostics, check, index, citation, control)
}
fn source_check<'a>(
    reference: &'a crate::architecture::SourceRef,
    source: &SourceProvenance,
    capture: &mut Option<super::source::Capture<'a>>,
    cache: &mut super::source::ReconstructionCache<'a>,
    extractor_matches: bool,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<EvidenceCheck, super::EvidenceError> {
    control.visit(0)?;
    if reference.repository != source.repository || reference.commit != source.evaluated_commit {
        return Ok(status_check(
            CheckStatus::Invalid,
            Some("current_revision_mismatch"),
        ));
    }
    match capture {
        Some(capture) => {
            cache.check_reference(reference, source, capture, extractor_matches, control)
        }
        None => Ok(status_check(
            CheckStatus::Unknown,
            Some("source_input_unavailable"),
        )),
    }
}

pub(super) struct ValidationProgress<'a> {
    pub(super) diagnostics: &'a mut Vec<EvidenceDiagnostic>,
    pub(super) known_invalid: &'a mut bool,
}

pub(super) fn validate_in_run<'a>(
    model: &crate::architecture::compile::CompiledModel,
    source: &'a SourceProvenance,
    mappings: &'a [MappingSubmission],
    repositories: &'a [crate::architecture::compile::RepositoryBinding],
    run: &crate::architecture::compile::evidence_support::Run,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
    progress: ValidationProgress<'_>,
) -> Result<EvidenceValidation, super::EvidenceError> {
    let ValidationProgress {
        diagnostics,
        known_invalid,
    } = progress;
    use super::{identity, source as source_ops, Assertion};
    use crate::architecture::compile::evidence_support as support;
    use crate::extraction_control::sort_by;
    let policy = model.effective_policy();
    let (mapping_facts_digest, payload_bytes) =
        identity::facts(source, mappings, policy, run, control)?;
    // The declared build digest is retained, but no Q1 provider is available.
    record_diagnostic(
        diagnostics,
        &status_check(CheckStatus::Unknown, Some("build_context_unqualified")),
        None,
        None,
        control,
    )?;
    let extractor = identity::extractor_in_run(run)?;
    let extractor_matches = extractor.digest() == &source.extractor_digest;
    if !extractor_matches {
        record_diagnostic(
            diagnostics,
            &status_check(CheckStatus::Unknown, Some("extractor_mismatch")),
            None,
            None,
            control,
        )?;
    }
    let mut binding = None;
    for candidate in repositories {
        control.visit(0)?;
        if candidate.identity() == &source.repository {
            if binding.is_some() {
                return Err(super::EvidenceError::new(
                    "architecture_configuration_required",
                    "duplicate repository binding",
                ));
            }
            binding = Some(candidate);
        }
    }
    let mut capture = if let Some(binding) = binding {
        match source_ops::Capture::begin(source, binding, policy, payload_bytes, run) {
            Ok(capture) => Some(capture),
            Err(error) if error.code() == "architecture_input_unavailable" => {
                record_diagnostic(
                    diagnostics,
                    &status_check(CheckStatus::Unknown, Some("source_input_unavailable")),
                    None,
                    None,
                    control,
                )?;
                None
            }
            Err(error)
                if error.code() == "invalid_reference" || error.code() == "path_not_confined" =>
            {
                *known_invalid = true;
                record_diagnostic(
                    diagnostics,
                    &status_check(CheckStatus::Invalid, Some("invalid_evaluated_tree")),
                    None,
                    None,
                    control,
                )?;
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        record_diagnostic(
            diagnostics,
            &status_check(CheckStatus::Unknown, Some("source_input_unavailable")),
            None,
            None,
            control,
        )?;
        None
    };
    let model_compile_input_digest = support::compile_input(model, run)?;
    let mut sorted = Vec::new();
    sorted
        .try_reserve(mappings.len())
        .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
    for item in mappings {
        control.visit(0)?;
        sorted.push(item);
    }
    sort_by(&mut sorted, control, |a, b| {
        (
            a.source_ref.repository.as_str(),
            a.source_ref.commit.as_str(),
            a.source_ref.node_kind.as_str(),
            a.source_ref.node_id.as_str(),
        )
            .cmp(&(
                b.source_ref.repository.as_str(),
                b.source_ref.commit.as_str(),
                b.source_ref.node_kind.as_str(),
                b.source_ref.node_id.as_str(),
            ))
    })?;
    let mut cache = source_ops::ReconstructionCache::new();
    let mut records = Vec::new();
    records
        .try_reserve(sorted.len())
        .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
    for (index, item) in sorted.into_iter().enumerate() {
        control.visit(0)?;
        let submission = super::input::copy_submission(item, control)?;
        let (competitors, responsibility_check, rationale_check, inference_check, history_check) =
            model_checks(
                model,
                &submission,
                control,
                known_invalid,
                diagnostics,
                index as u64,
            )?;
        let mapping_source_check = source_check(
            &item.source_ref,
            source,
            &mut capture,
            &mut cache,
            extractor_matches,
            control,
        )?;
        observe_check(
            &mapping_source_check,
            Some(index as u64),
            None,
            diagnostics,
            known_invalid,
            control,
        )?;
        let mut ordered = Vec::new();
        ordered
            .try_reserve(item.structural_citations.len())
            .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
        for citation in &item.structural_citations {
            control.visit(0)?;
            ordered.push(citation);
        }
        sort_by(&mut ordered, control, |a, b| {
            a.id.as_str().cmp(b.id.as_str())
        })?;
        let mut citations = Vec::new();
        citations
            .try_reserve(ordered.len())
            .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
        for citation in ordered {
            control.visit(0)?;
            let mut checks = Vec::new();
            checks
                .try_reserve(1)
                .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
            macro_rules! append_source_check {
                ($reference:expr) => {{
                    let check = source_check(
                        $reference,
                        source,
                        &mut capture,
                        &mut cache,
                        extractor_matches,
                        control,
                    )?;
                    observe_check(
                        &check,
                        Some(index as u64),
                        Some(&citation.id),
                        diagnostics,
                        known_invalid,
                        control,
                    )?;
                    checks.push(check);
                }};
            }
            append_source_check!(&citation.source_ref);
            match &citation.assertion {
                Assertion::ScopeContains { member } => {
                    checks.try_reserve(1).map_err(|_| {
                        super::EvidenceError::new("resource_limit", "allocation failed")
                    })?;
                    append_source_check!(member);
                }
                Assertion::ReachesExportedItem { path, .. } => {
                    checks.try_reserve(path.len()).map_err(|_| {
                        super::EvidenceError::new("resource_limit", "allocation failed")
                    })?;
                    for reference in path {
                        append_source_check!(reference);
                    }
                }
                _ => {}
            }
            let assertion_check = match &citation.assertion {
                Assertion::NodeExists => checks[0].clone(),
                Assertion::ScopeContains { member } => {
                    if checks
                        .iter()
                        .any(|check| check.status == CheckStatus::Invalid)
                    {
                        status_check(CheckStatus::Invalid, Some("invalid_scope_endpoints"))
                    } else if checks
                        .iter()
                        .any(|check| check.status == CheckStatus::Unknown)
                    {
                        status_check(CheckStatus::Unknown, Some("scope_unavailable"))
                    } else if cache.contains(&citation.source_ref, member, control)? {
                        status_check(CheckStatus::Valid, None)
                    } else {
                        status_check(CheckStatus::Invalid, Some("scope_not_directly_contains"))
                    }
                }
                _ => status_check(CheckStatus::Unknown, Some("predicate_unqualified")),
            };
            // node_exists repeats the source check verbatim; report it once.
            if !matches!(citation.assertion, Assertion::NodeExists) {
                observe_check(
                    &assertion_check,
                    Some(index as u64),
                    Some(&citation.id),
                    diagnostics,
                    known_invalid,
                    control,
                )?;
            }
            let id = Id::new(crate::extraction_control::copy_text(
                citation.id.as_str(),
                control,
            )?)
            .map_err(|e| super::EvidenceError::new("architecture_internal_error", e))?;
            citations.push(CitationCheck {
                id,
                source_checks: checks,
                assertion_check,
            });
        }
        let mut reasons = Vec::new();
        reasons
            .try_reserve(1)
            .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
        reasons.push("ownership_unqualified".to_string());
        if item.structural_citations.is_empty() {
            record_diagnostic(
                diagnostics,
                &status_check(CheckStatus::Unknown, Some("insufficient_evidence")),
                Some(index as u64),
                None,
                control,
            )?;
            reasons
                .try_reserve(1)
                .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
            reasons.push("insufficient_evidence".to_string());
        }
        if item.discriminator.as_str() != "responsibility-evidence/0.1" {
            record_diagnostic(
                diagnostics,
                &status_check(CheckStatus::Unknown, Some("unsupported_evidence")),
                Some(index as u64),
                None,
                control,
            )?;
            reasons
                .try_reserve(1)
                .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
            reasons.push("unsupported_evidence".to_string());
        }
        for check in [
            &mapping_source_check,
            &responsibility_check,
            &rationale_check,
            &inference_check,
            &history_check,
        ] {
            for reason in &check.reason_codes {
                control.visit(0)?;
                reasons.try_reserve(1).map_err(|_| {
                    super::EvidenceError::new("resource_limit", "allocation failed")
                })?;
                reasons.push(crate::extraction_control::copy_text(reason, control)?);
            }
        }
        for citation in &citations {
            for check in citation
                .source_checks
                .iter()
                .chain(std::iter::once(&citation.assertion_check))
            {
                for reason in &check.reason_codes {
                    control.visit(0)?;
                    reasons.try_reserve(1).map_err(|_| {
                        super::EvidenceError::new("resource_limit", "allocation failed")
                    })?;
                    reasons.push(crate::extraction_control::copy_text(reason, control)?);
                }
            }
        }
        sort_by(&mut reasons, control, |a, b| a.cmp(b))?;
        let mut unique_reasons = Vec::new();
        unique_reasons
            .try_reserve(reasons.len())
            .map_err(|_| super::EvidenceError::new("resource_limit", "allocation failed"))?;
        for reason in reasons {
            control.visit(0)?;
            if unique_reasons.last() != Some(&reason) {
                unique_reasons.push(reason);
            }
        }
        records.push(EvidenceRecord {
            submission,
            competing_components: competitors,
            source_check: mapping_source_check,
            responsibility_check,
            rationale_check,
            inference_check,
            history_check,
            citations,
            ownership_qualification: "unknown",
            reason_codes: unique_reasons,
        });
    }
    sort_by(diagnostics, control, |a, b| {
        (
            a.mapping_index,
            a.citation_id.as_ref().map(Id::as_str),
            a.code.as_str(),
            a.subject.as_deref(),
            a.message.as_str(),
        )
            .cmp(&(
                b.mapping_index,
                b.citation_id.as_ref().map(Id::as_str),
                b.code.as_str(),
                b.subject.as_deref(),
                b.message.as_str(),
            ))
    })?;
    let source = SourceProvenance {
        repository: crate::architecture::Text::new(crate::extraction_control::copy_text(
            source.repository.as_str(),
            control,
        )?)
        .map_err(|e| super::EvidenceError::new("architecture_internal_error", e))?,
        evaluated_commit: source.evaluated_commit.clone(),
        evaluated_tree: source.evaluated_tree.clone(),
        extractor_digest: source.extractor_digest.clone(),
        build_context_digest: source.build_context_digest.clone(),
    };
    Ok(EvidenceValidation {
        evidence_version: "architecture-evidence/0.1",
        mapping_facts_digest,
        source,
        model_id: Id::new(crate::extraction_control::copy_text(
            model.root().model_id.as_str(),
            control,
        )?)
        .map_err(|e| super::EvidenceError::new("architecture_internal_error", e))?,
        model_compile_input_digest,
        extractor_identity: extractor,
        effective_policy: policy.clone(),
        records,
        diagnostics: std::mem::take(diagnostics),
        has_invalid_evidence: *known_invalid,
        has_unknown: true,
        canonical_json: Vec::new(),
    })
}

// Canonical logical output is emitted from borrowed records. Fixed-size policy
// and extractor metadata are sorted separately; submitted arrays never become
// a second owned JSON tree.
struct CanonicalSubmission<'a>(&'a MappingSubmission);
struct CanonicalCitation<'a>(&'a super::StructuralCitation);
struct CanonicalCitations<'a>(&'a [super::StructuralCitation]);
struct CanonicalRationale<'a>(&'a super::Rationale);
struct CanonicalResponsibilities<'a>(&'a [super::ResponsibilityExplanation]);
struct CanonicalResponsibility<'a>(&'a super::ResponsibilityExplanation);
struct CanonicalCompetitors<'a>(&'a [super::ComponentExplanation]);
struct CanonicalCompetitor<'a>(&'a super::ComponentExplanation);
struct CanonicalInference<'a>(&'a super::InferenceProvenance);
struct CanonicalPrevious<'a>(&'a super::PreviousMapping);
struct CanonicalLineage<'a>(&'a super::LineageRecord);
struct CanonicalArtifacts<'a>(&'a [crate::architecture::ArtifactRef]);
struct CanonicalCheck<'a>(&'a EvidenceCheck);
struct CanonicalChecks<'a>(&'a [EvidenceCheck]);
struct CanonicalCitationCheck<'a>(&'a CitationCheck);
struct CanonicalCitationChecks<'a>(&'a [CitationCheck]);
struct CanonicalRecord<'a>(&'a EvidenceRecord);
struct CanonicalRecords<'a>(&'a [EvidenceRecord]);
struct CanonicalDiagnostic<'a>(&'a EvidenceDiagnostic);
struct CanonicalDiagnostics<'a>(&'a [EvidenceDiagnostic]);
struct CanonicalValidation<'a> {
    result: &'a EvidenceValidation,
    policy: &'a serde_json::Value,
    extractor: &'a serde_json::Value,
}
macro_rules! canonical_seq {
    ($name:ident,$wrap:ident) => {
        impl Serialize for $name<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq;
                let mut seq = s.serialize_seq(Some(self.0.len()))?;
                for item in self.0 {
                    seq.serialize_element(&$wrap(item))?;
                }
                seq.end()
            }
        }
    };
}
canonical_seq!(CanonicalCitations, CanonicalCitation);
canonical_seq!(CanonicalResponsibilities, CanonicalResponsibility);
canonical_seq!(CanonicalCompetitors, CanonicalCompetitor);
canonical_seq!(CanonicalChecks, CanonicalCheck);
canonical_seq!(CanonicalCitationChecks, CanonicalCitationCheck);
canonical_seq!(CanonicalRecords, CanonicalRecord);
canonical_seq!(CanonicalDiagnostics, CanonicalDiagnostic);
impl Serialize for CanonicalArtifacts<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for item in self.0 {
            seq.serialize_element(&super::identity::Artifact::from(item))?;
        }
        seq.end()
    }
}
macro_rules! canonical_map {
    ($name:ident, $value:ident, { $($key:literal => $expr:expr),* $(,)? }) => {
        impl Serialize for $name<'_> {fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{use serde::ser::SerializeMap;let $value=self.0;let mut map=s.serialize_map(None)?;$(map.serialize_entry($key,&$expr)?;)*map.end()}}
    };
}
canonical_map!(CanonicalSubmission,m,{
    "change_kind"=>m.change_kind,"discriminator"=>m.discriminator,"inference_provenance"=>CanonicalInference(&m.inference_provenance),
    "previous_mapping"=>m.previous_mapping.as_ref().map(CanonicalPrevious),"proposed_component"=>m.proposed_component,
    "rationale"=>CanonicalRationale(&m.rationale),"responsibility_ids"=>m.responsibility_ids,
    "source_ref"=>super::identity::Source::from(&m.source_ref),"structural_citations"=>CanonicalCitations(&m.structural_citations),
});
canonical_map!(CanonicalCitation,c,{"assertion"=>super::identity::AssertionView(&c.assertion),"id"=>c.id,"source_ref"=>super::identity::Source::from(&c.source_ref)});
canonical_map!(CanonicalRationale,r,{"competing_components"=>CanonicalCompetitors(&r.competing_components),"responsibilities"=>CanonicalResponsibilities(&r.responsibilities)});
canonical_map!(CanonicalResponsibility,r,{"explanation"=>r.explanation,"responsibility_id"=>r.responsibility_id});
canonical_map!(CanonicalCompetitor,c,{"component"=>c.component,"explanation"=>c.explanation});
canonical_map!(CanonicalInference,i,{"agent_id"=>i.agent_id,"input_digest"=>i.input_digest,"model_id"=>i.model_id,"result_digest"=>i.result_digest,"rule_version"=>i.rule_version});
canonical_map!(CanonicalPrevious,p,{"component"=>p.component,"evidence_ref"=>super::identity::Artifact::from(&p.evidence_ref),"lineage"=>p.lineage.as_ref().map(CanonicalLineage),"mapping_facts_digest"=>p.mapping_facts_digest,"source_ref"=>super::identity::Source::from(&p.source_ref)});
canonical_map!(CanonicalLineage,l,{"basis"=>l.basis,"current"=>super::identity::Source::from(&l.current),"evidence_refs"=>CanonicalArtifacts(&l.evidence_refs),"previous"=>super::identity::Source::from(&l.previous),"rule_version"=>l.rule_version});
canonical_map!(CanonicalCheck,c,{"reason_codes"=>c.reason_codes,"status"=>c.status});
canonical_map!(CanonicalCitationCheck,c,{"assertion_check"=>CanonicalCheck(&c.assertion_check),"id"=>c.id,"source_checks"=>CanonicalChecks(&c.source_checks)});
canonical_map!(CanonicalRecord,r,{"citations"=>CanonicalCitationChecks(&r.citations),"competing_components"=>r.competing_components,"history_check"=>CanonicalCheck(&r.history_check),"inference_check"=>CanonicalCheck(&r.inference_check),"ownership_qualification"=>r.ownership_qualification,"rationale_check"=>CanonicalCheck(&r.rationale_check),"reason_codes"=>r.reason_codes,"responsibility_check"=>CanonicalCheck(&r.responsibility_check),"source_check"=>CanonicalCheck(&r.source_check),"submission"=>CanonicalSubmission(&r.submission)});
canonical_map!(CanonicalDiagnostic,d,{"citation_id"=>d.citation_id,"code"=>d.code,"mapping_index"=>d.mapping_index,"message"=>d.message,"subject"=>d.subject});
impl Serialize for CanonicalValidation<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let r = self.result;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("diagnostics", &CanonicalDiagnostics(&r.diagnostics))?;
        m.serialize_entry("effective_policy", self.policy)?;
        m.serialize_entry("evidence_version", r.evidence_version)?;
        m.serialize_entry("extractor_identity", self.extractor)?;
        m.serialize_entry("has_invalid_evidence", &r.has_invalid_evidence)?;
        m.serialize_entry("has_unknown", &r.has_unknown)?;
        m.serialize_entry("mapping_facts_digest", &r.mapping_facts_digest)?;
        m.serialize_entry("model_compile_input_digest", &r.model_compile_input_digest)?;
        m.serialize_entry("model_id", &r.model_id)?;
        m.serialize_entry("records", &CanonicalRecords(&r.records))?;
        m.serialize_entry("source", &super::identity::Provenance::from(&r.source))?;
        m.end()
    }
}

pub(super) fn canonical_bytes(
    result: &EvidenceValidation,
    run: &crate::architecture::compile::evidence_support::Run,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<Vec<u8>, super::EvidenceError> {
    run.check()?;
    // These values contain only the fixed built-in descriptor and numeric
    // policy, not candidate records or source bytes.
    let mut policy = serde_json::to_value(&result.effective_policy)
        .map_err(|e| super::EvidenceError::new("architecture_internal_error", e.to_string()))?;
    let mut extractor = serde_json::to_value(&result.extractor_identity)
        .map_err(|e| super::EvidenceError::new("architecture_internal_error", e.to_string()))?;
    policy.sort_all_objects();
    extractor.sort_all_objects();
    run.check()?;
    let view = CanonicalValidation {
        result,
        policy: &policy,
        extractor: &extractor,
    };
    Ok(
        crate::architecture::compile::evidence_support::stream_bytes(
            &view,
            result.effective_policy.max_result_bytes(),
            run,
            control,
        )?,
    )
}

#[cfg(test)]
mod assembly_tests {
    use super::*;
    use crate::{
        architecture::{
            compile::{compile, evidence_support, ExecutionContext, ModelInput, RepositoryBinding},
            Digest, GitObjectId, RelativePath, Text,
        },
        extraction_control::ExtractionControl,
    };
    use std::{
        path::Path,
        sync::{atomic::AtomicBool, Arc},
    };
    fn git(path: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().to_owned()
    }
    #[test]
    fn empty_mapping_validates_tree_and_streams_canonical_result() {
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
        let policy = ArchitectureExecutionPolicy::from_json(serde_json::Value::Object(
            fields
                .into_iter()
                .map(|v| (v.into(), 100_000.into()))
                .collect(),
        ))
        .unwrap();
        let path = RelativePath::new("root").unwrap();
        let input = ModelInput::inline_files(
            path.clone(),
            [(
                path,
                b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\n"
                    .to_vec(),
            )],
        )
        .unwrap();
        let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
        let model = compile(input, None, &policy, None, &context).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "cod23-assembly-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["config", "user.name", "test"]);
        git(&dir, &["config", "user.email", "test@example.invalid"]);
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn hello() {}").unwrap();
        git(&dir, &["add", "src/lib.rs"]);
        git(&dir, &["commit", "-qm", "fixture"]);
        let extractor = super::super::source_extractor_identity(&context).unwrap();
        let source = SourceProvenance {
            repository: Text::new("repo").unwrap(),
            evaluated_commit: GitObjectId::new(git(&dir, &["rev-parse", "HEAD"])).unwrap(),
            evaluated_tree: GitObjectId::new(git(&dir, &["rev-parse", "HEAD^{tree}"])).unwrap(),
            extractor_digest: extractor.digest().clone(),
            build_context_digest: Digest::new(format!("sha256:{}", "a".repeat(64))).unwrap(),
        };
        let binding = RepositoryBinding::new(source.repository.clone(), dir.join(".git")).unwrap();
        let bindings = [binding];
        let (_, run) = evidence_support::start(&policy, None, &context).unwrap();
        let mut checkpoint = || {
            run.check().map_err(|e| {
                crate::extraction_control::ExtractionError::new(e.code(), "interrupted")
            })
        };
        let mut control = ExtractionControl::new(
            &mut checkpoint,
            policy.max_parse_depth(),
            policy.max_traversal_steps(),
            policy.max_projected_vertices(),
            policy.max_projected_edges(),
        );
        let mut diagnostics = Vec::new();
        let result = validate_in_run(
            &model,
            &source,
            &[],
            &bindings,
            &run,
            &mut control,
            ValidationProgress {
                diagnostics: &mut diagnostics,
                known_invalid: &mut false,
            },
        )
        .unwrap();
        assert!(result.records().is_empty());
        assert!(!result.has_invalid_evidence());
        assert!(result.has_unknown());
        let actual = canonical_bytes(&result, &run, &mut control).unwrap();
        let mut expected = serde_json::to_value(&result).unwrap();
        expected.sort_all_objects();
        assert_eq!(actual, serde_json::to_vec(&expected).unwrap());
        use sha2::{Digest as _, Sha256};
        let file_digest = format!("sha256:{:x}", Sha256::digest(b"fn hello() {}"));
        let function = serde_json::json!({"repository":"repo","commit":source.evaluated_commit.as_str(),"path":"src/lib.rs","node_kind":"Function","node_id":"Function:216f9df7cf9055f46285","symbol":"src.lib.hello","byte_span":{"start":0,"end":13},"file_digest":file_digest});
        let scope = serde_json::json!({"repository":"repo","commit":source.evaluated_commit.as_str(),"path":"src/lib.rs","node_kind":"Scope","node_id":"Scope:41046104907937f3c6e1","symbol":"src.lib.<scope>","byte_span":{"start":0,"end":13},"file_digest":file_digest});
        let raw = serde_json::json!({"source_ref":function,"proposed_component":"missing","responsibility_ids":["task"],"structural_citations":[{"id":"node","source_ref":function,"assertion":{"kind":"node_exists"}},{"id":"scope","source_ref":scope,"assertion":{"kind":"scope_contains","member":function}},{"id":"export","source_ref":function,"assertion":{"kind":"exported_item","crate":"mycrate","namespace":"value","item_path":"hello"}}],"rationale":{"responsibilities":[{"responsibility_id":"task","explanation":"reason"}],"competing_components":[]},"inference_provenance":{"agent_id":"agent","model_id":"model","input_digest":file_digest,"rule_version":"responsibility-evidence/0.1","result_digest":file_digest},"previous_mapping":null,"change_kind":"initial","discriminator":"responsibility-evidence/0.1"});
        let mapping: MappingSubmission = serde_json::from_value(raw.clone()).unwrap();
        let mappings = [mapping];
        let mut diagnostics = Vec::new();
        let mut invalid = false;
        let result = validate_in_run(
            &model,
            &source,
            &mappings,
            &bindings,
            &run,
            &mut control,
            ValidationProgress {
                diagnostics: &mut diagnostics,
                known_invalid: &mut invalid,
            },
        )
        .unwrap();
        assert!(result.has_invalid_evidence() && result.has_unknown());
        assert_eq!(
            result.records()[0].source_check().status(),
            CheckStatus::Valid
        );
        let checks = result.records()[0].citations();
        assert_eq!(checks[0].id().as_str(), "export");
        assert_eq!(checks[0].assertion_check().status(), CheckStatus::Unknown);
        assert_eq!(checks[1].assertion_check().status(), CheckStatus::Valid);
        assert_eq!(checks[2].assertion_check().status(), CheckStatus::Valid);
        let actual = canonical_bytes(&result, &run, &mut control).unwrap();
        let mut expected = serde_json::to_value(&result).unwrap();
        expected.sort_all_objects();
        assert_eq!(actual, serde_json::to_vec(&expected).unwrap());
        let public =
            super::super::validate_evidence(&model, &source, &mappings, &bindings, &context)
                .unwrap();
        assert!(public.has_invalid_evidence() && public.has_unknown());
        assert_eq!(
            public.records()[0].source_check().status(),
            CheckStatus::Valid
        );
        let mut expected = serde_json::to_value(&public).unwrap();
        expected.sort_all_objects();
        assert_eq!(
            public.canonical_json(),
            serde_json::to_vec(&expected).unwrap()
        );
        let mut bad = raw.clone();
        bad["source_ref"]["byte_span"]["start"] = serde_json::json!(1);
        let bad: MappingSubmission = serde_json::from_value(bad).unwrap();
        let bad_result =
            super::super::validate_evidence(&model, &source, &[bad], &bindings, &context).unwrap();
        assert!(bad_result.has_invalid_evidence() && bad_result.has_unknown());
        assert_eq!(
            bad_result.records()[0].source_check().status(),
            CheckStatus::Invalid
        );
        let mut incompatible = source.clone();
        incompatible.extractor_digest = Digest::new(format!("sha256:{}", "0".repeat(64))).unwrap();
        let unknown =
            super::super::validate_evidence(&model, &incompatible, &mappings, &bindings, &context)
                .unwrap();
        assert_eq!(
            unknown.records()[0].source_check().status(),
            CheckStatus::Unknown
        );
        assert!(unknown.has_invalid_evidence() && unknown.has_unknown());
        let mut wrong_tree = source.clone();
        wrong_tree.evaluated_tree =
            GitObjectId::new("0".repeat(wrong_tree.evaluated_tree.as_str().len())).unwrap();
        let wrong =
            super::super::validate_evidence(&model, &wrong_tree, &[], &bindings, &context).unwrap();
        assert!(wrong.has_invalid_evidence() && wrong.has_unknown());
        let missing = super::super::validate_evidence(&model, &source, &[], &[], &context).unwrap();
        assert!(!missing.has_invalid_evidence() && missing.has_unknown());
        // Policy itself is part of the output, so converge its decimal size.
        let mut limit = 100_000u64;
        loop {
            let mut raw_policy = serde_json::to_value(&policy).unwrap();
            raw_policy["max_result_bytes"] = serde_json::json!(limit);
            let bounded = ArchitectureExecutionPolicy::from_json(raw_policy).unwrap();
            let p = RelativePath::new("root").unwrap();
            let input = ModelInput::inline_files(
                p.clone(),
                [(
                    p,
                    b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\n"
                        .to_vec(),
                )],
            )
            .unwrap();
            let bounded_model = compile(input, None, &bounded, None, &context).unwrap();
            let result = super::super::validate_evidence(
                &bounded_model,
                &source,
                &mappings,
                &bindings,
                &context,
            )
            .unwrap();
            let actual = result.canonical_json().len() as u64;
            if actual == limit {
                break;
            }
            limit = actual;
        }
        let mut raw_policy = serde_json::to_value(&policy).unwrap();
        raw_policy["max_result_bytes"] = serde_json::json!(limit - 1);
        let bounded = ArchitectureExecutionPolicy::from_json(raw_policy).unwrap();
        let p = RelativePath::new("root").unwrap();
        let input = ModelInput::inline_files(
            p.clone(),
            [(
                p,
                b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\n"
                    .to_vec(),
            )],
        )
        .unwrap();
        let bounded_model = compile(input, None, &bounded, None, &context).unwrap();
        let error = super::super::validate_evidence(
            &bounded_model,
            &source,
            &mappings,
            &bindings,
            &context,
        )
        .unwrap_err();
        assert_eq!(error.code(), "resource_limit");
        assert!(error.has_known_invalid_evidence() && error.has_known_unknown());
        assert!(error
            .diagnostics()
            .iter()
            .any(|d| d.code() == "invalid_evidence_detected"));
        assert!(error
            .diagnostics()
            .iter()
            .any(|d| d.code() == "qualification_unknown"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
