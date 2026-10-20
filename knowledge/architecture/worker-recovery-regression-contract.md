---
description: PR145 recovery requirements, stable Scryer traceability, manual-publication regressions and baseline qualification.
tags:
- contract
- graph-runtime
- recovery
- testing
timestamp: 2026-10-20
title: Worker recovery contracts and regression gates
type: contract
---
# Worker recovery contracts and regression gates

## Identity, authority and status

This contract makes the two recovery fixes in merged [PR #145](https://github.com/rabii-chaarani/codebaseGraph/pull/145) inspectable as requirements and executed regression evidence. It preserves that delivery identity and its existing Scryer nodes and claims. The accepted legacy-state policy and process-containment architecture come from that approved work.

- Current preparation: Scryer change `chg-2n8vd6`, specifications and tests only, requested on 2026-10-20.
- Target baseline: main `512b5dc197adf5590afa5d2f7b44550dfa32d8a2` (3.0.4).
- Failure baseline: `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`; its relevant recovery paths match the observed v3.0.2 daemon.
- Approval scope: the user selected main, reuse of existing recovery work, and verification against the old failure baseline. New regressions run in normal CI; no ignore or expected-failure treatment is authorized. Runtime changes, installation and live-daemon restart are excluded.
- Architecture and execution ownership: Scryer Materialization Worker `node-208`, Build Guardian `node-z6qhch`, Repository Refresh Service `node-144`, and Graph Store `node-88`. Detailed future execution changes belong to their Scryer work, rather than a second architecture in this document.

The runtime fixes already exist on main. Test/specification preparation, runtime implementation, local verification, supported-platform qualification and aggregate closure remain separate states. This task does not reopen implementation or claim that the observed older daemon has been upgraded.

## Problem and observable outcome

Legacy `worker.json` stores only a numeric PID. The old recovery path repeatedly waits for that PID even when it belongs to an unrelated long-lived process. A successful standalone manual build can publish a current graph while automatic refresh remains blocked.

The old materialization supervisor also kills only its direct worker. A native phase process can outlive that worker or its coordinator. Publication, cleanup and retry must respect the complete owned execution boundary.

The required outcomes are safe legacy-state retirement, survival of unrelated processes, termination of owned process trees, preservation of the readable committed generation during failure, and eventual automatic processing of pending and subsequent edits.

## Requirements and compatibility

| ID | Required behavior | Scryer authority |
| --- | --- | --- |
| PR145-PID-R1 | Under exclusive worker ownership, retire legacy PID-only metadata without signalling or waiting on its PID. A numeric PID is diagnostic information, not cancellation authority. | Guardian `resp-xqsh3f`; Worker `resp-261` |
| PR145-PID-R2 | After legacy-state retirement, reconcile pending changes automatically. Manual publication does not substitute for proving that continuous refresh works. | Refresh `resp-147`, `resp-420kd4`; Worker `resp-262` |
| PR145-PID-R3 | Keep the unrelated process alive during recovery, writer contention, manual publication and subsequent refreshes. Preserve writer exclusivity and committed graph readability. | Guardian `resp-xqsh3f`; Worker `resp-261`; Graph Store invariants |
| PR145-TREE-R1 | Establish process containment before authorizing execution; terminate owned workers, native phases and timing wrappers on cancellation or owner loss. | Guardian `resp-r466ww`, `resp-89p7rb`; Worker `resp-4wjy8k` |
| PR145-TREE-R2 | Terminate the owned tree on resource-budget failure and release transient resources only after verified teardown. | Guardian `resp-xj7p0a`, `resp-k0j310`; Worker `resp-263`, `resp-265`, `resp-281` |
| PR145-TREE-R3 | While teardown is unknown, retain ownership evidence and possibly live candidates, block conflicting writers/publication, and retain unsatisfied refresh work. | Guardian `resp-sad4fa`; Refresh `resp-147`; Graph Store recovery |
| PR145-TREE-R4 | After verified teardown, allow manual and automatic writers to complete. No new generation may become active while the failed execution's owned phase still runs. | Worker `resp-261`, `resp-265`; Guardian `resp-k0j310` |
| PR145-COMPAT-R1 | Preserve public CLI/MCP interfaces, graph formats, memory settings and Direct destination identity. Unsupported v1 internal helper roles require a coordinated daemon restart before database work. | Guardian `resp-tvnmjs`; existing public/storage contracts |

Legacy PID-only retirement is the explicit compatibility policy already approved in PR #145. It does not permit unauthenticated cancellation of a process or destruction of v2 evidence. Version-2 ownership recovery requires authenticated control or verified empty containment.

## Failure and recovery

Cancellation, owner disconnect, startup failure, resource failure and unexpected worker exit share the established teardown contract. Attempts remain bounded by the existing five-second teardown attempt limit. Failed or unverified teardown reports retryable `containment_unknown`, preserves evidence and pending work, and permits a later safe retry without another source edit.

Worker ownership, storage writer ownership, atomic publication and reader leases remain distinct. Keep the previous generation readable and unchanged during interrupted work. A manual build must obey the same storage admission and containment barriers as an automatic build.

Private capability material remains private; regression evidence must not copy it into durable logs. Fault injection may control only processes identified by the current fixture's readiness/ownership handshake.

## Acceptance and verification

| Criterion | Given / When / Then | Regression or evidence |
| --- | --- | --- |
| PR145-PID-A1 | Given legacy metadata naming a live unrelated fixture process and an exclusive writer lease, when refresh starts, then metadata retirement proceeds without signalling that process and no publication bypasses the lease. | Existing `legacy_worker_pid_recovers_without_signalling_and_respects_writer_ownership` |
| PR145-PID-A2 | Given successful manual publication followed by legacy PID-only metadata naming the live unrelated fixture process, and a pending edit before monitoring starts, when automatic recovery runs, then the pending symbol is searchable, refresh is ready with no pending epoch, and a later independent edit also publishes while the unrelated process survives. | New `manual_publication_does_not_prevent_later_legacy_pid_recovery` |
| PR145-TREE-A1 | Given an owned native phase at a positive readiness barrier, when its outer worker fails and a manual writer is queued, then every observed active-generation change follows the original phase's exit; manual build succeeds and automatic pending/follow-up edits converge. | New `manual_publication_waits_for_failed_phase_tree_to_exit` |
| PR145-TREE-A2 | Given contained native processes, when cancellation, owner loss or measured RSS failure occurs, then those processes exit and automatic retry remains possible. | Existing cancellation, paused-worker/phase, owner-exit and measured-memory lifecycle cases |
| PR145-TREE-A3 | Given unavailable guardian authority and unverified survivors, when recovery or manual admission runs, then evidence/candidates remain, writes stay blocked, and the prior graph is readable; proven emptiness subsequently permits retry. | Existing unavailable-guardian and standalone-owner-loss cases |
| PR145-TREE-A4 | Given the macOS timing wrapper or native Windows process job, when teardown/probing runs, then wrapper descendants remain contained and observational probes do not terminate unauthenticated jobs. | Existing timing-wrapper and Windows observational-probe cases |
| PR145-COMPAT-A1 | Given retired helper protocol names, when invoked, then they fail with restart guidance before reading requests or doing database work. | Existing retired-role and phase-protocol tests |

The PR145-PID-A2 fixture restores the original version-1 `worker.json` after manual publication and asserts the complete record is present immediately before daemon startup. This makes the failure precondition explicit even if a future manual-build path retires the metadata. Unrelated-process survival, held-writer exclusion, pending content and an independent later edit remain required. An automatic-recovery-only negative control verifies that successful manual publication cannot satisfy the daemon-recovery assertions.

Tests assert observable graph content separately from health readiness. Readiness includes readable storage, ready refresh, no pending work, equal dirty/reconciled epochs, and a cleared error. Reconciliation age may be `overdue` after an expensive query; that alone does not invalidate successful content verification.

The native phase readiness barrier precedes database opening. The new phase test proves process/containment lifetime and publication ordering at that boundary; it does not independently prove resource release during an in-flight database COPY. Existing resource, failure and platform checks remain applicable and must not be relabelled as stronger evidence.

Every regression runs by default. An assertion failure on the old revision is recorded as a demonstrated failure only if its setup/readiness conditions were reached; a missing newer test hook is not evidence of either recovery defect. Main and old-baseline evidence identify their exact source, fixture method, environment and limitations.

## Review and execution evidence

Proposal review uses a fresh GPT-6.1 Sol reviewer at Max effort. The revised plan reuses the approved legacy retirement policy and real native lifecycle fixtures. New tests will be reviewed independently against the final integrated snapshot with the same required settings.

Executed on macOS arm64 with Rust 1.96.0 against main `512b5dc` plus the tests-only extension:

| Check | Observed result |
| --- | --- |
| New regressions, serial focused run | 2 passed |
| Complete affected suite, two workers | 19 passed, 2 pre-fault setup failures; 1 existing ignored subprocess helper |
| The two failed fixtures, unchanged source, serial rerun | 2 passed |
| Combined case coverage | All 21 distinct affected cases have a passing observation; this is not a single clean full-suite run |
| Old PID baseline | The exact new PID test exited 101 after successful manual publication, retaining v1 ownership and the explicit orphan-worker error |
| Old tree baseline | A positively stopped owned native phase survived its worker, was adopted by PID 1, and remained alive through successful manual publication; all owned executions were stopped and direct children reaped afterward |
| Formatting and strict workspace Clippy | Passed, including all targets and features |
| Scryer | Both current-source JUnit reports ingested; the failed claim cleared after its executed serial rerun; existing claims/test attachments retained |
| Configured wiki | Recommended validation and links passed; the configured site was rebuilt |

The two parallel failures occurred in existing `missing_guardian_closes_the_build_tree_and_automatic_refresh_retries` and `standalone_owner_loss_preserves_candidate_while_guardian_is_unavailable` setup, before their intended faults. Both passed unchanged when rerun serially. Their initial-execution exit cause remains unproven; the reports are preserved instead of weakening assertions or changing runtime behavior.

The machine-readable evidence manifest is `docs/testing/worker-recovery-evidence-2026-10-20.json`. It records exact source/report hashes, commands and outcome boundaries. Raw local reports and drivers remain under `/private/tmp/codebase-graph-recovery-spec-evidence/`; these temporary paths are not portable qualification artifacts.

The exact PID regression uses the old binary through the test harness's runtime binary override. The old tree driver uses passive process discovery and verifies fixture ownership before fault injection because that binary lacks the newer readiness hook. The demonstrated tree failure is a stopped orphan surviving publication, not absence of a test hook.

The configured wiki connector remains bound to the original checkout of this same repository and rejects the isolated main worktree as unregistered. Contract and memory pages were authored and validated through that connector, then transferred byte-for-byte into this change. No generated projections were edited manually.

Fresh independent proposal and source reviews used GPT-6.1 Sol at Max effort. The integrated source review initially found no confirmed P0–P3 defects. A later [PR #152 comment](https://github.com/rabii-chaarani/codebaseGraph/pull/152#discussion_r4320594256) requested an explicit post-publication legacy-state precondition.

The comment's caller premise was not reproduced at `6d603f0`: standalone CLI `run_materialize` constructs `CodebaseGraphApi::new()` and the default `ApiCore` does not isolate materialization. The coordinator-owned API selects the isolated explicit worker. A validation-only binary that rejects automatic v1 recovery, while preserving manual execution, made both the original fixture and the hardened fixture fail after successful manual publication with the v1 record retained. The initial control driver had expected the original fixture to pass; that hypothesis was falsified and its failed expectation is retained with the trial results. The hardening therefore makes the boundary explicit without claiming that the original test was proven vacuous. Follow-up source hashes, executed controls and qualification are recorded in `docs/testing/worker-recovery-evidence-2026-10-20.json`.

The existing architecture model was reused from the merged PR #145 work with unchanged identities. This task adds test attachments to `resp-xqsh3f`, `resp-265` and `resp-89p7rb`; no runtime claims were newly folded. Missing historical verdicts elsewhere in that imported model remain outside this scoped qualification.

Native execution here establishes local macOS evidence. Linux and Windows qualification remains with the ordinary native CI gates and Windows `native-tests` profile; this task adds no CI exclusions. The phase-before-open and sampled-observation limits above remain applicable.

## Handoff and closure

This tests-only change closes when the specifications are reconciled with existing Scryer intent, the new main regressions and relevant existing cases pass, the old failure paths are demonstrated or precisely qualified, JUnit evidence is ingested, independent findings are resolved, and wiki validation/link/build checks pass.

No runtime claim is newly folded or marked implemented by adding tests. Existing aggregate closure ownership remains with the repository maintainers responsible for PR #145's recovery claims and native qualification.

Operational deployment remains separate: an older running daemon needs the updated binary and coordinated restart to obtain the existing fixes. New source specifications or passing main tests do not update that daemon.

Related contracts: [Graph Freshness and Recovery](./graph-freshness-recovery.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [Native Release Verification](./release-verification.md).
