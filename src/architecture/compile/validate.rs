//! Validation shared by the compiler's typed document assembly.
use super::constraints::{ConstraintDocument, EdgeRule, OwnershipClause};
use super::policy::RunContext;
use super::{ArchitectureExecutionPolicy, CompileError, SourceLocation};
use crate::architecture::{ArchitectureDocument, Id, Level};
use std::collections::{HashMap, HashSet};

pub(super) fn diagnostic(
    code: &'static str,
    reason: &str,
    subject: &str,
    locations: &HashMap<String, SourceLocation>,
) -> CompileError {
    let mut result = CompileError::simple(code, reason);
    result.diagnostics[0].subject = Some(subject.to_owned());
    result.diagnostics[0].location = locations.get(subject).cloned();
    result
}
fn invalid(
    subject: &str,
    reason: &str,
    locations: &HashMap<String, SourceLocation>,
) -> CompileError {
    diagnostic("invalid_reference", reason, subject, locations)
}
fn normalize_ids(
    ids: &mut [Id],
    subject: &str,
    locations: &HashMap<String, SourceLocation>,
) -> Result<(), CompileError> {
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err(diagnostic(
            "architecture_invalid_input",
            "duplicate set entry",
            subject,
            locations,
        ));
    }
    Ok(())
}
fn ref_key(reference: &crate::architecture::ArtifactRef) -> (&str, &str, &str, &str) {
    (
        reference.repository.as_str(),
        reference.commit.as_str(),
        reference.path.as_str(),
        reference.content_digest.as_str(),
    )
}
fn normalize_relationships(
    docs: &mut [ArchitectureDocument],
    locations: &HashMap<String, SourceLocation>,
    run: &RunContext,
) -> Result<(), CompileError> {
    for doc in docs {
        for r in &mut doc.relationships {
            run.check()?;
            r.contract_refs.sort_by(|a, b| ref_key(a).cmp(&ref_key(b)));
            if r.contract_refs.windows(2).any(|w| w[0] == w[1]) {
                return Err(diagnostic(
                    "architecture_invalid_input",
                    "duplicate contract reference",
                    r.id.as_str(),
                    locations,
                ));
            }
        }
    }
    Ok(())
}
fn projection(
    rule: &EdgeRule,
    locations: &HashMap<String, SourceLocation>,
) -> Result<(), CompileError> {
    if rule.projection.version != "rust-q1/0.1" {
        return Err(diagnostic(
            "unsupported_version",
            "unsupported projection version",
            rule.id.as_str(),
            locations,
        ));
    }
    Ok(())
}
/// Validate the authored base snapshot. Scenario runtime targets are deliberately
/// not resolved here; they may be created by a later transformation.
pub(super) fn validate_base(
    docs: &mut [ArchitectureDocument],
    constraints: &mut [ConstraintDocument],
    locations: &HashMap<String, SourceLocation>,
    policy: &ArchitectureExecutionPolicy,
    run: &RunContext,
) -> Result<(), CompileError> {
    run.check()?;
    normalize_relationships(docs, locations, run)?;
    if let Some(error) =
        crate::architecture::validate_with_checkpoint(docs, || run.check())?.first()
    {
        let code = match error.code {
            "duplicate_id" => "duplicate_id",
            "invalid_parent" | "contradictory_parentage" => "invalid_hierarchy",
            "invalid_endpoint" => "invalid_reference",
            _ => "architecture_invalid_input",
        };
        return Err(diagnostic(code, error.code, &error.subject, locations));
    }
    let mut ids = HashSet::<String>::new();
    let mut components = HashMap::<String, HashSet<String>>::new();
    let mut containers = HashSet::<String>::new();
    let mut entities = 0u64;
    for doc in docs.iter() {
        for s in &doc.systems {
            ids.insert(s.id.as_str().to_owned());
            entities = entities
                .checked_add(1)
                .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
            for r in &s.responsibilities {
                ids.insert(r.id.as_str().to_owned());
                entities = entities.checked_add(1).ok_or_else(|| {
                    CompileError::simple("resource_limit", "entity count overflow")
                })?;
            }
        }
        for c in &doc.containers {
            containers.insert(c.id.as_str().to_owned());
            ids.insert(c.id.as_str().to_owned());
            entities = entities
                .checked_add(1)
                .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
            for r in &c.responsibilities {
                ids.insert(r.id.as_str().to_owned());
                entities = entities.checked_add(1).ok_or_else(|| {
                    CompileError::simple("resource_limit", "entity count overflow")
                })?;
            }
        }
        for c in &doc.components {
            ids.insert(c.id.as_str().to_owned());
            entities = entities
                .checked_add(1)
                .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
            let mut responsibility = HashSet::new();
            for r in &c.responsibilities {
                ids.insert(r.id.as_str().to_owned());
                responsibility.insert(r.id.as_str().to_owned());
                entities = entities.checked_add(1).ok_or_else(|| {
                    CompileError::simple("resource_limit", "entity count overflow")
                })?;
            }
            components.insert(c.id.as_str().to_owned(), responsibility);
        }
        for r in &doc.relationships {
            ids.insert(r.id.as_str().to_owned());
            entities = entities
                .checked_add(1)
                .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
        }
    }
    if entities > policy.max_authored_entities {
        return Err(CompileError::simple(
            "resource_limit",
            "authored entity limit exceeded",
        ));
    }
    let mut add_id = |id: &Id| -> Result<(), CompileError> {
        run.check()?;
        entities = entities
            .checked_add(1)
            .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
        if entities > policy.max_authored_entities {
            return Err(CompileError::simple(
                "resource_limit",
                "authored entity limit exceeded",
            ));
        }
        if !ids.insert(id.as_str().to_owned()) {
            return Err(diagnostic(
                "duplicate_id",
                "duplicate authored ID",
                id.as_str(),
                locations,
            ));
        }
        Ok(())
    };
    let mut budgets = HashSet::new();
    let mut pairs = 0u64;
    for doc in constraints.iter_mut() {
        for rule in &mut doc.ownership_rules {
            add_id(&rule.id)?;
            let Some(owned) = components.get(rule.component.as_str()) else {
                return Err(invalid(
                    rule.id.as_str(),
                    "ownership component does not exist",
                    locations,
                ));
            };
            if rule.all_of.is_empty() {
                return Err(invalid(
                    rule.id.as_str(),
                    "ownership requires all_of clauses",
                    locations,
                ));
            }
            normalize_ids(&mut rule.responsibility_ids, rule.id.as_str(), locations)?;
            if rule
                .responsibility_ids
                .iter()
                .any(|id| !owned.contains(id.as_str()))
            {
                return Err(invalid(
                    rule.id.as_str(),
                    "responsibility does not belong to component",
                    locations,
                ));
            }
            for clause in rule.all_of.iter().chain(&rule.none_of) {
                run.check()?;
                if matches!(
                    clause,
                    OwnershipClause::ReachesExportedItem {
                        max_hops: Some(0),
                        ..
                    }
                ) {
                    return Err(diagnostic(
                        "architecture_invalid_input",
                        "max_hops must be positive",
                        rule.id.as_str(),
                        locations,
                    ));
                }
            }
        }
        for rule in doc.allow.iter_mut().chain(&mut doc.require) {
            add_id(&rule.id)?;
            projection(rule, locations)?;
            if rule.from.is_empty() || rule.to.is_empty() {
                return Err(invalid(
                    rule.id.as_str(),
                    "edge rule endpoint set is empty",
                    locations,
                ));
            }
            normalize_ids(&mut rule.from, rule.id.as_str(), locations)?;
            normalize_ids(&mut rule.to, rule.id.as_str(), locations)?;
            let exists = |id: &Id| match rule.level {
                Level::Component => components.contains_key(id.as_str()),
                Level::Container => containers.contains(id.as_str()),
            };
            if rule.from.iter().chain(&rule.to).any(|id| !exists(id)) {
                return Err(invalid(
                    rule.id.as_str(),
                    "edge rule endpoint does not exist at level",
                    locations,
                ));
            }
            let n = (rule.from.len() as u64)
                .checked_mul(rule.to.len() as u64)
                .ok_or_else(|| {
                    CompileError::simple("resource_limit", "rule pair count overflow")
                })?;
            pairs = pairs.checked_add(n).ok_or_else(|| {
                CompileError::simple("resource_limit", "rule pair count overflow")
            })?;
            if pairs > policy.max_expanded_rule_pairs {
                return Err(CompileError::simple(
                    "resource_limit",
                    "rule pair limit exceeded",
                ));
            }
        }
        for budget in &mut doc.budgets {
            add_id(&budget.id)?;
            budgets.insert(budget.id.as_str().to_owned());
            if budget.projection.version != "rust-q1/0.1" {
                return Err(diagnostic(
                    "unsupported_version",
                    "unsupported projection version",
                    budget.id.as_str(),
                    locations,
                ));
            }
            if budget.origins.is_empty() {
                return Err(invalid(
                    budget.id.as_str(),
                    "budget origins empty",
                    locations,
                ));
            }
            normalize_ids(&mut budget.origins, budget.id.as_str(), locations)?;
            let exists = |id: &Id| match budget.level {
                Level::Component => components.contains_key(id.as_str()),
                Level::Container => containers.contains(id.as_str()),
            };
            if budget.origins.iter().any(|id| !exists(id)) {
                return Err(invalid(
                    budget.id.as_str(),
                    "budget origin does not exist at level",
                    locations,
                ));
            }
        }
        for waiver in &doc.waivers {
            add_id(&waiver.id)?;
        }
    }
    super::scenarios::validate_scenarios(constraints, &mut ids, locations, policy, run)?;
    for doc in constraints.iter() {
        for waiver in &doc.waivers {
            run.check()?;
            if !budgets.contains(waiver.budget_id.as_str()) {
                return Err(invalid(
                    waiver.id.as_str(),
                    "waiver budget does not exist",
                    locations,
                ));
            }
        }
        for rule in &doc.require {
            for from in &rule.from {
                for to in &rule.to {
                    run.check()?;
                    let mut covered = false;
                    for allow in constraints.iter().flat_map(|d| &d.allow) {
                        run.check()?;
                        if allow.level == rule.level
                            && allow.projection == rule.projection
                            && allow
                                .from
                                .binary_search_by(|id| id.as_str().cmp(from.as_str()))
                                .is_ok()
                            && allow
                                .to
                                .binary_search_by(|id| id.as_str().cmp(to.as_str()))
                                .is_ok()
                        {
                            covered = true;
                            break;
                        }
                    }
                    if !covered {
                        return Err(diagnostic(
                            "contradictory_policy",
                            "required pair is not explicitly allowed",
                            rule.id.as_str(),
                            locations,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Decode the manifest without erasing field positions.
pub(super) fn decode_root(
    value: &super::yaml::Value,
    run: &RunContext,
) -> Result<crate::architecture::ModelRoot, CompileError> {
    use super::yaml::{Kind, Value};
    fn err(value: &Value, code: &'static str, reason: &str) -> CompileError {
        let mut error = CompileError::simple(code, reason);
        error.diagnostics[0].location = Some(value.location.clone());
        error
    }
    fn string(value: &Value) -> Result<&str, CompileError> {
        match &value.kind {
            Kind::String(s) => Ok(s),
            _ => Err(err(value, "architecture_invalid_input", "expected string")),
        }
    }
    fn paths(
        value: &Value,
        run: &RunContext,
    ) -> Result<Vec<crate::architecture::RelativePath>, CompileError> {
        let Kind::Sequence(items) = &value.kind else {
            return Err(err(value, "architecture_invalid_input", "expected list"));
        };
        let mut result = Vec::new();
        result
            .try_reserve(items.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        for item in items {
            run.check()?;
            result.push(
                crate::architecture::RelativePath::new(string(item)?)
                    .map_err(|_| err(item, "path_not_confined", "invalid relative path"))?,
            );
        }
        Ok(result)
    }
    let Kind::Mapping(entries) = &value.kind else {
        return Err(err(
            value,
            "architecture_invalid_input",
            "root must be a mapping",
        ));
    };
    let (mut version, mut model_id, mut architecture, mut constraints) = (None, None, None, None);
    for (key, field) in entries {
        run.check()?;
        match string(key)? {
            "schema_version" => {
                version = Some(
                    crate::architecture::SchemaVersion::new(string(field)?).map_err(|_| {
                        err(field, "unsupported_version", "unsupported schema version")
                    })?,
                )
            }
            "model_id" => {
                model_id =
                    Some(Id::new(string(field)?).map_err(|_| {
                        err(field, "architecture_invalid_input", "invalid model ID")
                    })?)
            }
            "architecture" => architecture = Some(paths(field, run)?),
            "constraints" => constraints = Some(paths(field, run)?),
            _ => return Err(err(key, "unknown_field", "unknown root field")),
        }
    }
    let missing = || {
        err(
            value,
            "architecture_invalid_input",
            "missing required root field",
        )
    };
    Ok(crate::architecture::ModelRoot {
        schema_version: version.ok_or_else(missing)?,
        model_id: model_id.ok_or_else(missing)?,
        architecture: architecture.ok_or_else(missing)?,
        constraints: constraints.ok_or_else(missing)?,
    })
}

/// Decode C4 architecture records with bounded, fixed grammar depth.
pub(super) fn decode_architecture(
    value: &super::yaml::Value,
    entities: &mut u64,
    entity_limit: u64,
    run: &RunContext,
) -> Result<ArchitectureDocument, CompileError> {
    use super::yaml::{Kind, Value};
    use crate::architecture::{
        ArtifactRef, Component, Container, ContainerKind, Digest, GitObjectId, Level, Relationship,
        RelativePath, Responsibility, SchemaVersion, System, Text,
    };
    fn bad(v: &Value, code: &'static str, message: &str) -> CompileError {
        let mut error = CompileError::simple(code, message);
        error.diagnostics[0].location = Some(v.location.clone());
        error
    }
    fn text(v: &Value) -> Result<&str, CompileError> {
        match &v.kind {
            Kind::String(s) => Ok(s),
            _ => Err(bad(v, "architecture_invalid_input", "expected string")),
        }
    }
    fn fields<'a>(
        v: &'a Value,
        names: &[&str],
        run: &RunContext,
    ) -> Result<HashMap<&'a str, &'a Value>, CompileError> {
        let Kind::Mapping(entries) = &v.kind else {
            return Err(bad(v, "architecture_invalid_input", "expected mapping"));
        };
        let mut result = HashMap::new();
        result
            .try_reserve(entries.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        for (key, value) in entries {
            run.check()?;
            let key_text = text(key)?;
            if !names.contains(&key_text) {
                return Err(bad(key, "unknown_field", "unknown field"));
            }
            result.insert(key_text, value);
        }
        for name in names {
            if !result.contains_key(name) {
                return Err(bad(
                    v,
                    "architecture_invalid_input",
                    "missing required field",
                ));
            }
        }
        Ok(result)
    }
    fn items(v: &Value) -> Result<&[Value], CompileError> {
        match &v.kind {
            Kind::Sequence(v) => Ok(v),
            _ => Err(bad(v, "architecture_invalid_input", "expected list")),
        }
    }
    fn id(v: &Value) -> Result<Id, CompileError> {
        Id::new(text(v)?).map_err(|_| bad(v, "architecture_invalid_input", "invalid ID"))
    }
    fn display(v: &Value) -> Result<Text, CompileError> {
        Text::new(text(v)?).map_err(|_| bad(v, "architecture_invalid_input", "empty text"))
    }
    fn account(count: usize, entities: &mut u64, limit: u64) -> Result<(), CompileError> {
        let count = u64::try_from(count)
            .map_err(|_| CompileError::simple("resource_limit", "entity count overflow"))?;
        *entities = entities
            .checked_add(count)
            .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
        if *entities > limit {
            return Err(CompileError::simple(
                "resource_limit",
                "authored entity limit exceeded",
            ));
        }
        Ok(())
    }
    fn responsibilities(
        v: &Value,
        entities: &mut u64,
        limit: u64,
        run: &RunContext,
    ) -> Result<Vec<Responsibility>, CompileError> {
        let values = items(v)?;
        account(values.len(), entities, limit)?;
        let mut out = Vec::new();
        out.try_reserve(values.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        for value in values {
            run.check()?;
            let f = fields(value, &["id", "statement"], run)?;
            out.push(Responsibility {
                id: id(f["id"])?,
                statement: display(f["statement"])?,
            });
        }
        Ok(out)
    }
    fn artifact(v: &Value, run: &RunContext) -> Result<ArtifactRef, CompileError> {
        let f = fields(v, &["repository", "commit", "path", "content_digest"], run)?;
        Ok(ArtifactRef {
            repository: display(f["repository"])?,
            commit: GitObjectId::new(text(f["commit"])?)
                .map_err(|_| bad(f["commit"], "architecture_invalid_input", "invalid commit"))?,
            path: RelativePath::new(text(f["path"])?)
                .map_err(|_| bad(f["path"], "path_not_confined", "invalid path"))?,
            content_digest: Digest::new(text(f["content_digest"])?).map_err(|_| {
                bad(
                    f["content_digest"],
                    "architecture_invalid_input",
                    "invalid digest",
                )
            })?,
        })
    }
    let f = fields(
        value,
        &[
            "schema_version",
            "systems",
            "containers",
            "components",
            "relationships",
        ],
        run,
    )?;
    let schema_version = SchemaVersion::new(text(f["schema_version"])?).map_err(|_| {
        bad(
            f["schema_version"],
            "unsupported_version",
            "unsupported schema version",
        )
    })?;
    let mut systems = Vec::new();
    let mut containers = Vec::new();
    let mut components = Vec::new();
    let mut relationships = Vec::new();
    account(items(f["systems"])?.len(), entities, entity_limit)?;
    for v in items(f["systems"])? {
        run.check()?;
        let p = fields(v, &["id", "name", "responsibilities"], run)?;
        systems
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        systems.push(System {
            id: id(p["id"])?,
            name: display(p["name"])?,
            responsibilities: responsibilities(p["responsibilities"], entities, entity_limit, run)?,
        });
    }
    account(items(f["containers"])?.len(), entities, entity_limit)?;
    for v in items(f["containers"])? {
        run.check()?;
        let p = fields(
            v,
            &["id", "name", "responsibilities", "system", "kind"],
            run,
        )?;
        let kind = match text(p["kind"])? {
            "application" => ContainerKind::Application,
            "data_store" => ContainerKind::DataStore,
            _ => {
                return Err(bad(
                    p["kind"],
                    "architecture_invalid_input",
                    "invalid container kind",
                ))
            }
        };
        containers
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        containers.push(Container {
            id: id(p["id"])?,
            name: display(p["name"])?,
            responsibilities: responsibilities(p["responsibilities"], entities, entity_limit, run)?,
            system: id(p["system"])?,
            kind,
        });
    }
    account(items(f["components"])?.len(), entities, entity_limit)?;
    for v in items(f["components"])? {
        run.check()?;
        let p = fields(v, &["id", "name", "responsibilities", "container"], run)?;
        components
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        components.push(Component {
            id: id(p["id"])?,
            name: display(p["name"])?,
            responsibilities: responsibilities(p["responsibilities"], entities, entity_limit, run)?,
            container: id(p["container"])?,
        });
    }
    account(items(f["relationships"])?.len(), entities, entity_limit)?;
    for v in items(f["relationships"])? {
        run.check()?;
        let p = fields(
            v,
            &["id", "level", "from", "to", "description", "contract_refs"],
            run,
        )?;
        let level = match text(p["level"])? {
            "component" => Level::Component,
            "container" => Level::Container,
            _ => {
                return Err(bad(
                    p["level"],
                    "architecture_invalid_input",
                    "invalid level",
                ))
            }
        };
        let mut contract_refs = Vec::new();
        for r in items(p["contract_refs"])? {
            run.check()?;
            contract_refs
                .try_reserve(1)
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            contract_refs.push(artifact(r, run)?);
        }
        relationships
            .try_reserve(1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        relationships.push(Relationship {
            id: id(p["id"])?,
            level,
            from: id(p["from"])?,
            to: id(p["to"])?,
            description: display(p["description"])?,
            contract_refs,
        });
    }
    Ok(ArchitectureDocument {
        schema_version,
        systems,
        containers,
        components,
        relationships,
    })
}

/// Index authored base declarations and scenario predicates for diagnostics.
/// Operation payloads are excluded except for nested scenario records; they
/// belong to transformed snapshots rather than the authored base model.
pub(super) fn index_record_locations(
    value: &super::yaml::Value,
    locations: &mut HashMap<String, SourceLocation>,
    run: &RunContext,
) -> Result<(), CompileError> {
    use super::yaml::{Kind, Value};
    fn field<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
        let Kind::Mapping(entries) = &v.kind else {
            return None;
        };
        entries.iter().find_map(|(key, value)| {
            if matches!(&key.kind, Kind::String(s) if s == name) {
                Some(value)
            } else {
                None
            }
        })
    }
    fn list(v: &Value) -> &[Value] {
        match &v.kind {
            Kind::Sequence(items) => items,
            _ => &[],
        }
    }
    fn add(v: &Value, locations: &mut HashMap<String, SourceLocation>) -> Result<(), CompileError> {
        if let Some(id) = field(v, "id") {
            if let Kind::String(text) = &id.kind {
                if Id::new(text).is_ok() && !locations.contains_key(text) {
                    locations
                        .try_reserve(1)
                        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                    locations.insert(text.clone(), id.location.clone());
                }
            }
        }
        Ok(())
    }
    for name in [
        "systems",
        "containers",
        "components",
        "relationships",
        "ownership_rules",
        "allow",
        "require",
        "budgets",
        "waivers",
    ] {
        if let Some(values) = field(value, name) {
            for record in list(values) {
                run.check()?;
                add(record, locations)?;
                if let Some(responsibilities) = field(record, "responsibilities") {
                    for responsibility in list(responsibilities) {
                        run.check()?;
                        add(responsibility, locations)?;
                    }
                }
            }
        }
    }
    let mut pending: Vec<&Value> = Vec::new();
    if let Some(values) = field(value, "scenarios") {
        pending
            .try_reserve(list(values).len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        pending.extend(list(values));
    }
    while let Some(scenario) = pending.pop() {
        run.check()?;
        add(scenario, locations)?;
        for name in ["preconditions", "obligations"] {
            if let Some(values) = field(scenario, name) {
                for predicate in list(values) {
                    run.check()?;
                    add(predicate, locations)?;
                }
            }
        }
        for name in ["baseline_transform", "candidate_transform"] {
            if let Some(values) = field(scenario, name) {
                for operation in list(values) {
                    run.check()?;
                    if let Some(record) =
                        field(operation, "constraint").and_then(|wrapper| field(wrapper, "record"))
                    {
                        if field(record, "parameters").is_some()
                            && field(record, "preconditions").is_some()
                        {
                            pending.try_reserve(1).map_err(|_| {
                                CompileError::simple("resource_limit", "allocation failed")
                            })?;
                            pending.push(record);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::compile::ExecutionContext;
    use std::sync::{atomic::AtomicBool, Arc};
    fn policy() -> ArchitectureExecutionPolicy {
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
        ArchitectureExecutionPolicy::from_json(serde_json::Value::Object(
            fields
                .into_iter()
                .map(|k| (k.into(), 100000.into()))
                .collect(),
        ))
        .unwrap()
    }
    fn base() -> ArchitectureDocument {
        serde_json::from_value(serde_json::json!({
        "schema_version":"0.1", "systems":[{"id":"sys","name":"System","responsibilities":[{"id":"r-sys","statement":"Own"}]}],
        "containers":[{"id":"con","name":"Container","system":"sys","kind":"application","responsibilities":[{"id":"r-con","statement":"Own"}]}],
        "components":[{"id":"a","name":"A","container":"con","responsibilities":[{"id":"r-a","statement":"Own"}]},{"id":"b","name":"B","container":"con","responsibilities":[{"id":"r-b","statement":"Own"}]}],"relationships":[]
    })).unwrap()
    }
    fn rules(allow: bool, require: bool) -> ConstraintDocument {
        let projection = serde_json::json!({"version":"rust-q1/0.1","domain_ref":format!("sha256:{}", "a".repeat(64))});
        let entry = |id| serde_json::json!({"id":id,"level":"component","from":["a"],"to":["b"],"projection":projection});
        serde_json::from_value(serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":if allow {vec![entry("allow-a")]} else {vec![]},"require":if require {vec![entry("require-a")]} else {vec![]},"budgets":[],"waivers":[],"scenarios":[]})).unwrap()
    }
    #[test]
    fn root_decoder_preserves_unknown_field_position() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("root.yaml").unwrap();
        let input = b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\nextra: 1\n";
        let value = super::super::yaml::parse(input, &path, 1000, 8, &ctx).unwrap();
        let error = decode_root(&value, &ctx).unwrap_err();
        assert_eq!(error.code(), "unknown_field");
        assert_eq!(error.diagnostics()[0].location().unwrap().line(), 5);
    }
    #[test]
    fn architecture_decoder_checks_fields_and_scalars() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("architecture.yaml").unwrap();
        let input = serde_json::to_string(&base()).unwrap();
        let value = super::super::yaml::parse(input.as_bytes(), &path, 10000, 32, &ctx).unwrap();
        assert_eq!(
            decode_architecture(&value, &mut 0, p.max_authored_entities, &ctx).unwrap(),
            base()
        );
        assert_eq!(
            decode_architecture(&value, &mut 0, 7, &ctx)
                .unwrap_err()
                .code(),
            "resource_limit"
        );
        let bad = input.replace("\"systems\":", "\"unexpected\":0,\"systems\":");
        let value = super::super::yaml::parse(bad.as_bytes(), &path, 10000, 32, &ctx).unwrap();
        assert_eq!(
            decode_architecture(&value, &mut 0, p.max_authored_entities, &ctx)
                .unwrap_err()
                .code(),
            "unknown_field"
        );
    }
    #[test]
    fn strict_edge_decoder_reports_unknown_key() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("rules.yaml").unwrap();
        let text = format!("id: rule\nlevel: component\nfrom: [a]\nto: [b]\nprojection: {{version: 'rust-q1/0.1', domain_ref: 'sha256:{}'}}\n", "a".repeat(64));
        let value = super::super::yaml::parse(text.as_bytes(), &path, 10000, 12, &ctx).unwrap();
        assert_eq!(
            super::super::constraints::decode::edge(&value, &ctx)
                .unwrap()
                .id
                .as_str(),
            "rule"
        );
        let text = format!("{text}extra: true\n");
        let value = super::super::yaml::parse(text.as_bytes(), &path, 10000, 12, &ctx).unwrap();
        let error = super::super::constraints::decode::edge(&value, &ctx).unwrap_err();
        assert_eq!(error.code(), "unknown_field");
        assert_eq!(error.diagnostics()[0].location().unwrap().line(), 6);
    }
    #[test]
    fn constraint_document_decodes_and_counts_records() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("rules.yaml").unwrap();
        let original = rules(true, true);
        let input = serde_json::to_string(&original).unwrap();
        let value = super::super::yaml::parse(input.as_bytes(), &path, 10000, 16, &ctx).unwrap();
        let (decoded, scenarios) =
            super::super::constraints::decode::document(&value, &mut 0, 2, &ctx).unwrap();
        assert!(scenarios.is_empty());
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        assert_eq!(
            super::super::constraints::decode::document(&value, &mut 0, 1, &ctx)
                .unwrap_err()
                .code(),
            "resource_limit"
        );
    }
    #[test]
    fn waiver_requires_valid_quoted_utc_timestamp() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("rules.yaml").unwrap();
        let digest = format!("sha256:{}", "a".repeat(64));
        let make = |expiry| serde_json::json!({"id":"waiver", "budget_id":"budget", "baseline":{"kind":"input", "id":digest}, "original_limit":1, "ceiling":2, "expires_at":expiry, "approval_ref":{"reference":{"repository":"repo", "commit":"a".repeat(40), "path":"approval.yaml", "content_digest":digest}, "context_id":"context"}});
        for expiry in [
            "2024-02-29T23:59:59Z",
            "2024-02-29t23:59:59z",
            "2024-02-29T23:59:59.12+00:00",
        ] {
            let input = serde_json::to_vec(&make(expiry)).unwrap();
            let value = super::super::yaml::parse(&input, &path, 10000, 16, &ctx).unwrap();
            assert!(
                super::super::constraints::decode::waiver(&value, &ctx).is_ok(),
                "{expiry}"
            );
        }
        for expiry in [
            "2023-02-29T23:59:59Z",
            "2024-02-29T23:59:59-00:00",
            "2024-02-29T23:59:59+01:00",
        ] {
            let input = serde_json::to_vec(&make(expiry)).unwrap();
            let value = super::super::yaml::parse(&input, &path, 10000, 16, &ctx).unwrap();
            assert!(
                super::super::constraints::decode::waiver(&value, &ctx).is_err(),
                "{expiry}"
            );
        }
    }
    #[test]
    fn cross_document_decoding_and_future_scenario_target() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("architecture.yaml").unwrap();
        let input = serde_json::to_vec(&base()).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let mut entities = 0;
        let architecture =
            decode_architecture(&value, &mut entities, p.max_authored_entities, &ctx).unwrap();
        let mut rule = serde_json::to_value(rules(true, true)).unwrap();
        rule["scenarios"] = serde_json::json!([{ "id":"scenario", "parameters":{}, "preconditions":[{"kind":"element_exists", "id":"exists", "element":{"level":"component", "id":"future"}}], "obligations":[], "baseline_transform":[], "candidate_transform":[] }]);
        let input = serde_json::to_vec(&rule).unwrap();
        let rule_path = crate::architecture::RelativePath::new("rules.yaml").unwrap();
        let value = super::super::yaml::parse(&input, &rule_path, 10000, 32, &ctx).unwrap();
        let (mut document, pending) = super::super::constraints::decode::document(
            &value,
            &mut entities,
            p.max_authored_entities,
            &ctx,
        )
        .unwrap();
        let mut counts = super::super::scenarios::decode::Counts {
            entities,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        document.scenarios = super::super::scenarios::decode::scenarios(
            pending,
            &mut counts,
            &mut Default::default(),
            &p,
            &ctx,
        )
        .unwrap();
        assert!(validate_base(
            &mut [architecture],
            &mut [document],
            &HashMap::new(),
            &p,
            &ctx
        )
        .is_ok());
    }
    #[test]
    fn canonical_validator_checkpoints_preserve_results_and_cancel() {
        let docs = [base()];
        let ordinary = crate::architecture::validate(&docs);
        let checked =
            crate::architecture::validate_with_checkpoint(&docs, || Ok::<(), ()>(())).unwrap();
        assert_eq!(
            ordinary
                .iter()
                .map(|d| (&d.subject, d.code))
                .collect::<Vec<_>>(),
            checked
                .iter()
                .map(|d| (&d.subject, d.code))
                .collect::<Vec<_>>()
        );
        let mut calls = 0;
        assert_eq!(
            crate::architecture::validate_with_checkpoint(&docs, || {
                calls += 1;
                if calls == 3 {
                    Err("cancelled")
                } else {
                    Ok(())
                }
            })
            .unwrap_err(),
            "cancelled"
        );
        let p = policy();
        let ctx = ExecutionContext::new(
            Arc::new(AtomicBool::new(false)),
            Some(std::time::Instant::now()),
        )
        .start(&p)
        .unwrap();
        assert_eq!(
            crate::architecture::validate_with_checkpoint(&docs, || ctx.check())
                .unwrap_err()
                .code(),
            "deadline_exceeded"
        );
    }
    #[test]
    fn semantic_diagnostic_retains_record_location() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("architecture.yaml").unwrap();
        let mut doc = base();
        doc.components[0].container = Id::new("missing").unwrap();
        let input = serde_json::to_vec(&doc).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let mut locations = HashMap::new();
        index_record_locations(&value, &mut locations, &ctx).unwrap();
        let decoded = decode_architecture(&value, &mut 0, p.max_authored_entities, &ctx).unwrap();
        let error = validate_base(&mut [decoded], &mut [], &locations, &p, &ctx).unwrap_err();
        assert_eq!(error.code(), "invalid_hierarchy");
        let location = error.diagnostics()[0].location().unwrap();
        assert_eq!(location.path().as_str(), "architecture.yaml");
        assert!(location.start() < location.end());
    }
    #[test]
    fn requirement_needs_matching_explicit_allow() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let loc = HashMap::new();
        assert!(validate_base(&mut [base()], &mut [rules(true, true)], &loc, &p, &ctx).is_ok());
        assert_eq!(
            validate_base(&mut [base()], &mut [rules(false, true)], &loc, &p, &ctx)
                .unwrap_err()
                .code(),
            "contradictory_policy"
        );
        assert!(validate_base(&mut [base()], &mut [rules(true, false)], &loc, &p, &ctx).is_ok());
    }
    #[test]
    fn projection_and_duplicate_references_are_not_silently_accepted() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let locations = HashMap::new();
        let mut constraint = rules(true, true);
        constraint.require[0].projection.domain_ref =
            crate::architecture::Digest::new(format!("sha256:{}", "b".repeat(64))).unwrap();
        assert_eq!(
            validate_base(&mut [base()], &mut [constraint], &locations, &p, &ctx)
                .unwrap_err()
                .code(),
            "contradictory_policy"
        );
        let reference: crate::architecture::ArtifactRef = serde_json::from_value(serde_json::json!({"repository":"repo","commit":"a".repeat(40),"path":"x","content_digest":format!("sha256:{}", "a".repeat(64))})).unwrap();
        let mut architecture = base();
        architecture
            .relationships
            .push(crate::architecture::Relationship {
                id: Id::new("relation").unwrap(),
                level: Level::Component,
                from: Id::new("a").unwrap(),
                to: Id::new("b").unwrap(),
                description: crate::architecture::Text::new("dependency").unwrap(),
                contract_refs: vec![reference.clone(), reference],
            });
        assert_eq!(
            validate_base(&mut [architecture], &mut [], &locations, &p, &ctx)
                .unwrap_err()
                .code(),
            "architecture_invalid_input"
        );
        let mut constraint = rules(false, false);
        constraint.scenarios = serde_json::from_value(serde_json::json!([{"id":"a","parameters":{},"preconditions":[],"obligations":[],"baseline_transform":[],"candidate_transform":[]}])).unwrap();
        assert_eq!(
            validate_base(&mut [base()], &mut [constraint], &locations, &p, &ctx)
                .unwrap_err()
                .code(),
            "duplicate_id"
        );
    }
    #[test]
    fn architecture_parent_and_pair_limit_reject() {
        let mut p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let mut doc = base();
        doc.components[0].container = Id::new("missing").unwrap();
        assert_eq!(
            validate_base(&mut [doc], &mut [], &HashMap::new(), &p, &ctx)
                .unwrap_err()
                .code(),
            "invalid_hierarchy"
        );
        p.max_expanded_rule_pairs = 1;
        assert_eq!(
            validate_base(
                &mut [base()],
                &mut [rules(true, true)],
                &HashMap::new(),
                &p,
                &ctx
            )
            .unwrap_err()
            .code(),
            "resource_limit"
        );
    }
}
