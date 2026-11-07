use codebase_graph::architecture::{
    compile::{
        compile,
        identity::{identify, qualify_artifacts},
        ArchitectureExecutionPolicy, ExecutionContext, ModelInput, RepositoryBinding,
    },
    ArtifactRef, Digest, GitObjectId, RelativePath, Text,
};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

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
fn path(s: &str) -> RelativePath {
    RelativePath::new(s).unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn structured(domain: &str, payload: &str) -> String {
    digest(format!("{domain}/0.1\n{payload}").as_bytes())
}
fn input(root: &str, arch: &str, rules: &str) -> ModelInput {
    ModelInput::inline_files(
        path("root"),
        [
            (path("root"), root.as_bytes().to_vec()),
            (path("arch"), arch.as_bytes().to_vec()),
            (path("rules"), rules.as_bytes().to_vec()),
        ],
    )
    .unwrap()
}
const ROOT: &str =
    "schema_version: '0.1'\nmodel_id: model\narchitecture: [arch]\nconstraints: [rules]\n";
const ARCH: &str =
    "schema_version: '0.1'\nsystems: []\ncontainers: []\ncomponents: []\nrelationships: []\n";
const RULES: &str = "schema_version: '0.1'\nownership_rules: []\nallow: []\nrequire: []\nbudgets: []\nwaivers: []\nscenarios: []\n";
fn compile_inline(
    root: &str,
    arch: &str,
    rules: &str,
) -> codebase_graph::architecture::compile::CompiledModel {
    compile(input(root, arch, rules), None, &policy(), None, &ctx()).unwrap()
}

#[test]
fn independent_canonical_oracles_and_raw_closure() {
    let model = compile_inline(ROOT, ARCH, RULES);
    let identity = identify(&model, &ctx()).unwrap();
    let intent = r#"{"allow":[],"budgets":[],"components":[],"containers":[],"model_id":"model","ownership_rules":[],"relationships":[],"require":[],"scenarios":[],"schema_version":"0.1","systems":[],"waivers":[]}"#;
    assert_eq!(
        identity.intent_digest().as_str(),
        structured("architecture-intent", intent)
    );
    let closure = format!(
        "[[\"arch\",\"{}\"],[\"root\",\"{}\"],[\"rules\",\"{}\"]]",
        digest(ARCH.as_bytes()),
        digest(ROOT.as_bytes()),
        digest(RULES.as_bytes())
    );
    assert_eq!(
        identity.closure_digest().as_str(),
        structured("architecture-closure", &closure)
    );
    let mut p = serde_json::to_value(policy()).unwrap();
    p.sort_all_objects();
    assert_eq!(
        identity.policy_digest().as_str(),
        structured("architecture-policy", &serde_json::to_string(&p).unwrap())
    );
    let changed = compile_inline(ROOT, &(ARCH.to_string() + "# comment\n"), RULES);
    let changed = identify(&changed, &ctx()).unwrap();
    assert_eq!(identity.intent_digest(), changed.intent_digest());
    assert_ne!(identity.closure_digest(), changed.closure_digest());
    assert_ne!(
        identity.compile_input_digest(),
        changed.compile_input_digest()
    );
}
#[test]
fn set_order_is_normalized_but_exact_strings_and_ids_are_not() {
    let a = json!({"schema_version":"0.1", "systems":[{"id":"z","name":"é\nZ","responsibilities":[{"id":"r-z","statement":"z"},{"id":"r-a","statement":"a"}]},{"id":"a","name":"A","responsibilities":[{"id":"r-other","statement":"other"}]}], "containers":[],"components":[],"relationships":[]});
    let mut b = a.clone();
    b["systems"].as_array_mut().unwrap().reverse();
    b["systems"][1]["responsibilities"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let ia = identify(&compile_inline(ROOT, &a.to_string(), RULES), &ctx()).unwrap();
    let ib = identify(&compile_inline(ROOT, &b.to_string(), RULES), &ctx()).unwrap();
    assert_eq!(ia.intent_digest(), ib.intent_digest());
    b["systems"][1]["name"] = json!("e\u{301}\nZ");
    let ic = identify(&compile_inline(ROOT, &b.to_string(), RULES), &ctx()).unwrap();
    assert_ne!(ia.intent_digest(), ic.intent_digest());
}
fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_owned()
}
fn reference(commit: &str, path: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        repository: Text::new("repo").unwrap(),
        commit: GitObjectId::new(commit).unwrap(),
        path: self::path(path),
        content_digest: Digest::new(digest(bytes)).unwrap(),
    }
}
#[test]
fn pinned_git_qualification_and_failure_boundaries() {
    let dir = std::env::temp_dir().join(format!(
        "cod22-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.name", "test"]);
    git(&dir, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(dir.join("file"), b"original").unwrap();
    std::fs::write(dir.join("literal*[?]"), b"literal").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("file", dir.join("link")).unwrap();
    git(&dir, &["add", "--all"]);
    git(&dir, &["commit", "-qm", "first"]);
    let first_commit = git(&dir, &["rev-parse", "HEAD"]);
    git(
        &dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &first_commit,
            "module",
        ],
    );
    git(&dir, &["commit", "-qm", "gitlink"]);
    let commit = git(&dir, &["rev-parse", "HEAD"]);
    let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
    let r = reference(&commit, "file", b"original");
    let literal = reference(&commit, "literal*[?]", b"literal");
    let qualified_literal = qualify_artifacts(
        std::slice::from_ref(&literal),
        std::slice::from_ref(&binding),
        &policy(),
        None,
        &ctx(),
    )
    .unwrap();
    assert_eq!(qualified_literal.artifacts()[0].reference(), &literal);
    assert_eq!(qualified_literal.artifacts()[0].bytes(), b"literal");
    std::fs::write(dir.join("file"), b"changed").unwrap();
    let q = qualify_artifacts(
        &[r.clone(), r.clone()],
        std::slice::from_ref(&binding),
        &policy(),
        None,
        &ctx(),
    )
    .unwrap();
    assert_eq!(q.artifacts().len(), 1);
    assert_eq!(q.artifacts()[0].bytes(), b"original");
    assert_eq!(q.artifacts()[0].reference(), &r);
    let mut logical = serde_json::to_value(&q).unwrap();
    let mut size = 1u64;
    loop {
        logical["effective_policy"]["max_result_bytes"] = size.into();
        let next = serde_json::to_vec(&logical).unwrap().len() as u64;
        if next == size {
            break;
        }
        size = next;
    }
    let mut result_policy = serde_json::to_value(policy()).unwrap();
    result_policy["max_result_bytes"] = size.into();
    let exact_result = ArchitectureExecutionPolicy::from_json(result_policy.clone()).unwrap();
    assert!(qualify_artifacts(
        std::slice::from_ref(&r),
        std::slice::from_ref(&binding),
        &exact_result,
        None,
        &ctx()
    )
    .is_ok());
    result_policy["max_result_bytes"] = (size - 1).into();
    let under_result = ArchitectureExecutionPolicy::from_json(result_policy).unwrap();
    assert_eq!(
        qualify_artifacts(
            std::slice::from_ref(&r),
            std::slice::from_ref(&binding),
            &under_result,
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "resource_limit"
    );
    assert_eq!(
        qualify_artifacts(std::slice::from_ref(&r), &[], &policy(), None, &ctx())
            .err()
            .unwrap()
            .code(),
        "architecture_input_unavailable"
    );
    assert_eq!(
        qualify_artifacts(
            std::slice::from_ref(&r),
            &[binding.clone(), binding.clone()],
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "invalid_reference"
    );
    let wrong = reference(&commit, "file", b"wrong");
    assert_eq!(
        qualify_artifacts(
            &[r.clone(), wrong.clone()],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "invalid_reference"
    );
    assert_eq!(
        qualify_artifacts(
            &[wrong],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "invalid_reference"
    );
    assert_eq!(
        qualify_artifacts(
            &[reference(&commit, "missing", b"")],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "architecture_input_unavailable"
    );
    #[cfg(unix)]
    assert_eq!(
        qualify_artifacts(
            &[reference(&commit, "link", b"original")],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "path_not_confined"
    );
    assert_eq!(
        qualify_artifacts(
            &[reference(&commit, "module", b"")],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "path_not_confined"
    );
    assert_eq!(
        qualify_artifacts(
            &[reference(&"0".repeat(40), "file", b"")],
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx()
        )
        .err()
        .unwrap()
        .code(),
        "architecture_input_unavailable"
    );
    let mut desc = serde_json::to_value(&r).unwrap();
    desc.sort_all_objects();
    let n = serde_json::to_vec(&desc).unwrap().len() as u64 + 8;
    let mut p = serde_json::to_value(policy()).unwrap();
    p["max_evidence_bytes"] = n.into();
    let exact = ArchitectureExecutionPolicy::from_json(p.clone()).unwrap();
    assert!(qualify_artifacts(
        std::slice::from_ref(&r),
        std::slice::from_ref(&binding),
        &exact,
        None,
        &ctx()
    )
    .is_ok());
    p["max_evidence_bytes"] = (n - 1).into();
    let under = ArchitectureExecutionPolicy::from_json(p).unwrap();
    assert_eq!(
        qualify_artifacts(&[r], &[binding], &under, None, &ctx())
            .err()
            .unwrap()
            .code(),
        "resource_limit"
    );
    // A blob object ID is not a commit, even when it exists locally.
    let blob = git(&dir, &["rev-parse", &format!("{commit}:file")]);
    let wrong_type = reference(&blob, "file", b"original");
    let bound = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
    assert_eq!(
        qualify_artifacts(&[wrong_type], &[bound], &policy(), None, &ctx())
            .err()
            .unwrap()
            .code(),
        "invalid_reference"
    );
    // A loose object with valid compressed bytes for a different blob must fail integrity verification.
    std::fs::write(dir.join("other"), b"different").unwrap();
    let other = git(&dir, &["hash-object", "-w", "other"]);
    let object_path = |id: &str| dir.join(".git/objects").join(&id[..2]).join(&id[2..]);
    let wrong_object = std::fs::read(object_path(&other)).unwrap();
    std::fs::remove_file(object_path(&blob)).unwrap();
    std::fs::write(object_path(&blob), wrong_object).unwrap();
    let error = qualify_artifacts(
        &[reference(&commit, "file", b"original")],
        &[RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap()],
        &policy(),
        None,
        &ctx(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), "invalid_reference");
    assert_eq!(error.diagnostics()[0].subject(), Some("file"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn cancellation_and_deadline_block_both_services() {
    let model = compile_inline(ROOT, ARCH, RULES);
    let canceled = ExecutionContext::new(Arc::new(AtomicBool::new(true)), None);
    let expired = ExecutionContext::new(Arc::new(AtomicBool::new(false)), Some(Instant::now()));
    for (c, code) in [(&canceled, "cancelled"), (&expired, "deadline_exceeded")] {
        assert_eq!(identify(&model, c).err().unwrap().code(), code);
        assert_eq!(
            qualify_artifacts(&[], &[], &policy(), None, c)
                .err()
                .unwrap()
                .code(),
            code
        );
    }
}

#[test]
fn nested_inventory_deduplicates_typed_references_and_preserves_operation_order() {
    let reference = json!({"repository":"repo","commit":"a".repeat(40),"path":"doc","content_digest":format!("sha256:{}", "b".repeat(64))});
    let nested = json!({"id":"nested","parameters":{"contract":{"kind":"contract_ref","reference":reference},"text":{"kind":"string","value":"not a locator"}},"preconditions":[{"kind":"contract_ref_equals","id":"predicate","relationship_id":"r","expected":[reference]}],"obligations":[],"baseline_transform":[{"kind":"add_relationship","relationship":{"id":"r","level":"component","from":"x","to":"y","description":"r","contract_refs":[reference]}}],"candidate_transform":[]});
    let scenario = json!({"id":"outer","parameters":{"contract":{"kind":"contract_ref","reference":reference}},"preconditions":[],"obligations":[],"baseline_transform":[{"kind":"add_constraint","constraint":{"kind":"scenario","record":nested}},{"kind":"remove_relationship","id":"r","expected_record_digest":format!("sha256:{}", "c".repeat(64))}],"candidate_transform":[]});
    let mut rules = json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[scenario]});
    let model = compile_inline(ROOT, ARCH, &rules.to_string());
    let first = identify(&model, &ctx()).unwrap();
    assert_eq!(first.linked_artifacts().len(), 1);
    assert_eq!(
        serde_json::to_value(first.linked_artifacts()[0].reference()).unwrap(),
        reference
    );
    let mut canonical_ref = reference.clone();
    canonical_ref.sort_all_objects();
    assert_eq!(
        first.linked_artifacts()[0].reference_digest().as_str(),
        structured(
            "architecture-artifact-reference",
            &canonical_ref.to_string()
        )
    );
    rules["scenarios"][0]["baseline_transform"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reversed = identify(&compile_inline(ROOT, ARCH, &rules.to_string()), &ctx()).unwrap();
    assert_ne!(first.intent_digest(), reversed.intent_digest());
}

#[test]
fn inline_and_git_closure_match_but_provenance_differs() {
    let dir = std::env::temp_dir().join(format!(
        "cod22-model-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.name", "test"]);
    git(&dir, &["config", "user.email", "test@example.invalid"]);
    for (name, content) in [("root", ROOT), ("arch", ARCH), ("rules", RULES)] {
        std::fs::write(dir.join(name), content).unwrap();
    }
    git(&dir, &["add", "--all"]);
    git(&dir, &["commit", "-qm", "model"]);
    let rev = codebase_graph::architecture::GitRevision {
        repository: Text::new("repo").unwrap(),
        commit: GitObjectId::new(git(&dir, &["rev-parse", "HEAD"])).unwrap(),
    };
    let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
    let pinned = compile(
        ModelInput::pinned_root(rev, path("root")).unwrap(),
        Some(&binding),
        &policy(),
        None,
        &ctx(),
    )
    .unwrap();
    let inline = compile_inline(ROOT, ARCH, RULES);
    let a = identify(&pinned, &ctx()).unwrap();
    let b = identify(&inline, &ctx()).unwrap();
    assert_eq!(a.intent_digest(), b.intent_digest());
    assert_eq!(a.closure_digest(), b.closure_digest());
    assert_ne!(a.compile_input_digest(), b.compile_input_digest());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deeply_nested_scenarios_identify_without_recursive_stack() {
    let mut nested = String::new();
    for _ in 0..300 {
        nested.push_str("{id: nested, parameters: {}, preconditions: [], obligations: [], baseline_transform: [{kind: add_constraint, constraint: {kind: scenario, record: ");
    }
    nested.push_str("{id: leaf, parameters: {}, preconditions: [], obligations: [], baseline_transform: [], candidate_transform: []}");
    for _ in 0..300 {
        nested.push_str("}}], candidate_transform: []}");
    }
    let root = "schema_version: '0.1'\nmodel_id: model\narchitecture: []\nconstraints: [rules]\n";
    let rules = format!("schema_version: '0.1'\nownership_rules: []\nallow: []\nrequire: []\nbudgets: []\nwaivers: []\nscenarios: [{nested}]\n");
    let input = ModelInput::inline_files(
        path("root"),
        [
            (path("root"), root.as_bytes().to_vec()),
            (path("rules"), rules.into_bytes()),
        ],
    )
    .unwrap();
    let mut p = serde_json::to_value(policy()).unwrap();
    p["max_file_bytes"] = 1000000.into();
    p["max_yaml_bytes"] = 1000000.into();
    p["max_result_bytes"] = 2000000.into();
    let model = compile(
        input,
        None,
        &ArchitectureExecutionPolicy::from_json(p).unwrap(),
        None,
        &ctx(),
    )
    .unwrap();
    assert!(identify(&model, &ctx())
        .unwrap()
        .linked_artifacts()
        .is_empty());
}

#[test]
fn path_and_policy_changes_invalidate_the_expected_constituents() {
    let original = compile_inline(ROOT, ARCH, RULES);
    let original = identify(&original, &ctx()).unwrap();
    let renamed_root = ROOT.replace("[arch]", "[other]");
    let renamed = ModelInput::inline_files(
        path("root"),
        [
            (path("root"), renamed_root.into_bytes()),
            (path("other"), ARCH.as_bytes().to_vec()),
            (path("rules"), RULES.as_bytes().to_vec()),
        ],
    )
    .unwrap();
    let renamed = compile(renamed, None, &policy(), None, &ctx()).unwrap();
    let renamed = identify(&renamed, &ctx()).unwrap();
    assert_eq!(original.intent_digest(), renamed.intent_digest());
    assert_ne!(original.closure_digest(), renamed.closure_digest());
    assert_ne!(
        original.compile_input_digest(),
        renamed.compile_input_digest()
    );
    let moved = ModelInput::inline_files(
        path("other-root"),
        [
            (path("other-root"), ROOT.as_bytes().to_vec()),
            (path("arch"), ARCH.as_bytes().to_vec()),
            (path("rules"), RULES.as_bytes().to_vec()),
        ],
    )
    .unwrap();
    let moved = compile(moved, None, &policy(), None, &ctx()).unwrap();
    let moved = identify(&moved, &ctx()).unwrap();
    assert_eq!(original.intent_digest(), moved.intent_digest());
    assert_ne!(original.closure_digest(), moved.closure_digest());
    assert_ne!(
        original.compile_input_digest(),
        moved.compile_input_digest()
    );
    let changed_root = compile_inline(&(ROOT.to_string() + "# comment\n"), ARCH, RULES);
    let changed_root = identify(&changed_root, &ctx()).unwrap();
    assert_eq!(original.intent_digest(), changed_root.intent_digest());
    assert_ne!(original.closure_digest(), changed_root.closure_digest());
    assert_ne!(
        original.compile_input_digest(),
        changed_root.compile_input_digest()
    );
    let mut p = serde_json::to_value(policy()).unwrap();
    p["max_traversal_steps"] = 99999.into();
    let changed = compile(
        input(ROOT, ARCH, RULES),
        None,
        &ArchitectureExecutionPolicy::from_json(p).unwrap(),
        None,
        &ctx(),
    )
    .unwrap();
    let changed = identify(&changed, &ctx()).unwrap();
    assert_eq!(original.intent_digest(), changed.intent_digest());
    assert_eq!(original.closure_digest(), changed.closure_digest());
    assert_ne!(original.policy_digest(), changed.policy_digest());
    assert_ne!(
        original.compile_input_digest(),
        changed.compile_input_digest()
    );
}

#[test]
fn moving_declarations_between_documents_preserves_intent() {
    let system_a = json!({"id":"a","name":"A","responsibilities":[{"id":"ra","statement":"A"}]});
    let system_b = json!({"id":"b","name":"B","responsibilities":[{"id":"rb","statement":"B"}]});
    let arch = |systems| {
        json!({"schema_version":"0.1","systems":systems,"containers":[],"components":[],"relationships":[]}).to_string().into_bytes()
    };
    let root = |references| {
        json!({"schema_version":"0.1","model_id":"model","architecture":references,"constraints":[]}).to_string().into_bytes()
    };
    let one = ModelInput::inline_files(
        path("root"),
        [
            (path("root"), root(json!(["a"]))),
            (path("a"), arch(json!([system_a, system_b]))),
        ],
    )
    .unwrap();
    let two = ModelInput::inline_files(
        path("root"),
        [
            (path("root"), root(json!(["a", "b"]))),
            (path("a"), arch(json!([system_a]))),
            (path("b"), arch(json!([system_b]))),
        ],
    )
    .unwrap();
    let first = compile(one, None, &policy(), None, &ctx()).unwrap();
    let second = compile(two, None, &policy(), None, &ctx()).unwrap();
    let a = identify(&first, &ctx()).unwrap();
    let b = identify(&second, &ctx()).unwrap();
    assert_eq!(a.intent_digest(), b.intent_digest());
    assert_ne!(a.closure_digest(), b.closure_digest());
}

#[test]
fn waiver_authorization_references_survive_nested_transform_inventory() {
    let digest = format!("sha256:{}", "d".repeat(64));
    let locator = |path: &str| json!({"repository":"repo","commit":"a".repeat(40),"path":path,"content_digest":digest});
    let waiver = |id: &str, path: &str| json!({"id":id,"budget_id":"budget","baseline":{"kind":"input","id":digest},"original_limit":1,"ceiling":2,"expires_at":"2028-01-01T00:00:00Z","approval_ref":{"reference":locator(path),"context_id":"context"}});
    let arch = json!({"schema_version":"0.1","systems":[{"id":"sys","name":"Sys","responsibilities":[{"id":"rs","statement":"own"}]}],"containers":[{"id":"cont","name":"Cont","kind":"application","system":"sys","responsibilities":[{"id":"rc","statement":"own"}]}],"components":[{"id":"comp","name":"Comp","container":"cont","responsibilities":[{"id":"rr","statement":"own"}]}],"relationships":[]});
    let budget = json!({"id":"budget","level":"component","origins":["comp"],"projection":{"version":"rust-q1/0.1","domain_ref":digest},"metric":"cyclic_nodes","absolute_max":1,"max_increase":0,"baseline":{"kind":"input","id":digest}});
    let nested = json!({"id":"nested","parameters":{},"preconditions":[],"obligations":[],"baseline_transform":[{"kind":"add_constraint","constraint":{"kind":"waiver","record":waiver("add-waiver","add") }},{"kind":"replace_constraint","constraint":{"kind":"waiver","record":waiver("replace-waiver","replace")},"expected_record_digest":digest}],"candidate_transform":[]});
    let scenario = json!({"id":"outer","parameters":{},"preconditions":[],"obligations":[],"baseline_transform":[{"kind":"add_constraint","constraint":{"kind":"scenario","record":nested}}],"candidate_transform":[]});
    let rules = json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[budget],"waivers":[waiver("base-waiver","base")],"scenarios":[scenario]});
    let model = compile_inline(ROOT, &arch.to_string(), &rules.to_string());
    let identity = identify(&model, &ctx()).unwrap();
    let refs: Vec<_> = identity
        .linked_artifacts()
        .iter()
        .map(|entry| serde_json::to_value(entry.reference()).unwrap())
        .collect();
    assert_eq!(
        refs,
        vec![locator("add"), locator("base"), locator("replace")]
    );
}

#[test]
fn nonempty_intent_matches_independent_canonical_payload() {
    let arch = json!({"schema_version":"0.1","systems":[{"id":"sys","name":"System","responsibilities":[{"id":"z","statement":"Z"},{"id":"a","statement":"A"}]}],"containers":[],"components":[],"relationships":[]});
    let element = json!({"level":"component","id":"future"});
    let pred = |id| json!({"kind":"element_exists","id":id,"element":element});
    let scenario = json!({"id":"scenario","parameters":{},"preconditions":[pred("p-z"),pred("p-a")],"obligations":[pred("obligation")],"baseline_transform":[],"candidate_transform":[]});
    let rules = json!({"schema_version":"0.1","ownership_rules":[],"allow":[],"require":[],"budgets":[],"waivers":[],"scenarios":[scenario]});
    let model = compile_inline(ROOT, &arch.to_string(), &rules.to_string());
    let identity = identify(&model, &ctx()).unwrap();
    let expected = r#"{"allow":[],"budgets":[],"components":[],"containers":[],"model_id":"model","ownership_rules":[],"relationships":[],"require":[],"scenarios":[{"baseline_transform":[],"candidate_transform":[],"id":"scenario","obligations":[{"element":{"id":"future","level":"component"},"id":"obligation","kind":"element_exists"}],"parameters":{},"preconditions":[{"element":{"id":"future","level":"component"},"id":"p-a","kind":"element_exists"},{"element":{"id":"future","level":"component"},"id":"p-z","kind":"element_exists"}]}],"schema_version":"0.1","systems":[{"id":"sys","name":"System","responsibilities":[{"id":"a","statement":"A"},{"id":"z","statement":"Z"}]}],"waivers":[]}"#;
    assert_eq!(
        identity.intent_digest().as_str(),
        structured("architecture-intent", expected)
    );
}

#[test]
fn unreadable_commit_tree_and_blob_are_distinct_from_missing_objects() {
    for (kind, format) in ["commit", "tree", "blob"].into_iter().flat_map(|kind| {
        ["sha1", "sha256"]
            .into_iter()
            .map(move |format| (kind, format))
    }) {
        let dir = std::env::temp_dir().join(format!(
            "cod22-corrupt-{kind}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q", &format!("--object-format={format}")])
            .output()
            .unwrap();
        if !init.status.success() && format == "sha256" {
            std::fs::remove_dir_all(dir).unwrap();
            continue;
        }
        assert!(init.status.success());
        git(&dir, &["config", "core.fsyncObjectFiles", "true"]);
        git(&dir, &["config", "user.name", "test"]);
        git(&dir, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(dir.join("file"), b"content").unwrap();
        std::fs::write(dir.join("other"), b"other").unwrap();
        git(&dir, &["add", "--all"]);
        git(&dir, &["commit", "-qm", "first"]);
        let commit = git(&dir, &["rev-parse", "HEAD"]);
        let id = match kind {
            "commit" => commit.clone(),
            "tree" => git(&dir, &["rev-parse", "HEAD^{tree}"]),
            "blob" => git(&dir, &["rev-parse", "HEAD:file"]),
            _ => unreachable!(),
        };
        let obj = dir.join(".git/objects").join(&id[..2]).join(&id[2..]);
        let valid = std::fs::read(&obj).unwrap();
        let binding = RepositoryBinding::new(Text::new("repo").unwrap(), dir.join(".git")).unwrap();
        let refs = [
            reference(&commit, "file", b"content"),
            reference(&commit, "other", b"other"),
        ];
        let qualified = qualify_artifacts(
            &refs,
            std::slice::from_ref(&binding),
            &policy(),
            None,
            &ctx(),
        )
        .unwrap();
        assert_eq!(
            qualified.artifacts().len(),
            2,
            "valid with warning config: {kind} {format}"
        );
        let missing_commit = reference(&"0".repeat(commit.len()), "file", b"content");
        assert_eq!(
            qualify_artifacts(
                &[missing_commit],
                std::slice::from_ref(&binding),
                &policy(),
                None,
                &ctx()
            )
            .err()
            .unwrap()
            .code(),
            "architecture_input_unavailable"
        );
        let unavailable_binding =
            RepositoryBinding::new(Text::new("repo").unwrap(), dir.join("missing-repository"))
                .unwrap();
        assert_eq!(
            qualify_artifacts(&refs, &[unavailable_binding], &policy(), None, &ctx())
                .err()
                .unwrap()
                .code(),
            "architecture_input_unavailable"
        );
        for (label, corrupt) in [
            ("invalid-zlib", b"not-zlib".as_slice()),
            ("truncated", &valid[..valid.len() / 2]),
        ] {
            std::fs::remove_file(&obj).unwrap();
            std::fs::write(&obj, corrupt).unwrap();
            let error = qualify_artifacts(
                &refs,
                std::slice::from_ref(&binding),
                &policy(),
                None,
                &ctx(),
            )
            .err()
            .expect("corrupt object cannot yield partial success");
            assert_eq!(error.code(), "invalid_reference", "corrupt {kind} {label}");
            assert_eq!(error.diagnostics()[0].subject(), Some("file"));
        }
        std::fs::remove_file(&obj).unwrap();
        let error = qualify_artifacts(&refs, &[binding], &policy(), None, &ctx())
            .err()
            .expect("missing object cannot yield partial success");
        assert_eq!(
            error.code(),
            "architecture_input_unavailable",
            "missing {kind}"
        );
        assert_eq!(error.diagnostics()[0].subject(), Some("file"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
