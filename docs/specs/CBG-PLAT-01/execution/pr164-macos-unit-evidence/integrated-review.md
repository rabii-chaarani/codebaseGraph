# PR164 macOS unit failure: integrated source review

## Verdict

**Accept the reviewed test-only patch; no source-review blockers found.** Rebuild and run the focused tests from these final sources before claiming local validation. Native macOS CI is still required. The original macOS error payload remains missing, so this patch is a justified fixture contract correction, not a confirmed diagnosis of the archived failure.

## Reviewed scope and identity

Read the final isolated-worktree files after the AGENTS/wiki/graph discovery recorded in `proposal-review.md`:

- `src/adapters/cli/tests/mcp.rs` SHA-256 `0b3fdd1a58939c3dc28ab04fd528fb68ea0dd893c1445c99815036f1e9f1f651`
- `src/coordinator/tests.rs` SHA-256 `a57902231b187e47b48188156eeb9133d7d431470bd4a9c6bc17633a40515d75`

Compared both against the earlier reviewed contents. No production changes are part of these reviewed deltas. Did not read graph-missing native-test or maintenance files. This reviewer ran no build, test, formatter, push, or source edit. The parent's reported format check and running build are not reviewer-executed evidence; the parent correctly plans a rebuild after the final diagnostic/classifier edits.

## Acceptance findings

### Same-session behavior and exact retry classification

- `mcp_test_stdio_requests` invokes the real `serve_mcp_stdio` once in a scoped server thread. The loopback socket pair is a test-only duplex `BufRead`/`Write` adapter, not a second MCP transport implementation.
- Each logical request is written and read sequentially. Only the current request can be retried. Initialization, tool listing, completed health/search calls, and the whole transcript are not replayed.
- `mcp_test_graph_busy` requires `tools/call`, matching response id/tool, boolean `isError=true`, exact code string `graph_busy`, and boolean `retryable=true` under the actual `result.structuredContent.error` path. It does not match arbitrary retryable or transport failures.
- Search now opts into structured content while retaining `output_format=json` and its original content assertions. The original health/refresh, catalog/schema, search-hit, and query-rejection checks remain intact. Exactly five terminal logical responses are asserted.
- The matcher regression covers mismatched id/tool, non-error success, another retryable error code, false/null/string retryability, and a non-tool request. Missing retryability also cannot equal boolean true in this predicate.

### Panic and stream cleanup

- `Client` owns both client handles inside the scope closure. Its `Drop` calls `shutdown(Both)`, so a loop assertion or I/O panic closes communication before `thread::scope` joins the server. This avoids the identified open-client/read/join deadlock.
- Normal completion explicitly drops the client before joining. Server return values and panics are not ignored.
- The setup uses a connected loopback pair and small existing messages. No general-purpose transport framework, production lifecycle hook, or extra readiness abstraction was added.

### Fixed test-side deadline

- There is one new **15-second test-side admission/retry deadline**, created immediately before the first protocol frame after fixture setup. It is not renewed for another attempt or request.
- Remaining time is applied as client socket read/write timeouts. I/O errors are immediate failures, never retry signals. A typed busy reply past the deadline fails with the complete response; top-of-loop expiry and read errors retain the request and last busy response.
- This is a deliberate new fixture policy, not a previously existing test allowance or product timeout change. Socket timeouts apply to blocking I/O; the same pre-write remaining value is used for that attempt's reads. Consequently this must **not** be described as an exact 15-second end-to-end wall-clock limit, native cancellation, or teardown bound. It is sufficient to prevent renewed busy retry budgets without altering production semantics.

### Deterministic coordinator admission regression

`maintenance_busy_health_is_rejected_before_dispatch_and_recovers_after_release` uses the existing real coordinator socket/dispatcher harness:

1. Acquires a genuine maintenance execution permit before the graph request.
2. Issues `health` through `CoordinatorClient` and asserts exact `graph_busy`, retryable true, and zero executor callback invocations.
3. Releases the permit through a channel; joining the holder proves the permit was actually dropped before the next request.
4. Issues health again, asserts its successful response, and verifies exactly one executor invocation.

On assertion unwind, the release sender is dropped and the holder exits its receive wait, dropping the permit before scope joining completes. No sleep, mocked busy envelope, or racy idle snapshot is used. The executor callback returns a test payload; this test proves real coordinator pre-dispatch admission and subsequent execution, not native health/storage correctness. Existing MCP assertions separately check the real health/search payloads.

## Residual validation and claims

The finite parent-run Windows protocol probe returned initialize plus 30 health responses with no errors. It did not reproduce busy or classify the missing macOS response. Windows passes and this deterministic regression can establish that the patch preserves local behavior and explicitly handles legal pre-dispatch rejection; they cannot establish the original macOS cause.

Before reporting completed validation, use the post-edit binary to run the exact stdio test, matcher regression, new coordinator regression, and relevant existing coordinator no-replay/permit tests. Preserve full results and source/binary identity. Native macOS qualification remains pending until that platform executes these sources.

No durable wiki entry was recorded: the historical diagnosis is still unconfirmed and this report is a scoped review, not a new verified repository contract.
