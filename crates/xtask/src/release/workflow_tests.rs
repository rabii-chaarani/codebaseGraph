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
        fs::create_dir_all(root.join("dist/release-assets")).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("calls"), "").unwrap();
        for target in crate::NATIVE_TARGETS {
            let archive = target.archive_name("2.0.0");
            fs::write(
                root.join("dist/release-assets").join(&archive),
                b"validated archive",
            )
            .unwrap();
            fs::write(
                root.join("dist/release-assets")
                    .join(format!("{archive}.sha256")),
                b"validated sidecar",
            )
            .unwrap();
        }
        fs::write(
            root.join("bin/gh"),
            r##"#!/bin/bash
set -euo pipefail
echo "$*" >> "$RUNNER_TEMP/calls"
release_file="$RUNNER_TEMP/release"
assets="$RUNNER_TEMP/assets"
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
    [[ ! -e "$release_file" ]] || { echo 'Duplicate release' >&2; exit 95; }
    cp "${!#}" "$RUNNER_TEMP/request"
    jq --argjson freeze "$MOCK_FREEZE" '. + {id:7,immutable:((.draft | not) and $freeze)}' "$RUNNER_TEMP/request" > "$release_file"
    [[ "$MOCK_LOST_CREATE" != true ]] || exit 92
    cat "$release_file" ;;
  *"--method PATCH"*"/releases/7 --input "*)
    [[ "$(jq -r .immutable "$release_file")" != true ]] || exit 94
    cp "${!#}" "$RUNNER_TEMP/publish-request"
    jq --slurpfile patch "$RUNNER_TEMP/publish-request" --argjson freeze "$MOCK_FREEZE" '. + $patch[0] | .immutable = ((.draft | not) and $freeze)' "$release_file" > "$RUNNER_TEMP/updated"
    mv "$RUNNER_TEMP/updated" "$release_file"
    [[ "$MOCK_LOST_PUBLISH" != true ]] || exit 91
    cat "$release_file" ;;
  *"releases/tags/v2.0.0")
    # The real tag endpoint does not establish whether a draft exists.
    if [[ -e "$release_file" && "$(jq -r .draft "$release_file")" == false ]]; then cat "$release_file"
    else printf '{"status":"404"}'; exit 1; fi ;;
  *"releases/7/assets?per_page=100")
    for asset in "$assets"/*; do
      [[ -f "$asset" ]] || continue
      jq -n --arg name "$(basename "$asset")" --argjson size "$(wc -c < "$asset")" '{id:$name,name:$name,size:$size}'
    done | jq -s '[.]' ;;
  *"releases/assets/"*)
    name="${!#}"
    name="${name##*/}"
    cat "$assets/$name" ;;
  *"releases/7") cat "$release_file" ;;
  *"releases?per_page=100")
    # Separate pages exercise authenticated paginated draft discovery.
    pages="$MOCK_RELEASES"
    if [[ -e "$RUNNER_TEMP/uploaded" ]]; then pages="$(jq --argjson late "$MOCK_LATE_RELEASES" '. + $late' <<<"$pages")"; fi
    jq --slurpfile release <(if [[ -e "$release_file" ]]; then cat "$release_file"; fi) '. + [$release]' <<<"$pages" ;;
  "release upload "*)
    [[ "$(jq -r .immutable "$release_file")" != true ]] || { echo 'Immutable release rejects asset upload' >&2; exit 93; }
    [[ "$MOCK_UPLOAD_FAIL" != true ]] || exit 90
    shift 3
    for file in "$@"; do
      [[ "$file" != --* ]] || break
      cp "$file" "$assets/$(basename "$file")"
    done
    touch "$RUNNER_TEMP/uploaded"
    if [[ -n "$MOCK_CORRUPT" ]]; then printf 'invalidated bytes' > "$assets/$MOCK_CORRUPT"; fi ;;
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
            ("AUTOMATIC","true".into()),("MOCK_FREEZE","true".into()),
            ("MOCK_UPLOAD_FAIL","false".into()),("MOCK_CORRUPT","".into()),
            ("MOCK_LOST_CREATE","false".into()),("MOCK_LOST_PUBLISH","false".into()),
            ("MOCK_LATE_RELEASES","[]".into()),
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

    fn release(&self) -> Value {
        serde_json::from_slice(&fs::read(self.root.join("release")).unwrap()).unwrap()
    }

    fn seed_release(&self, draft: bool, immutable: bool) {
        fs::write(
            self.root.join("ref"),
            json!({"object":{"type":"commit","sha":SHA}}).to_string(),
        )
        .unwrap();
        fs::write(self.root.join("release"), json!({"id":7,"tag_name":"v2.0.0","draft":draft,"prerelease":false,"immutable":immutable}).to_string()).unwrap();
    }

    fn seed_assets(&self) {
        for entry in fs::read_dir(self.root.join("dist/release-assets")).unwrap() {
            let entry = entry.unwrap();
            fs::copy(
                entry.path(),
                self.root.join("assets").join(entry.file_name()),
            )
            .unwrap();
        }
    }

    fn calls(&self) -> String {
        fs::read_to_string(self.root.join("calls")).unwrap()
    }

    fn assert_no_writes(&self) {
        let calls = self.calls();
        assert!(
            !calls.contains("--method POST")
                && !calls.contains("--method PATCH")
                && !calls.contains("--method DELETE")
                && !calls.contains("release upload"),
            "{calls}"
        );
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

fn publisher() -> String {
    step("publish-release-assets", "Create only the verified release")
}

#[test]
fn immutable_release_uploads_before_publication_and_retries_without_writes() {
    let f = Fixture::new();
    assert_success(&f.execute(&publisher(), &[]));
    let release: Value =
        serde_json::from_slice(&fs::read(f.root.join("release")).unwrap()).unwrap();
    assert_eq!(release["draft"], false);
    assert_eq!(release["immutable"], true);
    assert_eq!(
        fs::read_dir(f.root.join("assets")).unwrap().count(),
        crate::NATIVE_TARGETS.len() * 2
    );
    fs::write(f.root.join("calls"), "").unwrap();
    assert_success(&f.execute(&publisher(), &[]));
    let calls = fs::read_to_string(f.root.join("calls")).unwrap();
    assert!(
        !calls.contains("--method POST")
            && !calls.contains("--method PATCH")
            && !calls.contains("release upload"),
        "{calls}"
    );
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
    assert_eq!(request["draft"], true);
    let published: Value =
        serde_json::from_slice(&fs::read(f.root.join("publish-request")).unwrap()).unwrap();
    assert_eq!(published["make_latest"], "true");
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
        serde_json::from_slice(&fs::read(f.root.join("publish-request")).unwrap()).unwrap();
    assert_eq!(request["make_latest"], "false");
}

#[test]
fn draft_partial_upload_resumes_and_latest_is_selected_at_publication() {
    let f = Fixture::new();
    f.seed_release(true, false);
    let asset = crate::NATIVE_TARGETS[0].archive_name("2.0.0");
    fs::write(f.root.join("assets").join(asset), b"partial old bytes").unwrap();
    assert_success(&f.execute(
        &publisher(),
        &[(
            "MOCK_LATE_RELEASES",
            r#"[[{"tag_name":"v3.0.0","draft":false,"prerelease":false}]]"#,
        )],
    ));
    assert_eq!(f.release()["draft"], false);
    assert!(!f.calls().contains("--method POST"));
    let patch: Value =
        serde_json::from_slice(&fs::read(f.root.join("publish-request")).unwrap()).unwrap();
    assert_eq!(patch["make_latest"], "false");
}

#[test]
fn upload_or_remote_byte_failure_never_publishes() {
    let archive = crate::NATIVE_TARGETS[0].archive_name("2.0.0");
    let sidecar = format!("{archive}.sha256");
    for overrides in [
        vec![("MOCK_UPLOAD_FAIL", "true")],
        vec![("MOCK_CORRUPT", archive.as_str())],
        vec![("MOCK_CORRUPT", sidecar.as_str())],
    ] {
        let f = Fixture::new();
        assert!(!f.execute(&publisher(), &overrides).status.success());
        assert_eq!(f.release()["draft"], true);
        assert!(!f.calls().contains("--method PATCH"));
    }
}

#[test]
fn immutable_missing_or_mismatched_assets_fail_without_mutation() {
    for corrupt in [false, true] {
        for sidecar in [false, true] {
            let f = Fixture::new();
            f.seed_release(false, true);
            f.seed_assets();
            let name = crate::NATIVE_TARGETS[0].archive_name("2.0.0");
            let name = if sidecar {
                format!("{name}.sha256")
            } else {
                name
            };
            let path = f.root.join("assets").join(name);
            if corrupt {
                // Keep size unchanged: matching metadata alone must not hide corruption.
                let size = fs::metadata(&path).unwrap().len();
                fs::write(path, vec![b'x'; size as usize]).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
            assert!(!f.execute(&publisher(), &[]).status.success());
            f.assert_no_writes();
        }
    }
}

#[test]
fn accepted_writes_with_lost_responses_resume_without_replay() {
    for lost in ["MOCK_LOST_CREATE", "MOCK_LOST_PUBLISH"] {
        let f = Fixture::new();
        assert!(!f.execute(&publisher(), &[(lost, "true")]).status.success());
        let calls = f.calls();
        assert_eq!(
            calls
                .matches("--method POST repos/owner/repo/releases --input")
                .count(),
            1
        );
        if lost == "MOCK_LOST_CREATE" {
            assert_eq!(f.release()["draft"], true);
            assert!(!calls.contains("release upload"));
        } else {
            assert_eq!(f.release()["immutable"], true);
            assert_eq!(calls.matches("--method PATCH").count(), 1);
        }
        fs::write(f.root.join("calls"), "").unwrap();
        assert_success(&f.execute(&publisher(), &[]));
        assert!(!f.calls().contains("--method POST"));
        if lost == "MOCK_LOST_PUBLISH" {
            f.assert_no_writes();
        }
    }
}

#[test]
fn existing_tag_mode_remains_assets_only_for_draft_mutable_and_immutable_releases() {
    for (draft, immutable) in [(true, false), (false, false), (false, true)] {
        let f = Fixture::new();
        f.seed_release(draft, immutable);
        if immutable {
            f.seed_assets();
        }
        assert_success(&f.execute(&publisher(), &[("AUTOMATIC", "false")]));
        assert_eq!(f.release()["draft"], draft);
        assert!(!f.calls().contains("--method POST") && !f.calls().contains("--method PATCH"));
        if immutable {
            f.assert_no_writes();
        }
    }
    let f = Fixture::new();
    f.seed_release(false, true);
    assert!(!f
        .execute(&publisher(), &[("AUTOMATIC", "false")])
        .status
        .success());
    f.assert_no_writes();
    let f = Fixture::new();
    assert!(!f
        .execute(&publisher(), &[("AUTOMATIC", "false")])
        .status
        .success());
    f.assert_no_writes();
}

#[test]
fn published_mutable_partial_assets_can_resume_without_republishing() {
    let f = Fixture::new();
    f.seed_release(false, false);
    assert_success(&f.execute(&publisher(), &[]));
    assert_eq!(f.release()["draft"], false);
    assert!(f.calls().contains("release upload"));
    assert!(!f.calls().contains("--method POST") && !f.calls().contains("--method PATCH"));
    fs::write(f.root.join("calls"), "").unwrap();
    assert_success(&f.execute(&publisher(), &[]));
    f.assert_no_writes();
}

#[test]
fn publisher_rejects_identity_metadata_prerelease_and_ambiguous_drafts_before_writes() {
    for kind in ["pr", "metadata", "prerelease", "ambiguous"] {
        let f = Fixture::new();
        f.seed_release(true, false);
        let extra;
        let overrides = match kind {
            "pr" => vec![("VERIFIED_PR", "125")],
            "metadata" => {
                fs::write(
                    f.root.join("dist/metadata/release-metadata.json"),
                    json!({"source_sha":MAIN,"tag_name":"v2.0.0"}).to_string(),
                )
                .unwrap();
                vec![]
            }
            "prerelease" => {
                let mut release = f.release();
                release["prerelease"] = json!(true);
                fs::write(f.root.join("release"), release.to_string()).unwrap();
                vec![]
            }
            _ => {
                extra = json!([[f.release()]]).to_string();
                vec![("MOCK_RELEASES", extra.as_str())]
            }
        };
        assert!(
            !f.execute(&publisher(), &overrides).status.success(),
            "{kind}"
        );
        f.assert_no_writes();
    }
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
fn staged_assets_match_the_supported_native_targets() {
    let script = step("validate-artifacts", "Stage complete validated set");
    for complete in [true, false] {
        let f = Fixture::new();
        let native = f.root.join("dist/native");
        fs::create_dir_all(&native).unwrap();
        for target in crate::NATIVE_TARGETS {
            let archive = target.archive_name("2.0.0");
            fs::write(native.join(&archive), b"validated archive").unwrap();
            fs::write(native.join(format!("{archive}.sha256")), b"checksum").unwrap();
        }
        if !complete {
            let archive = crate::NATIVE_TARGETS[0].archive_name("2.0.0");
            fs::remove_file(native.join(format!("{archive}.sha256"))).unwrap();
        }
        fs::remove_dir_all(f.root.join("dist/release-assets")).unwrap();
        let output = f.execute(&format!("set -euo pipefail\n{script}"), &[]);
        assert_eq!(
            output.status.success(),
            complete,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if complete {
            assert_eq!(
                fs::read_dir(f.root.join("dist/release-assets"))
                    .unwrap()
                    .count(),
                crate::NATIVE_TARGETS.len() * 2
            );
        }
    }
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
