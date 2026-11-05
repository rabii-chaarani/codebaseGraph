# Independent verification — immutable release publication correction

## Handoff status

**PASS for the assigned local implementation checks at the frozen source snapshot.** Independent full xtask execution ran **52 tests: 52 passed, 0 failed, 0 skipped**. Workflow policy, local release gate, formatting, strict xtask Clippy, and diff whitespace checks all passed. No compiler/runner warnings were emitted. Genuine JUnit is retained separately.

This report does **not** claim hosted GitHub Actions execution, live/public publication success, Windows native qualification, full-workspace qualification, PR completion, or aggregate closure. Exact-source Scryer binding/reconciliation and unavailable Linear synchronization are separate pending integration limits, not failed local tests.

## Scope, authority, and settings

| Field | Evidence |
|---|---|
| Actual repository/worktree | `/private/tmp/codebasegraph-immutable-release-assets-20261029` |
| Branch / permitted endpoint | `fix/immutable-release-assets`; tested, reviewed PR only. No merge/publication authority. |
| Baseline | `5889ed09954a4a07efd2590dd1bd380ee77be06a` |
| Tested Git HEAD / tree | `ad3beec36502448c238f236c9af16702ddce107c` / `05d9b6c06f347085642610d5b591bc06a176c2c6`, plus the three uncommitted files identified below |
| Existing canonical contract | `docs/release.md` at full baseline SHA above |
| Approval/scope record | `docs/testing/release-40851263747/approval-and-acceptance.md` at `ad3beec36502448c238f236c9af16702ddce107c` |
| Decision reference | User “try the required workflow correction” and “yes i approve the inspection”, supplied by Engineer; proposal SHA-256 `c552e03401ad3160b0472499df0a97f356e658f6affb12269a27c0526860eeb6` |
| Existing model scope | `resp-259`, `resp-276`; assigned `chg-6qg638`, parent restoring ledger `chg-4kq3vn`. No new claims or fold required/attempted. |
| Write limits | Verifier wrote only these outside-repository evidence/config/log files and native build outputs. No source/Git/model/tracking writes, dependency installs, integration/settings changes, live release action, or credential use. Frozen v4.0.3 was not touched. |
| Execution settings | `evidence_verifier` role map: model=`inherit`, reasoning effort=`inherit`; parent dispatched without explicit overrides. Runtime model/effort metadata is unexposed; no known mismatch. No delegation. |

Discovery exception applies only to `.github/workflows/release.yml` and `crates/xtask/src/release/workflow_tests.rs`. `docs/release.md` and `crates/xtask/src/main.rs` were graph-discovered by Engineer. No other source file was directly inspected outside that discovery scope. Compilers and repository tests naturally load their own declared inputs through Cargo.

## Frozen source identity and stability

All checks ran against these exact bytes. The three changed product files matched the Engineer-supplied hashes before execution and after the last command. `main.rs` is unchanged from baseline.

| File | Bytes | SHA-256 |
|---|---:|---|
| `.github/workflows/release.yml` | 31741 | `aca94a20d46b2ecca85a9c112bb21daa7df1fb418e84f44d28bd35037c706bf3` |
| `crates/xtask/src/release/workflow_tests.rs` | 24537 | `e567ddb0bb1679c81566497607dc14e54207de8e341c5ab18116f93f23f53de7` |
| `docs/release.md` | 15526 | `abe55daa1bd234badd8707e0b1f47a77dd90e37b632a3bdf3cd42e28d97ed948` |
| `crates/xtask/src/main.rs` | 105700 | `70507a624361a97a7bcdb28412b31112258fcb023fa8e80dccd943b113b335ad` |

