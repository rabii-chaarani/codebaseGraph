use super::{EvidenceError, MappingSubmission};
use crate::architecture::{
    compile::{
        evidence_support as support, ArchitectureExecutionPolicy,
        ArchitectureExecutionPolicyOverride, ExecutionContext,
    },
    Digest, SourceProvenance,
};
use serde::Serialize;

/// Identity of submitted facts alone, not an evaluation or acceptance identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MappingFactsDigest(Digest);
impl MappingFactsDigest {
    pub fn digest(&self) -> &Digest {
        &self.0
    }
}

#[derive(Serialize)]
struct Payload<'a> {
    evidence_version: &'static str,
    source: &'a SourceProvenance,
    mappings: &'a [MappingSubmission],
}
fn invalid(message: &str) -> EvidenceError {
    EvidenceError::new("architecture_invalid_input", message)
}
fn limit(message: &str) -> EvidenceError {
    EvidenceError::new("resource_limit", message)
}

/// Hash the complete canonical claims; this does not verify any submitted fact.
pub fn mapping_facts_digest(
    source: &SourceProvenance,
    mappings: &[MappingSubmission],
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<MappingFactsDigest, EvidenceError> {
    let (policy, run) = support::start(policy, overrides, context)?;
    if u64::try_from(mappings.len()).unwrap_or(u64::MAX) > policy.max_mapping_records() {
        return Err(limit("mapping count exceeded"));
    }
    // Sorting does not change JSON length. Admit the entire payload before
    // constructing sorting scratch so oversized submissions do not get copied.
    support::measure(
        &Payload {
            evidence_version: "architecture-evidence/0.1",
            source,
            mappings,
        },
        policy.max_evidence_bytes(),
        &run,
    )?;
    let mut ordered = Vec::new();
    ordered
        .try_reserve(mappings.len())
        .map_err(|_| limit("allocation failed"))?;
    for mapping in mappings {
        run.check()?;
        if u64::try_from(mapping.structural_citations.len()).unwrap_or(u64::MAX)
            > policy.max_citations_per_mapping()
        {
            return Err(limit("citation count exceeded"));
        }
        let mut rationale_bytes = 0u64;
        for text in mapping
            .rationale
            .responsibilities
            .iter()
            .map(|r| &r.explanation)
            .chain(
                mapping
                    .rationale
                    .competing_components
                    .iter()
                    .map(|c| &c.explanation),
            )
        {
            run.check()?;
            rationale_bytes = rationale_bytes
                .checked_add(text.as_str().len() as u64)
                .ok_or_else(|| limit("rationale size overflow"))?;
        }
        if rationale_bytes > policy.max_rationale_bytes() {
            return Err(limit("rationale bytes exceeded"));
        }
        let mut m = mapping.clone();
        support::sort(&mut m.responsibility_ids, &run, |a, b| {
            a.as_str().cmp(b.as_str())
        })?;
        if m.responsibility_ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(invalid("duplicate responsibility ID"));
        }
        support::sort(&mut m.rationale.responsibilities, &run, |a, b| {
            a.responsibility_id
                .as_str()
                .cmp(b.responsibility_id.as_str())
        })?;
        if m.rationale
            .responsibilities
            .windows(2)
            .any(|w| w[0].responsibility_id == w[1].responsibility_id)
        {
            return Err(invalid("duplicate rationale responsibility"));
        }
        support::sort(&mut m.rationale.competing_components, &run, |a, b| {
            a.component.as_str().cmp(b.component.as_str())
        })?;
        if m.rationale
            .competing_components
            .windows(2)
            .any(|w| w[0].component == w[1].component)
        {
            return Err(invalid("duplicate competitor"));
        }
        support::sort(&mut m.structural_citations, &run, |a, b| {
            a.id.as_str().cmp(b.id.as_str())
        })?;
        if m.structural_citations
            .windows(2)
            .any(|w| w[0].id == w[1].id)
        {
            return Err(invalid("duplicate citation"));
        }
        if let Some(lineage) = m.previous_mapping.as_mut().and_then(|p| p.lineage.as_mut()) {
            support::sort(&mut lineage.evidence_refs, &run, |a, b| {
                (
                    a.repository.as_str(),
                    a.commit.as_str(),
                    a.path.as_str(),
                    a.content_digest.as_str(),
                )
                    .cmp(&(
                        b.repository.as_str(),
                        b.commit.as_str(),
                        b.path.as_str(),
                        b.content_digest.as_str(),
                    ))
            })?;
            if lineage.evidence_refs.windows(2).any(|w| w[0] == w[1]) {
                return Err(invalid("duplicate lineage evidence"));
            }
        }
        ordered.push(m);
    }
    fn compare(a: &MappingSubmission, b: &MappingSubmission) -> std::cmp::Ordering {
        (
            &a.source_ref.repository.as_str(),
            &a.source_ref.commit.as_str(),
            &a.source_ref.node_kind.as_str(),
            &a.source_ref.node_id.as_str(),
        )
            .cmp(&(
                &b.source_ref.repository.as_str(),
                &b.source_ref.commit.as_str(),
                &b.source_ref.node_kind.as_str(),
                &b.source_ref.node_id.as_str(),
            ))
    }
    support::sort(&mut ordered, &run, compare)?;
    if ordered.windows(2).any(|w| compare(&w[0], &w[1]).is_eq()) {
        return Err(invalid("duplicate mapping key"));
    }
    let payload = Payload {
        evidence_version: "architecture-evidence/0.1",
        source,
        mappings: &ordered,
    };
    let _bytes = support::canonical(&payload, policy.max_evidence_bytes(), &run)?;
    Ok(MappingFactsDigest(support::hash(
        "architecture-mapping-facts",
        &payload,
        &run,
    )?))
}
