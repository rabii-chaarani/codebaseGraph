# CBG-RESTORE-01 Windows reset diagnostic brief

This is a bounded verification-method supplement under unchanged approved revision0.1 (d809fe0f8289bbf7d167a891bd7c31160f27f0e1), REST-R03/R05/R10/R11 and REST-A03/A06/A10/A12. It adds no public behavior, architecture claim, retry authority, platform waiver or user approval.

## Observed failure and authority

Published5418473ef77f64e613884f653bc5d2f246c4017b, CI841/run44082807085, Windows integration shard2/job134037786360 failed lifecycle_faults::owner_exit_cleans_paused_worker_and_loader_before_recovery at tests/refresh_recovery_process.rs256. A fresh HTTP graph_health invocation id71 was transmitted to the coordinator; post-send reading returned Windows10054. Four tests passed, one failed, one subprocess helper was ignored. The captured metadata/platform log does not identify a server-side close reason. Artifact14329586675, SHA25631f1c17dc317a08cb27c5532d712054b77796e0d78ec0d163e9241abaad0a51f, contains only platform logs/inventories/reports, not retained failed-fixture state.

The no-replay transport machinery is materially unchanged from frozen main. The HTTP path used no deadline and had many earlier calls, so neither initial HTTP readiness nor deadline expiration is an established cause. Coordinator owner close/shutdown, admission/spawn/clone failure, worker panic/response loss and response-writer failure remain hypotheses. Existing ambiguous-disconnect testing demonstrates the correct nonretryable mapping, not the CI cause. Do not claim the failure is infrastructure, fixed, or safely retryable.

Applicable existing Graph Coordinator serving/routing/deadline/no-replay intent and the restoration failure/preservation contract remain unchanged. No new responsibility or Scryer fold is needed for test-only observation.

## Exact owner and permitted change

One complete technical owner remains /root/restoration_implementation, default/Terra/medium/fork-none. Root owns Git/spec/Linear/delivery. The technical owner may write ONLY these already approved paths for this diagnostic slice:
- src/coordinator.rs
- src/coordinator/tests.rs
- tests/refresh_recovery_process.rs
- tests/support/lifecycle_faults.rs

No new file/module, MCP dispatcher or shared type/manifest/lock/config/public protocol ownership is granted. Request an explicit reassignment before any unlisted write. Preserve other edits and all prior evidence. No nested agents, Git/model/wiki/tracking/live mutations.

Implement the proposed coordinator event trace only under debug_assertions AND a dedicated fixture-only environment flag. Enable it only in the two owned recovery fixtures. Keep a small fixed rolling in-memory window of static event kinds, process-local connection sequence, scalar owner/client PID and whether this process owns the coordinator. Events may cover accept/register/spawn/clone, dispatch/worker outcome, response-write outcome, stop/close and client transport-read failure.

No tokens, endpoints, request bodies, query text, raw errors/environment/state JSON or user data may be recorded. No per-request synchronous file writes. Trace operations must be best-effort/nonpanicking, preserve lock order and avoid blocking request/response processing. Do not add catching/retry/success paths to make the failure disappear.

Render once on an unexpected client transport-read failure into the fixture's existing captured diagnostic channel, with an explicit total byte cap, and verify that channel is drained or otherwise cannot introduce pipe backpressure or change child ownership/lifetime. If the proposed same-process ring is incomplete, label that fact; do not infer missing server events as an observed cause. Changing diagnostic routing beyond this bounded channel requires root review before implementation. Preserve release omission and default-off behavior.

A bounded independent-fixture repetition may be used only to observe this intermittent failure: every iteration starts a new owned fixture and fails immediately on any unexpected result. It must not retry a failed request/operation until success, relax assertions/deadlines or broaden error classification. Record the actual method and results. Do not change production behavior before a causal path is established and root issues a separate bounded fix assignment.

## Verification and handoff

Verify bounded rendering, no sensitive payload capture, default-off and release omission, and unchanged single-submission/nonretryable ambiguous-disconnect behavior. Run existing affected coordinator/connection/shutdown/deadline tests, both owned native recovery cases, formatting and strict applicable Clippy. Retain source/command/toolchain/native/binary/runtime/JUnit identities and actual failures before target reuse. Check df before large builds; cargo-reclaim and shared/frozen targets remain unchanged.

Freeze the exact patch and quiesce before root review/commit. Fresh independent Sol/xhigh focused review must inspect the trace and its effects; independent Terra execution checks the affected behavior. Publish a new immutable CI snapshot only after those checks. The next Windows observation must preserve actual assertions and failure evidence; a trace-enabled green alone does not establish a cause or erase the earlier failed run. Final native/artifact/audit gates and reviewed publication remain mandatory.
