# COD-52 / COD-98 — final hosted-evidence review addendum

## Outcome

**No confirmed P0–P3 defect remains in the reviewed scope. Hosted verification now passes on both configured native targets.** The previous Windows-CI-pending limit is resolved for this exact run, not for all consumer environments or the whole platform programme.

Continuation of the independent read-only **GPT-6.1 Sol / Max** review using `adversarial-review-engineer`; actual launcher settings remain attested as `openai-codex/gpt-6.1-sol`, `thinking="max"`. COD-98 source review is in `final-review.md`. COD-52 source correctness/F1 resolution is attributed to its separate fresh reviewer in `/private/tmp/codebasegraph-cod52-evidence/final-review-followup.md`; this addendum reviews hosted evidence, not a new COD-52 source implementation.

## Exact revision and evidence checks

- PR157 head: `5e3e42f48a55581ef75b25ba21732913089f664d`.
- Actual tested synthetic merge: `f61579940edfa3f3bf67ac6b2e7abbf04ea37cfb`. The parent attests an empty head/merge tree diff; `case-evidence.json` records that result. No Git operation was run by this reviewer.
- [Run40457158678](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40457158678), attempt1, completed **success**. Parsed `run-final.json`/`run-api-current.json`: **22/22 jobs completed successfully**, including `required`, both native artifact jobs, generation/unit/integration cohorts, Clippy, format, audit and publish dry-run. Job status alone was not used as proof of the required cases.
- Cross-checked all **32 extracted case records across20 case entries** and all **30 source/CLI provenance records** against their exact raw-job-log lines. Re-hashed the four bounded source files after the run; they match the frozen review/provenance:
  - `src/db_writer/phase.rs`: `889b005587d1ce59a63ae35fdcbb1d799ae5248cc910e65cb102e23b80ffa9fa`.
  - `tests/copy_phase_process.rs`: `bd6d263acba7364a2846b9cb52a669a5281026adadd7b347689e3a5bc0fdbf94`.
  - `src/storage/locks.rs`: `3334713035db60f24a86c34e3fbdaed0724943fcbdf96103c7bf1a39e163f25c`.
  - `src/storage/maintenance_candidate.rs`: `8e539db80a81de8fb674270adaf608a551954db86352b3e87ad674d6badac765`.

## Named executed results

| Required behavior | Raw hosted evidence |
| --- | --- |
| COPY chunk/reopen/PK/connectors/search/control, Windows | Job122033480404, lines882–889: actual `copy_phase_process` binary, one running case, named chunk case, `PRIMARY_KEY_SCAN_NODE_TABLE` plan, standalone `ok`, then **1 passed /0 failed /0 ignored /4 filtered** in66.13s. No intervening case can account for that `ok`. |
| All five COPY process cases, Mac | Job122033480554, lines697–708: actual test binary, named chunk/PK-plan/completed `ok`, jumbo, Managed+Direct acknowledgement preservation/recovery, both identity-oracle cases; **5 passed /0 failed /0 ignored /0 filtered** in291.54s. |
| Remaining COPY cases, Windows | Job122033480543, lines878–885: jumbo, Managed+Direct acknowledgement preservation/recovery and malformed-evidence oracle **3 passed /0 failed**; job122033480738, lines880–885: reused-PID/distinct-instance and same-instance rejection oracle **1 passed /0 failed**. Combined with shard3, all five cases actually execute. |
| Singleton stable across calls/threads | Explicit `... ok`: Mac unit job122033480377:1037; Windows unit shard4 job122033480628:961. |
| COD-52 nested scans / corrected Windows fixture | Mac job122033480377:1245 and integration122033480554:883; Windows jobs122033480578:892,122033480686:1081 and122033480543:947 all explicitly pass the named case. |
| COD-52 pinned-root restart and real late receipt selection | Mac unit job122033480377:1242 and:1316 explicitly pass both named cases; pinned-root restart also passes in integration122033480554:880. These are macOS-specific regressions, not missing Windows cases. |

The source log headers identify macOS26.6.2/25G83 ARM64 and Windows Server2022/10.0.20348 X64, not a clean desktop matrix. They show the typed `xtask native-test` route and optimized **`native-tests`** profile, with bundled Windows extensions. The debug-only singleton/oracle cases themselves execute, so an ordinary release build did not silently omit them. Logs name compilation from each checked-out source and the captured compiler-produced CLI hashes. Relevant integration CLI hashes: Mac `13f3ad4f8b6ec515294af0cf03485320f61f18cac6b728f1a337f190a9c03339`; Windows chunk shard3 `fe204d9ec44bde89e951ae21175fddd719a2d06b12b458efaf52c7cd2043987b`, preservation shard1 `2bfbf62d1e870350f170549b26781b21d387d9f861f452b276a58c7ad1f627e5`, oracle shard4 `9af42b1bb8685eb6edce0b83ea0d0429458835ae228bd3dd6bc5d1f45039d1cb`. These are logged hosted hashes; binaries were not independently downloaded/re-hashed here.

## Limits preserved

- Windows emits existing dead-code/unused-import warnings (`home_dir`, `workspace_fifo_pin_name`, `remove_partial_state_tree`, `copy_dir_all`, `materialize_err`). Every distinct current warning message also occurs in the retained earlier failed Windows artifact. Coordinates are outside the four changed files; no warning attributable to this changed source appears. **Do not describe Windows as warning-free.** Existing warning cleanup and any programme-wide zero-warning gate remain separate; this patch introduces no warning finding.
- No clean Windows desktop, minimum macOS13.3, Rust1.82 execution, deployment or broad supported-consumer qualification is established. Static MSRV/API-age conclusions retain their earlier limits.
- The earlier broad local1,110-case debug run remains **incomplete**, with606 pass records and the captured-shipping-CLI fixture prerequisite failure; the later hosted success does not turn that interrupted run into a local pass.
- The old55-PID/56-event failure remains preserved. Without its raw phase trace, specific PID reuse is still unproven; this successful fixed oracle does not retrospectively establish the old run's cause.
- Source-model state remains an older-root limitation, not SHA-bound evidence for this worktree. This narrow fix restores existing claims; no new claim/topology is invented and no model/ledger mutation was performed by the reviewer.

## Closure recommendation

The source-review and configured hosted-execution evidence supports **only COD-52 R01/A01–A02/S01 and COD-98's approved process-identity verification fix**, subject to the coordinator's canonical evidence, ledger and Linear reconciliation. Leave the rest of COD-51, COD-61, milestone/aggregate ownership and consumer/minimum-platform qualification open. This is **not merge, deployment, service-restart or aggregate-closure approval**. A separate evidence-only stacked PR may preserve these results without changing the tested PR157 implementation head; this review neither creates nor approves that PR.

Evidence files: `hosted-run-40457158678/case-evidence.json`, `case-excerpts.txt`, `run-final.json`, `run-api-current.json` and cited raw `job-*.log`; retain the earlier local/failure reports alongside them. Recalled memory and governing Scryer closure/evidence rules. No further source reading, builds, tests, Git operations or external mutations were performed for this addendum. Only this requested report was written. No new reusable-memory claim is warranted by transient delivery evidence; no source constants were changed.
