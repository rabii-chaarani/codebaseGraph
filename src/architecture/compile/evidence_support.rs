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
pub(crate) fn canonical<T: Serialize>(
    value: &T,
    limit: u64,
    run: &Run,
) -> Result<Vec<u8>, CompileError> {
    output::evidence_bytes(value, limit, run)
}
pub(crate) fn hash<T: Serialize>(
    domain: &str,
    value: &T,
    run: &Run,
) -> Result<crate::architecture::Digest, CompileError> {
    output::hash_value(domain, value, run)
}
pub(crate) fn sort<T>(
    values: &mut [T],
    run: &Run,
    cmp: impl FnMut(&T, &T) -> std::cmp::Ordering,
) -> Result<(), CompileError> {
    identity::canonical::sort_by(values, run, cmp)
}

pub(crate) fn measure<T: Serialize>(value: &T, limit: u64, run: &Run) -> Result<u64, CompileError> {
    output::measure_serialized(value, limit, run)
}
