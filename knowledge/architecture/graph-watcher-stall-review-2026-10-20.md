---
description: Evidence-backed review of Qunys publication backlog and confirmed worker recovery defects at 6aa5ba8.
tags:
- review
- refresh
- recovery
- graph-runtime
timestamp: 2026-10-20
title: Graph watcher stall review — 2026-10-20
type: review
---
# Graph watcher stall review — 2026-10-20

## Scope and outcome

A fresh GPT-6.1 Sol reviewer at Max effort reviewed runtime revision `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`. The relevant refresh, materialization-worker, execution-pipeline and loader paths match the running Qunys v3.0.2 daemon. Managed storage differs only in an unrelated Windows cleanup branch. Reviewed source was unchanged.

The referenced Codex chat was **Start R1 performance work**, local thread `01a1565e-b0a3-7cf0-842a-00a285704c48`. Reading that chat established missing graph discovery for new compute files, but its historical MCP health response bodies were unavailable. Subsequent read-only observations show a live watcher with slow publication and writer contention. They do not establish a dead watcher at the initial report.

Scryer remains the authority for intended behavior: Repository Refresh Service `node-144` owns continuous convergence, retained pending work, reconciliation and health; Materialization Worker `node-208` owns active-build cancellation and resource reporting. This review introduces no architectural claims, product decisions or source fixes.

## Observed Qunys behavior

At 8:54–8:56 am on 2026-10-20, Australia/Darwin, health reported a native, running, live refresh task with pending dirty epochs and no error. Active generation `19355-1792451617562468000-1` had published at 8:52:36.895 am. All six new compute source/test files were represented; three still contained earlier source hashes while editing continued.

Candidate-ID allocation at 8:43:37.562 am preceded publication by **539.333 seconds**. This measures the complete candidate pipeline after writer admission, not only database loading: `src/storage/layout.rs:324-330` constructs the identifier from Unix nanoseconds, and `src/storage/managed.rs:341-357` allocates it after acquiring the writer lease. Metadata creation near publication is a different timestamp.

The active graph contained 437 files and approximately 1.57 GB of physical database data. The next automatic worker waited while the manual full build from the referenced chat held writer ownership. A later process snapshot showed that manual build spawning its native database loader.

The implementation explains this latency:

- `src/api/refresh.rs:1357,1427-1479` dispatches refresh synchronously. Reconciliation scheduling resumes when that execution returns.
- `src/execution/run.rs:35-56,120-150,172-197` reuses parsing partitions but stages the complete supported graph, builds complete search data and loads a replacement database.
- `src/api/materialization.rs:311-319` finalizes only after that pipeline; `src/storage/managed.rs:664-693` switches the active generation afterward.
- `src/storage/managed.rs:341-344` and `src/storage/locks.rs:125-126` serialize writers with a blocking exclusive lock.

The 30-second reconciliation interval schedules checks; it is not a completion deadline. A manual build can add writer contention while automatic refresh is working. The two observed recovery defects below were not established as causes of this particular Qunys delay.

## Confirmed findings

### P1 — Stale numeric PID metadata can indefinitely block automatic recovery

**Location:** `src/materialization_worker.rs:519-540,650-657`.

Worker state records a version, build identifier and numeric PID. Recovery checks whether that PID exists, waits five seconds, then fails without retiring the record. A crashed worker's PID can be reused by an unrelated long-lived process, causing every automatic retry to fail at the same gate.

A deterministic isolated fixture at the reviewed revision supplied a stale record naming a fixture-owned unrelated process. Automatic refresh became blocked. A manual full build then exited successfully and published a new generation, but the record remained and the next automatic attempt was still blocked. Removing only the deliberately stale fixture record restored running state, no pending work, equal dirty/reconciled epochs and a cleared error without another source edit. The unrelated process survived both attempts; every owned fixture process was reaped.

