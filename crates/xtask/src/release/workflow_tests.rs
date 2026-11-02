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
const NEXT_MAIN: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const TAG_OBJECT: &str = "cccccccccccccccccccccccccccccccccccccccc";
const NEW_TAG_OBJECT: &str = "dddddddddddddddddddddddddddddddddddddddd";

#[derive(Debug)]
struct ProcessCall {
    program: String,
    cwd: String,
    args: Vec<String>,
}

struct PreflightFailure {
    name: &'static str,
    report: Option<Value>,
    overrides: Vec<(&'static str, &'static str)>,
}

fn valid_preflight_report() -> Value {
    json!({
        "schema_version": 1,
        "source_sha": SHA,
        "version": "2.0.0",
        "tag_name": "v2.0.0",
        "verification": "publish-dry-run",
        "crate_bytes": 1,
        "crate_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    })
}

fn preflight_failures() -> Vec<PreflightFailure> {
    let valid = valid_preflight_report();
    let mut cases = vec![
        PreflightFailure {
            name: "failed result",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_RESULT", "failure")],
        },
        PreflightFailure {
            name: "cancelled result",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_RESULT", "cancelled")],
        },
        PreflightFailure {
            name: "missing result",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_RESULT", "")],
        },
        PreflightFailure {
            name: "unexpected skipped result",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_RESULT", "skipped")],
        },
        PreflightFailure {
            name: "missing report",
            report: None,
            overrides: vec![],
        },
        PreflightFailure {
            name: "wrong source environment",
            report: Some(valid.clone()),
            overrides: vec![("SOURCE_SHA", MAIN)],
        },
        PreflightFailure {
            name: "wrong version environment",
            report: Some(valid.clone()),
            overrides: vec![("RELEASE_VERSION", "2.0.1")],
        },
        PreflightFailure {
            name: "wrong tag environment",
            report: Some(valid.clone()),
            overrides: vec![("RELEASE_TAG", "v2.0.1")],
        },
        PreflightFailure {
            name: "wrong producer run",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_RUN_ID", "41")],
        },
        PreflightFailure {
            name: "missing producer attempt",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_ATTEMPT", "")],
        },
        PreflightFailure {
            name: "zero producer attempt",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_ATTEMPT", "0")],
        },
        PreflightFailure {
            name: "future producer attempt",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_ATTEMPT", "4")],
        },
        PreflightFailure {
            name: "invalid current attempt",
            report: Some(valid.clone()),
            overrides: vec![("GITHUB_RUN_ATTEMPT", "0")],
        },
        PreflightFailure {
            name: "wrong artifact attempt",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_ARTIFACT", "crate-preflight-42-2")],
        },
        PreflightFailure {
            name: "wrong artifact run",
            report: Some(valid.clone()),
            overrides: vec![("PREFLIGHT_ARTIFACT", "crate-preflight-41-3")],
        },
    ];
    for (name, field, value) in [
        ("wrong source report", "source_sha", json!(MAIN)),
        ("wrong version report", "version", json!("2.0.1")),
        ("wrong tag report", "tag_name", json!("v2.0.1")),
        ("wrong schema", "schema_version", json!(2)),
        ("wrong verification", "verification", json!("package")),
        ("zero package size", "crate_bytes", json!(0)),
        ("oversize package", "crate_bytes", json!(10_485_761)),
        ("fractional package size", "crate_bytes", json!(1.5)),
        ("string package size", "crate_bytes", json!("1")),
        (
            "uppercase digest",
            "crate_sha256",
            json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ),
        (
            "short digest",
            "crate_sha256",
            json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ),
    ] {
        let mut report = valid.clone();
        report[field] = value;
        cases.push(PreflightFailure {
            name,
            report: Some(report),
            overrides: vec![],
        });
    }
    cases
}

fn process_calls(path: &PathBuf) -> Vec<ProcessCall> {
    let bytes = fs::read(path).unwrap_or_default();
    let values = bytes
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8(value.to_vec()).unwrap())
        .collect::<Vec<_>>();
    let mut calls = Vec::new();
    let mut index = 0;
    while index < values.len() {
        let program = values[index].clone();
        let cwd = values[index + 1].clone();
        let count = values[index + 2].parse::<usize>().unwrap();
        index += 3;
        let end = index + count;
        assert!(end <= values.len(), "truncated subprocess call log");
        calls.push(ProcessCall {
            program,
            cwd,
            args: values[index..end].to_vec(),
        });
        index = end;
    }
    calls
}

struct Fixture {
    root: PathBuf,
    run: Value,
    pulls: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(crate::unique_temp_dir("release_workflow").unwrap()).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("dist/metadata")).unwrap();
        fs::create_dir_all(root.join("dist/release-assets")).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("workflow-helper/crates/xtask")).unwrap();
        fs::create_dir_all(root.join("selected-source")).unwrap();
        fs::write(root.join("calls"), "").unwrap();
        fs::write(root.join("process-calls"), "").unwrap();
        fs::write(root.join("curl-reads"), "0").unwrap();
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
printf '%s\0%s\0%s\0' "gh" "$PWD" "$#" >> "$RUNNER_TEMP/process-calls"
printf '%s\0' "$@" >> "$RUNNER_TEMP/process-calls"
echo "$*" >> "$RUNNER_TEMP/calls"
release_file="$RUNNER_TEMP/release"
assets="$RUNNER_TEMP/assets"
case "$*" in
  *"actions/runs/42") printf '%s' "$MOCK_RUN" ;;
  "api repos/owner/repo/git/ref/heads/main --jq .object.sha")
    [[ "$MOCK_MAIN_FAIL" != true ]] || { echo 'main lookup failed' >&2; exit 98; }
    printf '%s' "$MOCK_MAIN" ;;
  "api repos/owner/repo/compare/$MOCK_COMPARE_SOURCE...$MOCK_MAIN --jq .status")
    [[ "$MOCK_COMPARE_FAIL" != true ]] || { echo 'comparison failed' >&2; exit 98; }
    printf '%s' "$MOCK_COMPARISON" ;;
  *"/commits/"*"/pulls?per_page=100") printf '%s' "$MOCK_PULLS" ;;
  *"contents/.release-please-manifest.json?"*) printf 'eyIuIjoiMi4wLjAifQ==' ;;
  *"actions/workflows/ci.yml/runs?"*) printf '%s' "$MOCK_PROPOSAL_RUNS" ;;
  *"pulls?state=closed"*) printf '%s' "$MOCK_PENDING_PULLS" ;;
  *"--method POST"*"git/refs"*)
    [[ "$*" == *"sha=$SOURCE_SHA"* ]] || exit 96
    jq -n --arg sha "$SOURCE_SHA" '{object:{type:"commit",sha:$sha}}' > "$RUNNER_TEMP/ref"
    cat "$RUNNER_TEMP/ref" ;;
  "api repos/owner/repo/git/ref/tags/$MOCK_TAG_NAME")
    tag_reads=0
    [[ ! -f "$RUNNER_TEMP/tag-reads" ]] || tag_reads="$(cat "$RUNNER_TEMP/tag-reads")"
    tag_reads=$((tag_reads + 1))
    printf '%s' "$tag_reads" > "$RUNNER_TEMP/tag-reads"
    if [[ "$MOCK_TAG_FAIL" == true || ( "$MOCK_TAG_FAIL_AFTER" != 0 && "$tag_reads" -ge "$MOCK_TAG_FAIL_AFTER" ) ]]; then
      printf '{"status":"503"}'
      exit 1
    fi
    if [[ -e "$RUNNER_TEMP/ref" ]]; then
      if [[ "$MOCK_TAG_CHANGE_AFTER" != 0 && "$tag_reads" -ge "$MOCK_TAG_CHANGE_AFTER" ]]; then
        if [[ "$MOCK_FINAL_REF" == 'null' ]]; then printf '{"status":"404"}'; exit 1; fi
        printf '%s' "$MOCK_FINAL_REF"
      else
        cat "$RUNNER_TEMP/ref"
      fi
    elif [[ "$MOCK_REF" != 'null' ]]; then printf '%s' "$MOCK_REF"
    else printf '{"status":"404"}'; exit 1; fi ;;
  "api repos/owner/repo/git/tags/$MOCK_ANNOTATED_OBJECT --jq .object.sha")
    [[ "$MOCK_ANNOTATION_FAIL" != true ]] || { echo 'annotation lookup failed' >&2; exit 98; }
    printf '%s' "$MOCK_ANNOTATED_TARGET" ;;
  "api repos/owner/repo/git/tags/$MOCK_FINAL_ANNOTATED_OBJECT --jq .object.sha")
    [[ "$MOCK_ANNOTATION_FAIL" != true ]] || { echo 'annotation lookup failed' >&2; exit 98; }
    printf '%s' "$MOCK_FINAL_ANNOTATED_TARGET" ;;
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
  "pr edit 124 --repo owner/repo --add-label autorelease: tagged --remove-label autorelease: pending") touch "$RUNNER_TEMP/finalized" ;;
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
        fs::write(
            root.join("bin/cargo"),
            r##"#!/bin/bash
set -euo pipefail
printf '%s\0%s\0%s\0' "cargo" "$PWD" "$#" >> "$RUNNER_TEMP/process-calls"
printf '%s\0' "$@" >> "$RUNNER_TEMP/process-calls"
case "${1:-}" in
  run)
    [[ "$#" -eq 14 && "$PWD" == "$GITHUB_WORKSPACE" &&
       "$1" == run && "$2" == --manifest-path &&
       "$3" == "$GITHUB_WORKSPACE/workflow-helper/crates/xtask/Cargo.toml" &&
       "$4" == --locked && "$5" == -- && "$6" == release-package-preflight &&
       "$7" == --source-dir && "$8" == "$GITHUB_WORKSPACE/selected-source" &&
       "$9" == --source-sha && "${10}" == "$SOURCE_SHA" &&
       "${11}" == --tag && "${12}" == "$RELEASE_TAG" &&
       "${13}" == --output &&
       "${14}" == "$RUNNER_TEMP/crate-preflight-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT/crate-preflight.json" ]] || {
      echo "Unexpected crate preflight invocation: $*" >&2
      exit 96
    }
    case "$MOCK_PREFLIGHT_FAILURE" in
      dry-run) echo 'mock cargo dry-run failed' >&2; exit 41 ;;
      size) echo 'mock exact-size check failed' >&2; exit 42 ;;
      none) ;;
      *) echo "Unexpected preflight failure mode: $MOCK_PREFLIGHT_FAILURE" >&2; exit 97 ;;
    esac
    printf '{"schema_version":1,"source_sha":"%s","version":"2.0.0","tag_name":"%s","verification":"publish-dry-run","crate_bytes":1,"crate_sha256":"%064d"}\n'       "$SOURCE_SHA" "$RELEASE_TAG" 0 > "${14}"
    ;;
  publish)
    [[ "$#" -eq 2 && "$1" == publish && "$2" == --locked && "$PWD" == "$GITHUB_WORKSPACE" ]] || {
      echo "Unexpected Cargo publish invocation: $*" >&2
      exit 96
    }
    status="$MOCK_CARGO_PUBLISH_STATUS"
    [[ "$status" =~ ^[0-9]+$ ]] || { echo 'Invalid mock Cargo status' >&2; exit 97; }
    exit "$status"
    ;;
  *) echo "Unexpected Cargo invocation: $*" >&2; exit 97 ;;
