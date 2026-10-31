# Independent macOS ARM64 unit-failure review

## Evidence and scope

- Archived job: `123869404477`, run `40996079962`, `native (macos-arm64) / native tests (macos-arm64, unit)`.
- Job metadata names PR head `f2936a4c956c3cd90a45089dc7f7f041e7aef477`. The downloaded artifact's `source-sha.txt` and `report.json` identify the tested **merge** source as `ad040bd7aac026b56d327b0c02c86df1a8512c9d`; do not describe this as an exact-head execution.
- `job.log` lines 754–760 and archived `platform-test.log` agree: `mcp_stdio_serves_tools_and_tool_errors` failed at `src/adapters/cli/tests/mcp.rs:220`, because `responses[2]["result"]["isError"]` was `true`, expected `false`. Final result: 857 passed, 1 failed, 2 ignored.
- The failed response, error code, message, and stderr from any relevant worker are **not retained**. The artifact contains the platform log, inventory, ignore inventory, source SHA, and report; it does not contain the MCP response.
- `job.log:247` sets `RUST_TEST_THREADS=1`. This is not evidence of parallel Rust test-harness interference.
- Parent independently verified the failing test source is byte-identical between tested merge and working branch. Runtime-source comparisons to that merge remain the parent's responsibility. This review read only the isolated worktree at `C:/Users/rcha0053/Repositories/cbg-pr164` after graph discovery; graph line numbers are from the original checkout and are not revision proof.

## Observed source facts

- `src/adapters/mcp/stdio.rs:9–23` starts a coordinator API, then reads, handles, and writes each request sequentially. The test's five preloaded requests therefore do not themselves dispatch concurrently. Its failure is not an order assertion.
- `src/adapters/mcp/tools.rs` maps `ApiError` into `isError=true`, retaining `code`, `message`, `details`, and `retryable` when structured content is requested. This test already requests that content for health, but its old assertion did not print it.
- `src/api/core.rs:846–918` returns successful health payloads even when the database is unreadable (`ok=false`). Thus the observed `isError=true` is an API/runtime/admission/transport failure, **not merely pending refresh readiness or `ok=false`**.
- `src/coordinator.rs:877–982` publishes state and starts a thread that constructs maintenance and refresh before serving requests. Startup completion is asynchronous.
- Maintenance and graph requests share `ExecutionDispatcher`; `try_acquire_maintenance` marks it active (`coordinator.rs:301–312`). Rejected graph admission returns typed `graph_busy`, `retryable=true` (`1257–1267`).
- Startup refresh completion requests maintenance (`src/api/refresh.rs:1437–1452`, `2617–2683`). This supplies a plausible independent source of slot contention inside an otherwise serial test.
- Ordinary health preparation does not schedule maintenance before its own admission: `core.rs:493–506` does so only for explicit legacy-quiescence confirmation, which this fixture does not request.
- The coordinator's built-in safe-retry predicate retries **only** `coordinator_request_receive_failed && retryable` (`coordinator.rs:1436–1441`), not `graph_busy`. Do not infer a preexisting 15-second busy/readiness allowance from `COMMAND_RETRY_TIMEOUT`.

## Assessment

Best current hypothesis: the test assumes immediate graph admission while the newly started coordinator can legitimately own its executor for autonomous maintenance. If the actual response is `graph_busy`, that is a test scheduling/oracle issue, not evidence that the health operation produced bad data.

This remains **unconfirmed**. A runtime-resolution, identity, storage, or transport failure can produce the same observed Boolean. A Windows targeted pass does not reproduce or disprove the original macOS failure. No test/build was run by this reviewer.

## Smallest safe next action / correction

1. Keep the diagnostic-only change: print the complete health response in the existing exact `isError == false` assertion. Preserve all current health, refresh-enabled, search-content, and mutation-rejection assertions. Reproduce through the source-bound native-test interface before choosing behavioral changes.
2. If the response is not typed `graph_busy`, stop and diagnose that specific error. Do not poll every retryable error, retry ambiguous transport failures, disable refresh, weaken assertions, add sleeps, or widen a timeout.
3. For deterministic proof of the contention mechanism, extend the existing coordinator test harness: hold `dispatcher.try_acquire_maintenance()`; send a real graph request and assert exact `graph_busy` plus `retryable=true`; drop the permit using an explicit channel handshake; assert the next request succeeds. Existing `maintenance_permit_blocks_real_execution_until_actual_drop` (`src/coordinator/tests.rs:677–693`) and socket-level busy test (`203–248`) provide the components. This proves admission semantics without replaying an operation or relying on runner timing.
4. Only after matching the failing MCP envelope should the stdio test synchronize its fixture. Prefer a test-only channel at the actual maintenance/admission boundary, with completion before protocol assertions. A one-time `active_operation()==false` snapshot is not a stable barrier. Existing `BeforeRefresh` / `RefreshStarted` probes are lifecycle hooks, not maintenance-quiescence signals; they cannot safely substitute for that barrier. No already-available stable readiness handshake was found in the inspected source. If no narrow handshake is available, retain the diagnostic patch rather than add speculative production retry behavior.

## Workflow / changes

Read original checkout `AGENTS.md`; performed wiki recall/search and graph search/context before source reads. Recalled HTTP-daemon readiness history is advisory only: this failure is not proven to be that prior startup error. Graph lacks the newer native-test/maintenance files, so this review did not bypass discovery to read them. No repository source, service, tracker, GitHub, or runtime settings were changed by this reviewer. No constants were changed. No durable wiki memory was written because the causal claim is unconfirmed and the task is read-only.
