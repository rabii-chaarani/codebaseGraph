---
description: Prioritized stability work with acceptance criteria for MCP serving, recovery, release gates, and real hook delivery under refresh load.
resource: repository-stability
tags:
- backlog
- hooks
- mcp
- recovery
- stability
- testing
timestamp: 2026-09-24
title: CodebaseGraph Stability Backlog
type: plan
---
# CodebaseGraph stability backlog

Status: open stabilization work, consolidated on 2026-09-23. This is a task backlog, not a claim that the listed fixes are implemented.

The initial assessment covered v1.8.1 at `885454f2`. The hook follow-up was checked against worktree `73ecd03a`; line numbers may move. Revalidate each finding when implementing it. Preserve unrelated changes on `codex/remove-semantic-enrichment-residue`.

## Required tasks

| ID | Priority | Task | Status |
| --- | --- | --- | --- |
| STAB-01 | P1 | Isolate HTTP clients and bound transport waits and queued hook reads | Implemented; acceptance evidence linked |
| STAB-02 | P1 | Restore MCP sessions correctly after daemon restart | Implemented; acceptance evidence linked |
| STAB-03 | P1 | Make Direct publication recovery safe to repeat at every rename boundary | Open |
| STAB-04 | P2 | Enforce the required CI check before merging to main | Open |
| STAB-05 | P2 | Align the declared Rust minimum with locked dependencies | Open |
| STAB-06 | P2 | Bound retained MCP sessions | Open |
| STAB-07 | P2 | Prove sustained multi-client and refresh stability | Open |
| STAB-08 | P1 | Deliver useful hook context within the total latency budget | Open |
| STAB-09 | P2 | Preserve and classify typed transport timeout errors | Open |
| STAB-10 | P1 | Verify successful real hook-to-daemon lookups under refresh load | Open |

### STAB-01 — Isolate clients and bound transport waits

The serial daemon accept loop lets an incomplete request block other MCP clients, health checks, and shutdown. A v1.8.1 isolated probe confirmed this. The hook investigation adds a related case: after the client times out, an already-dispatched graph operation continues and temporarily delays later probes.

Work:
- Bound connection admission and header/body/response I/O using elapsed deadlines.
- Keep health, shutdown, and unrelated clients responsive during stalled connections and slow graph operations.
- Define bounded queueing and deadline propagation for hook read requests; discard expired queued reads and avoid repeated abandoned work.
- Assess safe cancellation of expired read-only work. Preserve legitimate long-running operation semantics and do not blindly replay mutations.

Acceptance: process tests cover idle clients, partial headers/bodies, slow readers, timed-out hook calls, and simultaneous health/other-session requests. Assert responsiveness, bounded pending work, stable daemon PID, and session continuity. Response write failures remain connection-local.

Source: `src/adapters/mcp/daemon.rs::daemon_accept_loop`, `src/adapters/mcp/dispatcher.rs::serve_http_dispatcher`, and `src/coordinator.rs`.

Implementation: `16ecb52` adds bounded nonblocking HTTP serving, absolute I/O deadlines, coordinator admission with one execution permit and no graph queue, hook timeout propagation, and shutdown acknowledgement followed by drain. Expired native reads retain their permit until they really finish. Ambiguous operation transport failures are not replayed. The daemon reports null executor state when a remote owner's activity cannot be observed.

