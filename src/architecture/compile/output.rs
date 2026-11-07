//! C16 canonical JSON encoding of the complete logical result.
use super::policy::RunContext;
use super::{CompileError, CompiledModel, ConstraintRecord, Operation, Scenario};
use serde::Serialize;
use std::io::{self, Write};

struct Sink<'a> {
    bytes: Vec<u8>,
    limit: u64,
    run: &'a RunContext,
    failure: Option<CompileError>,
}
impl Write for Sink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(e) = self.run.check() {
            self.failure = Some(e);
            return Err(io::Error::other("cancelled"));
        }
        let size = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("result size overflow"))?;
        if u64::try_from(size).map_or(true, |n| n > self.limit) {
            return Err(io::Error::other("result byte limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| io::Error::other("result allocation"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Sink<'_> {
    fn failure(&mut self) -> CompileError {
        self.failure.take().unwrap_or_else(|| {
            CompileError::simple("resource_limit", "complete result byte limit exceeded")
        })
    }
    fn raw(&mut self, bytes: &[u8]) -> Result<(), CompileError> {
        self.write_all(bytes).map_err(|_| self.failure())
    }
    fn value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CompileError> {
        // Each use is a fixed-depth record. serde_json's map representation sorts
        // keys; nested scenarios are emitted by the explicit stack below.
        let mut value = serde_json::to_value(value)
            .map_err(|e| CompileError::simple("architecture_internal_error", e.to_string()))?;
        // Workspace feature unification enables preserve_order, so sorting is
        // explicit even for these fixed-depth record values.
        value.sort_all_objects();
        serde_json::to_writer(&mut *self, &value).map_err(|_| self.failure())
    }
}
fn record(out: &mut Sink<'_>, value: &ConstraintRecord) -> Result<(), CompileError> {
    let (kind, write): (&str, &dyn erased::Record) = match value {
        ConstraintRecord::OwnershipRule(v) => ("ownership_rule", v),
        ConstraintRecord::Allow(v) => ("allow", v),
        ConstraintRecord::Require(v) => ("require", v),
        ConstraintRecord::Budget(v) => ("budget", v),
        ConstraintRecord::Waiver(v) => ("waiver", v),
        ConstraintRecord::Scenario(_) => {
            return Err(CompileError::simple(
                "architecture_internal_error",
                "scenario requires iterative encoding",
            ))
        }
    };
    out.raw(b"{\"kind\":")?;
    out.value(kind)?;
    out.raw(b",\"record\":")?;
    write.write(out)?;
    out.raw(b"}")
}
mod erased {
    use super::*;
    pub(super) trait Record {
        fn write(&self, out: &mut Sink<'_>) -> Result<(), CompileError>;
    }
    impl<T: Serialize> Record for T {
        fn write(&self, out: &mut Sink<'_>) -> Result<(), CompileError> {
            out.value(self)
        }
    }
}
fn scenarios(out: &mut Sink<'_>, values: &[Scenario]) -> Result<(), CompileError> {
    enum Job<'a> {
        Scenario(&'a Scenario),
        Tail(&'a Scenario),
        End(&'a Scenario),
        Ops(&'a [Operation], usize),
        Raw(&'static [u8]),
        Digest(&'a crate::architecture::Digest),
    }
    let mut stack = Vec::new();
    macro_rules! push {
        ($job:expr) => {{
            stack
                .try_reserve(1)
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            stack.push($job);
        }};
    }
    out.raw(b"[")?;
    for (index, scenario) in values.iter().enumerate() {
        if index != 0 {
            out.raw(b",")?;
        }
        push!(Job::Scenario(scenario));
        while let Some(job) = stack.pop() {
            out.run.check()?;
            match job {
                Job::Raw(bytes) => out.raw(bytes)?,
                Job::Digest(value) => out.value(value)?,
                Job::Scenario(s) => {
                    out.raw(b"{\"baseline_transform\":[")?;
                    push!(Job::Tail(s));
                    push!(Job::Ops(&s.baseline_transform, 0));
                }
                Job::Tail(s) => {
                    out.raw(b"],\"candidate_transform\":[")?;
                    push!(Job::End(s));
                    push!(Job::Ops(&s.candidate_transform, 0));
                }
                Job::End(s) => {
                    out.raw(b"],\"id\":")?;
                    out.value(&s.id)?;
                    out.raw(b",\"obligations\":")?;
                    out.value(&s.obligations)?;
                    out.raw(b",\"parameters\":")?;
                    out.value(&s.parameters)?;
                    out.raw(b",\"preconditions\":")?;
                    out.value(&s.preconditions)?;
                    out.raw(b"}")?;
                }
                Job::Ops(operations, i) => {
                    if i == operations.len() {
                        continue;
                    }
                    if i != 0 {
                        out.raw(b",")?;
                    }
                    push!(Job::Ops(operations, i + 1));
                    match &operations[i] {
                        Operation::AddConstraint {
                            constraint: ConstraintRecord::Scenario(child),
                        } => {
                            out.raw(b"{\"constraint\":{\"kind\":\"scenario\",\"record\":")?;
                            push!(Job::Raw(b"},\"kind\":\"add_constraint\"}"));
                            push!(Job::Scenario(child));
                        }
                        Operation::ReplaceConstraint {
                            constraint: ConstraintRecord::Scenario(child),
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"constraint\":{\"kind\":\"scenario\",\"record\":")?;
                            push!(Job::Raw(b",\"kind\":\"replace_constraint\"}"));
                            push!(Job::Digest(expected_record_digest));
                            push!(Job::Raw(b"},\"expected_record_digest\":"));
                            push!(Job::Scenario(child));
                        }
                        Operation::AddConstraint { constraint: value } => {
                            out.raw(b"{\"constraint\":")?;
                            record(out, value)?;
                            out.raw(b",\"kind\":\"add_constraint\"}")?;
                        }
                        Operation::ReplaceConstraint {
                            constraint: value,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"constraint\":")?;
                            record(out, value)?;
                            out.raw(b",\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"kind\":\"replace_constraint\"}")?;
                        }
                        Operation::AddElement { element } => {
                            out.raw(b"{\"element\":")?;
                            out.value(element)?;
                            out.raw(b",\"kind\":\"add_element\"}")?;
                        }
                        Operation::ReplaceElement {
                            element,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"element\":")?;
                            out.value(element)?;
                            out.raw(b",\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"kind\":\"replace_element\"}")?;
                        }
                        Operation::RemoveElement {
                            element_kind,
                            id,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"element_kind\":")?;
                            out.value(element_kind)?;
                            out.raw(b",\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"id\":")?;
                            out.value(id)?;
                            out.raw(b",\"kind\":\"remove_element\"}")?;
                        }
                        Operation::AddRelationship { relationship } => {
                            out.raw(b"{\"kind\":\"add_relationship\",\"relationship\":")?;
                            out.value(relationship)?;
                            out.raw(b"}")?;
                        }
                        Operation::ReplaceRelationship {
                            relationship,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"kind\":\"replace_relationship\",\"relationship\":")?;
                            out.value(relationship)?;
                            out.raw(b"}")?;
                        }
                        Operation::RemoveRelationship {
                            id,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"id\":")?;
                            out.value(id)?;
                            out.raw(b",\"kind\":\"remove_relationship\"}")?;
                        }
                        Operation::RemoveConstraint {
                            constraint_kind,
                            id,
                            expected_record_digest,
                        } => {
                            out.raw(b"{\"constraint_kind\":")?;
                            out.value(constraint_kind)?;
                            out.raw(b",\"expected_record_digest\":")?;
                            out.value(expected_record_digest)?;
                            out.raw(b",\"id\":")?;
                            out.value(id)?;
                            out.raw(b",\"kind\":\"remove_constraint\"}")?;
                        }
                    }
                }
            }
        }
    }
    out.raw(b"]")
}

pub(super) fn encode(model: &CompiledModel, run: &RunContext) -> Result<Vec<u8>, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit: model.effective_policy.max_result_bytes,
        run,
        failure: None,
    };
    out.raw(b"{\"architecture\":")?;
    out.value(&model.architecture)?;
    out.raw(b",\"closure\":[")?;
    for (index, file) in model.closure.iter().enumerate() {
        run.check()?;
        if index != 0 {
            out.raw(b",")?;
        }
        out.raw(b"{\"bytes\":[")?;
        for (i, byte) in file.bytes().iter().enumerate() {
            if i != 0 {
                out.raw(b",")?;
            }
            out.value(byte)?;
        }
        out.raw(b"],\"object_id\":")?;
        out.value(&file.blob())?;
        out.raw(b",\"path\":")?;
        out.value(file.path())?;
        out.raw(b"}")?;
    }
    out.raw(b"],\"constraints\":[")?;
    for (index, document) in model.constraints.iter().enumerate() {
        run.check()?;
        if index != 0 {
            out.raw(b",")?;
        }
        out.raw(b"{\"allow\":")?;
        out.value(&document.allow)?;
        out.raw(b",\"budgets\":")?;
        out.value(&document.budgets)?;
        out.raw(b",\"ownership_rules\":")?;
        out.value(&document.ownership_rules)?;
        out.raw(b",\"require\":")?;
        out.value(&document.require)?;
        out.raw(b",\"scenarios\":")?;
        scenarios(&mut out, &document.scenarios)?;
        out.raw(b",\"schema_version\":")?;
        out.value(&document.schema_version)?;
        out.raw(b",\"waivers\":")?;
        out.value(&document.waivers)?;
        out.raw(b"}")?;
    }
    out.raw(b"],\"effective_policy\":")?;
    out.value(&model.effective_policy)?;
    out.raw(b",\"provenance\":")?;
    if let Some(p) = &model.provenance {
        out.raw(b"{\"revision\":")?;
        out.value(p.revision())?;
        out.raw(b",\"tree\":")?;
        out.value(p.tree())?;
        out.raw(b"}")?;
    } else {
        out.raw(b"null")?;
    }
    out.raw(b",\"root\":")?;
    out.value(&model.root)?;
    out.raw(b"}")?;
    run.check()?;
    Ok(out.bytes)
}
