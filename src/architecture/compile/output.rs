//! C16 canonical JSON encoding of the complete logical result.
use super::policy::RunContext;
use super::{CompileError, CompiledModel, ConstraintRecord, Operation, Scenario};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::io::{self, Write};

pub(super) struct Sink<'a> {
    bytes: Vec<u8>,
    limit: u64,
    run: &'a RunContext,
    failure: Option<CompileError>,
    normalized: bool,
    hash: Option<Sha256>,
    count_only: bool,
    written: u64,
}
impl Write for Sink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(e) = self.run.check() {
            self.failure = Some(e);
            return Err(io::Error::other("cancelled"));
        }
        if let Some(hash) = &mut self.hash {
            for chunk in bytes.chunks(65_536) {
                if let Err(error) = self.run.check() {
                    self.failure = Some(error);
                    return Err(io::Error::other("cancelled"));
                }
                hash.update(chunk);
            }
            return Ok(bytes.len());
        }
        let size = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("result size overflow"))?;
        if size > self.limit {
            return Err(io::Error::other("result byte limit"));
        }
        self.written = size;
        if self.count_only {
            return Ok(bytes.len());
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
        if self.normalized {
            super::identity::canonical::normalize(&mut value, self.run)?;
        }
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
fn scenarios<'a>(
    out: &mut Sink<'_>,
    values: impl IntoIterator<Item = &'a Scenario>,
) -> Result<(), CompileError> {
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
    for (index, scenario) in values.into_iter().enumerate() {
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
                    if out.normalized {
                        out.predicates(&s.obligations)?;
                    } else {
                        out.value(&s.obligations)?;
                    }
                    out.raw(b",\"parameters\":")?;
                    out.value(&s.parameters)?;
                    out.raw(b",\"preconditions\":")?;
                    if out.normalized {
                        out.predicates(&s.preconditions)?;
                    } else {
                        out.value(&s.preconditions)?;
                    }
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
        normalized: false,
        hash: None,
        count_only: false,
        written: 0,
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

impl Sink<'_> {
    fn predicates(&mut self, predicates: &[super::Predicate]) -> Result<(), CompileError> {
        fn id(p: &super::Predicate) -> &str {
            use super::Predicate::*;
            match p {
                ElementExists { id, .. }
                | RelationshipExists { id, .. }
                | ContractRefEquals { id, .. }
                | Reachable { id, .. }
                | NoBypass { id, .. } => id.as_str(),
            }
        }
        let mut sorted = Vec::new();
        sorted
            .try_reserve(predicates.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        sorted.extend(predicates.iter());
        self.run.check()?;
        super::identity::canonical::sort_by(&mut sorted, self.run, |a, b| id(a).cmp(id(b)))?;
        self.run.check()?;
        self.raw(b"[")?;
        for (index, predicate) in sorted.iter().enumerate() {
            self.run.check()?;
            if index != 0 {
                self.raw(b",")?;
            }
            self.value(predicate)?;
        }
        self.raw(b"]")
    }
}

pub(super) fn hash_value<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
    run: &RunContext,
) -> Result<crate::architecture::Digest, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit: u64::MAX,
        run,
        failure: None,
        normalized: false,
        hash: Some(Sha256::new()),
        count_only: false,
        written: 0,
    };
    out.raw(domain.as_bytes())?;
    out.raw(b"/0.1\n")?;
    out.value(value)?;
    finish_hash(out)
}
fn finish_hash(mut out: Sink<'_>) -> Result<crate::architecture::Digest, CompileError> {
    out.run.check()?;
    let bytes = out.hash.take().expect("hash sink").finalize();
    crate::architecture::Digest::new(format!("sha256:{bytes:x}"))
        .map_err(|e| CompileError::simple("architecture_internal_error", e))
}

