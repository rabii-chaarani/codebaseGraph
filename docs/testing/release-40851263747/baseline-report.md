# Immutable release publication correction — baseline evidence

## Status and scope

Baseline-only assignment completed. No source changes, Git writes, publication, Actions dispatch, live credential use, Scryer/model writes, or evidence ingestion performed. Final verification is pending a separate assignment after implementation.

- Actual worktree: `/private/tmp/codebasegraph-immutable-release-assets-20261029`
- Branch: `fix/immutable-release-assets`
- Initial baseline HEAD: `5889ed09954a4a07efd2590dd1bd380ee77be06a`; tree `e13cb9657010f0407518f2fe3c1ba9fdc430956e`.
- Post-check HEAD advanced concurrently to `ad3beec36502448c238f236c9af16702ddce107c`; tree `05d9b6c06f347085642610d5b591bc06a176c2c6`. Verifier did not perform any Git write. This is not a single-HEAD whole-tree verification.
- Tracked diff, staged diff, and untracked inventory: empty before and after execution.
- The four relevant source-file bytes stayed identical to the initial baseline, checked before and after execution; hashes in `baseline-source-inventory.json`. The initial baseline-source evidence remains valid for those unchanged files. Commit differences are logged in `baseline-concurrent-commit-identity.log`.
- Canonical contract: `docs/release.md` at `5889ed09954a4a07efd2590dd1bd380ee77be06a`.
- User decision references supplied by Engineer: “try the required workflow correction” and “yes i approve the inspection”. The oracle is the existing release contract plus that correction request and proposal SHA-256 `c552e03401ad3160b0472499df0a97f356e658f6affb12269a27c0526860eeb6`.
- Documentation-only approval record at post-check commit: `docs/testing/release-40851263747/approval-and-acceptance.md`. The parent explicitly authorized this concurrent record commit; no workflow/test/main/product source changed. This docs-only commit does not make the relevant baseline execution stale.
- Discovery exception is limited to `.github/workflows/release.yml` and `crates/xtask/src/release/workflow_tests.rs`; docs and main.rs were graph-discovered by Engineer. No new work, requirement, or acceptance IDs invented.
- Existing claims: `resp-259` / `resp-276`; restoring ledger `chg-4kq3vn`. No claims assigned for folding.
- Linear unavailable; no delivery updates claimed.

## Executed implementation input identity

These bytes are identical in the original baseline and the docs-only post-check commit.

| File | Bytes | SHA-256 |
|---|---:|---|
| `.github/workflows/release.yml` | 28961 | `56b4530d355685f13da6b690ca28c401f9e3211d7db76d387c63281524fcc2e2` |
| `crates/xtask/src/release/workflow_tests.rs` | 13269 | `ce8e04c3afd53f9095d5e549a111ab0cb294b32105aac511fa7ba6bfd9b06c2a` |
| `crates/xtask/src/main.rs` | 105700 | `70507a624361a97a7bcdb28412b31112258fcb023fa8e80dccd943b113b335ad` |
| `docs/release.md` | 14790 | `5f867117736b465211c677ae7bbb19aec5d47ec7837228d7f50cfe0ba7fb3991` |

## Environment and safe execution

- macOS 26.6.2 (25G83), Darwin 25.6.0, ARM64.
- Cargo 1.96.0 (`30a34c682`, 2026-05-25); rustc 1.96.0 (`ac68faa20`, 2026-05-25).
- cargo-nextest 0.9.140 (`a9fef2964`, 2026-07-05), aarch64-apple-darwin.
- `/usr/bin/jq`: jq-1.7.1-apple; `/bin/bash`: GNU Bash 3.2.57(1)-release.
- `actionlint` is not installed; that check is unavailable, not passing.
- Native Cargo commands executed from the actual worktree using `--locked --offline`; no dependency installation, manifest changes, or repository setting changes.
- Dedicated verifier target directory outside the repository: `/private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target`. Cargo compilation log names the actual worktree's xtask package.
- Workflow Bash executes under the repository's closed mock `gh` fixture. Unknown GitHub calls fail with status 97; no real GitHub CLI is called by these tests. `GH_TOKEN`, `GITHUB_TOKEN`, and enterprise-token variables were unset for the initial probe. Subsequent Cargo processes inherited no token/password/secret/credential/API-key-like variables; the workflow fixture also removes GH_TOKEN/GITHUB_TOKEN.
- Genuine nextest JUnit enabled with an external report-only config (`baseline-nextest-report.toml`); no repository config edits. It changes only report output location/name.

## Executed evidence

### Targeted workflow, policy, and YAML tests

