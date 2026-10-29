# COD-109 / PR160 independent final review

## Verdict

**No confirmed P0–P3 defect in the reviewed integrated source and bounded evidence changes.** Proceed within the approved test-only repair scope to Git delivery and fresh final-head hosted CI. This is not a hosted-CI, merge, main-CI, supported-consumer, release or deployment verdict.

The source trace closes the demonstrated first-owner fixture timing gap without relaxing the strict closure oracle. Independent local execution passes the five approved checks, formatting and strict Clippy. The original CI pending-intent transition and local guardian EPERM origin remain unproven; this review does not turn those separate historical failures into one established runtime root cause.

## Exact reviewed snapshot and authority

- Checkout: `/tmp/codebasegraph-pr160/worktree`, not the dirty primary checkout.
- Integrated starting/current HEAD: `db35f8b8376f86c49a871f139dcf92f26fce24ed`, tree `64109b33e96725b44c54f0a634d0351820139045`; independently checked parents `12fdbb28f7b44185b8de695694c02d734555caff` (original PR) and `5889ed09954a4a07efd2590dd1bd380ee77be06a` (actual main/comparison baseline).
- Tested/reviewed source is **HEAD plus then-uncommitted** `tests/coordinator_process.rs`, SHA256 `d3dd06523d1ebc4250820770bb99a3c03a664e99a2870d4c66edf9351a80657a`. HEAD/tree alone does not identify those changed bytes.
- Exact proposal SHA256 `c46da1e8725d989a6b77e968faf9061b3c0a59adec13c4e13c3c379d134150f2`; independently rehashed and read final proposal-review resolution. Existing worker-recovery contract and Scryer proportional ledger `chg-n2nnjh` are reused; no new intended architecture/claim or changed requirement is inferred.
- Final documentation/evidence freeze: `/tmp/codebasegraph-pr160/docs-freeze.json`, SHA256 `dedf4e9db1746164131156eb03fb46423b0366deaa4729ec15ac7ef7a47e7f77`, identifies21 files. All21 independently rehashed and still match. Canonical repair record SHA256 `f8b7684a7b9ea79a5468968a283fff0337cdd523a3a3bfba88af77949044754e`. The earlier ten PR160 historical-evidence changes were also reviewed against actual main.
- Required/requested profile: fresh independent `openai-codex/gpt-6.1-sol`, Max effort, default-role equivalent, no inherited turns. Parent verifies actual spawn model `openai-codex/gpt-6.1-sol`, accepted `thinking=max` and fresh-session mapping; this session does not independently expose runtime settings.

Read required `codebasegraph-final-reviewer` and `adversarial-review-engineer` skills and engineering lenses. Recalled wiki memory, read `knowledge:architecture/worker-recovery-regression-contract`, and fetched Scryer orientation and governing proportionality/ledger/evidence/closure rules. Explicit semantic graph discovery/callgraph context preceded source reads. Graph/model remains older same-repository orientation, not exact-worktree proof. The user-authorized bounded reference-following exception was used only for the four named maintenance modules. Linear search exposed no usable live issue-search tool to this reviewer; no tracking update is claimed.

## Confirmed findings

None. No correction/re-review cycle is required for a confirmed defect in this snapshot.

## Source and failure-path checks

- **Actual exclusion through abrupt loss:** `tests/coordinator_process.rs:198–218` acquires real noncreating worker/writer observations once per attempt, then invokes the lock-held strict oracle with an empty preexisting map. Every inspected root's existing exclusive owner guard is retained through unchanged `kill_pid` → actual `Child::kill` plus `wait`. Root guards drop immediately after child wait returns, then writer/worker, before replacement election and the later independent acquisition.
- **No recovery starvation from a failed attempt:** a writer-lock failure drops the partially acquired worker handle; a strict inspection error drops its accumulated root vector; propagation out of the attempt closure drops both globals before polling. No lock-taking oracle wrapper is called recursively. All acquisition is nonblocking.
- **Independent control-write exclusion:** owner publication capability requires the actual root owner lease Exclusive (`maintenance_controls.rs:1685–1705`); terminal maintenance acquires that same owner (`2741–2759`, lock method2552–2570), as does root-removal replay (`maintenance_workspace_gate.rs:972–990`). Thus retained existing root guards exclude those verified mutations through the kill; no startup/maintenance lock was invented.
- **Strict semantics unchanged:** independently extracted and compared the old validator body to the final lock-held body. After only owner-vector initialization, per-root guard retention and tuple-return normalization, the bodies match byte-for-byte. All predicates, order and diagnostic branches remain. Higher valid intent rejects at609–618 before the existing same-incarnation/positive-empty-receipt exception635–672. An empty pre-kill map cannot use that exception. `observation_lock`, physical identity and actual-worker-state helpers are unchanged.
- **Coverage preserved:** all resource thresholds, edited publication/search assertions, abrupt first kill, replacement PID, later independent refresh, locked kill-all, physical-incarnation checks and strict final closure remain. No runtime, manifest, configuration, dedicated active-fault fixture or existing timeout constant changed from the integrated starting head. `git diff --check` passed. The real-file helper regression539–562 exercises worker contention, writer contention/partial-worker release, returned guard exclusion and release; it does not pretend to replay historical control bytes or active-build death.

