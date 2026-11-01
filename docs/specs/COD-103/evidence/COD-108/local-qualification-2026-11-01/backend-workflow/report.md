# COD-108 Backend workflow consumer handoff

## Status and scope

The resumed Backend slice is complete and source/Cargo writes are stopped. Worktree: `/private/tmp/codebasegraph-cod108-20261101`, branch `cod103-stack/cod-108`, original baseline HEAD `90af76d3453a6f5cc0f032045ed48c4fd09c275c`. Only the granted Rust implementation file `crates/xtask/src/main.rs` changed in this continuation. `crates/xtask/src/release/workflow_tests.rs` was intentionally left byte-identical at `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450`. Frozen core, consumer YAML/docs and non-owned configs remain byte-identical:

| Path | Bytes | SHA-256 |
|---|---:|---|
| `crates/xtask/src/main.rs` | 235135 | `94e36cb3ceed27f45b552e42c9d6a52aa7cfef8dd70876f6e6ab4e562ec8891d` |
| `crates/xtask/src/release/workflow_tests.rs` (unchanged) | 44385 | `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450` |
| frozen `crates/xtask/src/ci.rs` | 48671 | `b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f` |
| frozen `.github/workflows/ci.yml` | 21095 | `b59d7028101120f6885870cec054a65d9788f55d364dc0f2eb2b53bd4fd9c339` |
| frozen `docs/release.md` | 22125 | `90d84f6614be6cd74b8340dd73b1a2c1000457af319475cf8939b1c316ebf486` |

The worktree status still shows the already-authorized Production-owned CI YAML/docs deltas plus the Backend core files; those were not modified in this continuation. Cargo manifests/lock and native/release workflow/action remain unchanged.

## Implementation and actual-body tests

`check_workflow_policy` now aligns with the actual workflow: literal bases `main` plus approved COD105/COD106/COD56/COD107/COD108; exact PR activity set including labeled/unlabeled for identity updates; only main push and exact two events; one `repository-policy` owner for `release-gate` and xtask tests, distinct from package/archive job; exact eight required needs; heavy `clippy`, `publish-dry-run`, `native` guarded by full selector; native retention contract preserved and selector never broadened.

`ci_mode_policy_valid` validates actual classify outputs, read permissions, identity trust pins and current base check, protected B checkout, separate H object checkout, credentials off, checked Rust CLI wired for `ci-mode`, fallback/full reason and record preservation, single common repository-policy command, full-only heavy gates, and the exact aggregate interfaces/freshness wiring. It is typed over parsed actual CI YAML; regression mutations reject missing selector/policy gate, wrong protected checkout, head-helper path, unconditional heavy work, bad output binding, wrong event, and bogus successful required body.

Actual Bash extracted from the frozen YAML ran under local closed stubs:
- Identity step: eligible actual trusted release PR reaches `candidate=true`, returns exact base/head and `eligibility-pending` decision. Creator ID, draft, label, head/base SHA, repository ID and action mutations do not qualify; absent API chooses full; mismatched event repository fails before API calls.
- Decision step: actual script calls the Git object fetch and Cargo classifier command. Fixture records exact process arguments and cwd; Cargo runs in `trusted-base` and references `git-objects`, never executes head code. Valid schema-1 `lightweight` proof produces the light decision. Trusted-base failure, H fetch failure, and invalid helper output produce a valid full record with original SHAs and no positive metadata.
- Required aggregate step: actual body succeeds for eight-key full mode and trusted current lightweight mode with intentional heavy skips; rejects missing classify/repository-policy/native_retention, other missing/unexpected keys, malformed decision schema/mode/reason, common/full failures/cancelled/unexpected states, heavy successes in lightweight, stale current identity/base/head on both full and lightweight, and unexpected full skips. Main push passes only in full mode and does not call PR APIs. Summary/mode/intentional skips are asserted.
- Existing native-retention actual-body success/negative/invalid-input suites remain intact and pass, including COD-107-only behavior.

New test names are in `result.json`. The assigned `release/workflow_tests.rs` file did not need edits; this approved slice was implemented in the granted existing `main.rs` owner while testing current workflow bodies through its fixture helpers.

## Executed checks

Environment: Darwin 25.6.0 arm64; rustc 1.96.0 (ac68faa20 2026-05-25); cargo 1.96.0 (30a34c682 2026-05-25); git version 2.52.0.

- `cargo test -p xtask --locked`: exit 0, 102 passed, 0 failed.
- `cargo fmt --check`: exit 0.
- `cargo clippy -p xtask --all-targets --all-features --locked -- -D warnings`: exit 0, no warnings.
- `cargo run -p xtask --locked -- check-workflows`: exit 0, `workflow policy passed`.
- `cargo run -p xtask --locked -- release-gate`: exit 0, `release gate passed`.

Raw exact outputs and status are saved as private files in `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-7f3e2b93/workflow-tests-handoff/raw`; count 61. The private result manifest is `result.json` in this handoff directory. No JUnit was produced or synthesized.

## Editing-method incident/history

During the first attempted implementation phase, I disclosed to the parent that I had used `Path.write_text` for the initial new `ci.rs` creation and one direct rewrite of existing `main.rs` while removing temporary diagnostics, contrary to the exact-edit procedure. The parent explicitly authorized resuming under the procedural correction. No rollback/replay was used to claim retroactive compliance. All source edits in this resumed continuation were performed using exact-unique `edit` replacements. Prior fixture/setup failures are retained as raw logs; the final checks above are from the corrected final source.

## Limits and next owner

No workflow/API trust/hosted rollout was claimed; the local closed fixtures execute the actual three Bash consumers but do not qualify GitHub Actions scheduling, protected-base enforcement, hosted secrets, branch policy or Windows behavior. The immutable classifier/core and workflow/docs remain frozen. Independent verification, final review, COD-108 PR/bootstrap/full main CI, hosted lightweight rollout and all later delivery/aggregate gates remain with their recorded owners. No JUnit ingestion, Scryer fold, Git operation, API/tracker/wiki/model write, merge, release or Done action was performed.
