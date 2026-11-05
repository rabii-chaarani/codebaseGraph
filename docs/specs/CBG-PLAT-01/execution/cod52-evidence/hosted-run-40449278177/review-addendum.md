# COD-52 / PR157 — hosted verification review addendum

## Outcome

**The COD-52 regression cases pass on hosted macOS and Windows. The overall required run still fails.** The blocker is a different, unchanged Windows COPY process-test assertion:56 COPY trace records contain55 distinct numeric PIDs. This establishes duplicate numeric PID values in the trace, **not** a confirmed process-reuse, COPY/data-integrity or cursor-isolation defect. No new P0–P3 defect in the reviewed COD-52 diff is established by this failure.

Do not mark CI, Windows artifact qualification or COD-52 closure green. The local1110-test run remains in progress per coordinator; no full local pass is claimed here.

## Reviewer / revision / method

- Continuation of the fresh GPT-6.1 Sol Max read-only adversarial final review; actual execution settings were attested by the coordinator (`openai-codex/gpt-6.1-sol`, `thinking="max"`). Same required skill and source-review scope apply.
- PR157 head: `d2a442faaf4ca59dc5fe33f082b2072f49b97aa5`; baseline: `7038e66ff8271ffddc511f879cf8d28353a09b2d`.
- Hosted run: [40449278177](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40449278177), actual tested synthetic merge `f633445077d1c81888eb44f7a496927f54289ad7`. Coordinator evidence reports an empty head-versus-merge tree diff; this reviewer did not run Git.
- Independently reread frozen source hashes: locks `3334713035db60f24a86c34e3fbdaed0724943fcbdf96103c7bf1a39e163f25c`; candidate `8e539db80a81de8fb674270adaf608a551954db86352b3e87ad674d6badac765`. They match the reviewed frozen snapshot.
- Examined `hosted-ci-summary-final.json`, `hosted-case-records.txt`, and the bounded failed-job source log `windows-integration-failed.log`. Read graph-discovered `tests/copy_phase_process.rs` to establish the precise assertion and which checks precede/follow it. Recalled relevant repository memory only as advisory context.
- No builds, source edits, Git operations, CI reruns, external mutations, timeout increases or assertion changes were performed. Only this requested report was written.

## Actual hosted results

- Native Windows `nested_directory_scans_do_not_consume_the_outer_cursor` has explicit `... ok` records in unit shard1 job122007920956 (line892), unit shard2 job122007921364 (line1077), and even the otherwise-failing integration shard3 job122007921021 (line959). **F1 is now verified on native Windows**, not merely statically resolved.
- Native macOS unit job122007921553 explicitly passes the nested scan, interrupted/replaced-root retry and production receipt-selection regression (records1242/1245/1316). The job concludes success. Coordinator reports the complete macOS unit cohort as843 passed,2 ignored; that total is supplied by the coordinator rather than present in the narrow case extracts reviewed here.
- macOS generation/integration/artifact jobs succeed. All Windows unit/generation jobs and integration shards1/2/4 succeed. Integration shard3 fails as below.
- Run conclusion **failure**; `required` job122024829779 **failure**; Windows artifact job122024830234 **skipped**, not passing. Other green jobs do not override those outcomes.

## Confirmed failure — numeric-PID uniqueness assertion

Failure: `bounded_production_chunks_reopen_with_primary_keys_connectors_and_search`, `tests/copy_phase_process.rs:417–424`, Windows integration shard3 ([job122007921021](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40449278177/job/122007921021)).

Source trace:

1. Build a real isolated Direct graph with1400 functions, spill setting1MiB and debug trace output (lines338–355).
2. Filter trace records to `phase_kind == "copy"` (356–359).
3. Verify per-phase thread count, chunk sizes/paths, repeated node-table rotations, early primary-key-index ordering and no index phases after connectors (360–416).
4. Compare the cardinality of the set of `event["pid"].as_u64()` against COPY-event count (417–424).

Hosted log885–888 shows the assertion's **left55, right56**, after `assert_success(&built)` and preceding chunk/order checks. Therefore at least two COPY records share a numeric PID. The failure is not a native COPY error response, missing table, connector mismatch, search failure or directory-traversal assertion.

**Important execution boundary:** database reopen,1400-function/key lookup checks, connector equality, search and clean-control comparison occur only after line426 and were not reached in this failed Windows case. A successful build and earlier trace checks do not establish those downstream checks passed.

The coordinator verified empty baseline diffs for this test, `src/db_writer/phase.rs` and the staging renderer; the reviewed COD-52 source patch changes only the macOS production traversal plus rooted tests and macOS-only receipt-test code. Windows product traversal/COPY/staging behavior is unchanged. This failure is outside the approved R01/S01 cursor-isolation implementation path. No causal link from the COD-52 code change to duplicate PID records is established; calling it an introduced COD-52 regression would exceed the evidence. Conversely, unchanged source alone is not proof that a test can be ignored.

## Cause status — separate fact from hypothesis

- **Confirmed:**56 COPY records;55 distinct numeric PID values; failure at the unchanged cardinality assertion. The specific repeated value and per-record process-lifetime identities are not in the supplied extracts.
- **Plausible/likely, not proved:** normal Windows reuse of a numeric PID after a previous phase process terminated. Microsoft's primary documentation states a process identifier is valid from creation until termination; it does not promise a globally unique identifier across successive process lifetimes: https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers . The test compares historical numeric values, so its oracle is stronger than that lifetime guarantee.
- **Not established:** whether the observed pair represents two separate process incarnations, duplicate trace emission, actual process reuse, or another instrumentation/control-flow cause. Full trace records and process-instance/launch evidence are required to decide. Do not label PID reuse as proven, and do not claim data integrity failed or passed from this assertion.

## Safe next decision

Keep the required-CI blocker explicit and obtain separate authorization before changing the unrelated process-test/trace contract. Preserve the failing run and, if available, its `phases.jsonl` plus launch/process-creation identity evidence for a narrow follow-up. A future fix should verify actual per-phase isolation using process incarnations/launch evidence rather than simply relaxing/removing the assertion, adding retries or widening a timeout; the exact remedy depends on that follow-up's proven cause and approved scope.

The COD-52 source/test review remains satisfied for the recorded frozen snapshot, and named Windows nested-scan execution is no longer pending. Aggregate verification/qualification and maintainer closure remain blocked by the failed required run and unfinished local full evidence. No new durable memory is recorded: this is transient delivery evidence with an unresolved failure cause.