## Independent executed-evidence review

Reviewed verifier-executed evidence, not tests run by this reviewer: original final verification report, compiler JSON/stderr, capture/commands/results, nextest config/log, JUnit, formatting and Clippy logs.

Compiler events show the normal `codebase-graph@4.0.3` CLI and both test executables rebuilt (`fresh=false`) from the isolated source. Existing `native-tests` uses optimization3 with debug assertions and overflow checks. Unsupported public `--version` output is not treated as version proof. Private captured CLI SHA256: `cfa18cfce96d1f85b385e1c413d132eefe513b180955cda9d2f62eef49118edb`.

Actual nextest run `5e9de0dd-43db-4d51-92c9-1e3258446e06`, serial1/retries0/fail-fast:

| Named check | Actual result |
| --- | --- |
| `quiescence_observation_requires_and_holds_both_execution_leases` | PASS0.013s |
| `twenty_mcp_clients_share_one_coordinator_worker_and_take_over` | PASS46.000s |
| `lifecycle_faults::authenticated_cancellation_cleans_loader_and_retries_pending_edit` | PASS103.343s |
| `lifecycle_faults::owner_exit_cleans_paused_worker_and_loader_before_recovery` | PASS99.352s |
| `lifecycle_faults::unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven` | PASS105.239s |

All five passed on first execution;353.952s reported.22 other cases filtered, not executed. Resource observations below unchanged bounds: follower p95=12,402,688; owner idle=18,579,456; owner build growth=933,888; worker peak=162,807,808 bytes. `cargo fmt --all -- --check` passed exit0. `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` passed exit0/no warnings,34.414s wall; finished output was inspected.

Independently verified before/after maps equal across410 recorded inputs and all current recorded file hashes still match. Rehashed source manifest, commands, JUnit and private CLI against results; all match. Source manifest SHA256 `3e8e702e298f4a2159d43d3d9e35576189f97deeea82ef1892f457214bdebdfd`; JUnit SHA256 `9b6424c26465df13c837cd2f1d42cc695308e467003b7709f4ea3f9f62d4c8fe`.

## Final evidence/documentation check

Read final repair record and all frozen public evidence. Nineteen files that have local report/evidence originals are byte-for-byte copies, including the original baseline/verification/proposal reports, failed-before log and final JUnit/results. Remaining authored repair/source-identity records agree with those originals. All repair-record relative links resolve. Public snapshots retain digests/sequence facts and bounded public receipt fields, not raw control capabilities; no private executable was added.

The repair record correctly distinguishes uncommitted tested source from integrated HEAD/tree, five selected passes from22 filtered cases, local native evidence from hosted qualification, and original pending intent from local false-empty/EPERM. Historical reports' planning/status language is explicitly preserved as history. Final-review status is pending in that pre-report freeze; attaching this report and mechanically recording its result is the coordinator/Production handoff, not a source change or a circular self-hash claim.

Earlier PR160 COD-52/COD-98 documents remain bound to old implementation head `5e3e42f48a55581ef75b25ba21732913089f664d` / tested merge `f61579940edfa3f3bf67ac6b2e7abbf04ea37cfb`, not this repair. Retained run metadata shows22 successful jobs and explicit named test records. Those documents preserve incomplete local-run, preexisting Windows-warning and consumer-platform limits. Their success is not substituted for COD-109 final-head CI or a warning-free historical Windows claim. This was a bounded evidence consistency review, not new execution or a complete re-audit of the historical runtime changes.

## Conditional risks and limits

- The new pre-kill wait shares the remaining existing search deadline (`tests/coordinator_process.rs:170,209`), not a fresh ten-second cleanup window. A platform where searchable publication consumes that budget while terminal completion is still pending could fail this fixture. No actual corrected failure or supported-time guarantee establishes that condition; local execution passed. Treat as conditional timing sensitivity, not a confirmed defect or grounds to change approved constants now. A surviving failure requires diagnosis, not retries to green or a weaker oracle.
- Original CI's exact higher-intent write transition and local EPERM origin are untraced. Ready health, `Ok` results, absent PIDs and acquired post-hoc leases are not emptiness proof. The fix removes the demonstrated unsynchronized first-kill interval; it is not proof of all possible historical causes.
- Local macOS26.6.2 arm64/Rust1.96 evidence only. No full suite, current Windows, clean minimum/current consumer environment or release claim. Synchronized takeover is not active-build-loss evidence; the unchanged three active/failure tests supply separate exercised coverage.
- Exact-model Scryer ingestion/tracking reconciliation, fresh final-head hosted CI, actual server-established no-bypass merge with expected head, and main CI for the actual merge SHA remain coordinator/Production gates. API plans are not live merge evidence. No release, deployment or service restart is authorized by this review.

No constants changed by this reviewer; the implementation preserves all existing constants. Reusable knowledge was explicitly assessed: publication/search is not teardown, already captured in the existing contract/memory. No new candidate is warranted, and unresolved causes must not be stored as facts. Only this report was written; no dispatch, source edit, external-record mutation, Git mutation, build or test execution by this reviewer.
