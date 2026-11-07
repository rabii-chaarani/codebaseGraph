use super::super::{
    policy::RunContext, ArchitectureExecutionPolicy, ArchitectureExecutionPolicyOverride,
    ConstraintRecord, Operation, Parameter, Predicate, RepositoryBinding, Scenario,
};
use super::{raw_digest, CompileError, CompiledModel, ExecutionContext};
use crate::architecture::{ArtifactRef, GitObjectId};
use serde::Serialize;
use std::time::Instant;

fn key(r: &ArtifactRef) -> (&str, &str, &str, &str) {
    (
        r.repository.as_str(),
        r.commit.as_str(),
        r.path.as_str(),
        r.content_digest.as_str(),
    )
}
fn push<'a>(v: &mut Vec<&'a ArtifactRef>, r: &'a ArtifactRef) -> Result<(), CompileError> {
    v.try_reserve(1)
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    v.push(r);
    Ok(())
}
fn waiver<'a>(
    v: &mut Vec<&'a ArtifactRef>,
    w: &'a super::super::Waiver,
) -> Result<(), CompileError> {
    push(v, &w.approval_ref.reference)
}
fn relationship<'a>(
    v: &mut Vec<&'a ArtifactRef>,
    r: &'a crate::architecture::Relationship,
    run: &RunContext,
) -> Result<(), CompileError> {
    for item in &r.contract_refs {
        run.check()?;
        push(v, item)?;
    }
    Ok(())
}
pub(super) fn inventory(
    model: &CompiledModel,
    run: &RunContext,
) -> Result<Vec<ArtifactRef>, CompileError> {
    let mut refs = Vec::new();
    let mut pending: Vec<&Scenario> = Vec::new();
    for doc in &model.architecture {
        for r in &doc.relationships {
            run.check()?;
            relationship(&mut refs, r, run)?;
        }
    }
    for doc in &model.constraints {
        for w in &doc.waivers {
            run.check()?;
            waiver(&mut refs, w)?;
        }
        pending
            .try_reserve(doc.scenarios.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        pending.extend(doc.scenarios.iter());
    }
    while let Some(s) = pending.pop() {
        run.check()?;
        for p in s.parameters.values() {
            run.check()?;
            if let Parameter::ContractRef { reference } = p {
                push(&mut refs, reference)?;
            }
        }
        for p in s.preconditions.iter().chain(&s.obligations) {
            if let Predicate::ContractRefEquals { expected, .. } = p {
                for r in expected {
                    run.check()?;
                    push(&mut refs, r)?;
                }
            }
        }
        for operation in s.baseline_transform.iter().chain(&s.candidate_transform) {
            run.check()?;
            match operation {
                Operation::AddRelationship { relationship: r }
                | Operation::ReplaceRelationship {
                    relationship: r, ..
                } => relationship(&mut refs, r, run)?,
                Operation::AddConstraint { constraint }
                | Operation::ReplaceConstraint { constraint, .. } => match constraint {
                    ConstraintRecord::Waiver(w) => waiver(&mut refs, w)?,
                    ConstraintRecord::Scenario(child) => {
                        pending.try_reserve(1).map_err(|_| {
                            CompileError::simple("resource_limit", "allocation failed")
                        })?;
                        pending.push(child);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    run.check()?;
    super::canonical::sort_by(&mut refs, run, |a, b| key(a).cmp(&key(b)))?;
    run.check()?;
    let mut output: Vec<ArtifactRef> = Vec::new();
    for r in refs {
        run.check()?;
        if output.last().is_some_and(|prior| key(prior) == key(r)) {
            continue;
        }
        output
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        output.push(r.clone());
    }
    Ok(output)
}

#[derive(Serialize)]
pub struct QualifiedArtifact {
    reference: ArtifactRef,
    tree: GitObjectId,
    object_id: GitObjectId,
    bytes: Vec<u8>,
}
impl QualifiedArtifact {
    pub fn reference(&self) -> &ArtifactRef {
        &self.reference
    }
    pub fn tree(&self) -> &GitObjectId {
        &self.tree
    }
    pub fn object_id(&self) -> &GitObjectId {
        &self.object_id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Serialize)]
pub struct QualifiedArtifacts {
    artifacts: Vec<QualifiedArtifact>,
    effective_policy: ArchitectureExecutionPolicy,
}
impl QualifiedArtifacts {
    pub fn artifacts(&self) -> &[QualifiedArtifact] {
        &self.artifacts
    }
    pub fn effective_policy(&self) -> &ArchitectureExecutionPolicy {
        &self.effective_policy
    }
}

pub fn qualify_artifacts(
    refs: &[ArtifactRef],
    bindings: &[RepositoryBinding],
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<QualifiedArtifacts, CompileError> {
    let started = Instant::now();
    let effective_policy = policy.effective(overrides)?;
    let run = context.start_at(&effective_policy, started)?;
    run.check()?;
    let mut binding_refs = Vec::new();
    binding_refs
        .try_reserve(bindings.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    for binding in bindings {
        run.check()?;
        binding_refs.push(binding);
    }
    run.check()?;
    super::canonical::sort_by(&mut binding_refs, &run, |a, b| {
        a.identity().as_str().cmp(b.identity().as_str())
    })?;
    run.check()?;
    for pair in binding_refs.windows(2) {
        run.check()?;
        if pair[0].identity() == pair[1].identity() {
            return Err(CompileError::simple(
                "invalid_reference",
                "ambiguous repository binding",
            ));
        }
    }
    // Keep only distinct locators in scratch space even for a repeated input.
    let mut ordered: Vec<&ArtifactRef> = Vec::new();
    let mut evidence = 0u64;
    for reference in refs {
        run.check()?;
        match ordered.binary_search_by(|other| key(other).cmp(&key(reference))) {
            Ok(_) => continue,
            Err(index) => {
                for neighbor in [
                    index.checked_sub(1),
                    (index < ordered.len()).then_some(index),
                ]
                .into_iter()
                .flatten()
                {
                    let a = key(ordered[neighbor]);
                    let b = key(reference);
                    if (a.0, a.1, a.2) == (b.0, b.1, b.2) && a.3 != b.3 {
                        return Err(super::super::source::with_subject(
                            CompileError::simple(
                                "invalid_reference",
                                "conflicting content digests",
                            ),
                            reference.path.as_str(),
                        ));
                    }
                }
                let remaining = effective_policy.max_evidence_bytes - evidence;
                let descriptor_size =
                    super::super::output::measure_serialized(reference, remaining, &run)?;
                evidence = evidence.checked_add(descriptor_size).ok_or_else(|| {
                    CompileError::simple("resource_limit", "evidence byte limit exceeded")
                })?;
                ordered
                    .try_reserve(1)
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                ordered.insert(index, reference);
            }
        }
    }
    let mut artifacts = Vec::new();
    artifacts
        .try_reserve(ordered.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    for reference in ordered {
        run.check()?;
        let binding = binding_refs
            .binary_search_by(|b| b.identity().as_str().cmp(reference.repository.as_str()))
            .ok()
            .map(|i| binding_refs[i])
            .ok_or_else(|| {
                super::super::source::with_subject(
                    CompileError::simple(
                        "architecture_input_unavailable",
                        "repository binding unavailable",
                    ),
                    reference.path.as_str(),
                )
            })?;
        let remaining = effective_policy.max_evidence_bytes - evidence;
        let (tree, object_id, bytes) = super::super::source::capture_artifact(
            binding,
            reference,
            effective_policy.max_file_bytes.min(remaining),
            &run,
        )?;
        evidence = evidence
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= effective_policy.max_evidence_bytes)
            .ok_or_else(|| {
                CompileError::simple("resource_limit", "evidence byte limit exceeded")
            })?;
        if raw_digest(&bytes, &run)? != reference.content_digest {
            return Err(super::super::source::with_subject(
                CompileError::simple("invalid_reference", "artifact content digest mismatch"),
                reference.path.as_str(),
            ));
        }
        artifacts.push(QualifiedArtifact {
            reference: reference.clone(),
            tree,
            object_id,
            bytes,
        });
    }
    let result = QualifiedArtifacts {
        artifacts,
        effective_policy,
    };
    super::super::output::measure_serialized(
        &result,
        result.effective_policy.max_result_bytes,
        &run,
    )?;
    Ok(result)
}