esac
"##,
        )
        .unwrap();
        fs::write(
            root.join("bin/curl"),
            r##"#!/bin/bash
set -euo pipefail
printf '%s\0%s\0%s\0' "curl" "$PWD" "$#" >> "$RUNNER_TEMP/process-calls"
printf '%s\0' "$@" >> "$RUNNER_TEMP/process-calls"
[[ "$#" -eq 10 && "$1" == --fail && "$2" == --silent && "$3" == --show-error &&
   "$4" == --connect-timeout && "$5" == 10 && "$6" == --max-time && "$7" == 30 &&
   "$8" == --user-agent &&
   "$9" == "codebaseGraph-release-workflow ($GITHUB_SERVER_URL/$GITHUB_REPOSITORY)" &&
   "${10}" == "https://crates.io/api/v1/crates/$CRATE_NAME/$CRATE_VERSION" ]] || {
  echo "Unexpected crates.io query: $*" >&2
  exit 96
}
count="$(cat "$RUNNER_TEMP/curl-reads")"
count=$((count + 1))
printf '%s' "$count" > "$RUNNER_TEMP/curl-reads"
case "$MOCK_CURL_MODE" in
  present) exit 0 ;;
  absent) exit 22 ;;
  lost) if [[ "$count" == 1 ]]; then exit 22; else exit 0; fi ;;
  *) echo "Unexpected curl mode: $MOCK_CURL_MODE" >&2; exit 97 ;;
