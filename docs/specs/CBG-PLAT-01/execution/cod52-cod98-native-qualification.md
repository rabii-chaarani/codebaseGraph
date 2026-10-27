# COD-52 and COD-98 — exact-head native verification

## Result and source identity

[PR157](https://github.com/rabii-chaarani/codebaseGraph/pull/157), implementation head `5e3e42f48a55581ef75b25ba21732913089f664d`. [CI40457158678](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40457158678), attempt1, completed **success:22/22jobs**, no failed/skipped jobs. Both native artifact **build-and-smoke** jobs and required aggregate executed/passed; conditional internal-artifact uploads were skipped by design, not native build/smoke jobs. No package publication or consumer-install qualification is inferred.

Actual tested merge `f61579940edfa3f3bf67ac6b2e7abbf04ea37cfb` merges this head into baseline `7038e66ff8271ffddc511f879cf8d28353a09b2d`. Executed `git diff --stat <head> <merge>` is empty: the entire Git trees match. Exact [source/reviewer identities](native-run-40457158678/source-final.json), [job metadata](native-run-40457158678/run.json), [bounded source/CLI/test records](native-run-40457158678/case-excerpts.txt), [case evidence](native-run-40457158678/case-evidence.json), [raw-log hashes](native-run-40457158678/raw-log-identities.json), [artifact step metadata](native-run-40457158678/artifact-jobs.json).

This evidence-only stacked branch preserves PR157's passing implementation head. It does not change the implementation or silently advance the approved spec revisions.

## Approval, acceptance and executed evidence

- **COD-52 / CBG-PLAT-01S01 / R01 / A01–A02:** [spec rev0.10 full SHA1a9db7b558b60248888408daa6268a3b45f2f0df](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md); [exact user approval](https://github.com/rabii-chaarani/codebaseGraph/blob/9bf9212d9266c33b389829925b456c1da7320886/docs/specs/CBG-PLAT-01/approvals/cod52-macos-nested-scans.md). Existing Graph Store resp-232/233, Scryer `chg-bfthrc`; no changed claim/topology. Mac nested traversal, interruption/public-root replacement and actual terminal receipt child-veto/late-selection/cancel/retry tests pass. Nested traversal also actually passes on Windows. The original defect and production-selector omission are exposed by retained old-production failures in [execution record](cod52-macos-nested-scans.md).
- **COD-98 / CBG-COPY-01 verification follow-up / A3,A9:** [separate explicit approval](https://github.com/rabii-chaarani/codebaseGraph/blob/931c3ddaeccba9c3f94f75e3ae58be9e3016a4ef/docs/specs/CBG-COPY-01/approvals/process-identity-test.md), [source/localreview evidence](../../CBG-COPY-01/process-identity-execution.md); Scryer `chg-k66bxx`. The real formerly failing COPYchunk test now completes its native isolated-process, reopen, PKoperator/lookup, exactconnectors/search/cleancontrol checks on **both Mac and Windows**. Jumbo-row and actual Managed+Direct acknowledgement-failure/preservation/recovery cases also pass on both platforms. Both deterministic identity predicates and current-process singleton stability execute on both native platforms. The old55/56numericPIDfailure remains preserved, not rewritten as a proven PIDreuse diagnosis.

| Actual testcase | macOS job | Windows job |
|---|---|---|
| bounded_production_chunks_reopen_with_primary_keys_connectors_and_search |122033480554|122033480404|
| jumbo_staged_row_survives_real_copy_and_fresh_process_read |122033480554|122033480543|
| native_copy_acknowledgement_failure_preserves_managed_and_direct_bundles |122033480554|122033480543|
| phase_process_oracle_accepts_reused_pid_but_rejects_reused_process |122033480554|122033480738|
| phase_process_oracle_rejects_missing_or_invalid_evidence |122033480554|122033480543|
| phase_trace_process_instance_is_stable_across_calls_and_threads |122033480377|122033480628|
| nested_directory_scans_do_not_consume_the_outer_cursor |122033480377,122033480554|122033480543,122033480578,122033480686|
| interrupted_directory_scan_restarts_on_the_pinned_root_after_name_replacement |122033480377,122033480554|macOS-only regression|
| terminal_receipt_selection_reaches_late_directory_after_child_receipt_veto |122033480377|macOS-only regression|

Named results are actual Rust-harness pass records, not test discovery or job-green inference. The chunk test prints diagnostics between its test-name line and standalone `ok`; retained extracts include that final `ok` and the nonzero passed-test summary. Successful completion establishes downstream data checks ran rather than stopping at the earlier PIDassertion.

## Review and honest limits

Fresh GPT-6.1Sol/High proposal reviews and fresh GPT-6.1Sol/Max final source reviews are retained with each execution record. Their exact four source hashes still match the tested head. The COD-52 final reviewer caught the Windows advisory-directory test expectation; corrected within the test, then actually executed on Windows. No unresolved confirmed P0–P3source defect remains in these bounded reviewed changes. [Hosted evidence review](native-run-40457158678/hosted-final-review.md) independently cross-checks actual case records, binary/source provenance, run metadata and unchanged source hashes.

- LocalMac strictworkspace/alltargets/allfeaturesClippy and formatting pass; hostedfmt/clippy/audit/package/nativecohorts pass. NativeWindows logs still contain **preexisting unchanged** cfg-specific unused-function/import warnings (home_dir, workspace_fifo_pin_name and test-only imports). They are not newly introduced warnings, were not suppressed, and are not repaired under this bounded test/cursor scope. PassingCI is not a warning-free whole-platform claim.
- Earlier broad localdebug1110case run is **incomplete**:606completedpasses plus one missing-captured-CLI fixtureprerequisite failure, then terminated before follow-upbuilds. No full localpass is claimed. Final capturedCLI nativeCOPY5/5andphaseunit13/13passes remain exactsource evidence; hostednativeCI supplies broad current-source execution.
- Native runner execution and artifactsmoke do not qualify cleanWindows11desktop installs, macOS13.3/current cleanconsumer archives, bothPowerShellshells, plainCargosourceinstall runtimes or release rollout. Those remain COD-61/otherMaintenance work. No Linux/Intel/ARMplatform expansion, dependency/resource/containment/publicprotocol change.
- ConfiguredScryer model is olderroot orientation, not a worktree model; no newclaim existed tofold and no exactworktree verdict was fabricated/ingested against oldroot. TargetCLIgraph discovery preceded source reads; graphMCP outage was reported. Existing dirty usercheckout preserved. Reusableknowledge assessed: existing processidentity/qualification constraints plus retained focused evidence suffice; no filler memory or duplicatewiki spec created.

## Handoff

COD-52 and COD-98's approved implementation/review/native-test gates are satisfied by the exact tested source and linked evidence. Their delivery closure does not close COD-51's remaining approvals, COD-61consumerqualification, S09disposition, aggregateMaintenance milestone, PRmerge or deployment. Maintainer/user retains aggregateownership. **No merge, deployment, packagepublication or service restart performed/authorized.**
