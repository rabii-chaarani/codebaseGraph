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
    assert_eq!(digest(std::slice::from_ref(&a)), digest(&[b]));
    value["inference_provenance"]["agent_id"] = json!("different");
    let changed: MappingSubmission = serde_json::from_value(value).unwrap();
    assert_ne!(digest(std::slice::from_ref(&a)), digest(&[changed]));
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

#[test]
fn builtin_extractor_descriptor_hash_and_context_guard() {
    use codebase_graph::architecture::evidence::source_extractor_identity;
    let identity = source_extractor_identity(&ctx()).unwrap();
    let d = identity.descriptor();
    assert_eq!(d.version(), "native-source-evidence/0.1");
    assert_eq!(d.language(), "rust");
    assert_eq!(d.profile().grammar_package, "tree_sitter_rust");
    assert_eq!(d.node_identity_version(), "native-source-graph/1");
    assert_eq!(
        d.grammar_node_types_digest().as_str(),
        format!(
            "sha256:{:x}",
            Sha256::digest(tree_sitter_rust::NODE_TYPES.as_bytes())
        )
    );
    let mut descriptor = serde_json::to_value(d).unwrap();
    descriptor.sort_all_objects();
    let mut hash = Sha256::new();
    hash.update(b"architecture-source-extractor/0.1\n");
    hash.update(serde_json::to_vec(&descriptor).unwrap());
    assert_eq!(
        identity.digest().as_str(),
        format!("sha256:{:x}", hash.finalize())
    );
    let cancelled = ExecutionContext::new(Arc::new(AtomicBool::new(true)), None);
    assert_eq!(
        source_extractor_identity(&cancelled).unwrap_err().code(),
        "cancelled"
    );
    let expired = ExecutionContext::new(
        Arc::new(AtomicBool::new(false)),
        Some(std::time::Instant::now()),
    );
    assert_eq!(
        source_extractor_identity(&expired).unwrap_err().code(),
        "deadline_exceeded"
    );
}

#[test]
fn digest_output_and_traversal_budgets_are_admitted() {
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw["max_result_bytes"] = json!(73);
    let exact = ArchitectureExecutionPolicy::from_json(raw.clone()).unwrap();
    assert!(mapping_facts_digest(&source(), &[], &exact, None, &ctx()).is_ok());
    raw["max_result_bytes"] = json!(72);
    let under = ArchitectureExecutionPolicy::from_json(raw.clone()).unwrap();
    assert_eq!(
        mapping_facts_digest(&source(), &[], &under, None, &ctx())
            .unwrap_err()
            .code(),
        "resource_limit"
    );
    raw["max_result_bytes"] = json!(73);
    raw["max_traversal_steps"] = json!(1);
    let tiny = ArchitectureExecutionPolicy::from_json(raw).unwrap();
    let item: MappingSubmission = serde_json::from_value(mapping()).unwrap();
    assert_eq!(
        mapping_facts_digest(&source(), &[item], &tiny, None, &ctx())
            .unwrap_err()
            .code(),
        "resource_limit"
    );
}