Verification on macOS ARM64: all 48 focused tests passed, including real-process idle/partial/trickled connections, admission saturation, timed-out hook reads, busy replies, session continuity, stable PID, and shutdown drain. Strict workspace Clippy and formatting pass. Full workspace and Linux/macOS/Windows acceptance evidence is tracked in [PR #123 checks](https://github.com/rabii-chaarani/codebaseGraph/pull/123/checks). Scryer change `chg-5e3czh` is implemented with source/test anchors and ingested JUnit evidence.

The accepted scope is STAB-01 plus the hook deadline contract. Fast successful hook context delivery under refresh remains STAB-08/10; full typed timeout classification remains STAB-09.

### STAB-02 — Recover sessions after restart

Previously, unknown supplied session IDs received HTTP 400, and generated IDs restarted from a per-process counter.

Work: distinguish missing session headers (400) from unknown or terminated sessions (404); generate IDs unique across daemon lifetimes.

Acceptance: a real client retains its session across a daemon restart, receives 404, initializes again, and successfully calls a graph tool. An old ID never aliases a newly created session. Update the test that currently asserts 400 for an unknown supplied ID.

Source: `src/adapters/mcp/http.rs`, `src/adapters/mcp/state.rs`, `tests/http_daemon_process.rs`. Contract: [MCP session management](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports#session-management).

Implementation: `eae5ae3` returns HTTP 404 for unknown supplied IDs, including initialization and notifications, while missing required headers retain HTTP 400. Session IDs use 32 bytes of OS randomness; entropy failure or collision returns HTTP 500 without changing sessions. Ordinary handling and deferred tool admission share session classification, and existing-session reinitialization remains supported.

Verification on macOS ARM64: all 38 focused MCP, HTTP admission, and real-daemon tests passed. The restart regression covers clean shutdown and forced termination, initializes competing clients before checking retired IDs, then reinitializes after 404 and successfully calls `graph_health`. Tests also cover removed IDs, busy-executor precedence, entropy/collision failure, and session continuity. Formatting and strict workspace Clippy pass. Full workspace and Linux/macOS/Windows acceptance evidence is tracked in [PR #125 checks](https://github.com/rabii-chaarani/codebaseGraph/pull/125/checks). Scryer change `chg-b4am2n` is implemented with source/test anchors and ingested JUnit evidence.

The scope is the server contract. Automatic hook-client reconnect, persistent sessions, session expiry/capacity limits, and DELETE support are not added by STAB-02.

### STAB-03 — Make Direct recovery idempotent

A crash after a sidecar rename but before the DatabasePromoted checkpoint leaves a Prepared journal with no sidecar candidate. Recovery deletes the already-promoted valid destination and then repeatedly fails checksum validation.

Work: use journal sidecar checksum membership to distinguish an expected already-promoted sidecar from one intentionally absent in the new bundle. Retain and validate expected destinations.

Acceptance: inject interruption after each database, sidecar, and manifest rename and before each phase checkpoint. Repeated recovery preserves correct checksums and a matching database/manifest pair, restores successful reads, and removes the completed journal. Repository source files remain outside this derived-state operation.

Evidence: [verified Direct recovery finding](./memory/episodic/direct-prepared-replay-deletes-promoted-sidecar-2026-09-23.md). Source: `src/storage/direct.rs::promote_database_bundle`, `src/storage/test_harness.rs`.

### STAB-04 — Enforce the CI merge gate

The assessment found ruleset R disabled and no classic protection on main.

Work: recheck current repository settings, enable protection, and require the existing `required` aggregate CI check with branch freshness. Preserve intended access and bypass policies.

Acceptance: effective rules prevent merging when mandatory checks fail or are missing. Existing release automation remains functional. Reference: [Native Release Verification](./architecture/release-verification.md).

### STAB-05 — Verify the minimum supported Rust version

The root manifest declares Rust 1.82, but the locked `crc 3.4.0` dependency, reached through `lzma-rs`, requires Rust 1.83. An actual Rust 1.82 check failed.

Work: choose compatible dependency versions or raise and document the supported minimum; add a CI check using that minimum.

Acceptance: locked source builds on the declared supported toolchain for the applicable supported targets. Updating the version string alone is insufficient without a passing build.

### STAB-06 — Bound session retention

The HTTP session map grows with initialization and has no expiry, removal, or capacity bound. Hook invocations and endpoint verification also create sessions.

Work: introduce a bounded lifecycle, idle expiry, and an explicit termination policy. Expired sessions follow STAB-02.

Acceptance: repeated initialize/disconnect and hook cycles reach a documented session/memory ceiling without evicting active clients incorrectly.

### STAB-07 — Run sustained stability verification

After the runtime fixes, run a repeatable sustained workload combining multiple clients, hook invocation, source create/rename/delete events, active refresh and spill, config repair, and daemon/worker restarts.

Acceptance: record workload size, build mode, duration, request rates, latency distributions, context-injection success, freshness convergence, session counts, memory and temporary disk usage. Verify bounded resources, responsive health checks, automatic reconnection, valid generations, and recoverable failures on supported platforms. Define the duration and numerical reliability/resource thresholds before claiming a stability pass.

## Hook latency additions

### STAB-08 — Fit useful context into the hook budget

The supplied live diagnosis observed approximately 2.5 seconds for MCP `graph_health` and 4.36 seconds for hook-shaped `graph_search` while refresh and substantial spill activity were active. These are incident measurements, not universal latency guarantees. The total hook deadline is three seconds, while session opening and individual MCP calls are capped at 900 milliseconds.

`SessionStart` fails on graph health. `UserPromptSubmit` encounters the same health failure and would also exceed the search ceiling. Installation is correct; fail-open behavior protects the agent but supplies no graph context.

Work:
- Measure end-to-end and per-stage latency, including queue wait, repository identity verification, session setup, health, and search.
- Design a bounded fast path for hook health and search. Evaluate cheaper health evidence, bounded reads, and generation-aware caching/precomputation with explicit freshness and repository identity rules.
- Preserve the three-second fail-open contract. Merely increasing the 900 ms timeout cannot fit the observed health-plus-search sequence into three seconds.
- Coordinate with STAB-01 so refresh I/O and queued work do not consume the entire hook budget.

Acceptance: valid SessionStart and UserPromptSubmit events produce expected, nonempty context within the total deadline on a defined representative repository, both idle and during refresh/spill. Report sample count, p50/p95/p99, success rate, and budget breaches for cold and warm paths. Failure still exits successfully within the hook deadline; wrong-repository or unlabelled stale context is never injected.

Source: `src/agent_hooks.rs::HOOK_DEADLINE`, `open_session_with_budget`, `call_session_with_budget`. Evidence: [live hook latency diagnosis](./memory/episodic/agent-hook-live-mcp-budget-eagain-2026-09-23.md).

### STAB-09 — Classify timeouts without message matching

The per-call budget becomes a TCP read timeout. On macOS, expiry can appear as `Resource temporarily unavailable (os error 35)` / EAGAIN. The current string classifier does not recognize it. Classification alone cannot solve STAB-08, and the current retry path is session opening rather than a general retry of every tool call.

Work: retain structured I/O error kind and operation stage across the MCP client boundary. Handle TimedOut and WouldBlock consistently when they represent a socket deadline. Make retry decisions depend on remaining total budget, dispatch ambiguity, and operation safety.

Acceptance: deterministic tests cover macOS-style WouldBlock/EAGAIN and other platforms' timeout errors, delayed responses, exhausted budgets, and non-retryable errors. Retry only where safe; retries never extend the end-to-end deadline or create an unbounded queue of abandoned requests.

Source: `src/mcp_client.rs::loopback_http_json_request`, `src/agent_hooks.rs::retryable`.

### STAB-10 — Verify real successful hook delivery

Existing verification checks installed handlers and malformed-input fail-open behavior. The positive integration test responds immediately from a fake HTTP server. Neither proves a successful real hook-to-daemon lookup during refresh.

Work:
- Retain the fast fixture tests and add real packaged/built hook-to-daemon tests for both events.
- Extend verification to distinguish handler installation, fail-open behavior, and successful live context injection.
- Cover refresh/spill contention, delayed health/search, timeout classification, reconnects, and daemon health after the hook deadline.

Acceptance: tests assert relevant nonempty injected context and end-to-end completion time, not merely exit status zero or daemon survival. A correctly installed but latency-starved hook reports a degraded live lookup result rather than a successful delivery check. Tests demonstrate recovery and successful later requests after a timeout. Run the real-path checks on macOS, Linux, and Windows.

Source: `src/agent_hooks.rs::verify_agent_hooks`, `tests/agent_hooks_process.rs`, `tests/http_daemon_process.rs`, `tests/refresh_recovery_process.rs`.

## Delivery and closure

STAB-01 supplies client isolation and the hook deadline contract; STAB-08 remains the follow-up latency design. STAB-09 can proceed against that deadline contract and its own typed-error design. STAB-10 is their integration gate. Session recovery/retention and Direct crash recovery are separate bounded slices. CI enforcement and Rust compatibility can be handled independently.

Close a task only with the fixing commit or settings evidence and its passing acceptance checks. Earlier green CI and fail-open success are baseline evidence, not proof of successful hook context delivery. This backlog proposes future changes; it does not weaken the existing [Agent-Loop Hooks](./architecture/agent-loop-hooks.md) invariants.
