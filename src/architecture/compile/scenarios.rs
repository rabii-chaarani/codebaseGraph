//! Typed, isolated scenario grammar; execution belongs to the scenario runner.
use super::super::{ArtifactRef, Component, Container, Digest, Id, Level, Relationship, System};
use super::constraints::{Budget, EdgeRule, OwnershipRule, ProjectionRef, Waiver};
#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct ElementRef {
    pub(super) level: Level,
    pub(super) id: Id,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Parameter {
    #[non_exhaustive]
    ElementId { level: Level, id: Id },
    #[non_exhaustive]
    ContractRef { reference: ArtifactRef },
    #[non_exhaustive]
    Integer { value: u64 },
    #[non_exhaustive]
    String { value: String },
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    #[non_exhaustive]
    ElementExists { id: Id, element: ElementRef },
    #[non_exhaustive]
    RelationshipExists {
        id: Id,
        from: ElementRef,
        to: ElementRef,
        level: Level,
        projection: ProjectionRef,
    },
    #[non_exhaustive]
    ContractRefEquals {
        id: Id,
        relationship_id: Id,
        expected: Vec<ArtifactRef>,
    },
    #[non_exhaustive]
    Reachable {
        id: Id,
        from: ElementRef,
        to: ElementRef,
        level: Level,
        projection: ProjectionRef,
    },
    #[non_exhaustive]
    NoBypass {
        id: Id,
        from: ElementRef,
        to: ElementRef,
        via: ElementRef,
        level: Level,
        projection: ProjectionRef,
    },
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ElementRecord {
    #[non_exhaustive]
    System(System),
    #[non_exhaustive]
    Container(Container),
    #[non_exhaustive]
    Component(Component),
}
#[cfg_attr(test, derive(Clone, Debug, Serialize, Deserialize))]
#[cfg_attr(
    test,
    serde(
        tag = "kind",
        content = "record",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ConstraintRecord {
    #[non_exhaustive]
    OwnershipRule(OwnershipRule),
    #[non_exhaustive]
    Allow(EdgeRule),
    #[non_exhaustive]
    Require(EdgeRule),
    #[non_exhaustive]
    Budget(Budget),
    #[non_exhaustive]
    Waiver(Waiver),
    #[non_exhaustive]
    Scenario(Box<Scenario>),
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    System,
    Container,
    Component,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    OwnershipRule,
    Allow,
    Require,
    Budget,
    Waiver,
    Scenario,
}
#[cfg_attr(test, derive(Clone, Debug, Serialize, Deserialize))]
#[cfg_attr(
    test,
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum Operation {
    #[non_exhaustive]
    AddElement { element: ElementRecord },
    #[non_exhaustive]
    ReplaceElement {
        element: ElementRecord,
        expected_record_digest: Digest,
    },
    #[non_exhaustive]
    RemoveElement {
        element_kind: ElementKind,
        id: Id,
        expected_record_digest: Digest,
    },
    #[non_exhaustive]
    AddRelationship { relationship: Relationship },
    #[non_exhaustive]
    ReplaceRelationship {
        relationship: Relationship,
        expected_record_digest: Digest,
    },
    #[non_exhaustive]
    RemoveRelationship {
        id: Id,
        expected_record_digest: Digest,
    },
    #[non_exhaustive]
    AddConstraint { constraint: ConstraintRecord },
    #[non_exhaustive]
    ReplaceConstraint {
        constraint: ConstraintRecord,
        expected_record_digest: Digest,
    },
    #[non_exhaustive]
    RemoveConstraint {
        constraint_kind: ConstraintKind,
        id: Id,
        expected_record_digest: Digest,
    },
}
#[cfg_attr(test, derive(Clone, Debug, Serialize, Deserialize))]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Scenario {
    pub(super) id: Id,
    pub(super) parameters: BTreeMap<String, Parameter>,
    pub(super) preconditions: Vec<Predicate>,
    pub(super) obligations: Vec<Predicate>,
    pub(super) baseline_transform: Vec<Operation>,
    pub(super) candidate_transform: Vec<Operation>,
}

// Embedded scenarios are unbounded up to policy depth. Dropping the nested
// transform tree must not consume one native stack frame per scenario.
impl Drop for Scenario {
    fn drop(&mut self) {
        fn collect(operations: Vec<Operation>, pending: &mut Vec<Scenario>) {
            for operation in operations {
                match operation {
                    Operation::AddConstraint {
                        constraint: ConstraintRecord::Scenario(scenario),
                    }
                    | Operation::ReplaceConstraint {
                        constraint: ConstraintRecord::Scenario(scenario),
                        ..
                    } => pending.push(*scenario),
                    _ => {}
                }
            }
        }
        let mut pending = Vec::new();
        collect(std::mem::take(&mut self.baseline_transform), &mut pending);
        collect(std::mem::take(&mut self.candidate_transform), &mut pending);
        while let Some(mut scenario) = pending.pop() {
            collect(
                std::mem::take(&mut scenario.baseline_transform),
                &mut pending,
            );
            collect(
                std::mem::take(&mut scenario.candidate_transform),
                &mut pending,
            );
        }
    }
}

/// Check local predicates and account for nested scenarios without resolving
/// transform targets against the initial snapshot.
pub(super) fn validate_scenarios(
    documents: &[super::constraints::ConstraintDocument],
    ids: &mut std::collections::HashSet<String>,
    locations: &std::collections::HashMap<String, super::SourceLocation>,
    policy: &super::ArchitectureExecutionPolicy,
    run: &super::policy::RunContext,
) -> Result<(), super::CompileError> {
    use super::CompileError;
    let mut pending = Vec::new();
    for document in documents {
        pending
            .try_reserve(document.scenarios.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        pending.extend(document.scenarios.iter().map(|scenario| (scenario, true)));
    }
    let mut scenarios = 0u64;
    let mut operations = 0u64;
    while let Some((scenario, base)) = pending.pop() {
        let mut local_ids = std::collections::HashSet::new();
        let namespace = if base { &mut *ids } else { &mut local_ids };
        run.check()?;
        scenarios = scenarios
            .checked_add(1)
            .ok_or_else(|| CompileError::simple("resource_limit", "scenario count overflow"))?;
        if scenarios > policy.max_scenarios {
            return Err(CompileError::simple(
                "resource_limit",
                "scenario limit exceeded",
            ));
        }
        if !namespace.insert(scenario.id.as_str().to_owned()) {
            return Err(super::validate::diagnostic(
                "duplicate_id",
                "duplicate scenario ID",
                scenario.id.as_str(),
                locations,
            ));
        }
        for name in scenario.parameters.keys() {
            run.check()?;
            Id::new(name).map_err(|_| {
                CompileError::simple("architecture_invalid_input", "invalid parameter name")
            })?;
        }
        for predicates in [&scenario.preconditions, &scenario.obligations] {
            for predicate in predicates {
                run.check()?;
                let id = match predicate {
                    Predicate::ElementExists { id, .. }
                    | Predicate::RelationshipExists { id, .. }
                    | Predicate::ContractRefEquals { id, .. }
                    | Predicate::Reachable { id, .. }
                    | Predicate::NoBypass { id, .. } => id,
                };
                if !namespace.insert(id.as_str().to_owned()) {
                    return Err(super::validate::diagnostic(
                        "duplicate_id",
                        "duplicate predicate ID",
                        id.as_str(),
                        locations,
                    ));
                }
                match predicate {
                    Predicate::RelationshipExists {
                        from,
                        to,
                        level,
                        projection,
                        ..
                    }
                    | Predicate::Reachable {
                        from,
                        to,
                        level,
                        projection,
                        ..
                    } => {
                        if from.level != *level || to.level != *level {
                            return Err(super::validate::diagnostic(
                                "invalid_reference",
                                "predicate endpoint level mismatch",
                                id.as_str(),
                                locations,
                            ));
                        }
                        if projection.version != "rust-q1/0.1" {
                            return Err(super::validate::diagnostic(
                                "unsupported_version",
                                "unsupported projection version",
                                id.as_str(),
                                locations,
                            ));
                        }
                    }
                    Predicate::NoBypass {
                        from,
                        to,
                        via,
                        level,
                        projection,
                        ..
                    } => {
                        if from.level != *level
                            || to.level != *level
                            || via.level != *level
                            || from == to
                            || from == via
                            || to == via
                        {
                            return Err(super::validate::diagnostic(
                                "invalid_reference",
                                "invalid no_bypass endpoints",
                                id.as_str(),
                                locations,
                            ));
                        }
                        if projection.version != "rust-q1/0.1" {
                            return Err(super::validate::diagnostic(
                                "unsupported_version",
                                "unsupported projection version",
                                id.as_str(),
                                locations,
                            ));
                        }
                        let mut paired = false;
                        for other in predicates {
                            run.check()?;
                            if let Predicate::Reachable {
                                from: f,
                                to: t,
                                level: l,
                                projection: p,
                                ..
                            } = other
                            {
                                if f == from && t == to && l == level && p == projection {
                                    paired = true;
                                    break;
                                }
                            }
                        }
                        if !paired {
                            return Err(super::validate::diagnostic(
                                "architecture_invalid_input",
                                "no_bypass requires paired reachable",
                                id.as_str(),
                                locations,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        for operation in scenario
            .baseline_transform
            .iter()
            .chain(&scenario.candidate_transform)
        {
            run.check()?;
            operations = operations.checked_add(1).ok_or_else(|| {
                CompileError::simple("resource_limit", "operation count overflow")
            })?;
            if operations > policy.max_proposal_operations {
                return Err(CompileError::simple(
                    "resource_limit",
                    "operation limit exceeded",
                ));
            }
            if let Operation::AddConstraint {
                constraint: ConstraintRecord::Scenario(inner),
            }
            | Operation::ReplaceConstraint {
                constraint: ConstraintRecord::Scenario(inner),
                ..
            } = operation
            {
                pending
                    .try_reserve(1)
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                pending.push((inner, false));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::compile::{ArchitectureExecutionPolicy, ExecutionContext};
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
    fn doc(paired: bool, mismatch: bool) -> super::super::constraints::ConstraintDocument {
        let projection = serde_json::json!({"version":"rust-q1/0.1","domain_ref":format!("sha256:{}", "a".repeat(64))});
        let from = serde_json::json!({"level":"component","id":"new-a"});
        let to = serde_json::json!({"level":"component","id":"new-b"});
        let via =
            serde_json::json!({"level": if mismatch {"container"} else {"component"},"id":"new-c"});
        let mut predicates = vec![
            serde_json::json!({"kind":"no_bypass","id":"no-bypass","from":from,"to":to,"via":via,"level":"component","projection":projection}),
        ];
        if paired {
            predicates.push(serde_json::json!({"kind":"reachable","id":"reach","from":from,"to":to,"level":"component","projection":projection}));
        }
        serde_json::from_value(serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[{"id":"scenario","parameters":{},"preconditions":predicates,"obligations":[],"baseline_transform":[],"candidate_transform":[]}]})).unwrap()
    }
    #[test]
    fn deeply_embedded_scenario_drop_is_iterative() {
        fn empty() -> Scenario {
            Scenario {
                id: Id::new("scenario").unwrap(),
                parameters: Default::default(),
                preconditions: vec![],
                obligations: vec![],
                baseline_transform: vec![],
                candidate_transform: vec![],
            }
        }
        let mut scenario = empty();
        for _ in 0..5000 {
            let mut outer = empty();
            outer.baseline_transform.push(Operation::AddConstraint {
                constraint: ConstraintRecord::Scenario(Box::new(scenario)),
            });
            scenario = outer;
        }
        drop(scenario);
    }
    #[test]
    fn parameter_refs_require_the_correct_local_type() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("scenario.yaml").unwrap();
        let params = super::super::yaml::parse(b"target: {kind: element_id, level: component, id: future}\nwrong: {kind: string, value: future}", &path, 1000, 12, &ctx).unwrap();
        let bindings = decode::parameters(&params, &ctx).unwrap();
        let negative_zero =
            super::super::yaml::parse(b"{n: {kind: integer, value: -0}}", &path, 1000, 12, &ctx)
                .unwrap();
        assert!(decode::parameters(&negative_zero, &ctx).is_err());
        let predicate = super::super::yaml::parse(
            b"{kind: element_exists, id: exists, element: {parameter_ref: target}}",
            &path,
            1000,
            12,
            &ctx,
        )
        .unwrap();
        assert!(matches!(
            decode::predicate(&predicate, &bindings, &ctx).unwrap(),
            Predicate::ElementExists {
                element: ElementRef {
                    level: Level::Component,
                    ..
                },
                ..
            }
        ));
        let wrong = super::super::yaml::parse(
            b"{kind: element_exists, id: exists, element: {parameter_ref: wrong}}",
            &path,
            1000,
            12,
            &ctx,
        )
        .unwrap();
        assert!(decode::predicate(&wrong, &bindings, &ctx).is_err());
        assert!(decode::predicate(&predicate, &decode::Bindings::new(), &ctx).is_err());
    }
    #[test]
    fn substitution_copies_whole_values_and_keeps_scope() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("scenario.yaml").unwrap();
        let params = super::super::yaml::parse(b"target: {kind: element_id, level: component, id: future}\nn: {kind: integer, value: 7}", &path, 1000, 12, &ctx).unwrap();
        let bindings = decode::parameters(&params, &ctx).unwrap();
        let source = super::super::yaml::parse(
            b"{element: {parameter_ref: target}, count: {parameter_ref: n}}",
            &path,
            1000,
            12,
            &ctx,
        )
        .unwrap();
        let mut budget = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        let copied = decode::substitute(&source, &bindings, &mut budget, &ctx).unwrap();
        let super::super::yaml::Kind::Mapping(entries) = &copied.kind else {
            panic!()
        };
        assert!(matches!(
            &entries[0].1.kind,
            super::super::yaml::Kind::Mapping(_)
        ));
        assert!(
            matches!(&entries[1].1.kind, super::super::yaml::Kind::Integer(n, _) if n.as_u64() == Some(7))
        );
        assert!(decode::substitute(&source, &decode::Bindings::new(), &mut budget, &ctx).is_err());
    }
    #[test]
    fn prepare_counts_and_decodes_one_scope() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("scenario.yaml").unwrap();
        let input = b"{id: sample, parameters: {target: {kind: element_id, level: component, id: future}}, preconditions: [{kind: element_exists, id: exists, element: {parameter_ref: target}}], obligations: [], baseline_transform: [], candidate_transform: []}";
        let value = super::super::yaml::parse(input, &path, 1000, 12, &ctx).unwrap();
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        let prepared =
            decode::prepare(&value, &mut counts, &mut Default::default(), &p, &ctx).unwrap();
        assert_eq!(
            (counts.entities, counts.scenarios, counts.operations),
            (2, 1, 0)
        );
        assert_eq!(prepared.scenario.preconditions.len(), 1);
        assert!(prepared.baseline.is_empty() && prepared.candidate.is_empty());
    }
    #[test]
    fn operations_and_embedded_scope_assemble_iteratively() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("scenario.yaml").unwrap();
        let digest = format!("sha256:{}", "a".repeat(64));
        let system = serde_json::json!({"kind":"system","record":{"id":"future","name":"Future","responsibilities":[{"id":"future-resp","statement":"Own"}]}});
        let relationship = serde_json::json!({"id":"future-rel","level":"component","from":"x","to":"y","description":"Future","contract_refs":[]});
        let projection = serde_json::json!({"version":"rust-q1/0.1","domain_ref":digest});
        let constraint = serde_json::json!({"kind":"allow","record":{"id":"future-allow","level":"component","from":["x"],"to":["y"],"projection":projection}});
        let operations = vec![
            serde_json::json!({"kind":"add_element","element":system}),
            serde_json::json!({"kind":"replace_element","element":system,"expected_record_digest":digest}),
            serde_json::json!({"kind":"remove_element","element_kind":"system","id":"future","expected_record_digest":digest}),
            serde_json::json!({"kind":"add_relationship","relationship":relationship}),
            serde_json::json!({"kind":"replace_relationship","relationship":relationship,"expected_record_digest":digest}),
            serde_json::json!({"kind":"remove_relationship","id":"future-rel","expected_record_digest":digest}),
            serde_json::json!({"kind":"add_constraint","constraint":constraint}),
            serde_json::json!({"kind":"replace_constraint","constraint":constraint,"expected_record_digest":digest}),
            serde_json::json!({"kind":"remove_constraint","constraint_kind":"allow","id":"future-allow","expected_record_digest":digest}),
        ];
        let scenario = serde_json::json!([{ "id":"scenario", "parameters":{}, "preconditions":[], "obligations":[], "baseline_transform":operations, "candidate_transform":[] }]);
        let input = serde_json::to_vec(&scenario).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let super::super::yaml::Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        let decoded =
            decode::scenarios(values, &mut counts, &mut Default::default(), &p, &ctx).unwrap();
        assert_eq!(decoded[0].baseline_transform.len(), 9);
        assert_eq!(counts.operations, 9);
        let bad = serde_json::json!([{ "id":"outer", "parameters":{"target":{"kind":"element_id","level":"component","id":"future"}}, "preconditions":[], "obligations":[], "baseline_transform":[{"kind":"add_constraint","constraint":{"kind":"scenario","record":{"id":"inner","parameters":{},"preconditions":[{"kind":"element_exists","id":"exists","element":{"parameter_ref":"target"}}],"obligations":[],"baseline_transform":[],"candidate_transform":[]}}}], "candidate_transform":[] }]);
        let input = serde_json::to_vec(&bad).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let super::super::yaml::Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        assert!(decode::scenarios(values, &mut counts, &mut Default::default(), &p, &ctx).is_err());
        let mut local = bad.clone();
        local[0]["baseline_transform"][0]["constraint"]["record"]["parameters"]["target"] =
            serde_json::json!({"kind":"element_id", "level":"component", "id":"inner-target"});
        let input = serde_json::to_vec(&local).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let super::super::yaml::Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        assert!(decode::scenarios(values, &mut counts, &mut Default::default(), &p, &ctx).is_ok());
        let mut malformed = scenario.clone();
        malformed[0]["baseline_transform"][1]
            .as_object_mut()
            .unwrap()
            .remove("expected_record_digest");
        let input = serde_json::to_vec(&malformed).unwrap();
        let value = super::super::yaml::parse(&input, &path, 10000, 32, &ctx).unwrap();
        let super::super::yaml::Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        assert!(decode::scenarios(values, &mut counts, &mut Default::default(), &p, &ctx).is_err());
    }
    #[test]
    fn nested_scenarios_decode_without_recursion() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        let path = crate::architecture::RelativePath::new("deep.yaml").unwrap();
        let mut input = String::from("[");
        for _ in 0..1000 {
            input.push_str("{id: scenario, parameters: {}, preconditions: [], obligations: [], baseline_transform: [{kind: add_constraint, constraint: {kind: scenario, record: ");
        }
        input.push_str("{id: leaf, parameters: {}, preconditions: [], obligations: [], baseline_transform: [], candidate_transform: []}");
        for _ in 0..1000 {
            input.push_str("}}], candidate_transform: []}");
        }
        input.push(']');
        let value =
            super::super::yaml::parse(input.as_bytes(), &path, 1000000, 10000, &ctx).unwrap();
        let super::super::yaml::Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        let mut counts = decode::Counts {
            entities: 0,
            scenarios: 0,
            operations: 0,
            expanded_bytes: 0,
            expanded_limit: u64::MAX,
        };
        let decoded =
            decode::scenarios(values, &mut counts, &mut Default::default(), &p, &ctx).unwrap();
        assert_eq!((counts.scenarios, counts.operations), (1001, 1000));
        drop(decoded);
    }
    #[test]
    fn predicates_pair_without_requiring_base_targets() {
        let p = policy();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
            .start(&p)
            .unwrap();
        assert!(validate_scenarios(
            &[doc(true, false)],
            &mut Default::default(),
            &Default::default(),
            &p,
            &ctx
        )
        .is_ok());
        assert!(validate_scenarios(
            &[doc(false, false)],
            &mut Default::default(),
            &Default::default(),
            &p,
            &ctx
        )
        .is_err());
        assert!(validate_scenarios(
            &[doc(true, true)],
            &mut Default::default(),
            &Default::default(),
            &p,
            &ctx
        )
        .is_err());
    }
}

/// Scenario-local typed substitutions. Parameter values themselves are decoded
/// literally and never recursively resolve another parameter.
pub(super) mod decode {
    use super::super::constraints::decode as d;
    use super::super::{
        policy::RunContext,
        yaml::{Kind, Value},
        CompileError,
    };
    use super::*;
    use crate::architecture::Digest;
    use std::collections::BTreeMap;
    pub(in crate::architecture::compile) type Bindings = BTreeMap<String, Parameter>;
    fn binding<'a>(
        value: &Value,
        bindings: &'a Bindings,
        run: &RunContext,
    ) -> Result<Option<&'a Parameter>, CompileError> {
        let Kind::Mapping(fields) = &value.kind else {
            return Ok(None);
        };
        let found = fields
            .iter()
            .find(|(k, _)| matches!(&k.kind, Kind::String(s) if s == "parameter_ref"));
        let Some((key, reference)) = found else {
            return Ok(None);
        };
        if fields.len() != 1 {
            return Err(d::error(
                key,
                "architecture_invalid_input",
                "parameter reference must be whole object",
            ));
        }
        run.check()?;
        let name = d::id(reference)?;
        bindings.get(name.as_str()).map(Some).ok_or_else(|| {
            d::error(
                reference,
                "invalid_reference",
                "parameter not bound in this scenario",
            )
        })
    }
    fn string<'a>(
        value: &'a Value,
        bindings: &'a Bindings,
        run: &RunContext,
    ) -> Result<&'a str, CompileError> {
        match binding(value, bindings, run)? {
            Some(Parameter::String { value }) => Ok(value),
            Some(_) => Err(d::error(
                value,
                "architecture_invalid_input",
                "parameter has wrong type for string",
            )),
            None => d::text(value),
        }
    }
    fn id(value: &Value, bindings: &Bindings, run: &RunContext) -> Result<Id, CompileError> {
        Id::new(string(value, bindings, run)?)
            .map_err(|_| d::error(value, "architecture_invalid_input", "invalid ID"))
    }
    fn level(value: &Value, bindings: &Bindings, run: &RunContext) -> Result<Level, CompileError> {
        match string(value, bindings, run)? {
            "component" => Ok(Level::Component),
            "container" => Ok(Level::Container),
            _ => Err(d::error(
                value,
                "architecture_invalid_input",
                "invalid level",
            )),
        }
    }
    fn element(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<ElementRef, CompileError> {
        match binding(value, bindings, run)? {
            Some(Parameter::ElementId { level, id }) => Ok(ElementRef {
                level: *level,
                id: id.clone(),
            }),
            Some(_) => Err(d::error(
                value,
                "architecture_invalid_input",
                "parameter has wrong type for element",
            )),
            None => {
                let f = d::fields(value, &["level", "id"], &[], run)?;
                Ok(ElementRef {
                    level: level(f["level"], bindings, run)?,
                    id: id(f["id"], bindings, run)?,
                })
            }
        }
    }
    fn projection(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<super::ProjectionRef, CompileError> {
        if binding(value, bindings, run)?.is_some() {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "parameter has wrong type for projection",
            ));
        }
        let f = d::fields(value, &["version", "domain_ref"], &[], run)?;
        let version = string(f["version"], bindings, run)?;
        if version != "rust-q1/0.1" {
            return Err(d::error(
                f["version"],
                "unsupported_version",
                "unsupported projection version",
            ));
        }
        let domain_ref = Digest::new(string(f["domain_ref"], bindings, run)?).map_err(|_| {
            d::error(
                f["domain_ref"],
                "architecture_invalid_input",
                "invalid digest",
            )
        })?;
        Ok(super::ProjectionRef {
            version: version.to_owned(),
            domain_ref,
        })
    }
    fn artifact(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<ArtifactRef, CompileError> {
        match binding(value, bindings, run)? {
            Some(Parameter::ContractRef { reference }) => Ok(reference.clone()),
            Some(_) => Err(d::error(
                value,
                "architecture_invalid_input",
                "parameter has wrong type for artifact",
            )),
            None => {
                use crate::architecture::{GitObjectId, RelativePath, Text};
                let f = d::fields(
                    value,
                    &["repository", "commit", "path", "content_digest"],
                    &[],
                    run,
                )?;
                Ok(ArtifactRef {
                    repository: Text::new(string(f["repository"], bindings, run)?).map_err(
                        |_| {
                            d::error(
                                f["repository"],
                                "architecture_invalid_input",
                                "empty repository",
                            )
                        },
                    )?,
                    commit: GitObjectId::new(string(f["commit"], bindings, run)?).map_err(
                        |_| d::error(f["commit"], "architecture_invalid_input", "invalid commit"),
                    )?,
                    path: RelativePath::new(string(f["path"], bindings, run)?)
                        .map_err(|_| d::error(f["path"], "path_not_confined", "invalid path"))?,
                    content_digest: Digest::new(string(f["content_digest"], bindings, run)?)
                        .map_err(|_| {
                            d::error(
                                f["content_digest"],
                                "architecture_invalid_input",
                                "invalid digest",
                            )
                        })?,
                })
            }
        }
    }
    pub(in crate::architecture::compile) fn parameters(
        value: &Value,
        run: &RunContext,
    ) -> Result<Bindings, CompileError> {
        let Kind::Mapping(entries) = &value.kind else {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "expected parameter mapping",
            ));
        };
        let mut out = BTreeMap::new();
        for (key, value) in entries {
            run.check()?;
            let name = d::id(key)?;
            let Kind::Mapping(fields) = &value.kind else {
                return Err(d::error(
                    value,
                    "architecture_invalid_input",
                    "expected tagged parameter",
                ));
            };
            let kind = fields
                .iter()
                .find_map(|(key, value)| {
                    if matches!(&key.kind, Kind::String(s) if s == "kind") {
                        Some(value)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    d::error(
                        value,
                        "architecture_invalid_input",
                        "missing parameter kind",
                    )
                })?;
            let parameter = match d::text(kind)? {
                "element_id" => {
                    let f = d::fields(value, &["kind", "level", "id"], &[], run)?;
                    Parameter::ElementId {
                        level: d::level(f["level"])?,
                        id: d::id(f["id"])?,
                    }
                }
                "contract_ref" => {
                    let f = d::fields(value, &["kind", "reference"], &[], run)?;
                    Parameter::ContractRef {
                        reference: d::artifact(f["reference"], run)?,
                    }
                }
                "integer" => {
                    let f = d::fields(value, &["kind", "value"], &[], run)?;
                    Parameter::Integer {
                        value: d::integer(f["value"])?,
                    }
                }
                "string" => {
                    let f = d::fields(value, &["kind", "value"], &[], run)?;
                    Parameter::String {
                        value: d::text(f["value"])?.to_owned(),
                    }
                }
                _ => {
                    return Err(d::error(
                        kind,
                        "architecture_invalid_input",
                        "invalid parameter kind",
                    ))
                }
            };
            out.insert(name.as_str().to_owned(), parameter);
        }
        Ok(out)
    }
    pub(in crate::architecture::compile) fn predicate(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<Predicate, CompileError> {
        let Kind::Mapping(fields) = &value.kind else {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "expected predicate mapping",
            ));
        };
        let kind = fields
            .iter()
            .find_map(|(key, value)| {
                if matches!(&key.kind, Kind::String(s) if s == "kind") {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                d::error(
                    value,
                    "architecture_invalid_input",
                    "missing predicate kind",
                )
            })?;
        let kind = string(kind, bindings, run)?;
        match kind {
            "element_exists" => {
                let f = d::fields(value, &["kind", "id", "element"], &[], run)?;
                Ok(Predicate::ElementExists {
                    id: id(f["id"], bindings, run)?,
                    element: element(f["element"], bindings, run)?,
                })
            }
            "contract_ref_equals" => {
                let f = d::fields(
                    value,
                    &["kind", "id", "relationship_id", "expected"],
                    &[],
                    run,
                )?;
                let values = d::items(f["expected"])?;
                let mut expected = Vec::new();
                expected
                    .try_reserve(values.len())
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                for item in values {
                    run.check()?;
                    expected.push(artifact(item, bindings, run)?);
                }
                expected.sort_by(|a, b| {
                    (
                        &a.repository.as_str(),
                        &a.commit.as_str(),
                        &a.path.as_str(),
                        &a.content_digest.as_str(),
                    )
                        .cmp(&(
                            &b.repository.as_str(),
                            &b.commit.as_str(),
                            &b.path.as_str(),
                            &b.content_digest.as_str(),
                        ))
                });
                if expected.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err(d::error(
                        f["expected"],
                        "architecture_invalid_input",
                        "duplicate contract reference",
                    ));
                }
                Ok(Predicate::ContractRefEquals {
                    id: id(f["id"], bindings, run)?,
                    relationship_id: id(f["relationship_id"], bindings, run)?,
                    expected,
                })
            }
            "relationship_exists" | "reachable" | "no_bypass" => {
                let required = if kind == "no_bypass" {
                    &["kind", "id", "from", "to", "via", "level", "projection"][..]
                } else {
                    &["kind", "id", "from", "to", "level", "projection"][..]
                };
                let f = d::fields(value, required, &[], run)?;
                let id = id(f["id"], bindings, run)?;
                let from = element(f["from"], bindings, run)?;
                let to = element(f["to"], bindings, run)?;
                let level = level(f["level"], bindings, run)?;
                let projection = projection(f["projection"], bindings, run)?;
                match kind {
                    "reachable" => Ok(Predicate::Reachable {
                        id,
                        from,
                        to,
                        level,
                        projection,
                    }),
                    "relationship_exists" => Ok(Predicate::RelationshipExists {
                        id,
                        from,
                        to,
                        level,
                        projection,
                    }),
                    _ => Ok(Predicate::NoBypass {
                        id,
                        from,
                        to,
                        via: element(f["via"], bindings, run)?,
                        level,
                        projection,
                    }),
                }
            }
            _ => Err(d::error(
                value,
                "architecture_invalid_input",
                "invalid predicate kind",
            )),
        }
    }
    pub(in crate::architecture::compile) struct Counts {
        pub(in crate::architecture::compile) entities: u64,
        pub(in crate::architecture::compile) scenarios: u64,
        pub(in crate::architecture::compile) operations: u64,
        pub(in crate::architecture::compile) expanded_bytes: u64,
        pub(in crate::architecture::compile) expanded_limit: u64,
    }
    pub(in crate::architecture::compile) struct Prepared<'a> {
        pub(in crate::architecture::compile) scenario: Scenario,
        pub(in crate::architecture::compile) baseline: &'a [Value],
        pub(in crate::architecture::compile) candidate: &'a [Value],
    }
    /// Decode one lexical scenario scope. Its transform lists remain pending
    /// until the iterative operation assembler resolves nested scenarios.
    pub(in crate::architecture::compile) fn prepare<'a>(
        value: &'a Value,
        counts: &mut Counts,
        locations: &mut std::collections::HashMap<String, super::super::SourceLocation>,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<Prepared<'a>, CompileError> {
        let f = d::fields(
            value,
            &[
                "id",
                "parameters",
                "preconditions",
                "obligations",
                "baseline_transform",
                "candidate_transform",
            ],
            &[],
            run,
        )?;
        let preconditions = d::items(f["preconditions"])?;
        let obligations = d::items(f["obligations"])?;
        let baseline = d::items(f["baseline_transform"])?;
        let candidate = d::items(f["candidate_transform"])?;
        let predicate_count = preconditions
            .len()
            .checked_add(obligations.len())
            .ok_or_else(|| CompileError::simple("resource_limit", "predicate count overflow"))?;
        let operations = baseline
            .len()
            .checked_add(candidate.len())
            .ok_or_else(|| CompileError::simple("resource_limit", "operation count overflow"))?;
        counts.entities = counts
            .entities
            .checked_add(1)
            .and_then(|x| x.checked_add(u64::try_from(predicate_count).ok()?))
            .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
        counts.scenarios = counts
            .scenarios
            .checked_add(1)
            .ok_or_else(|| CompileError::simple("resource_limit", "scenario count overflow"))?;
        counts.operations = counts
            .operations
            .checked_add(
                u64::try_from(operations).map_err(|_| {
                    CompileError::simple("resource_limit", "operation count overflow")
                })?,
            )
            .ok_or_else(|| CompileError::simple("resource_limit", "operation count overflow"))?;
        if counts.entities > policy.max_authored_entities
            || counts.scenarios > policy.max_scenarios
            || counts.operations > policy.max_proposal_operations
        {
            return Err(CompileError::simple(
                "resource_limit",
                "scenario resource limit exceeded",
            ));
        }
        preview(f["parameters"], None, counts, run, PreviewMode::Normal)?;
        let bindings = parameters(f["parameters"], run)?;
        preview(value, Some(&bindings), counts, run, PreviewMode::Shell)?;
        let commas = baseline
            .len()
            .saturating_sub(1)
            .checked_add(candidate.len().saturating_sub(1))
            .ok_or_else(|| CompileError::simple("resource_limit", "operation count overflow"))?;
        let mut budget = BudgetWriter {
            counts,
            run,
            failure: None,
        };
        for _ in 0..commas {
            budget.raw(b",")?;
        }
        fn predicates(
            values: &[Value],
            bindings: &Bindings,
            run: &RunContext,
        ) -> Result<Vec<Predicate>, CompileError> {
            let mut output = Vec::new();
            output
                .try_reserve(values.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            for value in values {
                run.check()?;
                output.push(predicate(value, bindings, run)?);
            }
            Ok(output)
        }
        let id = id(f["id"], &bindings, run)?;
        let preconditions = predicates(preconditions, &bindings, run)?;
        let obligations = predicates(obligations, &bindings, run)?;
        fn register(
            id: &Id,
            source: &Value,
            locations: &mut std::collections::HashMap<String, super::super::SourceLocation>,
        ) -> Result<(), CompileError> {
            if !locations.contains_key(id.as_str()) {
                locations
                    .try_reserve(1)
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                locations.insert(id.as_str().to_owned(), source.location.clone());
            }
            Ok(())
        }
        register(&id, f["id"], locations)?;
        for (raw, typed) in d::items(f["preconditions"])?
            .iter()
            .zip(&preconditions)
            .chain(d::items(f["obligations"])?.iter().zip(&obligations))
        {
            run.check()?;
            let record_id = match typed {
                Predicate::ElementExists { id, .. }
                | Predicate::RelationshipExists { id, .. }
                | Predicate::ContractRefEquals { id, .. }
                | Predicate::Reachable { id, .. }
                | Predicate::NoBypass { id, .. } => id,
            };
            let Kind::Mapping(entries) = &raw.kind else {
                unreachable!()
            };
            let source = entries
                .iter()
                .find_map(|(key, value)| {
                    if matches!(&key.kind, Kind::String(s) if s == "id") {
                        Some(value)
                    } else {
                        None
                    }
                })
                .expect("decoded predicate has id");
            register(record_id, source, locations)?;
        }
        Ok(Prepared {
            scenario: Scenario {
                id,
                parameters: bindings,
                preconditions,
                obligations,
                baseline_transform: Vec::new(),
                candidate_transform: Vec::new(),
            },
            baseline,
            candidate,
        })
    }
    fn expected_digest(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<Digest, CompileError> {
        Digest::new(string(value, bindings, run)?).map_err(|_| {
            d::error(
                value,
                "architecture_invalid_input",
                "invalid expected digest",
            )
        })
    }
    /// Decode removal operations; add/replace records use their complete typed
    /// record decoders and the nested-scenario assembler.
    pub(in crate::architecture::compile) fn remove_operation(
        value: &Value,
        bindings: &Bindings,
        run: &RunContext,
    ) -> Result<Operation, CompileError> {
        let Kind::Mapping(entries) = &value.kind else {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "expected operation mapping",
            ));
        };
        let kind = entries
            .iter()
            .find_map(|(key, value)| {
                if matches!(&key.kind, Kind::String(s) if s == "kind") {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                d::error(
                    value,
                    "architecture_invalid_input",
                    "missing operation kind",
                )
            })?;
        match string(kind, bindings, run)? {
            "remove_element" => {
                let f = d::fields(
                    value,
                    &["kind", "element_kind", "id", "expected_record_digest"],
                    &[],
                    run,
                )?;
                let element_kind = match string(f["element_kind"], bindings, run)? {
                    "system" => ElementKind::System,
                    "container" => ElementKind::Container,
                    "component" => ElementKind::Component,
                    _ => {
                        return Err(d::error(
                            f["element_kind"],
                            "architecture_invalid_input",
                            "invalid element kind",
                        ))
                    }
                };
                Ok(Operation::RemoveElement {
                    element_kind,
                    id: id(f["id"], bindings, run)?,
                    expected_record_digest: expected_digest(
                        f["expected_record_digest"],
                        bindings,
                        run,
                    )?,
                })
            }
            "remove_relationship" => {
                let f = d::fields(value, &["kind", "id", "expected_record_digest"], &[], run)?;
                Ok(Operation::RemoveRelationship {
                    id: id(f["id"], bindings, run)?,
                    expected_record_digest: expected_digest(
                        f["expected_record_digest"],
                        bindings,
                        run,
                    )?,
                })
            }
            "remove_constraint" => {
                let f = d::fields(
                    value,
                    &["kind", "constraint_kind", "id", "expected_record_digest"],
                    &[],
                    run,
                )?;
                let constraint_kind = match string(f["constraint_kind"], bindings, run)? {
                    "ownership_rule" => ConstraintKind::OwnershipRule,
                    "allow" => ConstraintKind::Allow,
                    "require" => ConstraintKind::Require,
                    "budget" => ConstraintKind::Budget,
                    "waiver" => ConstraintKind::Waiver,
                    "scenario" => ConstraintKind::Scenario,
                    _ => {
                        return Err(d::error(
                            f["constraint_kind"],
                            "architecture_invalid_input",
                            "invalid constraint kind",
                        ))
                    }
                };
                Ok(Operation::RemoveConstraint {
                    constraint_kind,
                    id: id(f["id"], bindings, run)?,
                    expected_record_digest: expected_digest(
                        f["expected_record_digest"],
                        bindings,
                        run,
                    )?,
                })
            }
            _ => Err(d::error(
                kind,
                "architecture_invalid_input",
                "not a remove operation",
            )),
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum PreviewMode {
        Normal,
        Shell,
        Operation,
    }
    struct BudgetWriter<'a> {
        counts: &'a mut Counts,
        run: &'a RunContext,
        failure: Option<CompileError>,
    }
    impl std::io::Write for BudgetWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Err(e) = self.run.check() {
                self.failure = Some(e);
                return Err(std::io::Error::other("cancelled"));
            }
            self.counts.expanded_bytes = self
                .counts
                .expanded_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| std::io::Error::other("size overflow"))?;
            if self.counts.expanded_bytes > self.counts.expanded_limit {
                return Err(std::io::Error::other("result limit"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl BudgetWriter<'_> {
        fn failure(&mut self) -> CompileError {
            self.failure.take().unwrap_or_else(|| {
                CompileError::simple("resource_limit", "expanded result byte limit exceeded")
            })
        }
        fn raw(&mut self, bytes: &[u8]) -> Result<(), CompileError> {
            use std::io::Write;
            self.write_all(bytes).map_err(|_| self.failure())
        }
        fn json<T: serde::Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CompileError> {
            serde_json::to_writer(&mut *self, value).map_err(|_| self.failure())
        }
        fn parameter(&mut self, parameter: &Parameter) -> Result<(), CompileError> {
            match parameter {
                Parameter::String { value } => self.json(value),
                Parameter::Integer { value } => self.json(value),
                Parameter::ElementId { level, id } => {
                    self.raw(b"{\"id\":")?;
                    self.json(id)?;
                    self.raw(b",\"level\":")?;
                    self.json(level)?;
                    self.raw(b"}")
                }
                Parameter::ContractRef { reference } => {
                    self.raw(b"{\"commit\":")?;
                    self.json(&reference.commit)?;
                    self.raw(b",\"content_digest\":")?;
                    self.json(&reference.content_digest)?;
                    self.raw(b",\"path\":")?;
                    self.json(&reference.path)?;
                    self.raw(b",\"repository\":")?;
                    self.json(&reference.repository)?;
                    self.raw(b"}")
                }
            }
        }
    }
    pub(in crate::architecture::compile) fn separators(
        counts: &mut Counts,
        amount: usize,
        run: &RunContext,
    ) -> Result<(), CompileError> {
        let mut out = BudgetWriter {
            counts,
            run,
            failure: None,
        };
        for _ in 0..amount {
            out.raw(b",")?;
        }
        Ok(())
    }
    /// Count the exact canonical JSON of this source subtree without copying
    /// expanded bindings. A nested scenario record is counted in its own scope.
    fn preview<'a>(
        value: &'a Value,
        bindings: Option<&'a Bindings>,
        counts: &mut Counts,
        run: &RunContext,
        mode: PreviewMode,
    ) -> Result<(), CompileError> {
        enum Job<'a> {
            Node(&'a Value, PreviewMode, bool),
            Raw(&'static [u8]),
            Key(&'a str),
        }
        let mut out = BudgetWriter {
            counts,
            run,
            failure: None,
        };
        let mut stack = Vec::new();
        macro_rules! push {
            ($v:expr) => {{
                stack
                    .try_reserve(1)
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                stack.push($v);
            }};
        }
        push!(Job::Node(value, mode, bindings.is_some()));
        while let Some(job) = stack.pop() {
            out.run.check()?;
            match job {
                Job::Raw(bytes) => out.raw(bytes)?,
                Job::Key(key) => out.json(key)?,
                Job::Node(value, mode, resolve) => {
                    if resolve {
                        if let Some(parameter) =
                            binding(value, bindings.expect("scope supplied"), out.run)?
                        {
                            out.parameter(parameter)?;
                            continue;
                        }
                    }
                    match &value.kind {
                        Kind::Null => out.raw(b"null")?,
                        Kind::Bool(v) => out.json(v)?,
                        Kind::Integer(v, _) => out.json(v)?,
                        Kind::String(v) => out.json(v)?,
                        Kind::Sequence(values) => {
                            out.raw(b"[")?;
                            push!(Job::Raw(b"]"));
                            for (i, item) in values.iter().enumerate().rev() {
                                push!(Job::Node(
                                    item,
                                    if mode == PreviewMode::Operation {
                                        mode
                                    } else {
                                        PreviewMode::Normal
                                    },
                                    resolve
                                ));
                                if i > 0 {
                                    push!(Job::Raw(b","));
                                }
                            }
                        }
                        Kind::Mapping(entries) => {
                            let nested = mode == PreviewMode::Operation
                                && entries.iter().any(|(key, val)| {
                                    matches!(&key.kind, Kind::String(s) if s == "record")
                                        && matches!(&val.kind, Kind::Mapping(_))
                                        && d::fields(
                                            val,
                                            &[
                                                "id",
                                                "parameters",
                                                "preconditions",
                                                "obligations",
                                                "baseline_transform",
                                                "candidate_transform",
                                            ],
                                            &[],
                                            out.run,
                                        )
                                        .is_ok()
                                });
                            let mut sorted = Vec::new();
                            sorted.try_reserve(entries.len()).map_err(|_| {
                                CompileError::simple("resource_limit", "allocation failed")
                            })?;
                            for (key, val) in entries {
                                let Kind::String(key) = &key.kind else {
                                    return Err(d::error(
                                        key,
                                        "architecture_invalid_input",
                                        "mapping key must be a string",
                                    ));
                                };
                                sorted.push((key.as_str(), val));
                            }
                            sorted.sort_by(|a, b| a.0.cmp(b.0));
                            out.raw(b"{")?;
                            push!(Job::Raw(b"}"));
                            for (i, (key, val)) in sorted.into_iter().enumerate().rev() {
                                if mode == PreviewMode::Shell && key == "parameters" { /* counted before binding allocation */
                                } else if mode == PreviewMode::Shell
                                    && matches!(key, "baseline_transform" | "candidate_transform")
                                {
                                    push!(Job::Raw(b"[]"));
                                } else if nested && key == "record" { /* child scope counts this value */
                                } else {
                                    push!(Job::Node(
                                        val,
                                        if mode == PreviewMode::Operation {
                                            mode
                                        } else {
                                            PreviewMode::Normal
                                        },
                                        resolve
                                    ));
                                }
                                push!(Job::Raw(b":"));
                                push!(Job::Key(key));
                                if i > 0 {
                                    push!(Job::Raw(b","));
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    // Iteratively copy a single fixed-grammar operation payload while resolving
    // whole-object bindings; nested scenario records are assembled separately.
    pub(super) fn substitute(
        value: &Value,
        bindings: &Bindings,
        _counts: &mut Counts,
        run: &RunContext,
    ) -> Result<Value, CompileError> {
        fn string_value(
            s: &str,
            location: &super::super::SourceLocation,
        ) -> Result<Value, CompileError> {
            let mut text = String::new();
            text.try_reserve(s.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            text.push_str(s);
            Ok(Value {
                location: location.clone(),
                quoted: true,
                kind: Kind::String(text),
            })
        }
        fn mapping(
            entries: &[(&str, &str)],
            location: &super::super::SourceLocation,
        ) -> Result<Value, CompileError> {
            let mut values = Vec::new();
            values
                .try_reserve(entries.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            for (key, value) in entries {
                values.push((string_value(key, location)?, string_value(value, location)?));
            }
            Ok(Value {
                location: location.clone(),
                quoted: false,
                kind: Kind::Mapping(values),
            })
        }
        struct Frame<'a> {
            source: &'a Value,
            index: usize,
            values: Vec<Value>,
            entries: Vec<(Value, Value)>,
            key: Option<Value>,
        }
        let mut frames: Vec<Frame<'_>> = Vec::new();
        let mut current = value;
        loop {
            run.check()?;
            let bound = binding(current, bindings, run)?;
            let produced = match bound {
                Some(Parameter::String { value }) => Some(string_value(value, &current.location)?),
                Some(Parameter::Integer { value }) => Some(Value {
                    location: current.location.clone(),
                    quoted: false,
                    kind: Kind::Integer((*value).into(), false),
                }),
                Some(Parameter::ElementId { level, id }) => Some(mapping(
                    &[
                        (
                            "level",
                            match level {
                                Level::Component => "component",
                                Level::Container => "container",
                            },
                        ),
                        ("id", id.as_str()),
                    ],
                    &current.location,
                )?),
                Some(Parameter::ContractRef { reference }) => Some(mapping(
                    &[
                        ("repository", reference.repository.as_str()),
                        ("commit", reference.commit.as_str()),
                        ("path", reference.path.as_str()),
                        ("content_digest", reference.content_digest.as_str()),
                    ],
                    &current.location,
                )?),
                None => match &current.kind {
                    Kind::Null => Some(Value {
                        location: current.location.clone(),
                        quoted: false,
                        kind: Kind::Null,
                    }),
                    Kind::Bool(v) => Some(Value {
                        location: current.location.clone(),
                        quoted: false,
                        kind: Kind::Bool(*v),
                    }),
                    Kind::Integer(v, negative) => Some(Value {
                        location: current.location.clone(),
                        quoted: false,
                        kind: Kind::Integer(v.clone(), *negative),
                    }),
                    Kind::String(v) => {
                        let mut result = string_value(v, &current.location)?;
                        result.quoted = current.quoted;
                        Some(result)
                    }
                    Kind::Sequence(values) if values.is_empty() => Some(Value {
                        location: current.location.clone(),
                        quoted: false,
                        kind: Kind::Sequence(Vec::new()),
                    }),
                    Kind::Mapping(values) if values.is_empty() => Some(Value {
                        location: current.location.clone(),
                        quoted: false,
                        kind: Kind::Mapping(Vec::new()),
                    }),
                    Kind::Sequence(values) => {
                        frames.try_reserve(1).map_err(|_| {
                            CompileError::simple("resource_limit", "allocation failed")
                        })?;
                        frames.push(Frame {
                            source: current,
                            index: 0,
                            values: Vec::new(),
                            entries: Vec::new(),
                            key: None,
                        });
                        current = &values[0];
                        None
                    }
                    Kind::Mapping(values) => {
                        frames.try_reserve(1).map_err(|_| {
                            CompileError::simple("resource_limit", "allocation failed")
                        })?;
                        frames.push(Frame {
                            source: current,
                            index: 0,
                            values: Vec::new(),
                            entries: Vec::new(),
                            key: None,
                        });
                        current = &values[0].0;
                        None
                    }
                },
            };
            let Some(mut result) = produced else {
                continue;
            };
            loop {
                let Some(frame) = frames.last_mut() else {
                    return Ok(result);
                };
                match &frame.source.kind {
                    Kind::Sequence(source) => {
                        frame.values.try_reserve(1).map_err(|_| {
                            CompileError::simple("resource_limit", "allocation failed")
                        })?;
                        frame.values.push(result);
                        frame.index += 1;
                        if frame.index < source.len() {
                            current = &source[frame.index];
                            break;
                        }
                        let done = frames.pop().unwrap();
                        result = Value {
                            location: done.source.location.clone(),
                            quoted: false,
                            kind: Kind::Sequence(done.values),
                        };
                    }
                    Kind::Mapping(source) => {
                        if let Some(key) = frame.key.take() {
                            frame.entries.try_reserve(1).map_err(|_| {
                                CompileError::simple("resource_limit", "allocation failed")
                            })?;
                            frame.entries.push((key, result));
                            frame.index += 1;
                            if frame.index < source.len() {
                                current = &source[frame.index].0;
                                break;
                            }
                            let done = frames.pop().unwrap();
                            result = Value {
                                location: done.source.location.clone(),
                                quoted: false,
                                kind: Kind::Mapping(done.entries),
                            };
                        } else {
                            frame.key = Some(result);
                            current = &source[frame.index].1;
                            break;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    pub(in crate::architecture::compile) enum PendingOperation<'a> {
        Ready(Box<Operation>),
        Nested {
            value: &'a Value,
            expected: Option<Digest>,
        },
    }
    fn synthetic_document(
        record: Value,
        list: &str,
        names: &[&str],
    ) -> Result<Value, CompileError> {
        let location = record.location.clone();
        fn string(
            value: &str,
            location: &super::super::SourceLocation,
        ) -> Result<Value, CompileError> {
            let mut text = String::new();
            text.try_reserve(value.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            text.push_str(value);
            Ok(Value {
                location: location.clone(),
                quoted: true,
                kind: Kind::String(text),
            })
        }
        let mut entries = Vec::new();
        entries
            .try_reserve(names.len() + 1)
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        entries.push((
            string("schema_version", &location)?,
            string("0.1", &location)?,
        ));
        let mut record = Some(record);
        for name in names {
            let mut values = Vec::new();
            if *name == list {
                values
                    .try_reserve(1)
                    .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                values.push(record.take().expect("record list exists"));
            }
            entries.push((
                string(name, &location)?,
                Value {
                    location: location.clone(),
                    quoted: false,
                    kind: Kind::Sequence(values),
                },
            ));
        }
        Ok(Value {
            location,
            quoted: false,
            kind: Kind::Mapping(entries),
        })
    }
    fn element_record(
        value: &Value,
        bindings: &Bindings,
        counts: &mut Counts,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<ElementRecord, CompileError> {
        let wrapper = d::fields(value, &["kind", "record"], &[], run)?;
        let kind = string(wrapper["kind"], bindings, run)?;
        let list = match kind {
            "system" => "systems",
            "container" => "containers",
            "component" => "components",
            _ => {
                return Err(d::error(
                    wrapper["kind"],
                    "architecture_invalid_input",
                    "invalid element record kind",
                ))
            }
        };
        let record = substitute(wrapper["record"], bindings, counts, run)?;
        let document = synthetic_document(
            record,
            list,
            &["systems", "containers", "components", "relationships"],
        )?;
        let mut typed = super::super::validate::decode_architecture(
            &document,
            &mut counts.entities,
            policy.max_authored_entities,
            run,
        )?;
        let result = match kind {
            "system" => ElementRecord::System(typed.systems.remove(0)),
            "container" => ElementRecord::Container(typed.containers.remove(0)),
            _ => ElementRecord::Component(typed.components.remove(0)),
        };
        let empty = match &result {
            ElementRecord::System(v) => v.responsibilities.is_empty(),
            ElementRecord::Container(v) => v.responsibilities.is_empty(),
            ElementRecord::Component(v) => v.responsibilities.is_empty(),
        };
        if empty {
            return Err(d::error(
                wrapper["record"],
                "architecture_invalid_input",
                "embedded element requires responsibilities",
            ));
        }
        Ok(result)
    }
    fn relationship_record(
        value: &Value,
        bindings: &Bindings,
        counts: &mut Counts,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<Relationship, CompileError> {
        let record = substitute(value, bindings, counts, run)?;
        let document = synthetic_document(
            record,
            "relationships",
            &["systems", "containers", "components", "relationships"],
        )?;
        let mut typed = super::super::validate::decode_architecture(
            &document,
            &mut counts.entities,
            policy.max_authored_entities,
            run,
        )?;
        let mut relation = typed.relationships.remove(0);
        relation.contract_refs.sort_by(|a, b| {
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
        });
        if relation
            .contract_refs
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "duplicate contract reference",
            ));
        }
        Ok(relation)
    }
    fn constraint_record(
        value: &Value,
        bindings: &Bindings,
        counts: &mut Counts,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<ConstraintRecord, CompileError> {
        let wrapper = d::fields(value, &["kind", "record"], &[], run)?;
        let kind = string(wrapper["kind"], bindings, run)?;
        let list = match kind {
            "ownership_rule" => "ownership_rules",
            "allow" => "allow",
            "require" => "require",
            "budget" => "budgets",
            "waiver" => "waivers",
            _ => {
                return Err(d::error(
                    wrapper["kind"],
                    "architecture_invalid_input",
                    "invalid non-scenario constraint kind",
                ))
            }
        };
        let record = substitute(wrapper["record"], bindings, counts, run)?;
        let document = synthetic_document(
            record,
            list,
            &[
                "ownership_rules",
                "allow",
                "require",
                "budgets",
                "waivers",
                "scenarios",
            ],
        )?;
        let (mut typed, _) = d::document(
            &document,
            &mut counts.entities,
            policy.max_authored_entities,
            run,
        )?;
        fn set(ids: &mut [Id], value: &Value, nonempty: bool) -> Result<(), CompileError> {
            if nonempty && ids.is_empty() {
                return Err(d::error(
                    value,
                    "architecture_invalid_input",
                    "empty required set",
                ));
            }
            ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            if ids.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(d::error(
                    value,
                    "architecture_invalid_input",
                    "duplicate set entry",
                ));
            }
            Ok(())
        }
        match kind {
            "ownership_rule" => {
                let mut rule = typed.ownership_rules.remove(0);
                if rule.all_of.is_empty() {
                    return Err(d::error(
                        value,
                        "architecture_invalid_input",
                        "ownership requires all_of clauses",
                    ));
                }
                set(&mut rule.responsibility_ids, value, false)?;
                Ok(ConstraintRecord::OwnershipRule(rule))
            }
            "allow" | "require" => {
                let mut rule = if kind == "allow" {
                    typed.allow.remove(0)
                } else {
                    typed.require.remove(0)
                };
                set(&mut rule.from, value, true)?;
                set(&mut rule.to, value, true)?;
                if kind == "allow" {
                    Ok(ConstraintRecord::Allow(rule))
                } else {
                    Ok(ConstraintRecord::Require(rule))
                }
            }
            "budget" => {
                let mut budget = typed.budgets.remove(0);
                set(&mut budget.origins, value, true)?;
                Ok(ConstraintRecord::Budget(budget))
            }
            _ => Ok(ConstraintRecord::Waiver(typed.waivers.remove(0))),
        }
    }
    pub(in crate::architecture::compile) fn operation<'a>(
        value: &'a Value,
        bindings: &Bindings,
        counts: &mut Counts,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<PendingOperation<'a>, CompileError> {
        let Kind::Mapping(entries) = &value.kind else {
            return Err(d::error(
                value,
                "architecture_invalid_input",
                "expected operation mapping",
            ));
        };
        let kind = entries
            .iter()
            .find_map(|(key, value)| {
                if matches!(&key.kind, Kind::String(s) if s == "kind") {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                d::error(
                    value,
                    "architecture_invalid_input",
                    "missing operation kind",
                )
            })?;
        let kind = string(kind, bindings, run)?;
        preview(value, Some(bindings), counts, run, PreviewMode::Operation)?;
        if matches!(
            kind,
            "remove_element" | "remove_relationship" | "remove_constraint"
        ) {
            return Ok(PendingOperation::Ready(Box::new(remove_operation(
                value, bindings, run,
            )?)));
        }
        let replace = matches!(
            kind,
            "replace_element" | "replace_relationship" | "replace_constraint"
        );
        let field = match kind {
            "add_element" | "replace_element" => "element",
            "add_relationship" | "replace_relationship" => "relationship",
            "add_constraint" | "replace_constraint" => "constraint",
            _ => {
                return Err(d::error(
                    value,
                    "architecture_invalid_input",
                    "invalid operation kind",
                ))
            }
        };
        let required = if replace {
            vec!["kind", field, "expected_record_digest"]
        } else {
            vec!["kind", field]
        };
        let f = d::fields(value, &required, &[], run)?;
        let expected = if replace {
            Some(expected_digest(f["expected_record_digest"], bindings, run)?)
        } else {
            None
        };
        if field == "constraint" {
            let wrapper = d::fields(f[field], &["kind", "record"], &[], run)?;
            if string(wrapper["kind"], bindings, run)? == "scenario" {
                return Ok(PendingOperation::Nested {
                    value: wrapper["record"],
                    expected,
                });
            }
        }
        let result = match (field, expected) {
            ("element", None) => Operation::AddElement {
                element: element_record(f[field], bindings, counts, policy, run)?,
            },
            ("element", Some(expected_record_digest)) => Operation::ReplaceElement {
                element: element_record(f[field], bindings, counts, policy, run)?,
                expected_record_digest,
            },
            ("relationship", None) => Operation::AddRelationship {
                relationship: relationship_record(f[field], bindings, counts, policy, run)?,
            },
            ("relationship", Some(expected_record_digest)) => Operation::ReplaceRelationship {
                relationship: relationship_record(f[field], bindings, counts, policy, run)?,
                expected_record_digest,
            },
            ("constraint", None) => Operation::AddConstraint {
                constraint: constraint_record(f[field], bindings, counts, policy, run)?,
            },
            ("constraint", Some(expected_record_digest)) => Operation::ReplaceConstraint {
                constraint: constraint_record(f[field], bindings, counts, policy, run)?,
                expected_record_digest,
            },
            _ => unreachable!(),
        };
        Ok(PendingOperation::Ready(Box::new(result)))
    }
    /// Assemble nested scenarios in postorder using explicit frames; each child
    /// obtains only its own parameter scope.
    pub(in crate::architecture::compile) fn scenarios(
        values: &[Value],
        counts: &mut Counts,
        locations: &mut std::collections::HashMap<String, super::super::SourceLocation>,
        policy: &super::super::ArchitectureExecutionPolicy,
        run: &RunContext,
    ) -> Result<Vec<Scenario>, CompileError> {
        struct Frame<'a> {
            prepared: Prepared<'a>,
            baseline: usize,
            candidate: usize,
            return_to: Option<(bool, Option<Digest>)>,
        }
        fn frame<'a>(
            value: &'a Value,
            counts: &mut Counts,
            locations: &mut std::collections::HashMap<String, super::super::SourceLocation>,
            policy: &super::super::ArchitectureExecutionPolicy,
            run: &RunContext,
            return_to: Option<(bool, Option<Digest>)>,
        ) -> Result<Frame<'a>, CompileError> {
            let mut prepared = prepare(value, counts, locations, policy, run)?;
            prepared
                .scenario
                .baseline_transform
                .try_reserve(prepared.baseline.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            prepared
                .scenario
                .candidate_transform
                .try_reserve(prepared.candidate.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            Ok(Frame {
                prepared,
                baseline: 0,
                candidate: 0,
                return_to,
            })
        }
        let incoming = u64::try_from(values.len())
            .map_err(|_| CompileError::simple("resource_limit", "scenario count overflow"))?;
        if counts
            .scenarios
            .checked_add(incoming)
            .is_none_or(|n| n > policy.max_scenarios)
            || counts
                .entities
                .checked_add(incoming)
                .is_none_or(|n| n > policy.max_authored_entities)
        {
            return Err(CompileError::simple(
                "resource_limit",
                "scenario resource limit exceeded",
            ));
        }
        let mut result = Vec::new();
        result
            .try_reserve(values.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        let mut stack = Vec::new();
        for value in values {
            run.check()?;
            stack
                .try_reserve(1)
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            stack.push(frame(value, counts, locations, policy, run, None)?);
            while !stack.is_empty() {
                run.check()?;
                let top = stack.last_mut().expect("nonempty stack");
                let next = if top.baseline < top.prepared.baseline.len() {
                    let value = &top.prepared.baseline[top.baseline];
                    top.baseline += 1;
                    Some((true, value))
                } else if top.candidate < top.prepared.candidate.len() {
                    let value = &top.prepared.candidate[top.candidate];
                    top.candidate += 1;
                    Some((false, value))
                } else {
                    None
                };
                if let Some((baseline, value)) = next {
                    match operation(
                        value,
                        &top.prepared.scenario.parameters,
                        counts,
                        policy,
                        run,
                    )? {
                        PendingOperation::Ready(operation) => {
                            if baseline {
                                top.prepared.scenario.baseline_transform.push(*operation);
                            } else {
                                top.prepared.scenario.candidate_transform.push(*operation);
                            }
                        }
                        PendingOperation::Nested { value, expected } => {
                            let child = frame(
                                value,
                                counts,
                                locations,
                                policy,
                                run,
                                Some((baseline, expected)),
                            )?;
                            stack.try_reserve(1).map_err(|_| {
                                CompileError::simple("resource_limit", "allocation failed")
                            })?;
                            stack.push(child);
                        }
                    }
                } else {
                    let finished = stack.pop().expect("nonempty stack");
                    if let Some((baseline, expected)) = finished.return_to {
                        let constraint =
                            ConstraintRecord::Scenario(Box::new(finished.prepared.scenario));
                        let operation = match expected {
                            Some(expected_record_digest) => Operation::ReplaceConstraint {
                                constraint,
                                expected_record_digest,
                            },
                            None => Operation::AddConstraint { constraint },
                        };
                        let parent = stack.last_mut().expect("nested frame has parent");
                        if baseline {
                            parent.prepared.scenario.baseline_transform.push(operation);
                        } else {
                            parent.prepared.scenario.candidate_transform.push(operation);
                        }
                    } else {
                        result.push(finished.prepared.scenario);
                    }
                }
            }
        }
        Ok(result)
    }
}

impl ElementRef {
    pub fn level(&self) -> Level {
        self.level
    }
    pub fn id(&self) -> &Id {
        &self.id
    }
}
impl Scenario {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn parameters(&self) -> &BTreeMap<String, Parameter> {
        &self.parameters
    }
    pub fn preconditions(&self) -> &[Predicate] {
        &self.preconditions
    }
    pub fn obligations(&self) -> &[Predicate] {
        &self.obligations
    }
    pub fn baseline_transform(&self) -> &[Operation] {
        &self.baseline_transform
    }
    pub fn candidate_transform(&self) -> &[Operation] {
        &self.candidate_transform
    }
}
