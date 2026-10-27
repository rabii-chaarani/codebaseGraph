# PR #153 review follow-up

Baseline: `6e1543b038961261555d35e55a4095afae02392e`. Scope: DC-S3/DC-S4, RAB-8/RAB-9; canonical wiki `architecture/graph-disk-cleanup-contract`, approved revision 0.2. Scryer bugfix ledger `chg-ndq0yv`. User request: address PR #153 comments.

## Changes and acceptance

- discussion_r4372938440: inapplicable Direct legacy confirmation is discarded at request/startup admission before source handles are captured; it no longer blocks modern candidate selection/recovery. A real interrupted Direct metadata transaction recovers through startup, request(true/false), and confirm-and-wait; Managed confirmation remains deferred until enrollment.
- discussion_r4372938443: advisory legacy-generation discovery completes beyond 100 entries, retaining fixed scratch and one selected name. The regression scans 153 entries twice, excludes active/origin-marked/unknown entries, preserves their bytes, and verifies per-entry cancellation.
- Removed the advisory literal 100-entry cutoff only. Authoritative 100-entry/100-ms mutation budget, ownership checks, retention rules and public contracts are unchanged. Full advisory scanning is permitted by approved revision 0.2.
- Independent review found the expanded scan needed cancellation checks and the initial candidate-only bypass could retain unnecessary confirmation payload handles. Both were fixed: per-entry cancellation and ingress filtering with the original candidate guard restored. No shutdown deadline is claimed.

## Executed evidence

Darwin ARM64, rustc/cargo 1.96.0, native-tests profile, shared Cargo target with fresh compilation from this worktree. Source SHA-256:

- `src/storage/maintenance.rs`: `061191affdacb910abe21070c25ae3cc2979db2f87ea0127274dce61e1e19c9e`
- `src/storage/maintenance_candidate.rs`: `41917895967128bce8c7a5a3b4b6ad4ba55a2398232b8a3239c27e46795f47ba`
- `src/storage/maintenance_legacy_generations_engine.rs`: `35fd9ff8a8c198226844664605b3dba54038b95d908554912200ef238e17642a`

Both new regressions failed against the original behavior; baseline logs are retained here. The earlier Direct fixture setup attempt lacked tracked fresh ownership; that fixture was corrected before the recorded meaningful baseline failure.

- Intermediate pre-ingress snapshot: narrow 7/7 and broad 30/30 passed. These overlapping selections do not qualify the final ingress revision. Final results are recorded below.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`: passed after final source edits.
- `git diff --check`: passed.

Commands:

```text
CARGO_TARGET_DIR=/Users/rabii/Projects/Repositories/codebaseGraph/target cargo nextest run --config-file /private/tmp/pr153-nextest.toml --profile pr153 --cargo-profile native-tests -p codebase-graph --lib -E 'test(storage::maintenance_candidate::tests) or test(storage::maintenance_legacy_generations_engine::tests)' --no-fail-fast
CARGO_TARGET_DIR=/Users/rabii/Projects/Repositories/codebaseGraph/target cargo nextest run --config-file /private/tmp/pr153-nextest-broad.toml --profile pr153 --cargo-profile native-tests -p codebase-graph --lib -E 'test(storage::maintenance_candidate::tests) or test(storage::maintenance_legacy_)' --test-threads 1 --no-fail-fast
```

JUnit, baseline/final logs and Nextest configs are adjacent. Independent reviewer: fresh GPT-6.1 Sol, Max effort; see `review.md` for final snapshot and outcome.

## Limits and handoff

No full-suite, native Windows, live migration or rollout qualification claimed. Legacy regression proves shipping discovery selection/nonmutation, not complete large-backlog retirement. CI remains separate. Maintainer owns aggregate closure.

Linear unavailable; RAB-8/RAB-9 synchronization pending. Graph discovery was attempted through configured MCP and local CLI; configured graph is older and worktree graph DB is absent. Correct original implementation Scryer project was read; claims remain unchanged. Final report ingested: verdicts recorded for two existing claims; 45 cases had no attached test match. Empty bugfix ledger closed with rationale; no model intent changed. Its removed/mismatched source checkout has preexisting broken anchors/stale verdicts; global Scryer reconciliation is not claimed. No unrelated state was changed to hide those limits.

Reusable-memory assessment: no new memory warranted. Existing approved contract already covers fair progress, advisory budgeting and cancellation; task-specific evidence belongs here.

## Final ingress snapshot verification

Final serial native Nextest: **50/50 passed**, zero failures/errors; 790 tests excluded by filter. Run `f58f802c-25fd-401e-b79e-ec18806a307f`, 95.603s. Includes real Direct ingress/recovery, Managed pending confirmation, large legacy discovery/cancellation, cursor and scheduler/source-capture tests. The ingress test includes one existing 60s scheduler cooldown; no product interval changed.

```text
CARGO_TARGET_DIR=/Users/rabii/Projects/Repositories/codebaseGraph/target cargo nextest run --config-file /private/tmp/pr153-nextest-final.toml --profile pr153 --cargo-profile native-tests -p codebase-graph --lib -E 'test(storage::maintenance::tests) or test(storage::maintenance_candidate::tests) or test(storage::maintenance_legacy_) or test(storage::maintenance_scheduler::)' --test-threads 1 --no-fail-fast
```

`pr153-review-final-junit.xml`, `pr153-review-final.log`, `pr153-nextest-final.toml` identify final executed evidence. `pr153-ingress-clippy.log` is the final workspace/all-targets warnings-denied check. `pr153-ingress-baseline.log` reproduces the actual request-path failure with only the three ingress behaviors restored; final source was restored before execution. Earlier candidate-only baseline and 7/30-case logs are intermediate evidence, not final-source qualification.

Text logs have trailing whitespace and final blank lines normalized for Git; JUnit bytes remain unchanged.
