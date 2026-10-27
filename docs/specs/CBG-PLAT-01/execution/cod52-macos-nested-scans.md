# COD-52 — macOS nested directory scans

## Identity and scope

- Work: `CBG-PLAT-01/S01`, [COD-52](https://linear.app/codebasegraph/issue/COD-52), Maintenance `P-COD-2`. S00/COD-51 is cleared **only for this slice**.
- [Canonical spec](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md) revision0.10, approved full SHA `1a9db7b558b60248888408daa6268a3b45f2f0df`, R01 / A01–A02. [Exact user approval](../approvals/cod52-macos-nested-scans.md) records the2026-10-27 explicit “approved” response for COD-52 only.
- Source baseline main `7038e66ff8271ffddc511f879cf8d28353a09b2d` (4.0.0), after PR153 merged. Both target files were unchanged from reviewed CI-fix head `d6dd34493e9da6f80847240bafcc92dbcaed2c38` before this work.
- Source state: locally implemented; final full local verification and native Windows CI pending. No release/consumer qualification or aggregate closure. No merge, deployment, service restart or other slice implementation.

## Implementation and acceptance trace

| Criteria | Existing claim context | Implementation / verification |
|---|---|---|
| A01 / R01 | Graph Store resp-232/233 | `RootedDirectory::visit_entries_filtered` opens literal `.` relative to the retained directory FD with a fresh open file description, owns it with File, verifies physical identity, and enumerates on that local FD. Shared-cursor lseek removed on macOS only. Nested multi-batch exact-vector regression spans160directories plus1file and checks unfiltered/verified-directory traversal. Old production fails; corrected production passes. |
| A02 / R01 | Graph Store resp-232; canonical DC-S3-R3/DC-S4-R4 | Actual `prepare_terminal_output` regression uses genuine terminal registration/origin receipts, an empty parent whose child receipt remains, a later eligible directory beyond one native batch, immediate cancellation and successful reprepare/retry. Missing child veto would select the lexicographically smallest parent instead of target. Old production fails; corrected production passes. Rooted interruption/public-name-replacement test and existing authority/nonfollowing tests pass. Native Windows nested/storage execution remains pending. |

No public API, product constants, dependencies, buffer/parser policy, Windows/Linux production branch, deletion authority or child-retention rule changed. Fixture counts160/161 and native-order index128 are test data:128 is a conservative4096/32capacity bound, not the incidental85-entry observation or a new product setting. No `dup`/`try_clone`, public-path reopen or platform framework added.

## Exact source and environment

Final source SHA-256 values in [snapshot](cod52-evidence/final-frozen-snapshot.json):

- `src/storage/locks.rs`: `3334713035db60f24a86c34e3fbdaed0724943fcbdf96103c7bf1a39e163f25c`
- `src/storage/maintenance_candidate.rs`: `8e539db80a81de8fb674270adaf608a551954db86352b3e87ad674d6badac765`

Native macOS26.6.2 build25G83, ARM64, Rust1.96.0. Cached Ladybug0.19 native archive SHA-256 `a840ea7c538e67a8b9fc0d58495b5226dc9c0a5f2f144ff0c48c0f6ca1193ae6`. [Baseline provenance](cod52-evidence/baseline-provenance.json) records paths and unchanged manifest/lock hashes. The root package actually compiled from the isolated worktree; shared-target evidence is bound to source hashes and named tests, not merely pass totals. Rust1.82/macOS13.3 were not executed; API-age/MSRV source review is not native qualification.

## Executed results and revision limits

Commands and result codes: [executed-commands.json](cod52-evidence/executed-commands.json). JUnit reports are runner-produced, not inferred from configuration.

- Final strengthened fixture plus final corrected production: **15/15 native library regressions pass**,830filtered (not executed): [log](cod52-evidence/integrated-final-regressions.log), [JUnit](cod52-evidence/integrated-final-regressions.junit.xml).
- Same final receipt fixture with **only old macOS production restored**: expected selection-completeness **failure**,0pass/1fail; setup/child-veto/cancellation checks pass first: [log](cod52-evidence/receipt-final-old-production.log), [JUnit](cod52-evidence/receipt-final-old-production.junit.xml). Fixed production was restored before final checks.
- Earlier rooted regression against old production:1pass/1fail, failure is incomplete outer enumeration: [JUnit](cod52-evidence/regression-baseline.junit.xml).
- Storage-foundation: **267pass,1existing ignored subprocess-only `relative_barrier_subprocess_fixture`**; before final test-only candidate integration. Candidate is not compiled by this harness: [JUnit](cod52-evidence/storage-foundation-full.junit.xml).
- Cleanup integration: **11pass,0skipped**, before final test-only integration, including20public build/control updates, native-owner death/retry and actual first/follow-up publication birth: [JUnit](cod52-evidence/storage-maintenance-full.junit.xml).
- Final strict `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all --check` pass: [Clippy](cod52-evidence/clippy-final-frozen.log), [format](cod52-evidence/format-final-frozen.log). Initial Clippy manual-C-string warning was corrected to MSRV-compatible `c"."`.
- Final full library+foundation run is **in progress**, not passing yet. An earlier partial full-library run was terminated after a later build reused its binary path; [excluded partial evidence note](cod52-evidence/excluded-partial-library-run.md). It is not counted as full-suite verification.

## Independent review

Fresh GPT-6.1Sol/High [proposal review](cod52-evidence/proposal-review.md) green. Fresh GPT-6.1Sol/Max [initial final review](cod52-evidence/final-review.md) found a P2 Windows advisory-directory expectation in the new test; fixed by verifying hinted directory opens, without changing production. [Follow-up](cod52-evidence/final-review-followup.md) checks the final strengthened receipt/cancellation fixture and reports no unresolved P0–P3 source defect. Earlier fixture variants/evidence in that historical report retain their stated limits; the final strengthened fixture's exact results are above. Final full-suite/native Windows evidence still gates acceptance/closure.

## Hosted native execution — source head `d2a442faaf4ca59dc5fe33f082b2072f49b97aa5`

[CI run40449278177](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40449278177) finished **failure**:19jobs succeeded,2failed (one native integration and required aggregate),1Windows artifact skipped. [Job/case summary](cod52-evidence/hosted-run-40449278177/summary.json), [exact testcase and failure records](cod52-evidence/hosted-run-40449278177/case-records.txt).

Actual tested synthetic merge `f633445077d1c81888eb44f7a496927f54289ad7` merges the head into baseline `7038e66ff8271ffddc511f879cf8d28353a09b2d`. Executed `git diff --stat` between tested merge and head is empty: whole Git trees match. Named actual passes (not inferred from green jobs):

- Mac unit job122007921553: interruption/public-root substitution, nested scan and genuine terminal receipt selection; all843cases pass,2ignored. Mac integration/generation and native artifact build/smoke also pass.
- Windows unit jobs122007920956/122007921364: nested scan passes; the same test also passes in storage-foundation inside failed integration job122007921021. All Windows unit/generation cohorts and3of4integration shards pass.

Blocking Windows integration shard3 failure is **unchanged** `tests/copy_phase_process.rs:417`, `bounded_production_chunks_reopen_with_primary_keys_connectors_and_search`: the assertion compares unique numeric PID count to COPY event count and sees55vs56. This establishes duplicate PID values. PID reuse is a likely explanation, not confirmed raw-phase provenance or evidence of COPY/data loss. This test and COPY production paths are unchanged from baseline; no relation to the macOS-only production delta is established. Read-only triage discovered the file through the target graph; connected graph MCP became unavailable (`MCP session is not initialized`), CLI fallback used. No assertion/deadline weakened, unapproved test repair, CI retry or dependency change performed.

[Fresh reviewer evidence addendum](cod52-evidence/hosted-run-40449278177/review-addendum.md) confirms the scope distinction and notes that downstream reopen/PK/connector/search/control checks in the failed COPY case were not reached. Windows named nested-scan execution is now verified; the required aggregate is not.

Required aggregate remains failed and Windows artifact skipped, **not passing qualification**. COD-52 remains In Review; no merge/close/consumer qualification. User decision is required before expanding to repair the separate process-identity test. Final local library+foundation run is still active with slow cleanup coverage, not a passing full-suite result.

## Architecture, delivery and remaining gates

Scryer proportional ledger `chg-bfthrc` preserves existing Graph Store resp-232/233; no new claims/topology. Governing intent/rules and changed-symbol claims were checked. Configured graph/model remain bound to the older dirty root checkout; target-scoped local graph independently discovered implementation/fixture files. `orient(project=isolated_path)` reports no model. No model was copied/invented, worktree reports were not ingested as root verdicts, and unrelated drift/pending claims were not globally reconciled.

Remaining: final full local run, native Windows nested/storage execution at the delivered head/synthetic merge, current binary/source identities and review evidence reconciliation, then COD-52 acceptance. Minimum/current clean-consumer archives, other policies/slices, S09 disposition and maintainer aggregate closure remain separate COD-61/milestone gates. Existing dirty checkout is untouched. Reusable-knowledge assessment: this restores a specified invariant; ordinary shared-descriptor semantics and existing binary-provenance procedures need no filler memory or duplicate wiki contract.
