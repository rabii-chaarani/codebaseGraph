use codebase_graph::architecture::{
    compile::{ArchitectureExecutionPolicy, ExecutionContext},
    evidence::{mapping_facts_digest, MappingSubmission},
    SourceProvenance,
};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::sync::{atomic::AtomicBool, Arc};

fn policy() -> ArchitectureExecutionPolicy {
    let keys = [
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
    ArchitectureExecutionPolicy::from_json(Value::Object(
        keys.into_iter()
            .map(|k| (k.into(), 100_000.into()))
            .collect(),
    ))
    .unwrap()
}
fn ctx() -> ExecutionContext {
    ExecutionContext::new(Arc::new(AtomicBool::new(false)), None)
}
fn source() -> SourceProvenance {
    serde_json::from_value(json!({"repository":"repo", "evaluated_commit":"1111111111111111111111111111111111111111", "evaluated_tree":"2222222222222222222222222222222222222222", "extractor_digest":format!("sha256:{}", "a".repeat(64)), "build_context_digest":format!("sha256:{}", "b".repeat(64))})).unwrap()
}
fn mapping() -> Value {
    json!({"source_ref":{"repository":"repo","commit":"1111111111111111111111111111111111111111","path":"src/lib.rs","node_kind":"Function","node_id":"node","symbol":"foo","byte_span":{"start":1,"end":3},"file_digest":format!("sha256:{}","c".repeat(64))},"proposed_component":"alpha","responsibility_ids":["b","a"],"structural_citations":[],"rationale":{"responsibilities":[{"responsibility_id":"b","explanation":"two"},{"responsibility_id":"a","explanation":"one"}],"competing_components":[{"component":"z","explanation":"z"},{"component":"c","explanation":"c"}]},"inference_provenance":{"agent_id":"agent","model_id":"model","input_digest":format!("sha256:{}","d".repeat(64)),"rule_version":"responsibility-evidence/0.1","result_digest":format!("sha256:{}","e".repeat(64))},"previous_mapping":null,"change_kind":"initial","discriminator":"responsibility-evidence/0.1"})
}
fn digest(m: &[MappingSubmission]) -> String {
    mapping_facts_digest(&source(), m, &policy(), None, &ctx())
        .unwrap()
        .digest()
        .as_str()
        .to_owned()
}
#[test]
fn empty_canonical_domain_and_bytes_independent_oracle() {
    let payload = format!("{{\"evidence_version\":\"architecture-evidence/0.1\",\"mappings\":[],\"source\":{{\"build_context_digest\":\"sha256:{}\",\"evaluated_commit\":\"{}\",\"evaluated_tree\":\"{}\",\"extractor_digest\":\"sha256:{}\",\"repository\":\"repo\"}}}}", "b".repeat(64), "1".repeat(40), "2".repeat(40), "a".repeat(64));
    let expected = format!(
        "sha256:{:x}",
        Sha256::digest(format!("architecture-mapping-facts/0.1\n{payload}").as_bytes())
    );
    assert_eq!(digest(&[]), expected);
}
#[test]
fn sets_reorder_but_content_changes_and_duplicates_fail() {
    let a: MappingSubmission = serde_json::from_value(mapping()).unwrap();
    let mut value = mapping();
    value["responsibility_ids"]
        .as_array_mut()
        .unwrap()
        .reverse();
    value["rationale"]["responsibilities"]
        .as_array_mut()
        .unwrap()
        .reverse();
    value["rationale"]["competing_components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let b: MappingSubmission = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(digest(&[a.clone()]), digest(&[b]));
    value["inference_provenance"]["agent_id"] = json!("different");
    let changed: MappingSubmission = serde_json::from_value(value).unwrap();
    assert_ne!(digest(&[a.clone()]), digest(&[changed]));
    assert_eq!(
        mapping_facts_digest(&source(), &[a.clone(), a], &policy(), None, &ctx())
            .unwrap_err()
            .code(),
        "architecture_invalid_input"
    );
}
#[test]
fn wire_requires_nullable_fields_and_rejects_unknown_and_duplicate_keys() {
    let mut v = mapping();
    v.as_object_mut().unwrap().remove("previous_mapping");
    assert!(serde_json::from_value::<MappingSubmission>(v).is_err());
    let mut v = mapping();
    v["verdict"] = json!(true);
    assert!(serde_json::from_value::<MappingSubmission>(v).is_err());
    let raw = serde_json::to_string(&mapping()).unwrap().replacen(
        "{",
        "{\"change_kind\":\"initial\",",
        1,
    );
    assert!(serde_json::from_str::<MappingSubmission>(&raw).is_err());
}

#[test]
fn nonempty_canonical_oracle_and_ordered_witness() {
    let mut raw = mapping();
    let member = raw["source_ref"].clone();
    let mut other = member.clone();
    other["node_id"] = json!("other");
    raw["structural_citations"] = json!([{ "id":"citation", "source_ref":member, "assertion":{"kind":"reaches_exported_item", "crate":"crate-name", "namespace":"value", "item_path":"foo", "max_hops":null, "path":[member,other]} }]);
    let typed: MappingSubmission = serde_json::from_value(raw.clone()).unwrap();
    raw["responsibility_ids"].as_array_mut().unwrap().reverse();
    raw["rationale"]["responsibilities"]
        .as_array_mut()
        .unwrap()
        .reverse();
    raw["rationale"]["competing_components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let mut payload = json!({"evidence_version":"architecture-evidence/0.1", "source":source(), "mappings":[raw.clone()]});
    payload.sort_all_objects();
    let canonical = serde_json::to_string(&payload).unwrap();
    let expected = format!(
        "sha256:{:x}",
        Sha256::digest(format!("architecture-mapping-facts/0.1\n{canonical}").as_bytes())
    );
    assert_eq!(digest(&[typed]), expected);
    let before: MappingSubmission = serde_json::from_value(raw.clone()).unwrap();
    raw["structural_citations"][0]["assertion"]["path"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let after: MappingSubmission = serde_json::from_value(raw).unwrap();
    assert_ne!(digest(&[before]), digest(&[after]));
}

#[test]
fn representative_field_mutations_change_digest() {
    let base = mapping();
    let baseline = digest(&[serde_json::from_value(base.clone()).unwrap()]);
    for (pointer, replacement) in [
        ("/source_ref/path", json!("src/other.rs")),
        ("/source_ref/node_kind", json!("Scope")),
        ("/source_ref/node_id", json!("other")),
        ("/source_ref/symbol", json!("other")),
        ("/source_ref/byte_span/start", json!(0)),
        ("/proposed_component", json!("beta")),
        ("/responsibility_ids/0", json!("different")),
        (
            "/rationale/responsibilities/0/explanation",
            json!("changed"),
        ),
        (
            "/rationale/competing_components/0/explanation",
            json!("changed"),
        ),
        ("/inference_provenance/agent_id", json!("other")),
        ("/inference_provenance/model_id", json!("other")),
        ("/inference_provenance/rule_version", json!("other")),
        ("/change_kind", json!("retained")),
        ("/discriminator", json!("other")),
    ] {
        let mut value = base.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let changed = digest(&[serde_json::from_value(value).unwrap()]);
        assert_ne!(baseline, changed, "{pointer}");
    }
}

#[test]
fn cancellation_and_evidence_byte_limit_return_errors() {
    let cancelled = ExecutionContext::new(Arc::new(AtomicBool::new(true)), None);
    assert_eq!(
        mapping_facts_digest(&source(), &[], &policy(), None, &cancelled)
            .unwrap_err()
            .code(),
        "cancelled"
    );
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw["max_evidence_bytes"] = json!(1);
    let small = ArchitectureExecutionPolicy::from_json(raw).unwrap();
    assert_eq!(
        mapping_facts_digest(&source(), &[], &small, None, &ctx())
            .unwrap_err()
            .code(),
        "resource_limit"
    );
}

#[test]
fn history_citations_and_provenance_are_hashed() {
    let mut v = mapping();
    let src = v["source_ref"].clone();
    let artifact = json!({"repository":"repo", "commit":"1111111111111111111111111111111111111111", "path":"proof.md", "content_digest":format!("sha256:{}","f".repeat(64))});
    v["previous_mapping"] = json!({"source_ref":src,"component":"old","mapping_facts_digest":format!("sha256:{}","a".repeat(64)),"evidence_ref":artifact,"lineage":{"previous":src,"current":src,"rule_version":"v1","basis":"unique_structural_move","evidence_refs":[artifact]}});
    v["structural_citations"] = json!([{ "id":"cite", "source_ref":src, "assertion":{"kind":"linked_contract_anchor","reference":artifact,"anchor":"anchor"} }]);
    let original: MappingSubmission = serde_json::from_value(v.clone()).unwrap();
    let base = digest(std::slice::from_ref(&original));
    for (pointer, value) in [
        ("/previous_mapping/component", json!("other")),
        (
            "/previous_mapping/mapping_facts_digest",
            json!(format!("sha256:{}", "b".repeat(64))),
        ),
        ("/previous_mapping/source_ref/node_id", json!("previous")),
        ("/previous_mapping/evidence_ref/path", json!("other.md")),
        (
            "/previous_mapping/lineage/previous/node_id",
            json!("previous"),
        ),
        (
            "/previous_mapping/lineage/current/node_id",
            json!("current"),
        ),
        ("/previous_mapping/lineage/rule_version", json!("v2")),
        (
            "/previous_mapping/lineage/basis",
            json!("unchanged_qualified_identity"),
        ),
        (
            "/previous_mapping/lineage/evidence_refs/0/path",
            json!("other.md"),
        ),
        ("/structural_citations/0/id", json!("different")),
        (
            "/structural_citations/0/source_ref/node_id",
            json!("different"),
        ),
        (
            "/structural_citations/0/assertion/reference/path",
            json!("other.md"),
        ),
        ("/structural_citations/0/assertion/anchor", json!("other")),
    ] {
        let mut changed = v.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert_ne!(
            base,
            digest(&[serde_json::from_value(changed).unwrap()]),
            "{pointer}"
        );
    }
    let mut provenance = serde_json::to_value(source()).unwrap();
    for (pointer, value) in [
        ("/repository", json!("other")),
        (
            "/evaluated_commit",
            json!("3333333333333333333333333333333333333333"),
        ),
        (
            "/evaluated_tree",
            json!("3333333333333333333333333333333333333333"),
        ),
        (
            "/extractor_digest",
            json!(format!("sha256:{}", "c".repeat(64))),
        ),
        (
            "/build_context_digest",
            json!(format!("sha256:{}", "c".repeat(64))),
        ),
    ] {
        let saved = provenance.pointer(pointer).unwrap().clone();
        *provenance.pointer_mut(pointer).unwrap() = value;
        let changed = mapping_facts_digest(
            &serde_json::from_value(provenance.clone()).unwrap(),
            std::slice::from_ref(&original),
            &policy(),
            None,
            &ctx(),
        )
        .unwrap();
        assert_ne!(base, changed.digest().as_str(), "{pointer}");
        *provenance.pointer_mut(pointer).unwrap() = saved;
    }
}

#[test]
fn exact_and_minus_one_payload_budget() {
    let mut payload =
        json!({"evidence_version":"architecture-evidence/0.1", "source":source(), "mappings":[]});
    payload.sort_all_objects();
    let size = serde_json::to_vec(&payload).unwrap().len() as u64;
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw["max_evidence_bytes"] = json!(size);
    let exact = ArchitectureExecutionPolicy::from_json(raw.clone()).unwrap();
    assert!(mapping_facts_digest(&source(), &[], &exact, None, &ctx()).is_ok());
    raw["max_evidence_bytes"] = json!(size - 1);
    let under = ArchitectureExecutionPolicy::from_json(raw).unwrap();
    assert_eq!(
        mapping_facts_digest(&source(), &[], &under, None, &ctx())
            .unwrap_err()
            .code(),
        "resource_limit"
    );
}

#[test]
fn streamed_canonical_oracle_covers_each_assertion_shape() {
    let base = mapping();
    let reference = base["source_ref"].clone();
    let artifact = json!({"repository":"repo","commit":"1111111111111111111111111111111111111111","path":"contract.md","content_digest":format!("sha256:{}","f".repeat(64))});
    let variants = [
        json!({"kind":"node_exists"}),
        json!({"kind":"scope_contains","member":reference}),
        json!({"kind":"exported_item","crate":"crate-name","namespace":"value","item_path":"foo"}),
        json!({"kind":"reaches_exported_item","crate":"crate-name","namespace":"value","item_path":"foo","max_hops":2,"path":[reference]}),
        json!({"kind":"linked_contract_anchor","reference":artifact,"anchor":"anchor"}),
    ];
    for assertion in variants {
        let mut raw = base.clone();
        raw["structural_citations"] =
            json!([{"id":"citation","source_ref":reference,"assertion":assertion}]);
        let typed: MappingSubmission = serde_json::from_value(raw.clone()).unwrap();
        raw["responsibility_ids"].as_array_mut().unwrap().reverse();
        raw["rationale"]["responsibilities"]
            .as_array_mut()
            .unwrap()
            .reverse();
        raw["rationale"]["competing_components"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let mut payload = json!({"evidence_version":"architecture-evidence/0.1","mappings":[raw],"source":source()});
        payload.sort_all_objects();
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut hash = Sha256::new();
        hash.update(b"architecture-mapping-facts/0.1\n");
        hash.update(bytes);
        assert_eq!(digest(&[typed]), format!("sha256:{:x}", hash.finalize()));
    }
}
