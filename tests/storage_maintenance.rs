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

#[test]
fn repeated_health_entries_preserve_source_and_foreign_sentinels() {
    let root = fixture::FixtureRoot::new("health-confinement").unwrap();
    root.write_revision(20).unwrap();
    // These independent sentinels are outside registered program-owned state.
    // Exact observations include bytes, modes and modification timestamps.
    let protected = ["src", "target", "knowledge", "caller-staging", "foreign"];
    for directory in &protected[1..] {
        let path = root.path().join(directory);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("sentinel.bin"),
            b"must survive ordinary runtime entries\0\xff",
        )
        .unwrap();
    }
    #[cfg(unix)]
    let external = fixture::FixtureRoot::new("health-external-sentinel").unwrap();
    #[cfg(unix)]
    let external_before = {
        std::fs::write(
            external.path().join("sentinel.bin"),
            b"external bytes must survive",
        )
        .unwrap();
        std::os::unix::fs::symlink(external.path(), root.path().join("foreign/external-link"))
            .unwrap();
        fixture::observe_small_tree(external.path()).unwrap()
    };
    let before: Vec<_> = protected
        .iter()
        .map(|directory| fixture::observe_small_tree(&root.path().join(directory)).unwrap())
        .collect();
    for _ in 0..3 {
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
            .expect("ordinary health entry reports missing graph without touching foreign state");
        assert_eq!(response.payload["database_exists"], false);
        assert_eq!(response.payload["graph_readable"], false);
        #[cfg(unix)]
        assert_eq!(
            fixture::observe_small_tree(external.path()).unwrap(),
            external_before,
            "external symlink target changed across an unchanged runtime entry"
        );
        for (directory, observation) in protected.iter().zip(&before) {
            assert_eq!(
                &fixture::observe_small_tree(&root.path().join(directory)).unwrap(),
                observation,
                "protected tree {directory} changed across an unchanged runtime entry"
            );
        }
    }
}

/// Public producer/query/reuse coverage only. Disk convergence and injected
/// failure/retirement boundaries require the real collector hooks separately.
#[test]
fn twenty_public_builds_match_clean_control_and_preserve_unchanged_artifact() {
    use fixture::workload::{self, BehaviorSnapshot, StableArtifact, WorkloadPair, UPDATES};

    let pair = WorkloadPair::new().unwrap();
    workload::install(&pair.churn).expect("public setup must publish the initial fixture graph");
    let stable = StableArtifact::observe(&pair.churn).unwrap();
    for revision in 1..=UPDATES {
        pair.churn.write_revision(revision).unwrap();
        let response = workload::materialize(&pair.churn, "full")
            .unwrap_or_else(|error| panic!("revision {revision} materialization failed: {error}"));
        stable.assert_reused(&StableArtifact::observe(&pair.churn).unwrap(), &response);
        let unchanged = workload::unchanged_refresh(&pair.churn).unwrap_or_else(|error| {
            panic!("revision {revision} unchanged refresh failed: {error}")
        });
        println!("revision {revision}: published with stable artifact reuse; {unchanged}");

        assert_eq!(stable, StableArtifact::observe(&pair.churn).unwrap());
    }
    pair.assert_equal_final_sources();
    workload::install(&pair.control)
        .expect("public setup must publish the independent final-state control");
    let churn = BehaviorSnapshot::observe(&pair.churn, UPDATES).unwrap();
    let control = BehaviorSnapshot::observe(&pair.control, UPDATES).unwrap();
    assert_eq!(
        churn, control,
        "churn changed reachable graph/search/context behavior"
    );
}

#[cfg(debug_assertions)]
#[test]
fn native_phase_owner_death_preserves_prior_graph_and_retries_without_source_edit() {
    use fixture::{
        process::FaultFixture,
        workload::{self, BehaviorSnapshot, StableArtifact},
    };
    use std::time::Duration;

    let mut fixture = FaultFixture::new().unwrap();
    workload::install(fixture.root()).unwrap();
    let before = BehaviorSnapshot::observe(fixture.root(), 0).unwrap();
    let stable = StableArtifact::observe(fixture.root()).unwrap();
    fixture.root().write_revision(1).unwrap();
    let mut build = fixture.start_paused_build().unwrap();
    let phase = build
        .wait_for_native_phase(Duration::from_secs(20))
        .unwrap();
    phase.assert_candidate_present();
    assert_eq!(
        before,
        BehaviorSnapshot::observe(fixture.root(), 0).unwrap()
    );
    fixture.assert_external_unchanged();
    assert!(!build.interrupt_owner().unwrap().success());
    drop(build);
    fixture
        .wait_for_recovery(&phase, Duration::from_secs(30))
        .unwrap();
    assert_eq!(
        before,
        BehaviorSnapshot::observe(fixture.root(), 0).unwrap()
    );
    fixture.assert_external_unchanged();
    // No write_revision call follows the interruption: the pending source is
    // retried exactly once after real lifecycle leases/proofs establish empty
    // containment. Retained request bytes remain unchanged until this writer;
    // finish_recovery still requires their actual disposal after publication.
    let response = workload::materialize(fixture.root(), "full").unwrap();
    stable.assert_reused(&StableArtifact::observe(fixture.root()).unwrap(), &response);
    BehaviorSnapshot::observe(fixture.root(), 1).unwrap();
    fixture.finish_recovery(&phase, &response).unwrap();
}

#[cfg(debug_assertions)]
#[test]
fn isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth() {
    use fixture::{
        process::{CoveredEpochObservation, FaultFixture},
        workload,
    };
    use std::time::Duration;
    let mut fixture = FaultFixture::new().unwrap();
    fixture.reset_closed_managed_storage().unwrap();
    let mut mcp = fixture.start_mcp().unwrap();
    let first = mcp.wait_for_revision(0, Duration::from_secs(90)).unwrap();
    let first_coverage = CoveredEpochObservation::observe(fixture.root(), &first).unwrap();
    fixture.root().write_revision(1).unwrap();
    let followup = mcp.wait_for_revision(1, Duration::from_secs(90)).unwrap();
    let followup_coverage = CoveredEpochObservation::observe(fixture.root(), &followup).unwrap();
    followup_coverage.assert_same_birth_and_advanced_epoch(&first_coverage);
    let reaped = mcp.close().unwrap();
    fixture.root().write_revision(2).unwrap();
    let later = workload::materialize(fixture.root(), "full").unwrap();
    let health = fixture.public_health().unwrap();
    let later_coverage = CoveredEpochObservation::observe(fixture.root(), &health).unwrap();
    later_coverage.assert_same_birth_and_advanced_epoch(&followup_coverage);
    workload::BehaviorSnapshot::observe(fixture.root(), 2).unwrap();
    fixture.finish_worker_fixture(reaped, &later).unwrap();
}
