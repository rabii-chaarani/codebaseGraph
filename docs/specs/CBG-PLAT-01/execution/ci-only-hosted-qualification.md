# CBG-PLAT-01 — CI-only hosted verification at d6dd344

## Outcome

The approved CI-only fixes **COD-57, COD-58 and COD-59 satisfy their affected native checks** at the delivered source below. The independently verified logs show named test execution, not only green job status. These three implementation issues can close; the broader COD-51 approval gate, other maintenance fixes, COD-61 clean-consumer qualification and milestone remain open.

## Exact scope and source

- Approved implementation: S06/COD-57/R06/A10, S07/COD-58/R07/A11 and S08/COD-59/R08/A12 only, under [spec0.10 at full SHA1a9db7b558b60248888408daa6268a3b45f2f0df](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md) and the [explicit user approval](../approvals/ci-only-s06-s08.md).
- Source baseline: `862a8f88d485d5bd257ebd9456c194156fd4ee94`. Fix source: [`d6dd34493e9da6f80847240bafcc92dbcaed2c38`](https://github.com/rabii-chaarani/codebaseGraph/commit/d6dd34493e9da6f80847240bafcc92dbcaed2c38); existing [PR153](https://github.com/rabii-chaarani/codebaseGraph/pull/153) remains open/unmerged and its head was rechecked equal to this fix SHA when CI completed.
- Independent focused code review and44 selected local macOS tests were already recorded in [local execution](ci-only-s06-s08.md). All five reviewed source hashes still match the delivered source. No further implementation changes were made in response to this verification.

## Completed hosted CI — actual success, not configured or skipped

[Run 40426019260, attempt1](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260), event `pull_request`, is completed **success**, with GitHub `headSha=d6dd34493e9da6f80847240bafcc92dbcaed2c38`. Created `2026-10-27T00:22:51Z`; completed/updated `2026-10-27T01:42:13Z`.

**Actual hosted checkout:** synthetic PR merge `af71103436086bff1189e67e0f5f03464e977bd4`, merging `d6dd34493e9da6f80847240bafcc92dbcaed2c38` into `ca115690b226384164cf364458a0e3255b8908d3`; not a bare-head build. Every retrieved relevant job records the full merge ref and checkout commit. A read-only [head-to-tested-merge comparison](ci-only-evidence/head-vs-successful-ci-merge.json) shows **only `CHANGELOG.md` differs** from the delivered head, so the implementation/tests/workflow files match the reviewed code. Preserve that distinction rather than attributing this to an isolated-head build.

All **22 jobs** completed successfully:

| Job | Result / exact log |
|---|---|
| cargo clippy | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860390) |
| cargo audit | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860543) |
| native (macos-arm64) / native tests (macos-arm64, integration) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860630) |
| cargo publish dry-run | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860656) |
| cargo fmt | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860705) |
| native (windows-x86_64) / native tests (windows-x86_64, integration, shard 2) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860919) |
| native (windows-x86_64) / native tests (windows-x86_64, unit, shard 4) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860936) |
| native (macos-arm64) / native tests (macos-arm64, unit) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860962) |
| native (windows-x86_64) / native tests (windows-x86_64, integration, shard 3) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861031) |
| native (macos-arm64) / native tests (macos-arm64, generation) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861045) |
| native (windows-x86_64) / native tests (windows-x86_64, generation, shard 3) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861091) |
| native (windows-x86_64) / native tests (windows-x86_64, generation, shard 1) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861092) |
| native (windows-x86_64) / native tests (windows-x86_64, generation, shard 4) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861146) |
| native (windows-x86_64) / native tests (windows-x86_64, unit, shard 1) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861177) |
| native (windows-x86_64) / native tests (windows-x86_64, integration, shard 1) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861193) |
| native (windows-x86_64) / native tests (windows-x86_64, generation, shard 2) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861212) |
| native (windows-x86_64) / native tests (windows-x86_64, unit, shard 3) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861239) |
| native (windows-x86_64) / native tests (windows-x86_64, unit, shard 2) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861245) |
| native (windows-x86_64) / native tests (windows-x86_64, integration, shard 4) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861293) |
| native (macos-arm64) / native artifact (macos-arm64) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121950333792) |
| native (windows-x86_64) / native artifact (windows-x86_64) | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121951853727) |
| required | [success](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121958592900) |

The macOS and Windows artifact build-and-smoke jobs **executed and passed**; they were not skipped as in the old failed run. The final `required` aggregate passed. Cohort discovery, native CLI capture, inventories and artifact policy were not weakened to obtain this result.

## Case-level verification

The saved [case verification](ci-only-evidence/hosted-case-verification.md) and bounded [case/command excerpts](ci-only-evidence/hosted-case-excerpts.txt) distinguish actual named test execution from job-level success. Source checkout/merge, full command filters, nonzero harness summaries and per-native compiler-captured CLI hashes are retained there. Actual `test ... ok` was verified for:

| Acceptance | Actual current execution |
|---|---|
| A10 / COD-57 | Watch-help contract passed macOS unit121931860962 and Windows unit shard4/job121931860936. |
| A11 / COD-58 | Production-rendered COPY metadata fixture passed Windows unit shard1/job121931861177; the unchanged4-byte/physical-path oracle passed locally on Mac and the native Mac unit job passed. |
| A12 / COD-59 | Real first-creation/followup publication-birth test passed Windows integration shard4/job121931861293; new owned actual-contention/release regression passed integration shard2/job121931860919; missing-serial-lock propagation passed integration shard1/job121931861193. |

No requested testcase was merely discovered/filtered out. The Windows-only case actually ran and its enclosing storage-maintenance harness reported4 passed/0 failed/0 ignored. The existing five-second timeout/error branch and actual epoch/birth checks remain unchanged and reviewed; no new permanent-contention exhaustion test was executed or claimed. The controlled test also reaches the unchanged invalid-envelope rejection after release. Job retrieval hashes and exact extracts are in [retrieval-manifest.json](ci-only-evidence/retrieval-manifest.json); raw full logs remain at their GitHub job URLs and in the local retrieval directory, while the bounded exact excerpts are Git-tracked here.

## Closure limits

This evidence resolves the CI-only fixes' affected native checks. It does **not** implement or qualify the outstanding runtime packaging, directory-cursor, discovery, lock-mode or Cargo-default work; it does not fulfill S10/COD-61 clean-consumer/OS-floor/PowerShell/source-install qualification or close the maintenance milestone. Windows Server native CI is not Windows11 desktop consumer evidence. Hosted macOS latest is not macOS13.3 minimum testing. An artifact smoke on a prepared build host does not establish clean installed consumer support.

Do not reopen already settled policy decisions or treat these passing jobs as approval of additional implementation scope. COD-51 remains the gate for the other slices. No merge, deployment, service restart or manual CI rerun was performed. Existing five-second observer deadline,10-ms retry, epoch/birth/integrity assertions and production constants remain unchanged.
