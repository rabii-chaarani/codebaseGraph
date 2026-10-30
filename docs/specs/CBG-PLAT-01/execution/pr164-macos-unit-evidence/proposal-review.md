# PR164 macOS unit failure: independent proposal review

## Verdict

**Approve the narrow, test-fixture-only same-session correction, subject to the conditions below. Do not require the missing original macOS payload before correcting this independently demonstrable fixture assumption.** This is contract hardening, not a proven diagnosis or confirmed fix for the archived macOS failure. If the exact error classification, fixed bound, or deterministic regression cannot be delivered, push only the diagnostic assertion and collect the native payload first.

The diagnostic-only patch remains useful and should be retained. It is not itself a behavioral correction and must not be reported as having addressed the failure.

## Evidence and uncertainty

- Read `job.log`, `identity.json`, `diagnosis-review.md`, and the exact worktree test `src/adapters/cli/tests/mcp.rs` after AGENTS/wiki/graph discovery. Also checked graph-discovered stdio framing, MCP error mapping, coordinator admission/recovery, and existing coordinator tests. Did not read graph-missing native-test or maintenance files.
- The archived run executed merge `ad040bd7aac026b56d327b0c02c86df1a8512c9d`, not the named PR head. The macOS assertion establishes only `responses[2]["result"]["isError"] == true`; it does not retain an error code or payload. Result: 857 passed, 1 failed, 2 ignored. `RUST_TEST_THREADS=1` does not prevent autonomous coordinator work.
- The exact stdio fixture sends five requests and keeps refresh defaults enabled. Production stdio handles each frame serially within one `McpSession` (`src/adapters/mcp/stdio.rs:9–23`). This does not serialize those requests against autonomous maintenance.
- `ExecutionDispatcher::try_acquire_maintenance` and `try_submit` share the same exclusive active state (`src/coordinator.rs:301–325`). A rejected job is returned without being queued; the owner emits exact `graph_busy`, `retryable=true`, and drops that rejected job before execution (`1257–1267`). Thus this typed response proves that this operation did not start. It is not an ambiguous transport failure.
- The built-in recovery predicate does not retry `graph_busy` (`1436–1441`). Its 15-second recovery constant is not a preexisting busy allowance. The exact stdio test has **no existing deadline or timeout**.
- Reported Windows passes, including the parent's five diagnostic executions, do not reproduce or identify the original macOS error. The parent subsequently reported that the finite real-CLI stdio probe completed with exit 0, 31 responses (initialize plus 30 health calls), and zero `isError` results; raw `protocol-output.log` was retained. This reviewer did not execute that probe. No observed busy capture is claimed.

## Why a fixture correction is warranted

The test checks protocol negotiation, tool catalog/schema, successful health with refresh enabled, search content, and rejection of a write-capable query. It does not specify that the first graph admission must beat permitted background maintenance. Requiring immediate admission adds a scheduling assertion that the runtime contract does not promise.

Retrying only an explicitly rejected-before-dispatch logical request preserves the operation assertions and avoids replaying executed work. Applying the same rule independently to each existing graph request is safer than a startup-only health probe: maintenance can acquire the slot again between health, search, and query. An idle snapshot is not a stable barrier. This needs no production retry behavior or new production synchronization hook.

This reasoning is sufficient for a narrowly named fixture robustness change without asserting historical causation. An unrelated original error still fails immediately. Native CI and retained full responses remain necessary to establish whether the archived failure is actually resolved.

## Required scope and safety conditions

