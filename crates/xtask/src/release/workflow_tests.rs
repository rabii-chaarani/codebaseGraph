//! Execute the actual workflow Bash with a closed, local GitHub API fixture.
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MAIN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Fixture {
    root: PathBuf,
    run: Value,
    pulls: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = crate::unique_temp_dir("release_workflow").unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("dist/metadata")).unwrap();
        fs::write(
            root.join("bin/gh"),
            r##"#!/bin/bash
set -euo pipefail
echo "$*" >> "$RUNNER_TEMP/calls"
case "$*" in
  *"actions/runs/42") printf '%s' "$MOCK_RUN" ;;
  *"git/ref/heads/main"*) printf '%s' "$MOCK_MAIN" ;;
  *"compare/"*) printf '%s' "$MOCK_COMPARISON" ;;
  *"/commits/"*"/pulls?per_page=100") printf '%s' "$MOCK_PULLS" ;;
  *"contents/.release-please-manifest.json?"*) printf 'eyIuIjoiMi4wLjAifQ==' ;;
  *"actions/workflows/ci.yml/runs?"*) printf '%s' "$MOCK_PROPOSAL_RUNS" ;;
  *"pulls?state=closed"*) printf '%s' "$MOCK_PENDING_PULLS" ;;
  *"--method POST"*"git/refs"*)
    [[ "$*" == *"sha=$SOURCE_SHA"* ]] || exit 96
    jq -n --arg sha "$SOURCE_SHA" '{object:{type:"commit",sha:$sha}}' > "$RUNNER_TEMP/ref"
    cat "$RUNNER_TEMP/ref" ;;
  *"git/ref/tags/v2.0.0")
    if [[ -e "$RUNNER_TEMP/ref" ]]; then cat "$RUNNER_TEMP/ref"
    elif [[ "$MOCK_REF" != 'null' ]]; then printf '%s' "$MOCK_REF"
    else printf '{"status":"404"}'; exit 1; fi ;;
  *"--method POST"*"/releases --input "*)
    cp "${!#}" "$RUNNER_TEMP/request"
    printf '{"tag_name":"v2.0.0","draft":false,"prerelease":false}' > "$RUNNER_TEMP/release"
    cat "$RUNNER_TEMP/release" ;;
  *"releases/tags/v2.0.0")
    if [[ -e "$RUNNER_TEMP/release" ]]; then cat "$RUNNER_TEMP/release"
    else printf '{"status":"404"}'; exit 1; fi ;;
  *"releases?per_page=100") printf '%s' "$MOCK_RELEASES" ;;
  *) echo "Unexpected GitHub request: $*" >&2; exit 97 ;;
