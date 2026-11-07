//! Narrow bridge to the compiler's policy, canonical writer and hashing machinery.
use super::{
    identity, output, policy::RunContext, ArchitectureExecutionPolicy,
    ArchitectureExecutionPolicyOverride, CompileError, ExecutionContext,
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
pub(crate) fn sort<T>(
    values: &mut [T],
    run: &Run,
    cmp: impl FnMut(&T, &T) -> std::cmp::Ordering,
) -> Result<(), CompileError> {
    identity::canonical::sort_by(values, run, cmp)
}

pub(crate) fn stream_hash<T: Serialize>(
    value: &T,
    limit: u64,
    run: &Run,
) -> Result<(crate::architecture::Digest, u64), CompileError> {
    output::evidence_stream_hash(value, limit, run)
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
