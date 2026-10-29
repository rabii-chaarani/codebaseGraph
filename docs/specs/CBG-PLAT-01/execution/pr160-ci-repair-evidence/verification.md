# COD-109 / PR160 final verification

## Tested snapshot and authority
- Independent Evidence Verifier; inherited actual model/effort unexposed. No source edits, Git mutations, release, or primary dirty-checkout writes.
- Exact execution checkout: `/tmp/codebasegraph-pr160/worktree`; HEAD `db35f8b8376f86c49a871f139dcf92f26fce24ed` (mechanical integration of PR160 and main `5889ed09954a4a07efd2590dd1bd380ee77be06a`), tree `64109b33e96725b44c54f0a634d0351820139045`, plus approved frozen `tests/coordinator_process.rs` SHA256 `d3dd06523d1ebc4250820770bb99a3c03a664e99a2870d4c66edf9351a80657a`. Commit/tree alone does not contain the then-uncommitted tested fixture; full native-source hashes identify it.
- Package compiled as4.0.3. Public CLI `--version` is unsupported; version is established by exact manifest/source and compiler package message, not fabricated runtime output.
- Environment: macOS26.6.2 build25G83, Darwin25.6.0 arm64, rustc1.96.0 `ac68faa20` / cargo1.96.0 `30a34c682`; cargo-nextest0.9.140.
- Existing `native-tests` Cargo profile (release-derived, debug assertions, overflow checks, unwind) and isolated `/tmp/codebasegraph-pr160-target` used. No workload ceilings, test deadlines, hooks or control validations changed by verifier.

## Source, CLI and report binding
- Exact-source no-run build succeeded33.49s; compiler rebuilt normal CLI and both test binaries (`fresh=false`). Private normal CLI capture SHA256 `cfa18cfce96d1f85b385e1c413d132eefe513b180955cda9d2f62eef49118edb`; executable and test hashes/profile: `final-evidence/capture.json`.
- Full tracked native inputs unchanged after compile and tests. `final-evidence/source-before.json` records exact file hashes. Captured CLI unchanged after tests. All native input hashes and private CLI bytes remained unchanged after every check; postcheck hashes are in `final-evidence/source-after.json`. Source-manifest SHA256 `3e8e702e298f4a2159d43d3d9e35576189f97deeea82ef1892f457214bdebdfd`.
- Exact commands: `final-evidence/commands.json`; Cargo compiler JSON/stderr retained. Runtime override passed using normal `env CARGO_BIN_EXE_codebase-graph=<private capture>`.
- Nextest run `5e9de0dd-43db-4d51-92c9-1e3258446e06`; serial1, retries0, fail-fast. JUnit SHA256 `9b6424c26465df13c837cd2f1d42cc695308e467003b7709f4ea3f9f62d4c8fe` in `final-evidence/junit.xml`; log `final-evidence/nextest.log`. Five selected cases, no selected case skipped or retried;22 other cases outside this bounded scope were filtered.

## Acceptance evidence
| Behavior / preserved contract | Executed check | Result |
| --- | --- | --- |
| Actual worker/writer exclusion, partial guard release, held guard lifetime | `quiescence_observation_requires_and_holds_both_execution_leases` | PASS0.013s |
| Shared coordinator/resources, changed content/search, post-publication abrupt crash takeover, rebuilt generation and strict terminal workspace controls | `twenty_mcp_clients_share_one_coordinator_worker_and_take_over` | PASS46.000s |
| Authenticated cancellation, complete teardown and pending-edit retry (PR145-TREE-R1/R2/R4, RM2/RM5 preservation) | `lifecycle_faults::authenticated_cancellation_cleans_loader_and_retries_pending_edit` | PASS103.343s |
| Active paused worker+loader owner loss before recovery (PR145-TREE-A2/R1/R2) | `lifecycle_faults::owner_exit_cleans_paused_worker_and_loader_before_recovery` | PASS99.352s |
| Unknown authority retains evidence; verified emptiness permits continuation (PR145-TREE-A3/R3/R4, RM5/RA4) | `lifecycle_faults::unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven` | PASS105.239s |

Five passed on their first execution in353.952s reported /355.287s wall. Resource gates remained: follower p95=12,402,688; owner idle=18,579,456; owner build growth=933,888; worker peak=162,807,808 bytes. No thresholds changed.

## Static/compiler checks
- `cargo fmt --all -- --check`: PASS (exit0).
- Strict workspace Clippy all targets/features, locked and `-D warnings`: PASS (exit0, wall34.414s); no compiler warnings. Output `final-evidence/clippy.log`.
- Independent normalized source comparison found every original strict control-validation condition unchanged. Only approved guard capture/tuple/signature relocation applied (`final-evidence/strict-body-comparison.json`). Real root guards are kept across abrupt kill; no unknown receipt/pending intent exception broadened.

## Limits and remaining gates
- Historical baseline failure and original CI failure remain in `baseline-verification.md`. Local baseline reproduced owner closure with false empty receipt; original CI higher pending intent was not causally proven identical. A passing fixed regression is not a full historical root-cause claim.
- START-barrier exact-build negative control was rejected as invalid: it blocks pre-publication prerequisites, not this post-publication gate. The approved deterministic real-lock test establishes exclusion, not native teardown; unchanged active/unknown lifecycle tests provide separate executed evidence.
- This is bounded local macOS qualification, not Windows or broad native CI. Fresh final-head PR CI, applicable merge/mainCI and Production delivery remain pending.
- Configured Scryer graph/model is bound to older original checkout, not this exact worktree; no misleading JUnit ingestion/current-verdict claim. Parent's project-wide radius includes unrelated stale files and was not used to expand this approved scope.
- Reusable knowledge assessment: publication/search success is not worker teardown; this is already captured by the existing worker-recovery contract and memory. No new memory candidate is needed, and unresolved CI cause is not stored as fact. No private capability bytes copied to durable evidence.
