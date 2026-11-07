use codebase_graph::architecture::{
    compile::{compile, ArchitectureExecutionPolicy, ExecutionContext, ModelInput},
    RelativePath,
};
use std::sync::{atomic::AtomicBool, Arc};
fn assert_object_keys_sorted(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let keys = map.keys().map(String::as_str).collect::<Vec<_>>();
            assert!(
                keys.windows(2).all(|pair| pair[0] < pair[1]),
                "keys not sorted: {keys:?}"
            );
            for value in map.values() {
                assert_object_keys_sorted(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                assert_object_keys_sorted(value);
            }
        }
        _ => {}
    }
}
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
fn path(s: &str) -> RelativePath {
    RelativePath::new(s).unwrap()
}
fn input(extra: bool) -> ModelInput {
    let mut files = vec![(path("model/root.yaml"), b"schema_version: '0.1'\nmodel_id: model\narchitecture: [arch.yaml]\nconstraints: [rules.yaml]\n".to_vec()), (path("model/arch.yaml"), b"schema_version: '0.1'\nsystems: []\ncontainers: []\ncomponents: []\nrelationships: []\n".to_vec()), (path("model/rules.yaml"), b"schema_version: '0.1'\nownership_rules: []\nallow: []\nrequire: []\nbudgets: []\nwaivers: []\nscenarios: []\n".to_vec())];
    if extra {
        files.push((path("model/extra"), vec![]));
    }
    ModelInput::inline_files(path("model/root.yaml"), files).unwrap()
}
#[test]
fn compiles_complete_inline_closure() {
    let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let output = compile(input(false), None, &policy(), None, &context).unwrap();
    assert_eq!(output.root().model_id.as_str(), "model");
    assert_eq!(output.architecture().len(), 1);
    assert_eq!(output.constraints().len(), 1);
    assert_eq!(
        output
            .closure()
            .iter()
            .map(|f| f.path().as_str())
            .collect::<Vec<_>>(),
        vec!["model/arch.yaml", "model/root.yaml", "model/rules.yaml"]
    );
    assert!(output.provenance().is_none());
    assert_eq!(
        compile(input(true), None, &policy(), None, &context)
            .err()
            .unwrap()
            .code(),
        "architecture_invalid_input"
    );
}
#[test]
fn cancellation_and_deadline_prevent_success() {
    let cancelled = ExecutionContext::new(Arc::new(AtomicBool::new(true)), None);
    assert_eq!(
        compile(input(false), None, &policy(), None, &cancelled)
            .err()
            .unwrap()
            .code(),
        "cancelled"
    );
    let mut full_range = serde_json::to_value(policy()).unwrap();
    full_range["max_evaluation_duration_ms"] = u64::MAX.into();
    let full_range = ArchitectureExecutionPolicy::from_json(full_range).unwrap();
    let context = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    assert!(compile(input(false), None, &full_range, None, &context).is_ok());
    let expired = ExecutionContext::new(
        Arc::new(AtomicBool::new(false)),
        Some(std::time::Instant::now()),
    );
    assert_eq!(
        compile(input(false), None, &policy(), None, &expired)
            .err()
            .unwrap()
            .code(),
        "deadline_exceeded"
    );
}
fn policy_result(limit: u64) -> ArchitectureExecutionPolicy {
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
            .map(|k| {
                (
                    k.into(),
                    (if k == "max_result_bytes" {
                        limit
                    } else {
                        100000
                    })
                    .into(),
                )
            })
            .collect(),
    ))
    .unwrap()
}
#[test]
fn complete_result_limit_has_an_exact_boundary() {
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let mut low = 1;
    let mut high = 100000;
    while low < high {
        let middle = low + (high - low) / 2;
        if compile(input(false), None, &policy_result(middle), None, &ctx).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(compile(input(false), None, &policy_result(low), None, &ctx).is_ok());
    assert_eq!(
        compile(input(false), None, &policy_result(low - 1), None, &ctx)
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
}
#[test]
fn malformed_input_reports_file_and_span() {
    let files = vec![(
        path("root"),
        b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\nunknown: 1\n"
            .to_vec(),
    )];
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let error = compile(
        ModelInput::inline_files(path("root"), files).unwrap(),
        None,
        &policy(),
        None,
        &ctx,
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), "unknown_field");
    let location = error.diagnostics()[0].location().unwrap();
    assert_eq!(location.path().as_str(), "root");
    assert_eq!(location.line(), 5);
    assert!(location.start() < location.end());
}
fn full_input(require_allow: bool, with_bypass_pair: bool) -> ModelInput {
    let digest = format!("sha256:{}", "a".repeat(64));
    let root = serde_json::json!({"schema_version":"0.1","model_id":"model","architecture":["a.yaml"],"constraints":["c.yaml"]});
    let arch = serde_json::json!({"schema_version":"0.1","systems":[{"id":"sys","name":"System","responsibilities":[{"id":"rs","statement":"Own"}]}],"containers":[{"id":"container","system":"sys","kind":"application","name":"Container","responsibilities":[{"id":"rc","statement":"Own"}]}],"components":[{"id":"a","container":"container","name":"A","responsibilities":[{"id":"ra","statement":"Own"}]},{"id":"b","container":"container","name":"B","responsibilities":[{"id":"rb","statement":"Own"}]}],"relationships":[]});
    let proj = serde_json::json!({"version":"rust-q1/0.1","domain_ref":digest});
    let edge = |id| serde_json::json!({"id":id,"level":"component","from":["a"],"to":["b"],"projection":proj});
    let from = serde_json::json!({"level":"component","id":"a"});
    let to = serde_json::json!({"level":"component","id":"b"});
    let via = serde_json::json!({"level":"component","id":"future"});
    let mut predicates = vec![
        serde_json::json!({"kind":"no_bypass","id":"no-bypass","from":from,"to":to,"via":via,"level":"component","projection":proj}),
    ];
    if with_bypass_pair {
        predicates.push(serde_json::json!({"kind":"reachable","id":"reach","from":from,"to":to,"level":"component","projection":proj}));
    }
    let constraints = serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":if require_allow { vec![edge("allow-edge")] } else { vec![] },"require":[edge("require-edge")],"budgets":[],"waivers":[],"scenarios":[{"id":"scenario","parameters":{"target":{"kind":"element_id","level":"component","id":"future"}},"preconditions":predicates,"obligations":[{"kind":"element_exists","id":"exists","element":{"parameter_ref":"target"}}],"baseline_transform":[],"candidate_transform":[]}]});
    ModelInput::inline_files(
        path("root.yaml"),
        vec![
            (path("root.yaml"), serde_json::to_vec(&root).unwrap()),
            (path("a.yaml"), serde_json::to_vec(&arch).unwrap()),
            (path("c.yaml"), serde_json::to_vec(&constraints).unwrap()),
        ],
    )
    .unwrap()
}
#[test]
fn public_cross_document_constraints_and_scenarios() {
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let output = compile(full_input(true, true), None, &policy(), None, &ctx).unwrap();
    assert_eq!(output.architecture()[0].components.len(), 2);
    assert_eq!(
        output.constraints()[0].scenarios()[0].obligations().len(),
        1
    );
    let error = compile(full_input(false, true), None, &policy(), None, &ctx)
        .err()
        .unwrap();
    assert_eq!(error.code(), "contradictory_policy");
    assert_eq!(
        error.diagnostics()[0].location().unwrap().path().as_str(),
        "c.yaml"
    );
    assert!(compile(full_input(true, false), None, &policy(), None, &ctx).is_err());
}
#[test]
fn pinned_and_inline_captures_have_same_intent() {
    use codebase_graph::architecture::{
        compile::RepositoryBinding, GitObjectId, GitRevision, Text,
    };
    use std::process::Command;
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let inline = compile(input(false), None, &policy(), None, &ctx).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "cod21-public-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("model")).unwrap();
    for file in inline.closure() {
        std::fs::write(dir.join(file.path().as_str()), file.bytes()).unwrap();
    }
    let git = |args: &[&str]| {
        let result = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    git(&["add", "--all"]);
    git(&["commit", "-qm", "fixture"]);
    let revision = GitRevision {
        repository: Text::new("repo").unwrap(),
        commit: GitObjectId::new(git(&["rev-parse", "HEAD"])).unwrap(),
    };
    let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
    std::fs::write(dir.join("model/arch.yaml"), b"changed worktree").unwrap();
    let pinned = compile(
        ModelInput::pinned_root(revision, path("model/root.yaml")).unwrap(),
        Some(&binding),
        &policy(),
        None,
        &ctx,
    )
    .unwrap();
    assert_eq!(inline.root(), pinned.root());
    assert_eq!(inline.architecture(), pinned.architecture());
    assert_eq!(
        inline
            .closure()
            .iter()
            .map(|f| (f.path().as_str(), f.bytes()))
            .collect::<Vec<_>>(),
        pinned
            .closure()
            .iter()
            .map(|f| (f.path().as_str(), f.bytes()))
            .collect::<Vec<_>>()
    );
    assert!(pinned.closure().iter().all(|f| f.blob().is_some()));
    assert!(pinned.provenance().is_some());
    let encoded: serde_json::Value = serde_json::from_slice(pinned.canonical_json()).unwrap();
    assert_eq!(encoded["provenance"]["revision"]["repository"], "repo");
    assert!(encoded["provenance"]["tree"].as_str().is_some());
    assert!(encoded["closure"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["object_id"].as_str().is_some()));
    assert_eq!(
        serde_json::to_vec(&encoded).unwrap(),
        pinned.canonical_json()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn repeated_parameter_expansion_obeys_result_limit() {
    let root = serde_json::json!({"schema_version":"0.1","model_id":"model","architecture":[],"constraints":["c"]});
    let long_id = "a".repeat(3000);
    let predicates = (0..20).map(|i| serde_json::json!({"kind":"element_exists","id":format!("p{i}"),"element":{"level":"component","id":{"parameter_ref":"large"}}})).collect::<Vec<_>>();
    let constraint = serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[{"id":"scenario","parameters":{"large":{"kind":"string","value":long_id}},"preconditions":predicates,"obligations":[],"baseline_transform":[],"candidate_transform":[]}]});
    let input = || {
        ModelInput::inline_files(
            path("root"),
            vec![
                (path("root"), serde_json::to_vec(&root).unwrap()),
                (path("c"), serde_json::to_vec(&constraint).unwrap()),
            ],
        )
        .unwrap()
    };
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let error = compile(input(), None, &policy_result(20000), None, &ctx)
        .err()
        .unwrap();
    assert_eq!(error.code(), "resource_limit");
    assert!(error.diagnostics()[0].reason().contains("expanded"));
    assert!(compile(input(), None, &policy_result(100000), None, &ctx).is_ok());
}
#[test]
fn explicit_policy_and_core_boundaries_are_enforced() {
    use codebase_graph::architecture::compile::ArchitectureExecutionPolicyOverride;
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let all = serde_json::to_value(policy()).unwrap();
    for name in all.as_object().unwrap().keys() {
        let mut missing = all.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(
            ArchitectureExecutionPolicy::from_json(missing).is_err(),
            "missing {name}"
        );
        let mut zero = all.clone();
        zero[name] = 0.into();
        assert!(
            ArchitectureExecutionPolicy::from_json(zero).is_err(),
            "zero {name}"
        );
    }
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw.as_object_mut().unwrap().remove("max_evidence_bytes");
    assert!(ArchitectureExecutionPolicy::from_json(raw).is_err());
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw["max_evidence_bytes"] = 0.into();
    assert!(ArchitectureExecutionPolicy::from_json(raw).is_err());
    assert!(
        serde_json::from_value::<ArchitectureExecutionPolicyOverride>(
            serde_json::json!({"max_file_bytes":0})
        )
        .is_err()
    );
    let raise: ArchitectureExecutionPolicyOverride =
        serde_json::from_value(serde_json::json!({"max_file_bytes":100001})).unwrap();
    assert_eq!(
        compile(input(false), None, &policy(), Some(&raise), &ctx)
            .err()
            .unwrap()
            .code(),
        "architecture_configuration_required"
    );
    let lower: ArchitectureExecutionPolicyOverride =
        serde_json::from_value(serde_json::json!({"max_closure_files":3})).unwrap();
    assert!(compile(input(false), None, &policy(), Some(&lower), &ctx).is_ok());
    let lower: ArchitectureExecutionPolicyOverride =
        serde_json::from_value(serde_json::json!({"max_closure_files":2})).unwrap();
    assert_eq!(
        compile(input(false), None, &policy(), Some(&lower), &ctx)
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
    let lower: ArchitectureExecutionPolicyOverride =
        serde_json::from_value(serde_json::json!({"max_expanded_rule_pairs":2})).unwrap();
    assert!(compile(full_input(true, true), None, &policy(), Some(&lower), &ctx).is_ok());
    let lower: ArchitectureExecutionPolicyOverride =
        serde_json::from_value(serde_json::json!({"max_expanded_rule_pairs":1})).unwrap();
    assert_eq!(
        compile(full_input(true, true), None, &policy(), Some(&lower), &ctx)
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
}
#[test]
fn closure_errors_identify_the_manifest_reference() {
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let build = |references: &str, include_file: bool| {
        let root = format!(
            "schema_version: '0.1'\nmodel_id: model\narchitecture: {references}\nconstraints: []\n"
        );
        let mut files = vec![(path("dir/root"), root.into_bytes())];
        if include_file {
            files.push((path("dir/a"), b"schema_version: '0.1'\nsystems: []\ncontainers: []\ncomponents: []\nrelationships: []".to_vec()));
        }
        ModelInput::inline_files(path("dir/root"), files).unwrap()
    };
    for (references, include_file, code) in [
        ("[a, a]", true, "architecture_invalid_input"),
        ("[missing]", false, "architecture_input_unavailable"),
        ("[../escape]", false, "path_not_confined"),
        ("[root]", false, "architecture_invalid_input"),
        ("['*.yaml']", false, "path_not_confined"),
    ] {
        let error = compile(build(references, include_file), None, &policy(), None, &ctx)
            .err()
            .unwrap();
        assert_eq!(error.code(), code, "{references}");
        let location = error.diagnostics()[0].location().unwrap();
        assert_eq!(location.path().as_str(), "dir/root");
        assert_eq!(location.line(), 3);
        assert!(location.start() < location.end());
        if references == "[missing]" {
            assert_eq!(error.diagnostics()[0].subject(), Some("dir/missing"));
        }
    }
}
#[test]
fn public_transform_grammar_and_scope() {
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
    let root = serde_json::json!({"schema_version":"0.1","model_id":"model","architecture":[],"constraints":["c"]});
    let mut rules = serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[{"id":"scenario","parameters":{},"preconditions":[],"obligations":[],"baseline_transform":operations,"candidate_transform":[]}]});
    let compile_rules = |rules: &serde_json::Value| {
        let input = ModelInput::inline_files(
            path("root"),
            vec![
                (path("root"), serde_json::to_vec(&root).unwrap()),
                (path("c"), serde_json::to_vec(rules).unwrap()),
            ],
        )
        .unwrap();
        compile(
            input,
            None,
            &policy(),
            None,
            &ExecutionContext::new(Arc::new(AtomicBool::new(false)), None),
        )
    };
    let mut empty_responsibilities = rules.clone();
    empty_responsibilities["scenarios"][0]["baseline_transform"][0]["element"]["record"]
        ["responsibilities"] = serde_json::json!([]);
    assert!(compile_rules(&empty_responsibilities).is_err());
    let mut duplicate_endpoints = rules.clone();
    duplicate_endpoints["scenarios"][0]["baseline_transform"][6]["constraint"]["record"]["from"] =
        serde_json::json!(["x", "x"]);
    assert!(compile_rules(&duplicate_endpoints).is_err());
    let result = compile_rules(&rules).unwrap();
    let encoded: serde_json::Value = serde_json::from_slice(result.canonical_json()).unwrap();
    assert_object_keys_sorted(&encoded);
    assert_eq!(
        encoded["constraints"][0]["scenarios"][0]["baseline_transform"],
        rules["scenarios"][0]["baseline_transform"]
    );
    assert_eq!(
        serde_json::to_vec(&encoded).unwrap(),
        result.canonical_json()
    );
    assert_eq!(
        result.constraints()[0].scenarios()[0]
            .baseline_transform()
            .len(),
        9
    );
    let compile_with = |rules: &serde_json::Value, field: &str, limit: u64| {
        let input = ModelInput::inline_files(
            path("root"),
            vec![
                (path("root"), serde_json::to_vec(&root).unwrap()),
                (path("c"), serde_json::to_vec(rules).unwrap()),
            ],
        )
        .unwrap();
        let overrides = serde_json::from_value(serde_json::json!({field:limit})).unwrap();
        compile(
            input,
            None,
            &policy(),
            Some(&overrides),
            &ExecutionContext::new(Arc::new(AtomicBool::new(false)), None),
        )
    };
    assert!(compile_with(&rules, "max_proposal_operations", 9).is_ok());
    assert_eq!(
        compile_with(&rules, "max_proposal_operations", 8)
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
    let mut two = rules.clone();
    let mut second = two["scenarios"][0].clone();
    second["id"] = "scenario-two".into();
    second["baseline_transform"] = serde_json::json!([]);
    two["scenarios"].as_array_mut().unwrap().push(second);
    assert!(compile_with(&two, "max_scenarios", 2).is_ok());
    assert_eq!(
        compile_with(&two, "max_scenarios", 1).err().unwrap().code(),
        "resource_limit"
    );
    rules["scenarios"][0]["baseline_transform"][1]
        .as_object_mut()
        .unwrap()
        .remove("expected_record_digest");
    assert!(compile_rules(&rules).is_err());
    rules["scenarios"][0]["baseline_transform"] = serde_json::json!([{"kind":"add_constraint","constraint":{"kind":"scenario","record":{"id":"nested","parameters":{},"preconditions":[{"kind":"element_exists","id":"nested-exists","element":{"parameter_ref":"outer"}}],"obligations":[],"baseline_transform":[],"candidate_transform":[]}}}]);
    rules["scenarios"][0]["parameters"] =
        serde_json::json!({"outer":{"kind":"element_id","level":"component","id":"future"}});
    assert!(compile_rules(&rules).is_err());
    rules["scenarios"][0]["baseline_transform"][0]["constraint"]["record"]["parameters"]["outer"] =
        serde_json::json!({"kind":"element_id", "level":"component", "id":"inner-target"});
    let nested = compile_rules(&rules).unwrap();
    let json: serde_json::Value = serde_json::from_slice(nested.canonical_json()).unwrap();
    assert_eq!(
        json["constraints"][0]["scenarios"][0]["baseline_transform"][0]["constraint"]["record"]
            ["preconditions"][0]["element"]["id"],
        "inner-target"
    );
    assert_eq!(serde_json::to_vec(&json).unwrap(), nested.canonical_json());
}
#[test]
fn closure_and_entity_resource_boundaries() {
    use codebase_graph::architecture::compile::ArchitectureExecutionPolicyOverride;
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let baseline = compile(input(false), None, &policy(), None, &ctx).unwrap();
    let total: u64 = baseline
        .closure()
        .iter()
        .map(|f| f.bytes().len() as u64)
        .sum();
    let file = baseline
        .closure()
        .iter()
        .map(|f| f.bytes().len() as u64)
        .max()
        .unwrap();
    let override_for = |name: &str, n: u64| -> ArchitectureExecutionPolicyOverride {
        serde_json::from_value(serde_json::json!({name:n})).unwrap()
    };
    for (name, n) in [("max_yaml_bytes", total), ("max_file_bytes", file)] {
        assert!(
            compile(
                input(false),
                None,
                &policy(),
                Some(&override_for(name, n)),
                &ctx
            )
            .is_ok(),
            "{name}"
        );
        assert_eq!(
            compile(
                input(false),
                None,
                &policy(),
                Some(&override_for(name, n - 1)),
                &ctx
            )
            .err()
            .unwrap()
            .code(),
            "resource_limit",
            "{name}"
        );
    }
    assert!(compile(
        full_input(true, true),
        None,
        &policy(),
        Some(&override_for("max_authored_entities", 14)),
        &ctx
    )
    .is_ok());
    assert_eq!(
        compile(
            full_input(true, true),
            None,
            &policy(),
            Some(&override_for("max_authored_entities", 13)),
            &ctx
        )
        .err()
        .unwrap()
        .code(),
        "resource_limit"
    );
    let mut low = 1;
    let mut high = 30;
    while low < high {
        let mid = (low + high) / 2;
        if compile(
            full_input(true, true),
            None,
            &policy(),
            Some(&override_for("max_parse_depth", mid)),
            &ctx,
        )
        .is_ok()
        {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    assert!(compile(
        full_input(true, true),
        None,
        &policy(),
        Some(&override_for("max_parse_depth", low)),
        &ctx
    )
    .is_ok());
    assert_eq!(
        compile(
            full_input(true, true),
            None,
            &policy(),
            Some(&override_for("max_parse_depth", low - 1)),
            &ctx
        )
        .err()
        .unwrap()
        .code(),
        "resource_limit"
    );
}
#[test]
fn deep_scenario_compilation_and_output_accounting_are_iterative() {
    let mut nested = String::new();
    for _ in 0..300 {
        nested.push_str("{id: nested, parameters: {}, preconditions: [], obligations: [], baseline_transform: [{kind: add_constraint, constraint: {kind: scenario, record: ");
    }
    nested.push_str("{id: leaf, parameters: {}, preconditions: [], obligations: [], baseline_transform: [], candidate_transform: []}");
    for _ in 0..300 {
        nested.push_str("}}], candidate_transform: []}");
    }
    let root =
        b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: [c]\n".to_vec();
    let rules = format!("schema_version: '0.1'\nownership_rules: []\nallow: []\nrequire: []\nbudgets: []\nwaivers: []\nscenarios: [{nested}]\n").into_bytes();
    let input =
        ModelInput::inline_files(path("root"), vec![(path("root"), root), (path("c"), rules)])
            .unwrap();
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let mut raw = serde_json::to_value(policy()).unwrap();
    raw["max_file_bytes"] = 1000000.into();
    raw["max_yaml_bytes"] = 1000000.into();
    raw["max_result_bytes"] = 2000000.into();
    let p = ArchitectureExecutionPolicy::from_json(raw).unwrap();
    let output = compile(input, None, &p, None, &ctx).unwrap();
    assert_eq!(output.constraints()[0].scenarios().len(), 1);
    drop(output);
}
#[test]
fn canonical_complete_result_matches_independent_json_and_exact_limit() {
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let output = compile(input(false), None, &policy(), None, &ctx).unwrap();
    let root = serde_json::json!({"schema_version":"0.1","model_id":"model","architecture":["arch.yaml"],"constraints":["rules.yaml"]});
    let arch = serde_json::json!({"schema_version":"0.1","systems":[],"containers":[],"components":[],"relationships":[]});
    let rules = serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[]});
    let closure = output.closure().iter().map(|file| serde_json::json!({"bytes":file.bytes(),"object_id":null,"path":file.path().as_str()})).collect::<Vec<_>>();
    let mut expected = serde_json::json!({"architecture":[arch],"closure":closure,"constraints":[rules],"effective_policy":serde_json::to_value(policy()).unwrap(),"provenance":null,"root":root});
    let actual: serde_json::Value = serde_json::from_slice(output.canonical_json()).unwrap();
    assert_object_keys_sorted(&actual);
    assert!(output
        .canonical_json()
        .starts_with(b"{\"architecture\":[{\"components\":[]"));
    assert_eq!(
        output.canonical_json(),
        serde_json::to_vec(&expected).unwrap()
    );
    let mut limit = 1u64;
    loop {
        expected["effective_policy"]["max_result_bytes"] = limit.into();
        let next = serde_json::to_vec(&expected).unwrap().len() as u64;
        if next == limit {
            break;
        }
        limit = next;
    }
    let exact = compile(input(false), None, &policy_result(limit), None, &ctx).unwrap();
    assert_eq!(exact.canonical_json().len() as u64, limit);
    assert_eq!(
        exact.canonical_json(),
        serde_json::to_vec(&expected).unwrap()
    );
    assert_eq!(
        compile(input(false), None, &policy_result(limit - 1), None, &ctx)
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
}
#[test]
fn canonical_expansion_counts_escapes_wrappers_and_repetitions() {
    let root = serde_json::json!({"schema_version":"0.1","model_id":"model","architecture":[],"constraints":["c"]});
    let text = "quoted \" and slash \\\\ and newline\n".repeat(80);
    let operations = (0..10).map(|i| serde_json::json!({"kind":"add_element","element":{"kind":"system","record":{"id":format!("future-{i}"),"name":{"parameter_ref":"text"},"responsibilities":[{"id":format!("r-{i}"),"statement":"Own"}]}}})).collect::<Vec<_>>();
    let rule = serde_json::json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[{"id":"scenario","parameters":{"text":{"kind":"string","value":text}},"preconditions":[],"obligations":[],"baseline_transform":operations,"candidate_transform":[]}]});
    let source = || {
        ModelInput::inline_files(
            path("root"),
            vec![
                (path("root"), serde_json::to_vec(&root).unwrap()),
                (path("c"), serde_json::to_vec(&rule).unwrap()),
            ],
        )
        .unwrap()
    };
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let result = compile(source(), None, &policy_result(100000), None, &ctx).unwrap();
    let mut logical: serde_json::Value = serde_json::from_slice(result.canonical_json()).unwrap();
    assert_eq!(
        serde_json::to_vec(&logical).unwrap(),
        result.canonical_json()
    );
    let mut limit = 1u64;
    loop {
        logical["effective_policy"]["max_result_bytes"] = limit.into();
        let next = serde_json::to_vec(&logical).unwrap().len() as u64;
        if next == limit {
            break;
        }
        limit = next;
    }
    let exact = compile(source(), None, &policy_result(limit), None, &ctx).unwrap();
    assert_eq!(
        exact.canonical_json(),
        serde_json::to_vec(&logical).unwrap()
    );
    let error = compile(source(), None, &policy_result(limit - 1), None, &ctx)
        .err()
        .unwrap();
    assert_eq!(error.code(), "resource_limit");
    assert!(error.diagnostics()[0].reason().contains("expanded"));
}
#[cfg(unix)]
#[test]
fn pinned_git_formats_reject_unsafe_entries_and_wrong_object_bytes() {
    use codebase_graph::architecture::{
        compile::RepositoryBinding, GitObjectId, GitRevision, Text,
    };
    use std::process::Command;
    for format in ["sha1", "sha256"] {
        let dir = std::env::temp_dir().join(format!(
            "cod21-public-{format}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let output = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q", &format!("--object-format={format}")])
            .output()
            .unwrap();
        if !output.status.success() && format == "sha256" {
            std::fs::remove_dir_all(dir).unwrap();
            continue;
        }
        assert!(output.status.success());
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        git(&["config", "user.name", "test"]);
        git(&["config", "user.email", "test@example.invalid"]);
        let root = b"schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: []\n";
        std::fs::write(dir.join("root"), root).unwrap();
        std::fs::write(dir.join("other"), b"valid different object bytes").unwrap();
        std::fs::write(
            dir.join("link-manifest"),
            b"schema_version: '0.1'\nmodel_id: model\narchitecture: [link]\nconstraints: []\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("module-manifest"),
            b"schema_version: '0.1'\nmodel_id: model\narchitecture: [module]\nconstraints: []\n",
        )
        .unwrap();
        std::os::unix::fs::symlink("root", dir.join("link")).unwrap();
        git(&["add", "--all"]);
        git(&["commit", "-qm", "fixture"]);
        let first = git(&["rev-parse", "HEAD"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &first,
            "module",
        ]);
        git(&["commit", "-qm", "gitlink"]);
        let head = git(&["rev-parse", "HEAD"]);
        let revision = GitRevision {
            repository: Text::new("repo").unwrap(),
            commit: GitObjectId::new(head.clone()).unwrap(),
        };
        let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
        let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
        let capture = |name: &str| {
            compile(
                ModelInput::pinned_root(revision.clone(), path(name)).unwrap(),
                Some(&binding),
                &policy(),
                None,
                &ctx,
            )
        };
        let good = capture("root").unwrap();
        assert_eq!(
            good.provenance().unwrap().tree().as_str().len(),
            if format == "sha1" { 40 } else { 64 }
        );
        assert_eq!(
            capture("link-manifest").err().unwrap().code(),
            "path_not_confined"
        );
        assert_eq!(
            capture("module-manifest").err().unwrap().code(),
            "path_not_confined"
        );
        let original = git(&["rev-parse", "HEAD:root"]);
        let other = git(&["rev-parse", "HEAD:other"]);
        let object = |id: &str| dir.join(".git/objects").join(&id[..2]).join(&id[2..]);
        let valid_wrong_bytes = std::fs::read(object(&other)).unwrap();
        std::fs::remove_file(object(&original)).unwrap();
        std::fs::write(object(&original), valid_wrong_bytes).unwrap();
        let error = capture("root").err().unwrap();
        assert_eq!(error.code(), "architecture_input_unavailable");
        assert_eq!(error.diagnostics()[0].subject(), Some("root"));
        assert_eq!(git(&["rev-parse", "HEAD"]), head);
        assert_eq!(std::fs::read(dir.join("root")).unwrap(), root);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn canonical_optional_ownership_field_is_omitted_when_absent() {
    let ctx = ExecutionContext::new(Arc::new(AtomicBool::new(false)), None);
    let base = compile(full_input(true, true), None, &policy(), None, &ctx).unwrap();
    let mut files = Vec::new();
    for file in base.closure() {
        let bytes = if file.path().as_str() == "c.yaml" {
            let mut doc: serde_json::Value = serde_json::from_slice(file.bytes()).unwrap();
            doc["ownership_rules"] = serde_json::json!([{"id":"owner", "component":"a", "responsibility_ids":["ra"], "all_of":[{"kind":"reaches_exported_item", "crate":"crate", "namespace":"ns", "item_path":"item"}], "none_of":[]}]);
            serde_json::to_vec(&doc).unwrap()
        } else {
            file.bytes().to_vec()
        };
        files.push((file.path().clone(), bytes));
    }
    let output = compile(
        ModelInput::inline_files(path("root.yaml"), files).unwrap(),
        None,
        &policy(),
        None,
        &ctx,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(output.canonical_json()).unwrap();
    assert!(value["constraints"][0]["ownership_rules"][0]["all_of"][0]
        .get("max_hops")
        .is_none());
    assert_object_keys_sorted(&value);
}