esac
"##,
        )
        .unwrap();
        fs::write(
            root.join("bin/git"),
            r##"#!/bin/bash
set -euo pipefail
printf '%s\0%s\0%s\0' "git" "$PWD" "$#" >> "$RUNNER_TEMP/process-calls"
printf '%s\0' "$@" >> "$RUNNER_TEMP/process-calls"
[[ "$#" -eq 2 && "$1" == rev-parse && "$2" == HEAD ]] || {
  echo "Unexpected Git invocation: $*" >&2
  exit 96
}
printf '%s\n' "$MOCK_GIT_HEAD"
"##,
        )
        .unwrap();
        fs::write(
            root.join("bin/sleep"),
            r##"#!/bin/bash
set -euo pipefail
printf '%s\0%s\0%s\0' "sleep" "$PWD" "$#" >> "$RUNNER_TEMP/process-calls"
printf '%s\0' "$@" >> "$RUNNER_TEMP/process-calls"
[[ "$#" -eq 1 ]] || { echo "Unexpected sleep invocation: $*" >&2; exit 96; }
case "$1" in 15|30|45) exit 0 ;; *) echo "Unexpected sleep duration: $1" >&2; exit 97 ;; esac
"##,
        )
        .unwrap();
        for executable in ["gh", "cargo", "curl", "git", "sleep"] {
            fs::set_permissions(
                root.join("bin").join(executable),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
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
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new("bash");
        command
            .args(["-c", script])
            .current_dir(&self.root)
            .env_clear()
            .envs([
                ("PATH", path),
                ("GITHUB_REPOSITORY", "owner/repo".into()),
                ("GITHUB_SERVER_URL", "https://github.com".into()),
                ("GITHUB_WORKSPACE", self.root.display().to_string()),
                ("GITHUB_RUN_ID", "42".into()),
                ("GITHUB_RUN_ATTEMPT", "3".into()),
                ("CI_RUN_ID", "42".into()),
                ("EXPECTED_SHA", SHA.into()),
                ("REQUIRE_RELEASE", "true".into()),
                ("MOCK_RUN", self.run.to_string()),
                ("MOCK_PULLS", self.pulls.to_string()),
                ("MOCK_MAIN", MAIN.into()),
                ("MOCK_COMPARISON", "ahead".into()),
                ("MOCK_MAIN_FAIL", "false".into()),
                ("MOCK_COMPARE_FAIL", "false".into()),
                ("MOCK_COMPARE_SOURCE", SHA.into()),
                ("MOCK_REF", "null".into()),
                ("MOCK_TAG_NAME", "v2.0.0".into()),
                ("MOCK_TAG_FAIL", "false".into()),
                ("MOCK_TAG_FAIL_AFTER", "0".into()),
                ("MOCK_TAG_CHANGE_AFTER", "0".into()),
                (
                    "MOCK_FINAL_REF",
                    json!({"object":{"type":"commit","sha":SHA}}).to_string(),
                ),
                ("MOCK_ANNOTATION_FAIL", "false".into()),
                ("MOCK_ANNOTATED_OBJECT", TAG_OBJECT.into()),
                ("MOCK_ANNOTATED_TARGET", SHA.into()),
                ("MOCK_FINAL_ANNOTATED_OBJECT", NEW_TAG_OBJECT.into()),
                ("MOCK_FINAL_ANNOTATED_TARGET", MAIN.into()),
                ("MOCK_RELEASES", "[]".into()),
                ("AUTOMATIC", "true".into()),
                ("MOCK_FREEZE", "true".into()),
                ("MOCK_UPLOAD_FAIL", "false".into()),
                ("MOCK_CORRUPT", "".into()),
                ("MOCK_LOST_CREATE", "false".into()),
                ("MOCK_LOST_PUBLISH", "false".into()),
                ("MOCK_LATE_RELEASES", "[]".into()),
                ("MOCK_PENDING_PULLS", self.pulls.to_string()),
                (
                    "MOCK_PROPOSAL_RUNS",
                    json!([{"workflow_runs":[{"path":".github/workflows/ci.yml","head_sha":MAIN,"head_branch":"main","event":"push","status":"completed","conclusion":"success"}]}]).to_string(),
                ),
                ("SOURCE_SHA", SHA.into()),
                ("RELEASE_TAG", "v2.0.0".into()),
                ("RELEASE_VERSION", "2.0.0".into()),
                ("CRATE_NAME", "codebase-graph".into()),
                ("CRATE_VERSION", "2.0.0".into()),
                ("PREFLIGHT_RESULT", "skipped".into()),
                ("PREFLIGHT_ARTIFACT", "".into()),
                ("PREFLIGHT_RUN_ID", "".into()),
                ("PREFLIGHT_ATTEMPT", "".into()),
                ("MOCK_PREFLIGHT_FAILURE", "none".into()),
                ("MOCK_CURL_MODE", "absent".into()),
                ("MOCK_CARGO_PUBLISH_STATUS", "0".into()),
                ("MOCK_GIT_HEAD", SHA.into()),
                ("RELEASE_PR", "124".into()),
                ("VERIFIED_PR", "124".into()),
                ("RUNNER_TEMP", self.root.display().to_string()),
                ("GITHUB_OUTPUT", self.root.join("output").display().to_string()),
                (
                    "GITHUB_STEP_SUMMARY",
                    self.root.join("summary").display().to_string(),
                ),
            ])
            .envs(overrides.iter().copied());
        command.output().unwrap()
    }

    fn seed_preflight_report(&self, report: &Value) {
        let directory = self.root.join("dist/crate-preflight");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("crate-preflight.json"), report.to_string()).unwrap();
    }

    fn execute_with_preflight(
        &self,
        script: &str,
        report: Option<Value>,
        overrides: &[(&str, &str)],
    ) -> Output {
        let report_path = self.root.join("dist/crate-preflight/crate-preflight.json");
        if let Some(report) = report {
            self.seed_preflight_report(&report);
        } else {
            let _ = fs::remove_file(report_path);
        }
        let mut env = vec![
            ("PREFLIGHT_RESULT", "success"),
            ("PREFLIGHT_ARTIFACT", "crate-preflight-42-3"),
            ("PREFLIGHT_RUN_ID", "42"),
            ("PREFLIGHT_ATTEMPT", "3"),
        ];
        env.extend_from_slice(overrides);
        self.execute(script, &env)
    }

    fn process_calls(&self) -> Vec<ProcessCall> {
        process_calls(&self.root.join("process-calls"))
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

    fn seed_partial_assets(&self) {
        let archive = crate::NATIVE_TARGETS[0].archive_name("2.0.0");
        fs::copy(
            self.root.join("dist/release-assets").join(&archive),
            self.root.join("assets").join(archive),
        )
        .unwrap();
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

    fn resolve(&self, overrides: &[(&str, &str)]) -> Output {
        let mut env = vec![
            ("GITHUB_EVENT_NAME", "workflow_dispatch"),
            ("MANUAL_TAG", "v2.0.0"),
            ("MANUAL_SOURCE", "promote"),
            ("MANUAL_DRY_RUN", "false"),
            ("CI_RUN_ID", ""),
            ("VERIFIED_SHA", ""),
            ("RELEASE_PR", ""),
        ];
        env.extend_from_slice(overrides);
        self.execute(&step("release-target", "Resolve tag and source SHA"), &env)
    }

    fn clear_calls(&self) {
        fs::write(self.root.join("calls"), "").unwrap();
    }

    fn reset_tag_reads(&self) {
        let _ = fs::remove_file(self.root.join("tag-reads"));
    }

    fn assets_snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let mut assets = fs::read_dir(self.root.join("assets"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assets.sort_by(|left, right| left.0.cmp(&right.0));
        assets
    }

    fn release_snapshot(&self) -> Vec<u8> {
        fs::read(self.root.join("release")).unwrap()
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

fn crate_preflight() -> String {
    step("crate-preflight", "Verify exact-source package and size")
}

fn crate_publisher() -> String {
    step("publish-crate", "Publish crates.io package")
}

fn finalize_release() -> String {
    step(
        "finalize-release",
        "Mark only the completed release PR tagged",
    )
}

#[test]
fn immutable_release_uploads_before_publication_and_retries_without_writes() {
    let f = Fixture::new();
    assert_success(&f.execute_with_preflight(&publisher(), Some(valid_preflight_report()), &[]));
    let release: Value =
        serde_json::from_slice(&fs::read(f.root.join("release")).unwrap()).unwrap();
    assert_eq!(release["draft"], false);
    assert_eq!(release["immutable"], true);
    assert_eq!(
        fs::read_dir(f.root.join("assets")).unwrap().count(),
        crate::NATIVE_TARGETS.len() * 2
    );
    fs::write(f.root.join("calls"), "").unwrap();
    assert_success(&f.execute_with_preflight(&publisher(), Some(valid_preflight_report()), &[]));
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
fn crate_preflight_executes_the_selected_source_helper_and_names_the_current_attempt() {
    let f = Fixture::new();
    let output = f.execute(&crate_preflight(), &[]);
    assert_success(&output);

    let calls = f.process_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].program, "cargo");
    assert_eq!(calls[0].cwd, f.root.display().to_string());
    assert_eq!(
        calls[0].args,
        [
            "run",
            "--manifest-path",
            f.root
                .join("workflow-helper/crates/xtask/Cargo.toml")
                .to_str()
                .unwrap(),
            "--locked",
            "--",
            "release-package-preflight",
            "--source-dir",
            f.root.join("selected-source").to_str().unwrap(),
            "--source-sha",
            SHA,
            "--tag",
            "v2.0.0",
            "--output",
            f.root
                .join("crate-preflight-42-3/crate-preflight.json")
                .to_str()
                .unwrap(),
        ]
    );
    let report = f.root.join("crate-preflight-42-3/crate-preflight.json");
    assert!(report.is_file());
    assert!(!report.starts_with(f.root.join("selected-source")));
    assert_eq!(
        f.output(),
        "artifact-name=crate-preflight-42-3\nrun-id=42\nrun-attempt=3\n"
    );
    assert!(f.calls().is_empty());
}

#[test]
fn crate_preflight_child_failures_never_emit_a_report_or_job_outputs() {
    for (failure, status) in [("dry-run", 41), ("size", 42)] {
        let f = Fixture::new();
        let output = f.execute(&crate_preflight(), &[("MOCK_PREFLIGHT_FAILURE", failure)]);
        assert_eq!(output.status.code(), Some(status), "{failure}");
        assert!(f.output().is_empty(), "{failure}: {}", f.output());
        assert!(
            !f.root
                .join("crate-preflight-42-3/crate-preflight.json")
                .exists(),
            "{failure} emitted a successful report"
        );
        let calls = f.process_calls();
        assert_eq!(calls.len(), 1, "{failure}: {calls:?}");
        assert_eq!(calls[0].program, "cargo");
        assert!(f.calls().is_empty());
    }
}

#[test]
fn crate_preflight_rejects_invalid_run_identity_before_launch() {
    for (name, variable, value) in [
        ("run id", "GITHUB_RUN_ID", "0"),
        ("run id", "GITHUB_RUN_ID", "not-a-run"),
        ("attempt", "GITHUB_RUN_ATTEMPT", "0"),
        ("attempt", "GITHUB_RUN_ATTEMPT", ""),
    ] {
        let f = Fixture::new();
        let output = f.execute(&crate_preflight(), &[(variable, value)]);
        assert!(!output.status.success(), "accepted invalid {name} {value}");
        assert!(f.output().is_empty());
        let calls = f.process_calls();
        assert!(
            calls.is_empty(),
            "invalid {name} {value} launched subprocesses: {calls:?}"
        );
        assert!(f.calls().is_empty());
    }
}

#[test]
fn publisher_rejects_unbound_preflight_before_any_github_request() {
    for failure in preflight_failures() {
        let f = Fixture::new();
        let output =
            f.execute_with_preflight(&publisher(), failure.report.clone(), &failure.overrides);
        assert!(
            !output.status.success(),
            "accepted invalid preflight: {}",
            failure.name
        );
        assert!(f.calls().is_empty(), "{}: {}", failure.name, f.calls());
        assert!(f.process_calls().is_empty(), "{}", failure.name);
        assert!(!f.root.join("release").exists(), "{}", failure.name);
        assert!(f.assets_snapshot().is_empty(), "{}", failure.name);
    }
}

#[test]
fn crate_upload_rejects_unbound_preflight_before_registry_or_cargo_calls() {
    for failure in preflight_failures() {
        let f = Fixture::new();
        let output = f.execute_with_preflight(
            &crate_publisher(),
            failure.report.clone(),
            &failure.overrides,
        );
        assert!(
            !output.status.success(),
            "accepted invalid preflight: {}",
            failure.name
        );
        assert!(
            f.process_calls().is_empty(),
            "{}: {:?}",
            failure.name,
            f.process_calls()
        );
        assert!(f.calls().is_empty(), "{}: {}", failure.name, f.calls());
    }
}

#[test]
fn publisher_accepts_a_successful_prior_attempt_from_the_current_run() {
    let f = Fixture::new();
    let output = f.execute_with_preflight(
        &publisher(),
        Some(valid_preflight_report()),
        &[
            ("PREFLIGHT_ATTEMPT", "2"),
            ("PREFLIGHT_ARTIFACT", "crate-preflight-42-2"),
        ],
    );
    assert_success(&output);
    assert!(f.calls().contains("release upload"));
}

#[test]
fn crate_publish_checks_exact_version_without_repackaging_an_existing_version() {
    let f = Fixture::new();
    let output = f.execute_with_preflight(
        &crate_publisher(),
        Some(valid_preflight_report()),
        &[("MOCK_CURL_MODE", "present")],
    );
    assert_success(&output);
    let calls = f.process_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0].program, "git");
    assert_eq!(calls[0].args, ["rev-parse", "HEAD"]);
    assert_eq!(calls[0].cwd, f.root.display().to_string());
    assert_eq!(calls[1].program, "curl");
    assert_eq!(
        calls[1].args,
        [
            "--fail",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--user-agent",
            "codebaseGraph-release-workflow (https://github.com/owner/repo)",
            "https://crates.io/api/v1/crates/codebase-graph/2.0.0",
        ]
    );
    assert!(!calls.iter().any(|call| call.program == "cargo"));
    assert!(f.calls().is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("already published"));
}

#[test]
fn crate_publish_uses_only_normal_locked_publish_when_the_version_is_absent() {
    let f = Fixture::new();
    let output = f.execute_with_preflight(
        &crate_publisher(),
        Some(valid_preflight_report()),
        &[("MOCK_CURL_MODE", "absent")],
    );
    assert_success(&output);
    let calls = f.process_calls();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(calls[0].program, "git");
    assert_eq!(calls[1].program, "curl");
    assert_eq!(calls[2].program, "cargo");
    assert_eq!(calls[2].args, ["publish", "--locked"]);
    assert!(calls.iter().all(|call| !call
        .args
        .iter()
        .any(|arg| arg.contains("dry-run") || arg == "package")));
    assert!(f.calls().is_empty());
}

#[test]
fn crate_publish_reconciles_a_lost_response_without_replaying_upload() {
    let f = Fixture::new();
    let output = f.execute_with_preflight(
        &crate_publisher(),
        Some(valid_preflight_report()),
        &[
            ("MOCK_CURL_MODE", "lost"),
            ("MOCK_CARGO_PUBLISH_STATUS", "23"),
        ],
    );
    assert_success(&output);
    let calls = f.process_calls();
    assert_eq!(
        calls.iter().filter(|call| call.program == "curl").count(),
        2
    );
    let cargo = calls
        .iter()
        .filter(|call| call.program == "cargo")
        .collect::<Vec<_>>();
    assert_eq!(cargo.len(), 1, "{calls:?}");
    assert_eq!(cargo[0].args, ["publish", "--locked"]);
    assert!(!calls.iter().any(|call| call.program == "sleep"));
    assert!(f.calls().is_empty());
}

#[test]
fn crate_publish_caps_retries_and_logs_backoff_without_waiting() {
    let f = Fixture::new();
    let output = f.execute_with_preflight(
        &crate_publisher(),
        Some(valid_preflight_report()),
        &[
            ("MOCK_CURL_MODE", "absent"),
            ("MOCK_CARGO_PUBLISH_STATUS", "23"),
        ],
    );
    assert_eq!(output.status.code(), Some(23));
    let calls = f.process_calls();
    assert_eq!(
        calls.iter().filter(|call| call.program == "cargo").count(),
        4
    );
    assert_eq!(
        calls.iter().filter(|call| call.program == "curl").count(),
        5
    );
    let sleeps = calls
        .iter()
        .filter(|call| call.program == "sleep")
        .map(|call| call.args[0].as_str())
        .collect::<Vec<_>>();
    assert_eq!(sleeps, ["15", "30", "45"]);
    assert!(f.calls().is_empty());
}

#[test]
fn crate_publish_rejects_changed_head_or_version_before_registry_or_upload() {
    for (name, overrides) in [
        ("changed HEAD", vec![("MOCK_GIT_HEAD", MAIN)]),
        ("changed crate version", vec![("CRATE_VERSION", "2.0.1")]),
    ] {
        let f = Fixture::new();
        let output = f.execute_with_preflight(
            &crate_publisher(),
            Some(valid_preflight_report()),
            &overrides,
        );
        assert!(!output.status.success(), "accepted {name}");
        let calls = f.process_calls();
        assert_eq!(calls.len(), 1, "{name}: {calls:?}");
        assert_eq!(calls[0].program, "git");
        assert!(f.calls().is_empty());
    }
}

#[test]
fn native_publication_precedes_crate_upload_and_finalization() {
    let workflow: yaml_serde::Value =
        yaml_serde::from_str(include_str!("../../../../.github/workflows/release.yml")).unwrap();
    let publish_assets_job = &workflow["jobs"]["publish-release-assets"];
    let assets_needs = publish_assets_job["needs"].as_sequence().unwrap();
    assert!(assets_needs
        .iter()
        .any(|job| job.as_str() == Some("crate-preflight")));

    let crate_job = &workflow["jobs"]["publish-crate"];
    let crate_needs = crate_job["needs"].as_sequence().unwrap();
    for dependency in [
        "release-target",
        "publish-release-assets",
        "crate-preflight",
    ] {
        assert!(crate_needs
            .iter()
            .any(|job| job.as_str() == Some(dependency)));
    }
    let crate_condition = crate_job["if"].as_str().unwrap();
    for clause in [
        "needs.release-target.result == 'success'",
        "needs.publish-release-assets.result == 'success'",
        "needs.crate-preflight.result == 'success'",
        "needs.release-target.outputs.automatic == 'true'",
        "needs.release-target.outputs.publish_assets == 'true'",
    ] {
        assert!(
            crate_condition.contains(clause),
            "missing crate gate: {clause}"
        );
    }

    let finalize_job = &workflow["jobs"]["finalize-release"];
    let finalize_needs = finalize_job["needs"].as_sequence().unwrap();
    for dependency in ["release-target", "publish-release-assets", "publish-crate"] {
        assert!(finalize_needs
            .iter()
            .any(|job| job.as_str() == Some(dependency)));
    }
    let finalize_condition = finalize_job["if"].as_str().unwrap();
    for clause in [
        "needs.release-target.result == 'success'",
        "needs.publish-release-assets.result == 'success'",
        "needs.publish-crate.result == 'success'",
        "needs.release-target.outputs.automatic == 'true'",
    ] {
        assert!(
            finalize_condition.contains(clause),
            "missing finalization gate: {clause}"
        );
    }

    let f = Fixture::new();
    let report = Some(valid_preflight_report());
    assert_success(&f.execute_with_preflight(&publisher(), report.clone(), &[]));
    let crate_output =
        f.execute_with_preflight(&crate_publisher(), report, &[("MOCK_CURL_MODE", "absent")]);
    assert_success(&crate_output);
    assert_success(&f.execute(&finalize_release(), &[]));
    assert!(f.root.join("finalized").is_file());

    let calls = f.process_calls();
    let native_upload = calls
        .iter()
        .position(|call| {
            call.program == "gh" && call.args.starts_with(&["release".into(), "upload".into()])
        })
        .unwrap();
    let native_publish = calls
        .iter()
        .position(|call| {
            call.program == "gh"
                && call
                    .args
                    .starts_with(&["api".into(), "--method".into(), "PATCH".into()])
        })
        .unwrap();
    let crate_upload = calls
        .iter()
        .position(|call| call.program == "cargo" && call.args == ["publish", "--locked"])
        .unwrap();
    let finalize = calls
        .iter()
        .rposition(|call| {
            call.program == "gh" && call.args.starts_with(&["pr".into(), "edit".into()])
        })
        .unwrap();
    assert!(
        native_upload < native_publish && native_publish < crate_upload && crate_upload < finalize
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
    assert_success(&f.execute_with_preflight(&script, Some(valid_preflight_report()), &[]));
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
    assert_success(&f.execute_with_preflight(&script, Some(valid_preflight_report()), &[]));
    assert!(!fs::read_to_string(f.root.join("calls"))
        .unwrap()
        .contains("--method POST"));
}

#[test]
fn publisher_rejects_conflicting_tags_and_does_not_regress_latest() {
    let f = Fixture::new();
    let script = step("publish-release-assets", "Create only the verified release");
    let wrong = json!({"object":{"sha":MAIN,"type":"commit"}}).to_string();
    assert!(!f
        .execute_with_preflight(
            &script,
            Some(valid_preflight_report()),
            &[("MOCK_REF", &wrong)]
        )
        .status
        .success());
    assert!(!f.root.join("request").exists());
    assert_success(&f.execute_with_preflight(
        &script,
        Some(valid_preflight_report()),
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
    assert_success(&f.execute_with_preflight(
        &publisher(),
        Some(valid_preflight_report()),
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
        assert!(!f
            .execute_with_preflight(&publisher(), Some(valid_preflight_report()), &overrides)
            .status
            .success());
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
            assert!(!f
                .execute_with_preflight(&publisher(), Some(valid_preflight_report()), &[])
                .status
                .success());
            f.assert_no_writes();
        }
    }
}

#[test]
fn accepted_writes_with_lost_responses_resume_without_replay() {
    for lost in ["MOCK_LOST_CREATE", "MOCK_LOST_PUBLISH"] {
        let f = Fixture::new();
        assert!(!f
            .execute_with_preflight(
                &publisher(),
                Some(valid_preflight_report()),
                &[(lost, "true")],
            )
            .status
            .success());
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
        assert_success(&f.execute_with_preflight(
            &publisher(),
            Some(valid_preflight_report()),
            &[],
        ));
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
    assert_success(&f.execute_with_preflight(&publisher(), Some(valid_preflight_report()), &[]));
    assert_eq!(f.release()["draft"], false);
    assert!(f.calls().contains("release upload"));
    assert!(!f.calls().contains("--method POST") && !f.calls().contains("--method PATCH"));
    fs::write(f.root.join("calls"), "").unwrap();
    assert_success(&f.execute_with_preflight(&publisher(), Some(valid_preflight_report()), &[]));
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
            !f.execute_with_preflight(&publisher(), Some(valid_preflight_report()), &overrides)
                .status
                .success(),
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

fn resolver_ref(sha: &str) -> String {
    json!({"object":{"type":"commit","sha":sha}}).to_string()
}

fn assert_resolver_rejected(f: &Fixture, output: &Output) {
    assert!(
        !output.status.success(),
        "unexpected authorization: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(f.output().is_empty(), "{}", f.output());
    assert!(!f.root.join("summary").exists());
    f.assert_no_writes();
}

fn assert_publisher_rejected_without_mutation(
    f: &Fixture,
    output: &Output,
    release_before: &[u8],
    assets_before: &[(String, Vec<u8>)],
) {
    assert!(
        !output.status.success(),
        "unexpected upload: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(f.release_snapshot(), release_before);
    assert_eq!(f.assets_snapshot(), assets_before);
    f.assert_no_writes();
}

fn assert_fresh_legacy_guard_precedes_upload(calls: &str) {
    let requests = calls.lines().collect::<Vec<_>>();
    let tag_reads = requests
        .iter()
        .enumerate()
        .filter(|(_, request)| **request == "api repos/owner/repo/git/ref/tags/v2.0.0")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(tag_reads.len(), 2, "{calls}");
    let main = requests
        .iter()
        .position(|request| *request == "api repos/owner/repo/git/ref/heads/main --jq .object.sha")
        .unwrap();
    let compare = requests
        .iter()
        .position(|request| {
            *request == format!("api repos/owner/repo/compare/{SHA}...{NEXT_MAIN} --jq .status")
        })
        .unwrap();
    let upload = requests
        .iter()
        .position(|request| request.starts_with("release upload v2.0.0 "))
        .unwrap();
    assert!(
        tag_reads[0] < tag_reads[1] && tag_reads[1] < main && main < compare && compare < upload,
        "{calls}"
    );
}

fn prepare_legacy_draft(f: &Fixture, tag_ref: &str, complete_assets: bool) {
    let output = f.resolve(&[("MOCK_REF", tag_ref)]);
    assert_success(&output);
    assert!(f.output().contains("should-publish=true\n"));
    assert!(f.output().contains("publish_assets=true\n"));
    f.seed_release(true, false);
    fs::write(f.root.join("ref"), tag_ref).unwrap();
    if complete_assets {
        f.seed_assets();
    } else {
        f.seed_partial_assets();
    }
    f.clear_calls();
    f.reset_tag_reads();
}

#[test]
fn existing_tag_resolver_accepts_historical_lightweight_and_annotated_ancestors() {
    let annotated = json!({"object":{"type":"tag","sha":TAG_OBJECT}}).to_string();
    for (ref_value, main, comparison, annotation) in [
        (resolver_ref(SHA), SHA, "identical", false),
        (resolver_ref(SHA), NEXT_MAIN, "ahead", false),
        (annotated, NEXT_MAIN, "ahead", true),
    ] {
        let f = Fixture::new();
        let mut overrides = vec![
            ("MOCK_REF", ref_value.as_str()),
            ("MOCK_MAIN", main),
            ("MOCK_COMPARISON", comparison),
        ];
        if annotation {
            overrides.extend([
                ("MOCK_ANNOTATED_OBJECT", TAG_OBJECT),
                ("MOCK_ANNOTATED_TARGET", SHA),
            ]);
        }
        let output = f.resolve(&overrides);
        assert_success(&output);
        let outputs = f.output();
        assert!(outputs.contains("should-publish=true\n"), "{outputs}");
        assert!(outputs.contains("automatic=false\n"), "{outputs}");
        assert!(outputs.contains("publish_assets=true\n"), "{outputs}");
        assert!(
            outputs.contains(&format!("source-sha={SHA}\n")),
            "{outputs}"
        );
        assert!(outputs.contains("ci-run-id=\n"), "{outputs}");
        let calls = f.calls();
        assert!(
            calls.contains(&format!(
                "api repos/owner/repo/compare/{SHA}...{main} --jq .status"
            )),
            "{calls}"
        );
        assert_eq!(
            calls
                .lines()
                .filter(|call| *call == "api repos/owner/repo/git/ref/tags/v2.0.0")
                .count(),
            1,
            "{calls}"
        );
        f.assert_no_writes();
    }
}

#[test]
fn existing_tag_resolver_rejects_nonancestors_and_bad_identity_before_authorizing() {
    for comparison in ["behind", "diverged", "", "unknown-status"] {
        let f = Fixture::new();
        let ref_value = resolver_ref(SHA);
        let output = f.resolve(&[
            ("MOCK_REF", ref_value.as_str()),
            ("MOCK_COMPARISON", comparison),
        ]);
        assert_resolver_rejected(&f, &output);
        assert!(f.calls().contains(&format!(
            "api repos/owner/repo/compare/{SHA}...{MAIN} --jq .status"
        )));
    }

    let malformed_source = resolver_ref("not-a-full-source-sha");
    for (ref_value, extra) in [
        ("null".to_owned(), vec![]),
        (malformed_source, vec![]),
        ("not-json".to_owned(), vec![]),
        (resolver_ref(SHA), vec![("MOCK_TAG_FAIL", "true")]),
    ] {
        let f = Fixture::new();
        let output = f.resolve(
            &[("MOCK_REF", ref_value.as_str())]
                .into_iter()
                .chain(extra)
                .collect::<Vec<_>>(),
        );
        assert_resolver_rejected(&f, &output);
        assert!(!f.calls().contains("git/ref/heads/main"));
    }

    let annotated = json!({"object":{"type":"tag","sha":TAG_OBJECT}}).to_string();
    for extra in [
        vec![("MOCK_ANNOTATION_FAIL", "true")],
        vec![("MOCK_ANNOTATED_TARGET", "malformed-target")],
    ] {
        let f = Fixture::new();
        let output = f.resolve(
            &[
                ("MOCK_REF", annotated.as_str()),
                ("MOCK_ANNOTATED_OBJECT", TAG_OBJECT),
            ]
            .into_iter()
            .chain(extra)
            .collect::<Vec<_>>(),
        );
        assert_resolver_rejected(&f, &output);
        assert!(!f.calls().contains("git/ref/heads/main"));
    }

    for extra in [
        vec![("MOCK_MAIN", "malformed-main-sha")],
        vec![("MOCK_MAIN_FAIL", "true")],
        vec![("MOCK_COMPARE_FAIL", "true")],
        vec![("GITHUB_REPOSITORY", "fork/repo")],
        vec![("MOCK_COMPARE_SOURCE", MAIN)],
    ] {
        let f = Fixture::new();
        let ref_value = resolver_ref(SHA);
        let output = f.resolve(
            &[("MOCK_REF", ref_value.as_str())]
                .into_iter()
                .chain(extra)
                .collect::<Vec<_>>(),
        );
        assert_resolver_rejected(&f, &output);
        if !f.calls().contains("compare/") {
            assert!(!f.calls().contains("--method POST"));
        }
    }
}

fn publisher_job_scheduled_for_assets_output(
    condition: &str,
    automatic: Option<&str>,
    preflight_result: Option<&str>,
    publish_assets: Option<&str>,
) -> Result<bool, &'static str> {
    let condition = condition
        .trim()
        .strip_prefix("${{")
        .and_then(|condition| condition.strip_suffix("}}"))
        .ok_or("missing GitHub expression delimiters")?;
    let required_clauses = [
        "always()",
        "needs.release-target.result == 'success'",
        "needs.validate-artifacts.result == 'success'",
        "needs.release-target.outputs.automatic == 'true'",
        "needs.crate-preflight.result == 'success'",
        "needs.release-target.outputs.automatic == 'false'",
        "needs.crate-preflight.result == 'skipped'",
        "needs.release-target.outputs.publish_assets == 'true'",
    ];
    if required_clauses
        .iter()
        .any(|clause| condition.matches(*clause).count() != 1)
    {
        return Err("publisher gate is missing or duplicates a required condition");
    }
    let normalized = required_clauses
        .iter()
        .fold(condition.to_owned(), |expression, clause| {
            expression.replacen(*clause, "x", 1)
        });
    if normalized.split_whitespace().collect::<String>() != "x&&x&&x&&((x&&x)||(x&&x))&&x" {
        return Err("unsupported publisher gate expression");
    }

    let preflight_succeeded = match (automatic, preflight_result) {
        (Some("true"), Some("success")) | (Some("false"), Some("skipped")) => true,
        (Some("true"), Some(_)) | (Some("false"), Some(_)) => false,
        _ => return Err("missing or invalid publisher producer state"),
    };
    let publish_assets = match publish_assets {
        Some("true") => true,
        Some("false") => false,
        _ => return Err("missing or invalid publish_assets output"),
    };
    Ok(preflight_succeeded && publish_assets)
}

#[test]
fn legacy_dry_run_resolves_without_scheduling_the_publisher() {
    let f = Fixture::new();
    let ref_value = resolver_ref(SHA);
    let output = f.resolve(&[("MOCK_REF", ref_value.as_str()), ("MANUAL_DRY_RUN", "true")]);
    assert_success(&output);
    let outputs = f.output();
    assert!(outputs.contains("should-publish=true\n"), "{outputs}");
    assert!(outputs.contains("dry-run=true\n"), "{outputs}");
    assert!(outputs.contains("publish_assets=false\n"), "{outputs}");

    let workflow: yaml_serde::Value =
        yaml_serde::from_str(include_str!("../../../../.github/workflows/release.yml")).unwrap();
    let publisher_job = &workflow["jobs"]["publish-release-assets"];
    assert!(publisher_job["needs"]
        .as_sequence()
        .unwrap()
        .iter()
        .any(|job| { job.as_str() == Some("release-target") }));
    let condition = publisher_job["if"].as_str().unwrap();
    let automatic = outputs
        .lines()
        .find_map(|line| line.strip_prefix("automatic="));
    let publish_assets = outputs
        .lines()
        .find_map(|line| line.strip_prefix("publish_assets="));
    assert_eq!(automatic, Some("false"));
    assert_eq!(publish_assets, Some("false"));
    assert_eq!(
        publisher_job_scheduled_for_assets_output(
            condition,
            automatic,
            Some("skipped"),
            publish_assets,
        ),
        Ok(false)
    );
    assert_eq!(
        publisher_job_scheduled_for_assets_output(
            condition,
            Some("false"),
            Some("skipped"),
            Some("true"),
        ),
        Ok(true)
    );
    assert_eq!(
        publisher_job_scheduled_for_assets_output(
            condition,
            Some("true"),
            Some("success"),
            Some("true"),
        ),
        Ok(true)
    );
    for (automatic, preflight_result) in [
        ("true", "failure"),
        ("true", "skipped"),
        ("false", "success"),
    ] {
        assert_eq!(
            publisher_job_scheduled_for_assets_output(
                condition,
                Some(automatic),
                Some(preflight_result),
                Some("true"),
            ),
            Ok(false),
            "scheduled publisher with automatic={automatic}, preflight={preflight_result}",
        );
    }

    let required_clauses = [
        "always()",
        "needs.release-target.result == 'success'",
        "needs.validate-artifacts.result == 'success'",
        "needs.release-target.outputs.automatic == 'true'",
        "needs.crate-preflight.result == 'success'",
        "needs.release-target.outputs.automatic == 'false'",
        "needs.crate-preflight.result == 'skipped'",
        "needs.release-target.outputs.publish_assets == 'true'",
    ];
    for clause in required_clauses {
        let missing = condition.replacen(clause, "", 1);
        assert_ne!(
            missing, condition,
            "missing-condition mutation did not apply"
        );
        assert!(
            publisher_job_scheduled_for_assets_output(
                &missing,
                Some("false"),
                Some("skipped"),
                publish_assets,
            )
            .is_err(),
            "missing required condition was accepted: {clause}"
        );
        let duplicated = condition.replace(clause, &format!("{clause} && {clause}"));
        assert!(
            publisher_job_scheduled_for_assets_output(
                &duplicated,
                Some("false"),
                Some("skipped"),
                publish_assets,
            )
            .is_err(),
            "duplicate required condition was accepted: {clause}"
        );
    }
    let bypassed = condition.replace(
        required_clauses[7],
        "true || needs.release-target.outputs.publish_assets == 'true'",
    );
    assert!(publisher_job_scheduled_for_assets_output(
        &bypassed,
        Some("false"),
        Some("skipped"),
        publish_assets,
    )
    .is_err());
    let unsupported_after_false = condition.replace(
        required_clauses[7],
        "needs.release-target.outputs.publish_assets == 'true' && true",
    );
    assert!(publisher_job_scheduled_for_assets_output(
        &unsupported_after_false,
        Some("false"),
        Some("skipped"),
        Some("false"),
    )
    .is_err());
    assert!(publisher_job_scheduled_for_assets_output(
        condition,
        None,
        Some("skipped"),
        Some("false"),
    )
    .is_err());
    assert!(publisher_job_scheduled_for_assets_output(
        condition,
        Some("false"),
        Some("skipped"),
        None,
    )
    .is_err());
    f.assert_no_writes();
}

#[test]
fn legacy_publisher_rechecks_fresh_tag_and_main_before_upload_and_allows_advancement() {
    let light_ref = resolver_ref(SHA);
    for (draft, complete_assets) in [(true, true), (false, false)] {
        let f = Fixture::new();
        prepare_legacy_draft(&f, &light_ref, complete_assets);
        if !draft {
            let mut release = f.release();
            release["draft"] = json!(false);
            fs::write(f.root.join("release"), release.to_string()).unwrap();
        }
        let release_before = f.release_snapshot();
        let output = f.execute(
            &publisher(),
            &[
                ("AUTOMATIC", "false"),
                ("MOCK_MAIN", NEXT_MAIN),
                ("MOCK_COMPARISON", "ahead"),
            ],
        );
        assert_success(&output);
        assert_fresh_legacy_guard_precedes_upload(&f.calls());
        assert_eq!(f.release_snapshot(), release_before);
        assert_eq!(f.release()["draft"], draft);
        assert!(!f.calls().contains("--method POST"));
        assert!(!f.calls().contains("--method PATCH"));
        assert_eq!(f.calls().matches("release upload v2.0.0 ").count(), 1);
    }

    let annotated_ref = json!({"object":{"type":"tag","sha":TAG_OBJECT}}).to_string();
    let f = Fixture::new();
    prepare_legacy_draft(&f, &annotated_ref, true);
    fs::write(f.root.join("ref"), &annotated_ref).unwrap();
    let output = f.execute(
        &publisher(),
        &[
            ("AUTOMATIC", "false"),
            ("MOCK_FINAL_REF", annotated_ref.as_str()),
            ("MOCK_ANNOTATED_OBJECT", TAG_OBJECT),
            ("MOCK_ANNOTATED_TARGET", SHA),
            ("MOCK_FINAL_ANNOTATED_OBJECT", TAG_OBJECT),
            ("MOCK_FINAL_ANNOTATED_TARGET", SHA),
            ("MOCK_MAIN", NEXT_MAIN),
            ("MOCK_COMPARISON", "ahead"),
        ],
    );
    assert_success(&output);
    assert_fresh_legacy_guard_precedes_upload(&f.calls());
    let calls = f.calls();
    assert_eq!(
        calls
            .matches(&format!("git/tags/{TAG_OBJECT} --jq .object.sha"))
            .count(),
        2,
        "{calls}"
    );
}

#[test]
fn legacy_publisher_rejects_late_history_and_tag_races_without_mutation() {
    let light_ref = resolver_ref(SHA);
    for (draft, complete_assets, extra) in [
        (true, true, vec![("MOCK_COMPARISON", "diverged")]),
        (false, false, vec![("MOCK_COMPARE_FAIL", "true")]),
        (true, true, vec![("MOCK_COMPARISON", "malformed-status")]),
        (true, true, vec![("MOCK_MAIN", "malformed-main-sha")]),
        (true, true, vec![("MOCK_MAIN_FAIL", "true")]),
    ] {
        let f = Fixture::new();
        prepare_legacy_draft(&f, &light_ref, complete_assets);
        if !draft {
            let mut release = f.release();
            release["draft"] = json!(false);
            fs::write(f.root.join("release"), release.to_string()).unwrap();
        }
        let release_before = f.release_snapshot();
        let assets_before = f.assets_snapshot();
        let mut overrides = vec![("AUTOMATIC", "false"), ("MOCK_MAIN", NEXT_MAIN)];
        overrides.extend(extra);
        let output = f.execute(&publisher(), &overrides);
        assert_publisher_rejected_without_mutation(&f, &output, &release_before, &assets_before);
        assert!(!f.calls().contains("release upload"));
        assert!(f.calls().contains("git/ref/heads/main"));
    }

    for (final_ref, tag_fail_after) in [
        (resolver_ref(MAIN), "0"),
        ("null".to_owned(), "0"),
        (light_ref.clone(), "2"),
    ] {
        let f = Fixture::new();
        prepare_legacy_draft(&f, &light_ref, true);
        let release_before = f.release_snapshot();
        let assets_before = f.assets_snapshot();
        let output = f.execute(
            &publisher(),
            &[
                ("AUTOMATIC", "false"),
                ("MOCK_TAG_CHANGE_AFTER", "2"),
                ("MOCK_TAG_FAIL_AFTER", tag_fail_after),
                ("MOCK_FINAL_REF", final_ref.as_str()),
            ],
        );
        assert_publisher_rejected_without_mutation(&f, &output, &release_before, &assets_before);
        assert_eq!(f.calls().matches("git/ref/tags/v2.0.0").count(), 2);
        assert!(!f.calls().contains("git/ref/heads/main"));
    }

    let annotated_ref = json!({"object":{"type":"tag","sha":TAG_OBJECT}}).to_string();
    let changed_annotation = json!({"object":{"type":"tag","sha":NEW_TAG_OBJECT}}).to_string();
    let f = Fixture::new();
    prepare_legacy_draft(&f, &annotated_ref, true);
    fs::write(f.root.join("ref"), &annotated_ref).unwrap();
    let release_before = f.release_snapshot();
    let assets_before = f.assets_snapshot();
    let output = f.execute(
        &publisher(),
        &[
            ("AUTOMATIC", "false"),
            ("MOCK_TAG_CHANGE_AFTER", "2"),
            ("MOCK_FINAL_REF", changed_annotation.as_str()),
            ("MOCK_ANNOTATED_OBJECT", TAG_OBJECT),
            ("MOCK_ANNOTATED_TARGET", SHA),
            ("MOCK_FINAL_ANNOTATED_OBJECT", NEW_TAG_OBJECT),
            ("MOCK_FINAL_ANNOTATED_TARGET", MAIN),
        ],
    );
    assert_publisher_rejected_without_mutation(&f, &output, &release_before, &assets_before);
    let calls = f.calls();
    assert!(calls.contains(&format!("git/tags/{NEW_TAG_OBJECT} --jq .object.sha")));
    assert!(!calls.contains("git/ref/heads/main"));
    assert!(!calls.contains("release upload"));
}
