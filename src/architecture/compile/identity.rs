//! Identity constituents and explicit immutable linked-artifact qualification.
mod artifacts;
pub(super) mod canonical;

use super::{policy::RunContext, CompileError, CompiledModel, ExecutionContext};
use crate::architecture::{ArtifactRef, Digest, Id};
pub use artifacts::{qualify_artifacts, QualifiedArtifact, QualifiedArtifacts};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::time::Instant;

#[derive(Serialize)]
pub struct LinkedArtifact {
    reference: ArtifactRef,
    reference_digest: Digest,
}
impl LinkedArtifact {
    pub fn reference(&self) -> &ArtifactRef {
        &self.reference
    }
    pub fn reference_digest(&self) -> &Digest {
        &self.reference_digest
    }
}
#[derive(Serialize)]
pub struct CompiledIdentity {
    identity_version: &'static str,
    model_id: Id,
    intent_digest: Digest,
    closure_digest: Digest,
    policy_digest: Digest,
    compile_input_digest: Digest,
    linked_artifacts: Vec<LinkedArtifact>,
}
impl CompiledIdentity {
    pub fn identity_version(&self) -> &str {
        self.identity_version
    }
    pub fn model_id(&self) -> &Id {
        &self.model_id
    }
    pub fn intent_digest(&self) -> &Digest {
        &self.intent_digest
    }
    pub fn closure_digest(&self) -> &Digest {
        &self.closure_digest
    }
    pub fn policy_digest(&self) -> &Digest {
        &self.policy_digest
    }
    pub fn compile_input_digest(&self) -> &Digest {
        &self.compile_input_digest
    }
    pub fn linked_artifacts(&self) -> &[LinkedArtifact] {
        &self.linked_artifacts
    }
}
pub(super) fn raw_digest(bytes: &[u8], run: &RunContext) -> Result<Digest, CompileError> {
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(65536) {
        run.check()?;
        hash.update(chunk);
    }
    Digest::new(format!("sha256:{:x}", hash.finalize()))
        .map_err(|e| CompileError::simple("architecture_internal_error", e))
}

fn constituents(
    model: &CompiledModel,
    run: &RunContext,
) -> Result<(Digest, Digest, Digest, Digest), CompileError> {
    run.check()?;
    let intent_digest = super::output::intent_hash(model, run)?;
    let closure_digest = super::output::closure_hash(model, run)?;
    let policy_digest =
        super::output::hash_value("architecture-policy", &model.effective_policy, run)?;
    #[derive(Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Provenance<'a> {
        Inline,
        Git {
            repository: &'a crate::architecture::Text,
            commit: &'a crate::architecture::GitObjectId,
            tree: &'a crate::architecture::GitObjectId,
        },
    }
    #[derive(Serialize)]
    struct Input<'a> {
        identity_version: &'static str,
        schema_version: &'static str,
        model_id: &'a Id,
        intent_digest: &'a Digest,
        closure_digest: &'a Digest,
        policy_digest: &'a Digest,
        root_path: &'a crate::architecture::RelativePath,
        provenance: Provenance<'a>,
    }
    let provenance = match &model.provenance {
        Some(p) => Provenance::Git {
            repository: &p.revision().repository,
            commit: &p.revision().commit,
            tree: p.tree(),
        },
        None => Provenance::Inline,
    };
    let input = Input {
        identity_version: "architecture-identity/0.1",
        schema_version: "0.1",
        model_id: &model.root.model_id,
        intent_digest: &intent_digest,
        closure_digest: &closure_digest,
        policy_digest: &policy_digest,
        root_path: model.closure[model.root_index].path(),
        provenance,
    };
    let compile_input_digest =
        super::output::hash_value("architecture-compile-input", &input, run)?;
    Ok((
        intent_digest,
        closure_digest,
        policy_digest,
        compile_input_digest,
    ))
}

pub(super) fn compile_input_in_run(
    model: &CompiledModel,
    run: &RunContext,
) -> Result<Digest, CompileError> {
    constituents(model, run).map(|(_, _, _, input)| input)
}

pub fn identify(
    model: &CompiledModel,
    context: &ExecutionContext,
) -> Result<CompiledIdentity, CompileError> {
    let run = context.start_at(&model.effective_policy, Instant::now())?;
    let (intent_digest, closure_digest, policy_digest, compile_input_digest) =
        constituents(model, &run)?;
    let mut linked_artifacts = Vec::new();
    for reference in artifacts::inventory(model, &run)? {
        let reference_digest =
            super::output::hash_value("architecture-artifact-reference", &reference, &run)?;
        linked_artifacts
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        linked_artifacts.push(LinkedArtifact {
            reference,
            reference_digest,
        });
    }
    let result = CompiledIdentity {
        identity_version: "architecture-identity/0.1",
        model_id: model.root.model_id.clone(),
        intent_digest,
        closure_digest,
        policy_digest,
        compile_input_digest,
        linked_artifacts,
    };
    super::output::measure_serialized(&result, model.effective_policy.max_result_bytes, &run)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::{
        compile::{compile, ArchitectureExecutionPolicy, ModelInput},
        RelativePath,
    };
    use std::sync::{atomic::AtomicBool, Arc};

    #[test]
    fn identity_result_size_is_exact_even_with_policy_digest_changes() {
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
                .map(|k| (k.into(), 100000.into()))
                .collect(),
        ))
        .unwrap();
        let root = RelativePath::new("root").unwrap();
        let input = ModelInput::inline_files(
            root.clone(),
            [(
                root,
                b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\n"
                    .to_vec(),
            )],
        )
        .unwrap();
        let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
        let mut model = compile(input, None, &policy, None, &context).unwrap();
        let identity = identify(&model, &context).unwrap();
        let size = serde_json::to_vec(&identity).unwrap().len() as u64;
        model.effective_policy.max_result_bytes = size;
        assert!(identify(&model, &context).is_ok());
        model.effective_policy.max_result_bytes = size - 1;
        assert_eq!(
            identify(&model, &context).err().unwrap().code(),
            "resource_limit"
        );
    }
}
