# COD-108 macOS MCP failure — bounded read-only diagnosis

## Finding

The only actual failing assertion is the first `graph_health` tool call response (`id=3`) in `src/adapters/cli/tests/mcp.rs::mcp_stdio_serves_tools_and_tool_errors`, line220. It expected `responses[2].result.isError == false`; the actual decoded JSON value was boolean `true`. The prior `initialize` (id1), `tools/list` (id2), and test preconditions passed. Everything after line220—including health payload, refresh status, search id4, and write-like query rejection id5—was not reached.

The cause of `id=3` is unknown. The hosted panic excerpt reports the bool assertion only; neither the full `id=3` result JSON, its error content/structured error, nor MCP child stderr was retained in the job log, platform test log, or uploaded diagnostic artifact. The assertion cannot distinguish runtime resolution, unreadable graph, coordinator/refresh failure, or another API error. I do **not** classify this as a startup/readiness issue, storage defect, flake, pre-existing baseline failure, or unrelated-to-COD108 failure.

## Read-only source trace

1. `src/adapters/cli/tests/mcp.rs:118-126`: creates a temporary repository with `service.py` and calls shared `setup_search_fixture_repo(root)`.
2. `src/adapters/cli/tests/fixtures.rs:21-36`: helper runs the installed CLI’s `install --repo-root <root> --mode full --mcp-client none --json` (no `--no-fts`).
3. `mcp.rs:128-182`: serializes five newline-delimited MCP requests: initialize1, tools/list2, graph_health3 with structured output, graph_search4, write-like graph_query5.
4. `mcp.rs:194-200`: calls `serve_mcp_stdio(options, Cursor(input), &mut output)` and parses each output line as JSON.
5. `src/adapters/mcp/stdio.rs:9-22`: clones options, calls `start_configured_api`, reads messages sequentially, delegates each to `handle_mcp_message`, then writes the JSON-RPC response.
6. `src/adapters/mcp/protocol.rs:15-75`: tools/call extracts name/arguments and calls `mcp_call_tool_result_with_context`, returning its result under the request id.
7. `src/adapters/mcp/tools.rs:68-118,120-151`: resolves the API operation and executes it with the selected repo; API error responses are translated to MCP `isError:true` with an error tool/code/message/details/retryable payload. That payload was not captured for id3.
8. `src/adapters/mcp/refresh.rs:5-22`: resolved refresh policy Off selects no refresh config; otherwise it builds the configured refresh service before connecting the coordinator API.
9. `src/api/core.rs::execute_inner` around line370 resolves the `RepoRuntime`; `execute_health` at lines846-918 reads the DB via `count_graph_nodes` and returns `ok/graph_readable/error`. `src/api/graph_read.rs::count_graph_nodes` is the readability check. This is the next downstream boundary if the MCP response shows health data, but its execution/result was not retained.

## Change/baseline comparison

At base `90af76d3453a6f5cc0f032045ed48c4fd09c275c`, original COD108 `157a4170f21d8f75d7d02611dd4e4bdfc5ca1dfa`, and current PR head `0413e543668f3e08003aa6a0d4ead350f49a61cc`, the following Git blobs are identical: `mcp.rs` 036619cf…, shared `fixtures.rs` 81dbafac…, stdio adapter a67dd2c…, protocol 79ef6ca…, MCP result mapping ab63eed…, MCP startup adapter 2a9fca…, API core 6f055d3…, API context 6f5d25…, normalization 498c4b…, graph read 11a2d9…, Cargo.toml edc36c…, Cargo.lock 26a3c2…. This establishes that these files did not change in those snapshots; it does not show what result the base revision would produce and does not establish causality or pre-existence. COD108 source is scoped to xtask/CI policy and has no direct MCP operation code edit. Indirect/current-environment causality remains undetermined.

Actual hosted run41323156535 attempt1, source head0413e543…, run native artifact S1c1730dfd06f238a6f6546e7e842a3988e52d61e, command `cargo test --workspace --locked --profile native-tests --lib --bins --examples`. Setup/checkout/native dependencies succeeded; platform tests failed; diagnostic upload succeeded. Root library: 845 passed / 1 failed / 2 ignored. There were 0 compiler warnings. Job log: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-hosted-41323156535-attempt1/availability-checkpoint03-macos-failure/job-124897498528.log` (196,683 bytes, SHA256 `821e580b5cdf8ce20a4a6871004568f033d8c300a69fec17df167b9749f4a374`). Diagnostic zip: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-hosted-41323156535-attempt1/availability-checkpoint03-macos-failure/artifact-13092100404-test-diagnostics-macos-arm64-unit-shard0-1.zip` (46,954 bytes, SHA256 `647fb6f9228a42aaaf2c74182e55c0bb3d3c676858a355238113280a0846de70`). Its five verified members contain test output, source SHA, test inventory, ignored list and report; they do not contain response id3 JSON or MCP child stderr.

## Minimum next diagnostic (proposal only; no edit or run performed)

A single **diagnostic-only** edit in `src/adapters/cli/tests/mcp.rs`, restricted to this existing test: immediately before the current id3 `isError == false` assertion, assert the response id/method are id3’s `tools/call` and `graph_health`, then include the entire decoded id3 response (pretty JSON), full captured stdio bytes, and parsed stderr in the assertion diagnostic. Preserve the existing `isError=false`, `structuredContent`, refresh, text, search and write-rejection assertions exactly; do not retry, delay, ignore, timeout, normalize, add fallback, or weaken the oracle. The existing in-process `serve_mcp_stdio` call captures stdout but not stderr; if root library still omits the body, the minimal one-file diagnostic change is to run only this named test in a child process with isolated `--repo-root`, capturing stdout/stderr/exit status and its repo state, or add a test-only scoped stderr capture if no behavior change and safe isolation can be proven. The native CLI entry point establishes how stdio starts the configured MCP API; no child process or diagnostic was executed here.

When separately authorized by Engineer, Evidence Verifier—not this diagnostic agent—should own a single fresh named native test invocation on the exact current source SHA using the recorded native profile, macos-arm64, bundled-extension settings if required by the wrapper, and isolated temporary repo fixture; capture full request id3 response JSON, stdout/stderr, refresh state, effective repo/database/manifest, exit and exact command/env. Keep the original assertion strict. Only after the actual error payload exists can the fault be routed to runtime/DB/API owner or test fixture owner. Do not repeat the 41-minute full workspace run just to get a body; a narrow named filter is enough if the existing native wrapper can target it.

## Evidence boundaries

- Read-only diagnosis only; no Cargo/build/reproduction/source/API/Git/settings/CI/model/tracker/wiki mutation, no artifact download/duplicate watch, no source writes.
- Preserved verifier state indicates full failed-job log and one macOS diagnostic archive were already inspected/retained; no further API reads were made.
- Current base-source comparison is not a passing baseline test and not a cause finding.
- Existing memory about asynchronous daemon readiness is advisory only and does not establish this stdio response’s cause.
- Exact platform details available from the log: GitHub `macos-latest` label, `MACOSX_DEPLOYMENT_TARGET=13.3`, native-tests profile, and no compiler warnings. Exact runner OS/build, focused wrapper invocation/argv/features, and complete environment are not retained.
- Source is still STOP; next action requires a narrow owner grant for diagnostic capture, not a production fallback or test-oracle change.
