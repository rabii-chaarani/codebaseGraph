use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    adapters::mcp::{self, McpSession},
    api::{OkfWikiApi, WikiApiError, WikiOperationExecutor, WikiOperationRequest},
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, NoopRefreshNotifier, NoopValidator,
        RepositoryRoot,
    },
    service::LocalWikiService,
};
use serde_json::{json, Value};

struct TestBundle(PathBuf);

impl TestBundle {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "k-wiki-diagnostic-transports-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(
            root.join("docs/index.md"),
            "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
        )
        .unwrap();
        Self(root)
    }

    fn bundle(&self) -> PathBuf {
        self.0.join("docs")
    }

    fn api(&self) -> OkfWikiApi<LocalWikiService> {
        let authoring = AuthoringService::new(
            AuthoringConfig {
                repositories: vec![RepositoryRoot {
                    id: "repo".into(),
                    root_path: self.0.clone(),
                }],
                bundles: vec![BundleRoot {
                    id: "docs".into(),
                    repository_id: "repo".into(),
                    root_path: self.bundle(),
                }],
            },
            NoopValidator,
            NoopRefreshNotifier,
        )
        .unwrap();
        LocalWikiService::new(vec![self.bundle()])
            .with_authoring(authoring)
            .into_api()
    }
}

impl Drop for TestBundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn call_tool<E: WikiOperationExecutor>(api: &OkfWikiApi<E>, tool: &str, arguments: Value) -> Value {
    let response = mcp::handle_message(
        json!({"jsonrpc": "2.0", "id": 17, "method": "tools/call", "params": {"name": tool, "arguments": arguments}}),
        &mut McpSession { protocol_version: Some(mcp::protocol_version().into()), initialized: true },
        &mut |_| json!({"type": "object"}),
        &mut |name, arguments| mcp::dispatch_api(api, name, arguments),
    ).unwrap();
    // Assert on the final wire envelope, after both adapter layers and serialization.
    serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap()
}

#[test]
fn mcp_missing_concept_preserves_domain_error_on_the_wire() {
    let fixture = TestBundle::new();
    let response = call_tool(
        &fixture.api(),
        "wiki_get_concept",
        json!({"bundle_id": "docs", "concept_id": "missing"}),
    );
    assert_eq!(response["id"], 17);
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "concept was not found");
    assert_eq!(
        response["error"]["data"],
        json!({"code": "concept_not_found", "details": null, "retryable": false})
    );
    assert!(response.get("result").is_none());
}

#[test]
fn mcp_stale_authoring_write_preserves_conflict_on_the_wire() {
    let fixture = TestBundle::new();
    let api = fixture.api();
    let created = call_tool(
        &api,
        "wiki_create_page",
        json!({"bundle_id": "docs", "page_path": "note", "type": "note", "title": "Note", "body_markdown": "Original.", "include_structured_content": true}),
    );
    let hash = created["result"]["structuredContent"]["result"]["content_hash"]
        .as_str()
        .expect("created content hash");
    let changed = "---\ntype: note\ntitle: Note\n---\nChanged by another writer.\n";
    fs::write(fixture.bundle().join("note.md"), changed).unwrap();
    let response = call_tool(
        &api,
        "wiki_populate_page",
        json!({"bundle_id": "docs", "page_path": "note", "frontmatter": {"type": "note", "title": "Note"}, "body_markdown": "Replacement.", "expected_content_hash": hash}),
    );
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["data"]["code"], "write_conflict");
    assert_eq!(response["error"]["data"]["retryable"], false);
    assert!(response["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));
    assert_eq!(
        fs::read_to_string(fixture.bundle().join("note.md")).unwrap(),
        changed
    );
}

#[test]
fn mcp_retryable_api_error_preserves_details_on_the_wire() {
    let api = OkfWikiApi::new(|_: &WikiOperationRequest| {
        Err(
            WikiApiError::new("projection_busy", "retry projection refresh")
                .details(json!({"generation": 3, "reason": "refresh_in_progress"}))
                .retryable(true),
        )
    });
    let response = call_tool(&api, "wiki_list_bundles", json!({}));
    assert_eq!(
        response["error"],
        json!({"code": -32602, "message": "retry projection refresh", "data": {"code": "projection_busy", "details": {"generation": 3, "reason": "refresh_in_progress"}, "retryable": true}})
    );
}

#[test]
fn mcp_malformed_arguments_keep_rpc_classification() {
    let fixture = TestBundle::new();
    let response = call_tool(
        &fixture.api(),
        "wiki_get_concept",
        json!({"bundle_id": "docs"}),
    );
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "tool arguments are invalid");
}

#[test]
fn cli_validate_rejected_profiles_exit_nonzero_in_text_and_json() {
    let fixture = TestBundle::new();
    fs::write(
        fixture.bundle().join("note.md"),
        "---\ntitle: Note\n---\n# Note\n",
    )
    .unwrap();
    for profile in ["conformant", "recommended"] {
        for json_output in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_k-wiki"));
            command
                .arg("validate")
                .arg(fixture.bundle())
                .args(["--profile", profile]);
            if json_output {
                command.arg("--json");
            }
            let output = command.output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(1),
                "{profile} json={json_output}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if json_output {
                let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(payload["result"]["accepted"], false);
                assert!(payload["result"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["severity"] == "error"));
            } else {
                let text = String::from_utf8_lossy(&output.stdout);
                assert!(text.contains("bundle rejected"));
                assert!(text.contains("missing_type (note.md:1)"));
                assert!(text.contains("requires a non-empty 'type' field"));
            }
        }
    }
}

#[test]
fn cli_validate_accepted_profiles_exit_zero_in_text_and_json() {
    let fixture = TestBundle::new();
    // Complete metadata satisfies every profile without warnings.
    fs::write(
        fixture.bundle().join("note.md"),
        "---\ntype: note\ntitle: Note\ndescription: A diagnostic fixture.\n---\n# Note\n",
    )
    .unwrap();
    for profile in ["consume", "conformant", "recommended"] {
        for json_output in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_k-wiki"));
            command
                .arg("validate")
                .arg(fixture.bundle())
                .args(["--profile", profile]);
            if json_output {
                command.arg("--json");
            }
            let output = command.output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if json_output {
                let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(payload["result"]["accepted"], true);
                assert!(payload["result"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|item| item["severity"] != "error"));
            } else {
                assert!(String::from_utf8_lossy(&output.stdout).contains("bundle accepted"));
            }
        }
    }
}

#[test]
fn cli_validate_consume_accepts_error_diagnostics() {
    let fixture = TestBundle::new();
    fs::write(
        fixture.bundle().join("note.md"),
        "---\ntitle: Note\n---\n# Note\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_k-wiki"))
        .arg("validate")
        .arg(fixture.bundle())
        .args(["--profile", "consume", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["result"]["accepted"], true);
    assert!(payload["result"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["severity"] == "error"));
}

#[test]
fn cli_validate_warning_acceptance_depends_on_profile() {
    let fixture = TestBundle::new();
    fs::write(fixture.bundle().join("note.md"), "---\ntype: note\ntitle: Note\ndescription: A diagnostic fixture.\n---\n[Missing](missing.md)\n").unwrap();
    for (profile, accepted) in [
        ("consume", true),
        ("conformant", true),
        ("recommended", false),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_k-wiki"))
            .arg("validate")
            .arg(fixture.bundle())
            .args(["--profile", profile, "--json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(if accepted { 0 } else { 1 }));
        let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(payload["result"]["accepted"], accepted);
        assert!(payload["result"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["severity"] == "warning"));
    }
}