- Initial and final Git status/diff outputs are byte-identical: exactly the three named unstaged product files; staged diff empty; untracked inventory empty; HEAD/tree unchanged.
- Last-command stability timestamp: `2026-10-30T00:06:49.135893+00:00`.
- Preserved dirty diff SHA-256: `60cbb764495f8b579012bdcbbfe8f3f76a35300c583a107ef88405e033324632`. Snapshot logs: `final-initial-snapshot.log`, `final-post-check-snapshot.log`; inventory: `final-source-inventory.json`; stability: `final-stability.json`.
- The base-to-HEAD additional commit is the documentation-only approval record. This record did not change the baseline implementation inputs. Baseline results are separately retained in `baseline-report.md`.
- No dependency, manifest, target-inventory, release version, runner timeout, or numeric policy constant was changed by verification. The current two native targets still imply four public archive/sidecar assets.

## Native environment and isolation

- macOS **26.6.2** (25G83), Darwin **25.6.0**, ARM64.
- Cargo **1.96.0** (`30a34c682`, 2026-05-25); rustc **1.96.0** (`ac68faa20`, 2026-05-25).
- cargo-nextest **0.9.140** (`a9fef2964`, 2026-07-05), host `aarch64-apple-darwin`.
- `/bin/bash`: GNU Bash **3.2.57(1)-release**; `/usr/bin/jq`: **jq-1.7.1-apple**.
- Dedicated verifier build target: `/private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target`; not Production's build target or the primary checkout. Cargo compilation logs identify the actual worktree package.
- Cargo execution used native project commands with `--locked --offline`. All dependencies were already available. No dependencies/settings were installed or changed.
- No token/password/secret/credential/API-key-like environment variables were passed to Cargo. The repository fixture also removes GH_TOKEN/GITHUB_TOKEN and places its local mock `gh` first in PATH. Its final unmatched-request case fails closed (exit 97), rather than calling real GitHub.
- The actual workflow Bash is extracted by the test binary from the current YAML. Its mock models publication freezing assets, inaccessible draft lookup by tag, separate release-list pages, upload failure, byte corruption, and writes accepted despite lost responses.
- `final-nextest-report.toml` is an external report-only configuration, not repository settings. It sets the genuine JUnit path/name only. No test filter was supplied to final nextest execution.

## Actual executed checks

Every command below was run independently by this verifier from the actual worktree after the final implementation/test edit. Production's green logs were inspected for context but are not substituted for these executions.

| Check | Actual command | Result |
|---|---|---|
| `suite` | `cargo nextest run -p xtask --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target --config-file /private/tmp/codebasegraph-immutable-release-evidence-20261029/final-nextest-report.toml --user-config-file none --no-fail-fast --no-tests fail --test-threads 1` | **PASS**, exit 0, 20.369s |
| `check-workflows` | `cargo run -p xtask --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target -- check-workflows` | **PASS**, exit 0, 0.766s |
| `release-gate` | `cargo run -p xtask --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target -- release-gate` | **PASS**, exit 0, 0.516s |
| `fmt` | `cargo fmt --all -- --check` | **PASS**, exit 0, 1.880s |
| `clippy` | `cargo clippy -p xtask --all-targets --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target -- -D warnings` | **PASS**, exit 0, 2.778s |
| `diff-check` | `git diff --check` | **PASS**, exit 0, 0.030s |

Logs: `final-suite.log`, `final-workflow-policy.log`, `final-release-gate.log`, `final-fmt.log`, `final-clippy.log`, `final-diff-check.log`. Empty formatting/diff logs are successful exits, not missing execution. Structured commands/results: `final-commands.json`, `final-results.json`.

### Genuine runner report

- Report: `final-junit.xml`, emitted directly by cargo-nextest.
- Run ID: **55d7263f-ab59-47b9-ad8d-8cbefe4fc455**.
- Runner/JUnit inventory: **52 tests**, failures=0, errors=0; 0 skipped. Includes **18 actual-workflow Bash tests**, **13 workflow-policy tests**, metadata/provenance/archive/identity tests, native runner partition tests, and other xtask checks.
- JUnit SHA-256: `1c614c1eab8ab239d8b546601e77af248ff59f178c859c32e3e89b9e42f15606`.
- Full discovered-and-executed names: `final-test-inventory.txt`.
- `release-gate` was run in local mode. It does not claim the owner-controlled production environment confirmations or conda-forge scope were verified.

