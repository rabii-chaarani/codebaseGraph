//! Narrow bridge to the compiler's policy, canonical writer and hashing machinery.
use super::{
    output, policy::RunContext, ArchitectureExecutionPolicy, ArchitectureExecutionPolicyOverride,
    CompileError, ExecutionContext,
};
use serde::Serialize;
use std::time::Instant;

pub(crate) type Run = RunContext;
pub(crate) fn start(
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<(ArchitectureExecutionPolicy, Run), CompileError> {
    let started = Instant::now();
    let effective = policy.effective(overrides)?;
    let run = context.start_at(&effective, started)?;
    run.check()?;
    Ok((effective, run))
}
pub(crate) fn stream_hash<T: Serialize>(
    value: &T,
    limit: u64,
    run: &Run,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<(crate::architecture::Digest, u64), CompileError> {
    output::evidence_stream_hash(value, limit, run, control)
}

pub(crate) fn tree(
    binding: &super::RepositoryBinding,
    commit: &crate::architecture::GitObjectId,
    run: &Run,
) -> Result<crate::architecture::GitObjectId, CompileError> {
    super::source::evidence_tree(binding, commit, run)
}
pub(crate) fn file(
    binding: &super::RepositoryBinding,
    tree: &crate::architecture::GitObjectId,
    path: &crate::architecture::RelativePath,
    limit: u64,
    run: &Run,
) -> Result<Vec<u8>, CompileError> {
    super::source::evidence_file(binding, tree, path, limit, run)
}
pub(crate) fn raw_digest(
    bytes: &[u8],
    run: &Run,
) -> Result<crate::architecture::Digest, CompileError> {
    super::identity::raw_digest(bytes, run)
}

pub(crate) fn metadata_run(context: &ExecutionContext) -> Run {
    context.metadata_run()
}
pub(crate) fn metadata_hash<T: Serialize>(
    domain: &str,
    value: &T,
    run: &Run,
) -> Result<crate::architecture::Digest, CompileError> {
    output::hash_value(domain, value, run)
}

pub(crate) fn measure<T: Serialize>(value: &T, limit: u64, run: &Run) -> Result<u64, CompileError> {
    output::measure_serialized(value, limit, run)
}

pub(crate) fn compile_input(
    model: &super::CompiledModel,
    run: &Run,
) -> Result<crate::architecture::Digest, CompileError> {
    super::identity::compile_input_in_run(model, run)
}

pub(crate) fn stream_bytes<T: Serialize>(
    value: &T,
    limit: u64,
    run: &Run,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<Vec<u8>, CompileError> {
    output::evidence_stream_bytes(value, limit, run, control)
}
