# CBG-RESTORE-01 Windows diagnostic execution checkpoint

This supplements [implementation-evidence.md](implementation-evidence.md) and the committed [diagnostic brief](windows-reset-verification-brief.md). The approved restoration/CI/security contracts and their exact human decisions remain unchanged. This is verification work under REST-R05/R11, not a new public contract or a claimed Windows behavior fix.

## Failed native observation

Published 5418473ef77f64e613884f653bc5d2f246c4017b failed [CI841/run44082807085](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/44082807085): Windows integration shard2/job134037786360, lifecycle_faults::owner_exit_cleans_paused_worker_and_loader_before_recovery, tests/refresh_recovery_process.rs256. A post-send graph_health read returned nonretryable coordinator_transport_error/Windows 10054. That session's request IDs increment from 2, and this response was 71; HTTP initialization alone does not establish coordinator readiness or ownership. No deadline was supplied, and the recorded failure does not identify a server-side close reason.

Twenty-two jobs passed, including every macOS native test and the macOS artifact. Eleven other Windows test shards passed. Windows artifact was skipped and the required aggregate failed. These results are not final native qualification.

The retained job log has SHA256 637297e1dc8c5ec9dcee450431d72cc7fc917fca8bf7815ec598adbf89709729. Artifact14329586675 has SHA256 31f1c17dc317a08cb27c5532d712054b77796e0d78ec0d163e9241abaad0a51f; its 13 entries contain source SHA/platform logs/inventories/reports, not failed-fixture payload. The reset is not classified as infrastructure and has not been retried away.

## Reviewed observation method

Source **ebd93fb6ef5d972a027b7deffd409ee8b56f1ee7**, tree **f56bfcd64a59a3a9af2daa427c1a2885d88952ec**, adds only the bounded diagnostic slice after the restoration product. Its predecessor a19f75b carried the initial trace; ebd corrects the independent review's error-classification finding.

The coordinator trace requires debug_assertions and an exact fixture-only opt-in. It retains up to 32 events drawn from fixed kinds with process-local connection numbers and scalar PID/ownership observations, using try_lock. Its one-shot ASCII render is capped at 2,048 bytes and a detached writer captures only those bytes. It does not retain owner/client/native-lease objects or alter their lifetime.

Only the two owned recovery fixtures enable the flag. Their stderr is continuously drained; only strictly parsed CGR_COORD_TRACE records are retained. Pending lines are capped at 128 bytes and the tail at 8,192 bytes. Other bytes are discarded with an explicit nontrace_output_present indication, not reported as an empty stream. Untraced fixtures keep their original capture path.

No token, endpoint, payload, raw environment/state or arbitrary error text enters the trace. No request retry, assertion/deadline relaxation, new protocol, public option or production persistence is introduced. In-process owner-thread observation is expected in these unique fixtures; PID match is advisory and absence/mismatch is explicitly incomplete, never deletion or replay authority.

The original ApiError/deadline classification is now constructed once before diagnostic observation and returned unchanged. This fixes the reviewed diagnostic-order defect; it does not explain CI841, whose request had no deadline. All earlier no-replay and execution-permit behavior remains required.

Detached output can be lost when a child is terminated immediately. Missing events, external ownership and try_lock contention cannot establish the absence of a server event or a cause. A trace-enabled pass alone does not establish that the earlier reset was fixed.

## Independent execution

Fresh independent default/Terra/medium/fork-none execution checked the exact clean ebd source. Root was actually recompiled in the verifier's owned target with Rust 1.98.1, the same verified Ladybug 0.19/native019/OpenSSL inputs, CARGO_INCREMENTAL=0, debug assertions retained and workspace all features.

- Formatting and strict workspace/all-target/all-feature Clippy passed.
- Full nextest workspace: **885 passed, 0 failed, two ignored subprocess helpers**, 589.686s, four test threads/no-fail-fast.
- Default-off/exact opt-in, ring bounds, one-shot/no-payload, contention, strict filtering, pipe-drain backpressure, ambiguous nonretryable/no-replay and deadline checks passed. Both trace-enabled recovery fixtures passed on macOS.
- An actual unwrapped Cargo release-library check passed. Filtered compiler-artifact fields show a fresh root build, opt_level=3, debug_assertions=false, test=false. This checks release omission, not Windows or shipped-runtime qualification.

The private archive's implementation/independent-final/ebd-verification-report.json records commands/source/toolchain/environment. Actual ebd-junit-actual.xml SHA256 is **555c4c54d9deebd0a3fa82ca9a17a9a3f397154a2fdfaef3735f70d523f6cb1d**. ebd-executables.json inventories 86 preserved executables/dylibs with hashes/distinct inodes; ebd-release-artifact.jsonl retains only selected Cargo profile fields. Source checkout was /private/tmp/codebasegraph-restore-v304-trace-order-review-20261117; build target was /private/tmp/codebasegraph-restore-v304-finalverify-20261117-5c2c12c/target, owned serially by the same verifier.

A fresh independent prompt-only default/Sol/xhigh/fork-none reviewer used adversarial-review-engineer. It reviewed all four diagnostic files and the existing caller/teardown paths, found the two-deadline-sample ordering defect, then focused verification passed ebd after the immutable pre-observation classification correction. No confirmed P0–P3 remains in the diagnostic slice. The reviewer executed no tests or builds.

## Discovery and retained attempts

Graph MCP remains unavailable and the live graph is not repaired or bypassed. Frozen-main read-only coordinator dependency/change-impact/callgraph receipts provide bounded orientation. The previously unindexed src/coordinator/tests.rs was copied opaquely from a19 and indexed alone in an owned supplemental graph using the previously verified frozen CLI. Its File:4b3c782accca03862d3c content hash cb5460fffb4bedb62acfa362665ba0e91b6931e7def0bfb5eb3aca0ad21f3380 matches the reviewed source. The one-file graph and receipt remain in implementation/trace-test-graph; no current/live graph or installed binary changed.

The first synthetic trace test used an invalid fixture token and failed 4/5; the corrected 5/5 result and original failure/binary are retained. Earlier whole-stderr drain verification remains intermediate evidence; final verification uses trace-only redaction. A verbose metadata-check attempt was interrupted with exit 130 before qualification; only its newly created uninspected potentially raw-environment log was removed. Its method metadata remains. A separate workspace-wrapper probe produced an LLVM/objcopy warning and is not qualification; no global toolchain/config/environment changed. The plain owner and independent release checks passed without that wrapper.

All original preservation, approved identities, source/WIP/refs and frozen execution evidence remain intact. The original bundle is unchanged; an additional published-D bundle (SHA256 ed0aabbb2fa91f868f01092ea98c0de98384a590bae6cf86f2b66427ec738568) was restored by the coordinator to a separate private Git repository and fsck passed. Its native-D status remains failed, not qualified.

## Remaining delivery gate

Publish the reviewed diagnostic snapshot with its exact source/input rebind and observe required native Windows/macOS CI and artifacts, audit and repository/package checks. Preserve the first unexpected failure; do not relabel it or replay until success. Any behavior fix requires an established causal path and bounded assignment under the unchanged restoration contract.

The Windows reset cause remains unknown. Final qualification and the restoration PR handoff remain pending. C4 companions, including COD-25 WIP and COD-24 sequencing, have not advanced; merge, release, installation and live-store/client changes remain outside this work.