#[test]
fn public_validation_proves_raw_nodes_but_not_ownership() {
    use codebase_graph::architecture::{
        compile::{compile, ModelInput, RepositoryBinding},
        evidence::{source_extractor_identity, validate_evidence, CheckStatus},
        RelativePath, Text,
    };
    use std::path::Path;
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    for object_format in ["sha1", "sha256"] {
        let root =
            b"schema_version: '0.1'\nmodel_id: model\narchitecture: [arch]\nconstraints: []\n";
        let arch=br#"{"schema_version":"0.1","systems":[{"id":"sys","name":"System","responsibilities":[{"id":"sys-task","statement":"System task"}]}],"containers":[{"id":"container","name":"Container","responsibilities":[{"id":"container-task","statement":"Container task"}],"system":"sys","kind":"application"}],"components":[{"id":"alpha","name":"Alpha","responsibilities":[{"id":"task","statement":"Task"}],"container":"container"},{"id":"beta","name":"Beta","responsibilities":[{"id":"other","statement":"Other"}],"container":"container"}],"relationships":[]}"#;
        let p = |s| RelativePath::new(s).unwrap();
        let input = ModelInput::inline_files(
            p("root"),
            [(p("root"), root.to_vec()), (p("arch"), arch.to_vec())],
        )
        .unwrap();
        let model = compile(input, None, &policy(), None, &ctx()).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "cod23-public-{}-{}-{}",
            std::process::id(),
            object_format,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        git(
            &dir,
            &["init", "-q", &format!("--object-format={object_format}")],
        );
        git(&dir, &["config", "user.name", "test"]);
        git(&dir, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(dir.join("src/lib.rs"), b"fn hello() {}").unwrap();
        std::fs::write(dir.join("src/lib.txt"), b"fn hello() {}").unwrap();
        let large = vec![b'x'; 9000];
        std::fs::write(dir.join("src/large.rs"), &large).unwrap();
        std::fs::write(dir.join("src/literal*[?].rs"), b"fn hello() {}").unwrap();
        git(&dir, &["add", "--all"]);
        git(&dir, &["commit", "-qm", "fixture"]);
        let commit = git(&dir, &["rev-parse", "HEAD"]);
        let tree = git(&dir, &["rev-parse", "HEAD^{tree}"]);
        let digest = format!("sha256:{:x}", Sha256::digest(b"fn hello() {}"));
        let extractor = source_extractor_identity(&ctx()).unwrap();
        let provenance:SourceProvenance=serde_json::from_value(json!({"repository":"repo","evaluated_commit":commit,"evaluated_tree":tree,"extractor_digest":extractor.digest(),"build_context_digest":digest})).unwrap();
        let function = json!({"repository":"repo","commit":commit,"path":"src/lib.rs","node_kind":"Function","node_id":"Function:216f9df7cf9055f46285","symbol":"src.lib.hello","byte_span":{"start":0,"end":13},"file_digest":digest});
        let scope = json!({"repository":"repo","commit":commit,"path":"src/lib.rs","node_kind":"Scope","node_id":"Scope:41046104907937f3c6e1","symbol":"src.lib.<scope>","byte_span":{"start":0,"end":13},"file_digest":digest});
        let input = json!({"source_ref":function,"proposed_component":"alpha","responsibility_ids":["task"],"structural_citations":[{"id":"direct","source_ref":scope,"assertion":{"kind":"scope_contains","member":function}}],"rationale":{"responsibilities":[{"responsibility_id":"task","explanation":"responsibility"}],"competing_components":[{"component":"beta","explanation":"competitor"}]},"inference_provenance":{"agent_id":"agent","model_id":"model","input_digest":digest,"rule_version":"responsibility-evidence/0.1","result_digest":digest},"previous_mapping":null,"change_kind":"initial","discriminator":"responsibility-evidence/0.1"});
        let mapping: MappingSubmission = serde_json::from_value(input.clone()).unwrap();
        let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
        let bindings = [binding];
        std::fs::write(dir.join("src/lib.rs"), b"dirty working tree").unwrap();
        let result = validate_evidence(&model, &provenance, &[mapping], &bindings, &ctx()).unwrap();
        assert!(!result.has_invalid_evidence());
        assert!(result.has_unknown());
        assert!(result
            .diagnostics()
            .iter()
            .any(|d| d.code() == "build_context_unqualified"));
        let record = &result.records()[0];
        assert_eq!(record.source_check().status(), CheckStatus::Valid);
        assert_eq!(record.responsibility_check().status(), CheckStatus::Valid);
        assert_eq!(record.rationale_check().status(), CheckStatus::Valid);
        assert_eq!(record.inference_check().status(), CheckStatus::Valid);
        assert_eq!(record.history_check().status(), CheckStatus::Valid);
        assert_eq!(
            record.citations()[0].assertion_check().status(),
            CheckStatus::Valid
        );
        assert_eq!(record.ownership_qualification(), "unknown");
        for (pointer, replacement) in [
            ("/source_ref/node_id", json!("fabricated")),
            ("/source_ref/node_kind", json!("Symbol")),
            ("/source_ref/symbol", json!("hello")),
            ("/source_ref/byte_span/start", json!(1)),
            ("/source_ref/commit", json!("2".repeat(commit.len()))),
            (
                "/source_ref/file_digest",
                json!(format!("sha256:{}", "0".repeat(64))),
            ),
        ] {
            let mut changed = input.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            let changed: MappingSubmission = serde_json::from_value(changed).unwrap();
            let result =
                validate_evidence(&model, &provenance, &[changed], &bindings, &ctx()).unwrap();
            assert!(
                result.has_invalid_evidence() && result.has_unknown(),
                "{pointer}"
            );
            assert_eq!(
                result.records()[0].source_check().status(),
                CheckStatus::Invalid,
                "{pointer}"
            );
        }
        let mut literal = input.clone();
        literal["source_ref"]["path"] = json!("src/literal*[?].rs");
        literal["source_ref"]["node_id"] = json!("Function:8abd243fec2b586d3596");
        literal["source_ref"]["symbol"] = json!("src.literal*[?].hello");
        let literal: MappingSubmission = serde_json::from_value(literal).unwrap();
        let result = validate_evidence(&model, &provenance, &[literal], &bindings, &ctx()).unwrap();
        assert_eq!(
            result.records()[0].source_check().status(),
            CheckStatus::Valid
        );
        assert!(!result.has_invalid_evidence());
        let mut moved = input.clone();
        moved["source_ref"]["path"] = json!("src/literal*[?].rs");
        let moved: MappingSubmission = serde_json::from_value(moved).unwrap();
        let result = validate_evidence(&model, &provenance, &[moved], &bindings, &ctx()).unwrap();
        assert_eq!(
            result.records()[0].source_check().status(),
            CheckStatus::Invalid
        );
        let mut bad_links = input.clone();
        bad_links["rationale"]["competing_components"] = json!([]);
        bad_links["inference_provenance"]["rule_version"] = json!("different");
        let bad_links: MappingSubmission = serde_json::from_value(bad_links).unwrap();
        let result =
            validate_evidence(&model, &provenance, &[bad_links], &bindings, &ctx()).unwrap();
        assert!(result.has_invalid_evidence() && result.has_unknown());
        assert_eq!(
            result.records()[0].rationale_check().status(),
            CheckStatus::Invalid
        );
        assert_eq!(
            result.records()[0].inference_check().status(),
            CheckStatus::Invalid
        );
        let mut history = input.clone();
        history["change_kind"] = json!("retained");
        history["previous_mapping"] = json!({"source_ref":function,"component":"removed-owner","mapping_facts_digest":digest,"evidence_ref":{"repository":"repo","commit":commit,"path":"old.md","content_digest":digest},"lineage":null});
        let history: MappingSubmission = serde_json::from_value(history).unwrap();
        let result = validate_evidence(&model, &provenance, &[history], &bindings, &ctx()).unwrap();
        assert!(!result.has_invalid_evidence() && result.has_unknown());
        assert_eq!(
            result.records()[0].history_check().status(),
            CheckStatus::Unknown
        );
        // Preserve already-proven invalid evidence when later immutable file
        // capture exceeds a deterministic configured byte limit.
        let mut bounded_raw = serde_json::to_value(policy()).unwrap();
        bounded_raw["max_file_bytes"] = json!(8192);
        let bounded_policy = ArchitectureExecutionPolicy::from_json(bounded_raw).unwrap();
        let bounded_input = ModelInput::inline_files(
            p("root"),
            [(p("root"), root.to_vec()), (p("arch"), arch.to_vec())],
        )
        .unwrap();
        let bounded_model = compile(bounded_input, None, &bounded_policy, None, &ctx()).unwrap();
        let mut fabricated = function.clone();
        fabricated["node_id"] = json!("fabricated");
        let mut large_ref = function.clone();
        large_ref["path"] = json!("src/large.rs");
        large_ref["byte_span"] = json!({"start":0,"end":1});
        large_ref["file_digest"] = json!(format!("sha256:{:x}", Sha256::digest(&large)));
        let mut late_failure = input.clone();
        late_failure["structural_citations"] = json!([{ "id":"reach", "source_ref":fabricated, "assertion":{"kind":"reaches_exported_item","crate":"mycrate","namespace":"value","item_path":"hello","max_hops":null,"path":[large_ref]} }]);
        let late_failure: MappingSubmission = serde_json::from_value(late_failure).unwrap();
        let error = validate_evidence(
            &bounded_model,
            &provenance,
            &[late_failure],
            &bindings,
            &ctx(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "resource_limit");
        assert!(error.has_known_invalid_evidence() && error.has_known_unknown());
        assert!(error
            .diagnostics()
            .iter()
            .any(|d| d.code() == "source_node_mismatch"));
        let mut inference_failure = input.clone();
        inference_failure["source_ref"] = large_ref;
        inference_failure["inference_provenance"]["rule_version"] = json!("different");
        let inference_failure: MappingSubmission =
            serde_json::from_value(inference_failure).unwrap();
        let error = validate_evidence(
            &bounded_model,
            &provenance,
            &[inference_failure],
            &bindings,
            &ctx(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "resource_limit");
        assert!(error.has_known_invalid_evidence() && error.has_known_unknown());
        assert!(error
            .diagnostics()
            .iter()
            .any(|d| d.code() == "inference_version_mismatch"));
        // Three distinct captured files: unsupported primary plus two files
        // cited with bad digests. The middle correct claim reuses the first.
        let mut aggregate = input.clone();
        aggregate["source_ref"]["path"] = json!("src/lib.txt");
        let mut bad_first = function.clone();
        bad_first["file_digest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let mut bad_second = bad_first.clone();
        bad_second["path"] = json!("src/literal*[?].rs");
        aggregate["structural_citations"] = json!([
            {"id":"a-bad","source_ref":bad_first,"assertion":{"kind":"node_exists"}},
            {"id":"b-good","source_ref":function,"assertion":{"kind":"node_exists"}},
            {"id":"c-bad","source_ref":bad_second,"assertion":{"kind":"node_exists"}}
        ]);
        let mut payload = json!({"evidence_version":"architecture-evidence/0.1","source":provenance,"mappings":[aggregate.clone()]});
        payload.sort_all_objects();
        let payload_size = serde_json::to_vec(&payload).unwrap().len() as u64;
        let aggregate: MappingSubmission = serde_json::from_value(aggregate).unwrap();
        let mut exact_raw = serde_json::to_value(policy()).unwrap();
        exact_raw["max_evidence_bytes"] = json!(payload_size + 39);
        let exact_policy = ArchitectureExecutionPolicy::from_json(exact_raw.clone()).unwrap();
        let make_input = || {
            ModelInput::inline_files(
                p("root"),
                [(p("root"), root.to_vec()), (p("arch"), arch.to_vec())],
            )
            .unwrap()
        };
        let exact_model = compile(make_input(), None, &exact_policy, None, &ctx()).unwrap();
        let exact = validate_evidence(
            &exact_model,
            &provenance,
            std::slice::from_ref(&aggregate),
            &bindings,
            &ctx(),
        )
        .unwrap();
        assert!(exact.has_invalid_evidence() && exact.has_unknown());
        assert_eq!(
            exact.records()[0].citations()[0].source_checks()[0].status(),
            CheckStatus::Invalid
        );
        assert_eq!(
            exact.records()[0].citations()[1].source_checks()[0].status(),
            CheckStatus::Valid
        );
        assert_eq!(
            exact.records()[0].citations()[2].source_checks()[0].status(),
            CheckStatus::Invalid
        );
        exact_raw["max_evidence_bytes"] = json!(payload_size + 38);
        let under_policy = ArchitectureExecutionPolicy::from_json(exact_raw).unwrap();
        let under_model = compile(make_input(), None, &under_policy, None, &ctx()).unwrap();
        let error = validate_evidence(
            &under_model,
            &provenance,
            std::slice::from_ref(&aggregate),
            &bindings,
            &ctx(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "resource_limit");
        assert!(error.has_known_invalid_evidence() && error.has_known_unknown());
        assert!(error
            .diagnostics()
            .iter()
            .any(|d| d.code() == "invalid_source_reference"));
        let mut unsupported = input.clone();
        unsupported["discriminator"] = json!("responsibility-evidence/9");
        unsupported["inference_provenance"]["rule_version"] = json!("responsibility-evidence/9");
        let unsupported: MappingSubmission = serde_json::from_value(unsupported).unwrap();
        let result =
            validate_evidence(&model, &provenance, &[unsupported], &bindings, &ctx()).unwrap();
        assert!(!result.has_invalid_evidence() && result.has_unknown());
        assert!(result.records()[0]
            .reason_codes()
            .iter()
            .any(|v| v == "unsupported_evidence"));
        let mut language = input.clone();
        language["source_ref"]["path"] = json!("src/lib.txt");
        let language: MappingSubmission = serde_json::from_value(language).unwrap();
        let result =
            validate_evidence(&model, &provenance, &[language], &bindings, &ctx()).unwrap();
        assert_eq!(
            result.records()[0].source_check().status(),
            CheckStatus::Unknown
        );
        let cancelled = ExecutionContext::new(Arc::new(AtomicBool::new(true)), None);
        assert_eq!(
            validate_evidence(&model, &provenance, &[], &bindings, &cancelled)
                .unwrap_err()
                .code(),
            "cancelled"
        );
        let expired = ExecutionContext::new(
            Arc::new(AtomicBool::new(false)),
            Some(std::time::Instant::now()),
        );
        assert_eq!(
            validate_evidence(&model, &provenance, &[], &bindings, &expired)
                .unwrap_err()
                .code(),
            "deadline_exceeded"
        );
        let blob = git(&dir, &["rev-parse", "HEAD:src/lib.rs"]);
        let object = dir.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
        std::fs::remove_file(&object).unwrap();
        std::fs::write(object, b"corrupt").unwrap();
        let mapping: MappingSubmission = serde_json::from_value(input).unwrap();
        let result = validate_evidence(&model, &provenance, &[mapping], &bindings, &ctx()).unwrap();
        assert!(result.has_invalid_evidence() && result.has_unknown());
        assert_eq!(
            result.records()[0].source_check().status(),
            CheckStatus::Invalid
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn canonical_identity_covers_every_claim_field() {
    let mut base = mapping();
    let reference = base["source_ref"].clone();
    let artifact = json!({"repository":"repo","commit":"1".repeat(40),"path":"evidence.md","content_digest":format!("sha256:{}","f".repeat(64))});
    base["structural_citations"] = json!([{"id":"cite","source_ref":reference,"assertion":{"kind":"reaches_exported_item","crate":"crate-name","namespace":"value","item_path":"hello","max_hops":3,"path":[reference]}}]);
    base["previous_mapping"] = json!({"source_ref":reference,"component":"old","mapping_facts_digest":format!("sha256:{}","a".repeat(64)),"evidence_ref":artifact,"lineage":{"previous":reference,"current":reference,"rule_version":"v1","basis":"unique_structural_move","evidence_refs":[artifact]}});
    let expected = digest(&[serde_json::from_value(base.clone()).unwrap()]);
    let mutations = [
        ("/source_ref/repository", json!("other")),
        ("/source_ref/commit", json!("2".repeat(40))),
        ("/source_ref/path", json!("src/other.rs")),
        ("/source_ref/node_kind", json!("Scope")),
        ("/source_ref/node_id", json!("different")),
        ("/source_ref/symbol", json!("different")),
        ("/source_ref/byte_span/start", json!(0)),
        ("/source_ref/byte_span/end", json!(4)),
        (
            "/source_ref/file_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/proposed_component", json!("other")),
        ("/responsibility_ids/0", json!("other")),
        ("/structural_citations/0/id", json!("other")),
        ("/structural_citations/0/source_ref/node_id", json!("other")),
        ("/structural_citations/0/assertion/crate", json!("other")),
        ("/structural_citations/0/assertion/namespace", json!("type")),
        (
            "/structural_citations/0/assertion/item_path",
            json!("other"),
        ),
        ("/structural_citations/0/assertion/max_hops", json!(4)),
        (
            "/structural_citations/0/assertion/path/0/node_id",
            json!("other"),
        ),
        (
            "/rationale/responsibilities/0/responsibility_id",
            json!("other"),
        ),
        ("/rationale/responsibilities/0/explanation", json!("other")),
        (
            "/rationale/competing_components/0/component",
            json!("other"),
        ),
        (
            "/rationale/competing_components/0/explanation",
            json!("other"),
        ),
        ("/inference_provenance/agent_id", json!("other")),
        ("/inference_provenance/model_id", json!("other")),
        (
            "/inference_provenance/input_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/inference_provenance/rule_version", json!("other")),
        (
            "/inference_provenance/result_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/previous_mapping/source_ref/node_id", json!("other")),
        ("/previous_mapping/component", json!("other")),
        (
            "/previous_mapping/mapping_facts_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/previous_mapping/evidence_ref/repository", json!("other")),
        (
            "/previous_mapping/evidence_ref/commit",
            json!("2".repeat(40)),
        ),
        ("/previous_mapping/evidence_ref/path", json!("other.md")),
        (
            "/previous_mapping/evidence_ref/content_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/previous_mapping/lineage/previous/node_id", json!("other")),
        ("/previous_mapping/lineage/current/node_id", json!("other")),
        ("/previous_mapping/lineage/rule_version", json!("other")),
        (
            "/previous_mapping/lineage/basis",
            json!("unchanged_qualified_identity"),
        ),
        (
            "/previous_mapping/lineage/evidence_refs/0/repository",
            json!("other"),
        ),
        (
            "/previous_mapping/lineage/evidence_refs/0/commit",
            json!("2".repeat(40)),
        ),
        (
            "/previous_mapping/lineage/evidence_refs/0/path",
            json!("other.md"),
        ),
        (
            "/previous_mapping/lineage/evidence_refs/0/content_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/change_kind", json!("retained")),
        ("/discriminator", json!("other")),
    ];
    for (pointer, replacement) in mutations {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let actual = digest(&[serde_json::from_value(changed).unwrap()]);
        assert_ne!(expected, actual, "{pointer}");
    }
    for prefix in [
        "/previous_mapping/source_ref",
        "/previous_mapping/lineage/previous",
        "/previous_mapping/lineage/current",
        "/structural_citations/0/source_ref",
        "/structural_citations/0/assertion/path/0",
    ] {
        for (field, replacement) in [
            ("repository", json!("other")),
            ("commit", json!("2".repeat(40))),
            ("path", json!("src/other.rs")),
            ("node_kind", json!("Scope")),
            ("node_id", json!("other")),
            ("symbol", json!("other")),
            ("byte_span/start", json!(0)),
            ("byte_span/end", json!(4)),
            ("file_digest", json!(format!("sha256:{}", "0".repeat(64)))),
        ] {
            let pointer = format!("{prefix}/{field}");
            let mut changed = base.clone();
            *changed.pointer_mut(&pointer).unwrap() = replacement;
            assert_ne!(
                expected,
                digest(&[serde_json::from_value(changed).unwrap()]),
                "{pointer}"
            );
        }
    }
}