**Remedy and verification:** Bind recovery authority to a verified process incarnation or authenticated containment owner. Never signal a process solely because a persisted PID matches. Retain a regression that checks blocked automatic recovery, successful manual publication, another automatic attempt, safe stale-record retirement and recovery without another edit.

### P2 — Outer-worker cancellation leaves loader descendants unmanaged

**Location:** `src/materialization_worker.rs:312-375,627-645`; `src/db_writer/phase.rs:94-145,177-198`.

Cancellation and outer-worker RSS failure kill and reap the direct materialization child. That child can spawn a native database loader through the isolated database phase. The loader has no matching parent-death watchdog or build containment. The outer watchdog exits only the outer process. Consequently, an in-flight loader can outlive its owner and keep consuming resources after worker state is cleared.

This is confirmed by a complete static lifecycle trace. No new descendant fault injection was performed in this review. The loader writes a candidate and cannot independently activate it; its contribution to the observed Qunys delay is unproven.

**Remedy and verification:** Own the complete build process containment, including loaders and timing wrappers. Establish teardown before retiring evidence and admitting recovery. Pause a real loader, terminate its outer worker, and verify descendant exit, resource release, preserved active generation and successful automatic retry.

## Safeguards and remaining limits

The review verified bounded debounce, dirty-before-enqueue ordering, overflow-to-full-reconciliation behavior, periodic native/poll reconciliation, retained failed batches, paced retry deadlines, configuration reload and acknowledgement of only an attempt's captured epoch. Startup read leases are released before watching. These defeat simple lost-event, debounce-starvation and startup-read-lock explanations.

Health exposes pending work and worker identity but lacks a live phase, progress timestamp and writer-wait reason. Progress frames are emitted after materialization, and their contents are discarded by the drain. Scryer `resp-264` requires progress/resource reporting without an explicit live-streaming promise, so this is an operational limitation rather than another scored defect.

Neither the outer worker nor the database phase has an elapsed/progress timeout for an under-budget hang. No such hang was demonstrated, and no completion-latency requirement was supplied. Deadline behavior requires an explicit contract before a correction can be designed.

The version-2 guardian/containment paragraphs in the freshness and storage documentation do not describe this reviewed runtime snapshot. They cannot be counted as implemented safeguards. See the qualification in [Graph Freshness and Recovery](./graph-freshness-recovery.md).

## Executed verification and evidence

Darwin 25.6.0 arm64, Rust 1.96.0:

| Command | Executed result |
| --- | --- |
| `cargo test --offline --locked --lib api::refresh::tests -- --test-threads=1` | 18 passed, 0 failed |
| `cargo test --offline --locked --lib materialization_worker::tests -- --test-threads=1` | 7 passed, 0 failed |
| `cargo build --offline --locked --bin codebase-graph` | Passed, no reported warnings |
| Isolated stale-PID fixture against the freshly built binary | Reproduced blocked automatic recovery despite successful manual publication; fixture-only record removal restored recovery; all owned processes reaped |

Local evidence is retained under `/private/tmp/codebase-graph-watcher-review-20261020/`: `evidence-index.json`, `observation-232455.json`, `observation-232608.json`, `pid-reuse/result.json` and `pid-reuse/reproduce.py`. The fixture result SHA-256 is `c914928ec7ad794d1c034344d2aeeb2bb1206b6b53bbf1b79ad826f6420bfdf9`. Temporary evidence paths are local artifacts, not cross-platform qualification.

The investigation did not restart, rebuild or edit the incident repository. It did not run a new benchmark, load test, or descendant fault injection. Historical health payloads and a captured native phase stack were unavailable. Initial stall attribution remains limited to the observed publication backlog.

Existing active episodic memories already cover large-graph publication delay, stale worker PID recovery and escaped database descendants. This review verified their relevant current premises; no duplicate memory was created.

Address the confirmed recovery defects. For the observed delay, expose phase/writer-wait progress and measure the existing pipeline before choosing a performance change. A successful manual rebuild alone does not establish recovered continuous refresh.