```sh
cargo nextest run -p xtask --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target --config-file /private/tmp/codebasegraph-immutable-release-evidence-20261029/baseline-nextest-report.toml --user-config-file none --no-fail-fast --no-tests fail --test-threads 1 -E 'test(release::workflow_tests::) | test(workflow_policy) | test(workflow_yaml)'
```

Exit **0**. **24 tests ran and passed**: 10 actual-workflow Bash fixture tests, 13 workflow-policy mutation checks, and one YAML validation test. 20 other xtask tests were intentionally excluded by this baseline filter. Zero compiler or runner warnings were emitted.

Genuine JUnit: `baseline-junit.xml`, nextest run ID `cfa5be04-9b21-4518-b432-6a7e42b7da09`. JUnit reports tests=24, failures=0, errors=0. Log: `baseline-suite.log`.

### Policy against actual workflow files

```sh
cargo run -p xtask --locked --offline --target-dir /private/tmp/codebasegraph-immutable-release-evidence-20261029/cargo-target -- check-workflows
```

Exit **0**, `workflow policy passed`. Zero warnings. Log: `baseline-workflow-policy.log`.

No pre-existing policy failure was observed in this baseline.

## Criterion-level trace

| Existing contract / applicable claim | Source and tests | Result and limit |
|---|---|---|
| Exact release merge/SHA remains publishable after main advances; `resp-259` | `.github/workflows/release.yml`; `publisher_targets_original_sha_and_is_idempotent`, `publisher_rejects_conflicting_tags_and_does_not_regress_latest`, `verified_release_survives_main_advancing_and_ignores_other_pending_releases` | Baseline local fixture passes. Does not establish immutable release upload ordering. |
| Ordinary main CI cannot publish unrelated releases; `resp-276` | Same workflow; `ordinary_commit_cannot_authorize_release_and_tagged_retry_still_can`, `proposal_reports_pending_release_then_proceeds_after_finalization` | Baseline local fixture passes. |
| Failed/untrusted run/PR/history and unsafe recovery input fail closed | Workflow tests `verified_release_rejects_failed_ci_and_untrusted_run_identity`, `verified_release_rejects_untrusted_prs_and_removed_history`, `recovery_requires_one_target_and_forbids_ci_run_rebuilds`, `ci_run_recovery_dry_run_validates_without_authorizing_publication` | All pass locally using closed mocks. |
| Complete native asset staging | `staged_assets_match_the_supported_native_targets` | Complete set accepted; missing checksum rejected. No real native package build run. |
| Existing workflow policy guards | `crates/xtask/src/main.rs` tests, actual `check-workflows` command | 13 policy tests and actual policy check pass locally. |
| New draft → upload → verify → publish behavior; immutable existing release causes no mutation | Implementation not present at this baseline | **Pending final verification**; current baseline tests do not establish it. |

## Authority/tool binding limits

Configured wiki bundle `knowledge` consulted via MCP list/search and memory recall. Relevant memory: `exact-commit-release-publication-and-recovery-2026-09-24`; treated as advisory. Scryer governing rules and scoped orientation retrieved with explicit project `/Users/rabii/Projects/Repositories/codebaseGraph`. That older primary checkout confirms the existing `resp-259`/`resp-276` scope, but also contains unrelated pending work and drift. It is not the tested worktree or exact-source proof.

Graph MCP is not exposed in this session. Read-only CLI graph search/context was run against the registered primary checkout only for orientation (`baseline-graph-orientation-old-root.log`). No graph refresh was attempted.

**Exact-source Scryer ingestion/binding remains blocked/pending**: a read-only `orient` with absolute project `/private/tmp/codebasegraph-immutable-release-assets-20261029` returned “No model exists ... (model file is absent)”. Details: `baseline-scryer-worktree-binding.txt`. No project creation/reconfiguration was attempted. Target report fingerprints must not be ingested into the stale primary-root model. No target-result ingestion, test attachment, fold, drift reconciliation, or ledger mutation was attempted. Engineer owns resolution; no new architecture claims/fold are needed for this restoring small fix.

Local macOS fixture evidence is not hosted Actions execution, Windows qualification, full workspace qualification, or live/publication verification. No supported-platform or release closure claimed.

## Settings, reusable knowledge, next owner

- Role: `evidence_verifier`; model and reasoning effort map to `inherit`. Parent accepted assignment without explicit overrides. Runtime model/effort metadata is not exposed to this agent; no known mismatch.
- Reusable knowledge assessment: baseline confirms existing behavior and tool availability only; no new durable fact warrants a memory write. Read-only limits preserved.
- Next owner: Engineer/Production for approved correction; return the stable edited snapshot to Evidence Verifier for independent final checks. The final checks must cover draft creation, all four uploads and digest verification before publication, frozen existing release no-mutation paths, failures/retries, existing guards, and warnings.
