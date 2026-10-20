#[path = "support/storage_maintenance/mod.rs"]
mod fixture;

use codebase_graph::api::{
    CodebaseGraphApi, HealthRequest, OperationInvocation, OperationRequest, OutputFormat,
    RepoSelector,
};
use serde_json::{json, Value};

#[test]
fn nonboolean_quiescence_confirmation_is_rejected_before_repository_effects() {
    let root = fixture::FixtureRoot::new("bad-confirmation").unwrap();
    root.write_revision(20).unwrap();
    let before = fixture::observe_small_tree(root.path()).unwrap();
    for invalid in [Value::Null, json!("true"), json!(1), json!({}), json!([])] {
        let invocation = OperationInvocation {
            repo: RepoSelector {
                repo_root: Some(root.path().to_path_buf()),
                ..RepoSelector::default()
            },
            arguments: json!({ "confirm_legacy_quiescence": invalid }),
            output_format: OutputFormat::Typed,
        };
        let error = CodebaseGraphApi::new()
            .execute_invocation("health", &invocation)
            .expect_err("nonboolean confirmation must fail instead of authorizing migration");
        assert_eq!(error.code, "invalid_request");
        assert!(error
            .message
            .contains("confirm_legacy_quiescence must be a boolean"));
        assert_eq!(
            fixture::observe_small_tree(root.path()).unwrap(),
            before,
            "invalid confirmation must be rejected before repository effects"
        );
    }
}

#[test]
fn health_preserves_legacy_fields_and_marks_unobserved_maintenance_unknown() {
    let root = fixture::FixtureRoot::new("health-contract").unwrap();
    root.write_revision(20).unwrap();
    let response = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Health(HealthRequest {
            repo: RepoSelector {
                repo_root: Some(root.path().to_path_buf()),
                ..RepoSelector::default()
            },
            confirm_legacy_quiescence: false,
            refresh_status: None,
            output_format: OutputFormat::Typed,
        }))
        .expect("health reports a missing graph through its existing payload");
    let payload = response.payload;
    assert_eq!(payload["database_exists"], false);
    assert_eq!(payload["graph_readable"], false);
    assert!(payload["cleanup_pending"].is_boolean());
    let summary = &payload["maintenance"];
    assert!(
        summary.is_object(),
        "additive maintenance summary is required"
    );
    assert!(summary["requested"].is_boolean());
    assert!(summary["running"].is_boolean());
    assert!(summary["pending"].is_boolean());
    assert!(summary["reasons"].is_array());
    assert!(summary["errors"].is_array());

    // A later real collector may supply an observation. The unobserved branch
    // must never turn unavailable measurements into fabricated zero bytes.
    if summary["observed_at_unix_ms"].is_null() {
        assert_eq!(summary["measurement_complete"], false);
        assert_eq!(summary["observation_complete"], false);
        assert!(summary["observation_age_ms"].is_null());
        assert!(summary["inventory_status"].is_null());
        for disposition in [
            "total",
            "retained",
            "potential_reclamation",
            "deferred",
            "unknown",
        ] {
            assert_unknown_bucket(&summary["bytes"][disposition]);
        }
        for category in [
            "active_graph",
            "active_cache",
            "leased_generation",
            "live_run",
            "recovery_evidence",
            "artifact_temporary",
            "legacy_staging",
            "retired_direct",
            "coordination",
            "unknown",
        ] {
            assert_unknown_bucket(&summary["bytes"]["by_category"][category]);
        }
    }
}

fn assert_unknown_bucket(bucket: &Value) {
    let object = bucket.as_object().expect("each fixed byte bucket exists");
    assert!(object.contains_key("logical_bytes"));
    assert!(object.contains_key("allocated_bytes"));
    assert!(bucket["logical_bytes"].is_null());
    assert!(bucket["allocated_bytes"].is_null());
}