esac
"##,
        )
        .unwrap();
        fs::set_permissions(root.join("bin/gh"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.join("dist/metadata/release-metadata.json"), json!({
            "source_sha":SHA,"tag_name":"v2.0.0","version":"2.0.0","notes":"## 2.0.0\nVerified notes, not $(touch injected)."
        }).to_string()).unwrap();
        Self {
            root,
            run: json!({"id":42,"repository":{"full_name":"owner/repo"},"path":".github/workflows/ci.yml","event":"push","head_branch":"main","status":"completed","conclusion":"success","head_sha":SHA}),
            pulls: json!([[{
                "number":124,"merged_at":"2026-09-23","merge_commit_sha":SHA,
                "base":{"ref":"main","repo":{"full_name":"owner/repo"}},
                "head":{"ref":"release-please--branches--main--components--codebase-graph","repo":{"full_name":"owner/repo"}},
                "labels":[{"name":"autorelease: pending"}]
            }]]),
        }
    }

    fn execute(&self, script: &str, overrides: &[(&str, &str)]) -> Output {
        fs::write(self.root.join("output"), "").unwrap();
        let mut command = Command::new("bash");
        command.args(["-c",script]).current_dir(&self.root).envs([
            ("PATH",format!("{}:{}",self.root.join("bin").display(),std::env::var("PATH").unwrap())),
            ("GITHUB_REPOSITORY","owner/repo".into()),("CI_RUN_ID","42".into()),
            ("EXPECTED_SHA",SHA.into()),("REQUIRE_RELEASE","true".into()),
            ("MOCK_RUN",self.run.to_string()),("MOCK_PULLS",self.pulls.to_string()),
            ("MOCK_MAIN",MAIN.into()),("MOCK_COMPARISON","ahead".into()),
            ("MOCK_REF","null".into()),("MOCK_RELEASES","[]".into()),
            ("MOCK_PENDING_PULLS",self.pulls.to_string()),
            ("MOCK_PROPOSAL_RUNS",json!([{"workflow_runs":[{"path":".github/workflows/ci.yml","head_sha":MAIN,"head_branch":"main","event":"push","status":"completed","conclusion":"success"}]}]).to_string()),
            ("SOURCE_SHA",SHA.into()),("RELEASE_TAG","v2.0.0".into()),
            ("RELEASE_PR","124".into()),("VERIFIED_PR","124".into()),
            ("RUNNER_TEMP",self.root.display().to_string()),
            ("GITHUB_OUTPUT",self.root.join("output").display().to_string()),
            ("GITHUB_STEP_SUMMARY",self.root.join("summary").display().to_string()),
        ]).env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
        command.envs(overrides.iter().copied()).output().unwrap()
    }

    fn output(&self) -> String {
        fs::read_to_string(self.root.join("output")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn guard() -> String {
    let action: yaml_serde::Value = yaml_serde::from_str(include_str!(
        "../../../../.github/actions/verified-release/action.yml"
    ))
    .unwrap();
    action["runs"]["steps"][0]["run"].as_str().unwrap().into()
}

fn step(job: &str, name: &str) -> String {
    let workflow: yaml_serde::Value =
        yaml_serde::from_str(include_str!("../../../../.github/workflows/release.yml")).unwrap();
    workflow["jobs"][job]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|s| s["name"].as_str() == Some(name))
        .unwrap()["run"]
        .as_str()
        .unwrap()
        .into()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verified_release_survives_main_advancing_and_ignores_other_pending_releases() {
    let mut f = Fixture::new();
    let mut other = f.pulls[0][0].clone();
    other["number"] = json!(125);
    other["merge_commit_sha"] = json!(MAIN);
    f.pulls[0].as_array_mut().unwrap().push(other);
    assert_success(&f.execute(&guard(), &[]));
    assert!(f.output().contains("release-pr=124\n"));
    assert!(f.output().contains(&format!("source-sha={SHA}\n")));
}

#[test]
fn verified_release_rejects_failed_ci_and_untrusted_run_identity() {
    for (key, value) in [
        ("conclusion", json!("failure")),
        ("status", json!("in_progress")),
        ("event", json!("pull_request")),
        ("head_branch", json!("feature")),
        ("path", json!(".github/workflows/other.yml")),
        ("id", json!(43)),
        ("repository", json!({"full_name":"fork/repo"})),
    ] {
        let mut f = Fixture::new();
        f.run[key] = value;
        assert!(!f.execute(&guard(), &[]).status.success(), "{key}");
    }
}

#[test]
fn verified_release_rejects_untrusted_prs_and_removed_history() {
    for (path, value) in [
        ("/merge_commit_sha", json!(MAIN)),
        ("/merged_at", Value::Null),
        ("/base/ref", json!("other")),
        ("/base/repo/full_name", json!("fork/repo")),
        ("/head/repo/full_name", json!("fork/repo")),
        ("/head/ref", json!("feature")),
        ("/labels", json!([])),
    ] {
        let mut f = Fixture::new();
        *f.pulls[0][0].pointer_mut(path).unwrap() = value;
        assert!(!f.execute(&guard(), &[]).status.success(), "{path}");
    }
    let f = Fixture::new();
    assert!(!f
        .execute(&guard(), &[("MOCK_COMPARISON", "diverged")])
        .status
        .success());
    assert!(!f
        .execute(&guard(), &[("EXPECTED_SHA", MAIN)])
        .status
        .success());
}

#[test]
fn ordinary_commit_cannot_authorize_release_and_tagged_retry_still_can() {
    let mut f = Fixture::new();
    f.pulls = json!([[]]);
    assert!(!f.execute(&guard(), &[]).status.success());
    assert_success(&f.execute(&guard(), &[("REQUIRE_RELEASE", "false")]));
    assert!(f.output().contains("release-pr=\n"));
    let mut retry = Fixture::new();
    retry.pulls[0][0]["labels"] = json!([{"name":"autorelease: tagged"}]);
    assert_success(&retry.execute(&guard(), &[]));
}

#[test]
fn publisher_targets_original_sha_and_is_idempotent() {
    let f = Fixture::new();
    let script = step("publish-release-assets", "Create only the verified release");
    assert_success(&f.execute(&script, &[]));
    let request: Value =
        serde_json::from_slice(&fs::read(f.root.join("request")).unwrap()).unwrap();
    assert_eq!(request["target_commitish"], SHA);
    assert_eq!(request["tag_name"], "v2.0.0");
    assert_eq!(request["make_latest"], "true");
    assert!(!f.root.join("injected").exists());
    fs::write(f.root.join("calls"), "").unwrap();
    assert_success(&f.execute(&script, &[]));
    assert!(!fs::read_to_string(f.root.join("calls"))
        .unwrap()
        .contains("--method POST"));
}

#[test]
fn publisher_rejects_conflicting_tags_and_does_not_regress_latest() {
    let f = Fixture::new();
    let script = step("publish-release-assets", "Create only the verified release");
    let wrong = json!({"object":{"sha":MAIN,"type":"commit"}}).to_string();
    assert!(!f.execute(&script, &[("MOCK_REF", &wrong)]).status.success());
    assert!(!f.root.join("request").exists());
    assert_success(&f.execute(
        &script,
        &[(
            "MOCK_RELEASES",
            r#"[[{"tag_name":"v3.0.0","draft":false,"prerelease":false}]]"#,
        )],
    ));
    let request: Value =
        serde_json::from_slice(&fs::read(f.root.join("request")).unwrap()).unwrap();
    assert_eq!(request["make_latest"], "false");
}

#[test]
fn proposal_reports_pending_release_then_proceeds_after_finalization() {
    let f = Fixture::new();
    let script = step(
        "release-please",
        "Verify current main CI and detect blocked proposals",
    );
    let blocked = f.execute(&script, &[]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stdout).contains("resume-ci-run"));
    let mut tagged = f.pulls.clone();
    tagged[0][0]["labels"] = json!([{"name":"autorelease: tagged"}]);
    assert_success(&f.execute(&script, &[("MOCK_PENDING_PULLS", &tagged.to_string())]));
    assert!(f.output().contains("ready=true"));
    assert_success(&f.execute(&script, &[("MOCK_PROPOSAL_RUNS", "[]")]));
    assert!(!f.output().contains("ready=true"));
}

#[test]
fn ci_run_recovery_dry_run_validates_without_authorizing_publication() {
    let f = Fixture::new();
    let script = step("release-target", "Resolve tag and source SHA");
    let inputs = [
        ("GITHUB_EVENT_NAME", "workflow_dispatch"),
        ("MANUAL_TAG", ""),
        ("MANUAL_SOURCE", "promote"),
        ("MANUAL_DRY_RUN", "true"),
        ("VERIFIED_SHA", SHA),
    ];
    assert_success(&f.execute(&script, &inputs));
    let output = f.output();
    assert!(output.contains("should-publish=true\n"));
    assert!(output.contains("automatic=true\n"));
    assert!(output.contains("publish_assets=false\n"));
    assert!(output.contains("ci-run-id=42\n"));
    assert!(!fs::read_to_string(f.root.join("calls"))
        .unwrap()
        .contains("--method POST"));
    let mut publishing = inputs;
    publishing[3] = ("MANUAL_DRY_RUN", "false");
    assert_success(&f.execute(&script, &publishing));
    assert!(f.output().contains("publish_assets=true\n"));
}

#[test]
fn recovery_requires_one_target_and_forbids_ci_run_rebuilds() {
    let f = Fixture::new();
    let script = step("release-target", "Validate recovery inputs");
    for (tag, run, source, allowed) in [
        ("", "", "promote", false),
        ("v2.0.0", "42", "promote", false),
        ("", "42", "rebuild-if-missing", false),
        ("", "42", "promote", true),
        ("v2.0.0", "", "rebuild-if-missing", true),
    ] {
        let result = f.execute(
            &script,
            &[
                ("GITHUB_EVENT_NAME", "workflow_dispatch"),
                ("MANUAL_TAG", tag),
                ("RESUME_CI_RUN", run),
                ("MANUAL_SOURCE", source),
            ],
        );
        assert_eq!(result.status.success(), allowed, "{tag}/{run}/{source}");
    }
}