## Named acceptance trace

Acceptance numbers below are the existing scope record's items, not newly invented requirement or claim IDs. All cited workflow-test names are prefixed `release::workflow_tests::` in JUnit.

| Acceptance / behavior | Existing claim | Source and actual test anchors | Method and result |
|---|---|---|---|
| **1 — New release remains draft until four validated assets are uploaded and read back before publication** | Restores `resp-259` | `release.yml:479–522`; test file `:229` `immutable_release_uploads_before_publication_and_retries_without_writes`; `:327` `publisher_targets_original_sha_and_is_idempotent` | **PASS locally.** Tests execute actual Bash; creation request is draft at the original SHA; freeze-on-publication mock rejects post-freeze uploads; four uploaded assets exist and release finishes immutable. Source confirms per-asset name/size/download/byte comparison occurs before the publication PATCH. |
| **2 — Partial draft can resume without duplicate creation; select latest at publication** | Restores `resp-259` | `release.yml:461–486,510–522`; test `:367` `draft_partial_upload_resumes_and_latest_is_selected_at_publication` | **PASS locally.** Seeded partial draft is found through release-list pages; no POST occurs; upload/verification succeeds; a newer release appearing after upload produces `make_latest=false` on publication. |
| **2/5 — Upload failure or archive/sidecar corruption leaves draft unpublished** | Restores `resp-259` | `release.yml:488–522`; test `:387` `upload_or_remote_byte_failure_never_publishes` | **PASS locally.** Upload failure, archive corruption, and sidecar corruption each fail, leave `.draft=true`, and issue no publication PATCH. |
| **3/5 — Complete immutable release is read-only success; missing/corrupt assets fail with zero writes** | Restores `resp-259` | `release.yml:502–506`; tests `:229` and `:403` `immutable_missing_or_mismatched_assets_fail_without_mutation` | **PASS locally.** Matching immutable retry succeeds without POST/PATCH/upload. Missing archive/sidecar and same-size corrupt archive/sidecar cases each fail. Explicit `assert_no_writes` rejects POST, PATCH, DELETE, and upload calls. |
| **5 — Accepted create/PATCH with lost response stops; later resume avoids duplicate/frozen writes** | Restores `resp-259` | `release.yml:483,521–522`; test `:430` `accepted_writes_with_lost_responses_resume_without_replay` | **PASS locally.** Lost create response leaves draft and prevents upload in that invocation; later run reuses it without POST. Lost publication response leaves immutable state; later run verifies read-only without replay. No automatic write retry loop was added. |
| **4 — Existing-tag legacy mode stays assets-only; mutable published repair remains supported** | Existing release recovery contract | `release.yml:446–449,468–480,502–523`; tests `:458` `existing_tag_mode_remains_assets_only_for_draft_mutable_and_immutable_releases`, `:488` `published_mutable_partial_assets_can_resume_without_republishing` | **PASS locally.** Existing draft remains draft; no release POST/PATCH. Published mutable missing assets repair without republishing; later complete retry is read-only. Complete immutable succeeds read-only; incomplete/missing legacy target fails without writes. |
| **4/5 — Exact PR, SHA, metadata, non-prerelease and unambiguous draft identity** | `resp-259` / `resp-276` as applicable | `release.yml:414–448,461–486`; tests `:260`, `:272`, `:289`, `:315`, `:327`, `:348`, `:501` | **PASS locally.** Current-main advancement accepted; failed/untrusted CI identity, untrusted release PR/history, conflicting tag, wrong PR/metadata, prerelease and ambiguous drafts rejected. Tests assert no writes on the new rejection paths. |
| **4 — Complete native set, provenance, archive and metadata gates preserved** | Existing release contract | `release.yml:155–171,187–281,335–385`; `workflow_tests.rs:554` `staged_assets_match_the_supported_native_targets`; `main.rs:2474–2527` strict identity/provenance/archive tests | **PASS for local checks.** Complete four-file staging accepted and incomplete set rejected. Full xtask run includes metadata consistency and mixed-provenance rejection. Direct diff shows upstream CI/source/metadata/artifact gate code unchanged. No real retained Actions artifact was fetched or native archive built in this verification. |
| **4 — Dry-run, native-before-crate, finalization and ordinary-proposal gates** | `resp-259`, `resp-276` | `release.yml:135–146,394–400,525–539,609–630`; tests `:589` `ci_run_recovery_dry_run_validates_without_authorizing_publication`, `:615` `recovery_requires_one_target_and_forbids_ci_run_rebuilds`, `:315` ordinary-commit and `:536` proposal tests; `main.rs:2753–2789` publication-policy tests | **PASS local behavior plus inspected gate structure.** Dry-run outputs `publish_assets=false`; publishing job requires true and successful validation. Crate job requires assets success and automatic mode; finalization requires both publishers success. Upload/readback failure cannot satisfy these conditions. Legacy mode cannot satisfy automatic crate/finalization conditions. Hosted DAG scheduling itself was not executed. |