pub(super) fn intent_hash(
    model: &CompiledModel,
    run: &RunContext,
) -> Result<crate::architecture::Digest, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit: u64::MAX,
        run,
        failure: None,
        normalized: true,
        hash: Some(Sha256::new()),
        count_only: false,
        written: 0,
    };
    out.raw(b"architecture-intent/0.1\n{")?;
    macro_rules! records {
        ($name:literal, $docs:expr, $field:ident) => {{
            out.raw(concat!("\"", $name, "\":").as_bytes())?;
            let mut entries = Vec::new();
            for doc in $docs {
                run.check()?;
                entries
                    .try_reserve(doc.$field.len())
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                entries.extend(doc.$field.iter());
            }
            run.check()?;
            super::identity::canonical::sort_by(&mut entries, run, |a, b| {
                a.id.as_str().cmp(b.id.as_str())
            })?;
            run.check()?;
            out.raw(b"[")?;
            for (i, item) in entries.iter().enumerate() {
                run.check()?;
                if i != 0 {
                    out.raw(b",")?;
                }
                out.value(item)?;
            }
            out.raw(b"],")?;
        }};
    }
    records!("allow", &model.constraints, allow);
    records!("budgets", &model.constraints, budgets);
    records!("components", &model.architecture, components);
    records!("containers", &model.architecture, containers);
    out.raw(b"\"model_id\":")?;
    out.value(&model.root.model_id)?;
    out.raw(b",")?;
    records!("ownership_rules", &model.constraints, ownership_rules);
    records!("relationships", &model.architecture, relationships);
    records!("require", &model.constraints, require);
    out.raw(b"\"scenarios\":")?;
    let mut all = Vec::new();
    for doc in &model.constraints {
        run.check()?;
        all.try_reserve(doc.scenarios.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        all.extend(doc.scenarios.iter());
    }
    run.check()?;
    super::identity::canonical::sort_by(&mut all, run, |a, b| a.id.as_str().cmp(b.id.as_str()))?;
    run.check()?;
    scenarios(&mut out, all.iter().copied())?;
    out.raw(b",\"schema_version\":\"0.1\",")?;
    records!("systems", &model.architecture, systems);
    out.raw(b"\"waivers\":")?;
    let mut waivers = Vec::new();
    for doc in &model.constraints {
        run.check()?;
        waivers
            .try_reserve(doc.waivers.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        waivers.extend(doc.waivers.iter());
    }
    run.check()?;
    super::identity::canonical::sort_by(&mut waivers, run, |a, b| {
        a.id.as_str().cmp(b.id.as_str())
    })?;
    run.check()?;
    out.raw(b"[")?;
    for (i, item) in waivers.iter().enumerate() {
        run.check()?;
        if i != 0 {
            out.raw(b",")?;
        }
        out.value(item)?;
    }
    out.raw(b"]}")?;
    finish_hash(out)
}

pub(super) fn measure_serialized<T: Serialize>(
    value: &T,
    limit: u64,
    run: &RunContext,
) -> Result<u64, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit,
        run,
        failure: None,
        normalized: false,
        hash: None,
        count_only: false,
        written: 0,
    };
    out.count_only = true;
    serde_json::to_writer(&mut out, value).map_err(|_| out.failure())?;
    run.check()?;
    Ok(out.written)
}

pub(super) fn closure_hash(
    model: &CompiledModel,
    run: &RunContext,
) -> Result<crate::architecture::Digest, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit: u64::MAX,
        run,
        failure: None,
        normalized: false,
        hash: Some(Sha256::new()),
        count_only: false,
        written: 0,
    };
    out.raw(b"architecture-closure/0.1\n[")?;
    for (i, file) in model.closure.iter().enumerate() {
        run.check()?;
        if i != 0 {
            out.raw(b",")?;
        }
        out.raw(b"[")?;
        out.value(file.path())?;
        out.raw(b",")?;
        out.value(&super::identity::raw_digest(file.bytes(), run)?)?;
        out.raw(b"]")?;
    }
    out.raw(b"]")?;
    finish_hash(out)
}

/// Canonical JSON for a fixed-depth evidence value, bounded before publication.
pub(super) fn evidence_bytes<T: Serialize>(
    value: &T,
    limit: u64,
    run: &RunContext,
) -> Result<Vec<u8>, CompileError> {
    let mut out = Sink {
        bytes: Vec::new(),
        limit,
        run,
        failure: None,
        normalized: false,
        hash: None,
        count_only: false,
        written: 0,
    };
    out.value(value)?;
    run.check()?;
    Ok(out.bytes)
}