1. Keep one real `serve_mcp_stdio` invocation and one session. Send the existing logical requests interactively, in order. Retry only the current rejected request; never restart the server or replay initialize, tools/list, a successful request, or the whole transcript.
2. Classify the actual typed MCP envelope: `method == "tools/call"`, `result.isError` is boolean true, error `code` is string `graph_busy`, and `retryable` is boolean true. Validate the expected response id and error tool. No message substring, truthy coercion, generic `isError`, or generic `retryable` predicate. Every other response goes directly to its original exact assertions; transport errors fail immediately.
3. Errors are under `result.structuredContent.error` when structured content is requested, **not `result.error`**. Health/query already request structured content. Search currently requests typed JSON text without structured content. The simplest uniform fixture change is adding `include_structured_content: true` to that search request while preserving its existing JSON text/content checks. Parsing its typed JSON text exactly is also possible, but do not parse the block message heuristically.
4. Preserve all existing final assertions, including five terminal logical responses, health `ok=true`, refresh enabled/backend, health block text, search hit, and the expected query `ValueError`. A typed busy rejection is only an intermediate outcome, never the successful terminal result or the expected mutation rejection.
5. The parent proposes a **new explicit test-only 15-second busy-retry budget**, starting with the first protocol frame after fixture setup. This is acceptable as a declared new fixture policy, not an existing product allowance. Capture one monotonic deadline and never renew it per request or retry. When typed busy is returned after exhaustion, fail with the full last response, logical request id/tool, and attempt count; do not issue another retry. Preserve the existing ordinary request/response semantics rather than adding unrelated timeout changes. Do not describe this as a total protocol or wall-clock bound if blocking reads/writes remain unbounded. Do not widen production constants or disable refresh. No arbitrary startup sleep is needed.
6. A test-only TCP loopback pair plus a scoped server thread is a reasonable portable duplex adapter. Do not add production transport or lifecycle abstractions. Arrange shutdown/drop on all exit paths, including assertion panic, **before joining the scoped server**; otherwise an open client can leave the server blocked reading while the scope waits forever. In particular, client streams owned outside the scope closure must be moved into a closure-local owner, not merely borrowed. Fail immediately on observed I/O error/EOF/disconnect and retain request/response diagnostics. Remaining-deadline test-side socket timeouts are an option if a total protocol bound is intended, but preserving the existing no-response behavior is smaller; in that case call the new deadline only a busy-retry budget. Do not claim it cancels an already admitted native operation or bounds all teardown work.
7. Add the deterministic real-admission regression: hold a genuine maintenance permit; perform the real coordinator request; assert exact `graph_busy`, `retryable=true`, and zero executor invocations; release/drop the permit with an explicit handshake; issue the next request and assert success with exactly one invocation. Existing `maintenance_permit_blocks_real_execution_until_actual_drop` and `ping_stays_responsive_and_graph_requests_are_rejected_while_busy` supply the pieces. Do not substitute sleeps, a mocked busy response, or an `active_operation` snapshot.
8. Include a small classifier negative control: another retryable code, `graph_busy` with false/missing/non-boolean retryable, and non-tool/transport failure do not qualify. If practical, exercise the fixture at the MCP boundary so the real error envelope is verified, not only a hand-built JSON shape.

## Effect of the finite real stdio probe

- If it captures exact `graph_busy` plus boolean `retryable=true` with default refresh and the real fixture, it directly demonstrates the allowed response through this transport and strengthens the fixture correction. Retain the response and binary/source identity. Do not say it proves the missing archived macOS response or its precise source was maintenance.
- If all finite requests succeed, that does not disprove contention. The deterministic permit regression remains the proof of the admission mechanism, and the original failure remains unclassified.
- If it captures another error, diagnose that error; do not expand the retry predicate to make the probe or test pass.

## Review limits and workflow

No build, test execution, repository source edit, push, production setting change, or constant change was performed by this reviewer. The only written file is this requested report. Assumption: the runtime's exact typed busy reply is the pre-dispatch rejection verified above; no assumption is made about the archived response. The new 15-second fixture policy is the parent's proposal, not an already validated implementation.

Recalled wiki memory was advisory; the older HTTP readiness case is not evidence for this macOS stdio cause. No durable wiki memory is warranted from an unconfirmed historical diagnosis and an unimplemented proposal, so none was recorded.