### Baseline and regression distinction

Baseline unchanged-source execution independently passed 24 selected workflow/policy/YAML tests and actual workflow policy. It did not exercise immutable freeze ordering. Production's separately supplied `red-regression.log` was reviewed: enhanced freeze-on-publication test failed against the old workflow with `Immutable release rejects asset upload`. This red run is **worker-provided**, not represented as an independent verifier execution. Independent final nextest execution above ran all eight new behavior tests and the existing xtask inventory after implementation; all passed.

## Pending tooling and source-binding limits

1. **Scryer exact-source binding/reconciliation: pending/unavailable.** Read-only exact-worktree `orient` returned “No model exists at project:/private/tmp/codebasegraph-immutable-release-assets-20261029 yet (model file is absent)”. The configured primary-root model is older and contains unrelated drift/pending work. Its `resp-259`/`resp-276` orientation establishes intended scope only, not this exact source. Details: `baseline-scryer-worktree-binding.txt`. No project creation/reconfiguration, old-root JUnit ingestion, attachments, fold, drift reconciliation, or model writes were attempted. Genuine JUnit remains durable PR evidence. Engineer owns any required binding resolution; restoration needs no new architecture claims/fold.
2. **Linear unavailable.** No tracking updates were attempted or claimed; Engineer retains pending synchronization without an invented issue ID.
3. **actionlint unavailable.** Installed-tool probe found no actionlint; no install or settings change attempted. YAML parsing, actual Bash fixture tests and repository workflow policy passed, but do not masquerade as actionlint execution.
4. Graph MCP is not exposed. Registered primary-checkout graph CLI was used only for earlier read-only orientation, not exact-source evidence, and was not refreshed.
5. Full workspace/product suites were intentionally not run for this YAML/delivery-only change, as assigned. Strict Clippy scope is `-p xtask --all-targets`, not workspace/all-features. The full relevant xtask inventory ran without filtering. Windows native behavior, release packaging, hosted credentials/API semantics, live immutable settings and publication remain unqualified here.

No unresolved local test failure or compiler warning was observed. This does not waive the fresh independent Final Reviewer gate or repository-required hosted checks.

## Reusable knowledge and next owner

The reusable lesson supported by this work is that release-creation-only tests do not establish upload-before-freeze behavior: the fixture must freeze writes on publication and verify downloaded bytes. No memory/wiki mutation was made under this read-only assignment; Engineer may distill that lesson after its remaining gates without duplicating the release contract.

**Next owner:** Engineer for final independent review and scope/evidence reconciliation; Production may version this report, genuine JUnit and supporting evidence unchanged after authorization. Source hashes and Git status were still identical at the last command. Any later source/test change requires affected re-verification; documentation-only evidence persistence need not rerun unchanged implementation checks.
