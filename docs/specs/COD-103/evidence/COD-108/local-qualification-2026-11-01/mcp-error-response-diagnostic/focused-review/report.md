# COD-108 — focused MCP error-response diagnostic review

**PASS for the bounded diagnostic-message change and its affected local evidence only.** No new source defect or oracle weakening was found. **This is not a causal runtime fix:** actual hosted Mac graph_health failure remains unresolved, and fresh FULL hosted CI remains mandatory after a separately authorized diagnostic commit/push.

| Field | Binding |
| --- | --- |
| Scope | Same required independent Final Reviewer; only the failure message in existing `mcp_stdio_serves_tools_and_tool_errors`. No integrated full-review restart, runtime fix, retry, fallback, new behavior/claim or approval. |
| Snapshot | `/private/tmp/codebasegraph-cod108-20261101`, `cod103-stack/cod-108`; HEAD `0413e543668f3e08003aa6a0d4ead350f49a61cc`, tree `5141d876f604bea22efaea7224f47d50b3a2b81a`. Only dirty `src/adapters/cli/tests/mcp.rs`: **19557bytes / SHA256 `772e2d7c7ad22cc4155875b5cdbf5a480ab688891a7a29ebd563ea6e7349164a`**. Exact Git identity/diff/source rechecked at closure. |
| Authority | Existing approved316be4c4bb81d4f6e025016b12374836b4dcc1a8 / separate approval0e6008a1f46679a86aba85ac513beb05af94b0c0, Design8/R04R05/A04A05/briefchg-cw9n3h. Read existing empty diagnostic ledger `chg-c6x8xk`; no model write/lifecycle operation. Existing attached `resp-50` changes neither responsibility nor runtime behavior. |
| Next owner | Engineer separately authorizes any one diagnostic commit/push and fresh FULL CI. Verifier must inspect actual new-head hosted evidence; if the failure recurs, use its real full response before choosing a runtime fix. No A04/A05 handoff/COD110/Done/main/merge/release/aggregate closure. |

## Focused trace and counterevidence

The test creates its existing private fixture, sends initialize/tools-list and tool requests, calls **in-process** `serve_mcp_stdio(&options, Cursor(input), &mut output)` at195, then parses output into `serde_json::Value`. The third response belongs to id3 `graph_health` (request141–149). Assertions220–225 still require the same `responses[2]["result"]["isError"] == false`.

The only change formats that existing response with `serde_json::to_string_pretty(&responses[2])` in the assertion failure message. Reversing just this assertion expansion produces the **entire old22cd4cef51368a0f3050641e7e437d7fbc4d798e6337b87c3bf1b446cf3081bc file byte-for-byte**. Therefore every input/request, fixture, stdio/runtime action, previous comparison and later health/refresh/backend/text/search/forbidden-query assertion remains unchanged. No subprocess/stderr machinery, method-echo probe, helper, retry/timeout, extra assertion, ignored case or fallback was added.

Tried to falsify preservation of the oracle and attribution: the message neither changes operands nor converts an error into success; it serializes parsed response data, not executable text or another tool call. Current source still fails if the original isError=true condition recurs. The failure-output path is assessed statically; **both actual local methods passed**, so no failing id3 JSON was emitted or dynamically demonstrated. No forced/edited response or synthetic repro is claimed.

## Preserved actual failure; cause still unknown

Read and hash-checked all22 files in closed actual Mac failure report **`8a01801b16948f693a682fac60f0435ed592246f7f7db7803db838f9c55cd2cc`**. Raw job124897498528/run41323156535attempt1/sourceS `1c1730dfd06f238a6f6546e7e842a3988e52d61e` at oldmcp.rs220 truly reports `left: Bool(true), right:false`; raw log756–761 and1830–1833 preserve the failed case/libtest101 (845pass/1fail/2ignore). Full graph_health response was not archived. No startup/storage/environment/baseline/flake/unrelated cause is established from the Boolean.

Also read/check all14 files of unchanged local native report **42c3ef98193dd1b468b72d9a35ff2c6b7e38007b07f44df180f610e1e98dc00b**: one unchanged local pass is a separate observation, not proof the hosted failure is fixed or flaky. Recalled startup memory remains advisory and does not establish this cause. Prior Windows fixtureFR001/current actual Windows pass/history is not reopened or relabelled by this diagnostic review.

**Confirmed new defects in the diagnostic patch: none.** No root-cause finding ID/priority is invented for the still-undetermined Mac error. The real failing hosted outcome remains an acceptance blocker; this diagnostic-only PASS does not resolve it.

## Closed independent affected evidence

Directory: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-mcp-diagnostic-independent`.

- Report **`cbc299544472f8c8b5ac846c4039e2f1593ac3f87e5a0da1043fdfa5d12b8e4b`**.
- Result **`a5af4c433d7f2a724997ab59fffe3461090b5265bc766bc279d84294a51c06f0`**.
- Files **`ba984981a75a0044032e7e738a9dfe84f21c811e153425f9951ad4b2e6f61381`**; all **36** declared entries personally size/hash checked, zero mismatch. Actual commands/workdirs/profile/features/binaries/source proof/raw logs inspected.
- Formatting `cargo fmt --all --check`: PASS. Affected root lint `cargo clippy --offline --locked -p codebase-graph --lib --tests --all-features -- -D warnings`: PASS/0warnings (**lint compilation, not test execution**).
- Actual exact native Cargo command `cargo test --workspace --offline --locked --profile native-tests --lib --bins --examples adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors -- --exact --nocapture`: **one named case PASS6.76s**,0retry. Other4 harnesses run0cases;960 filtered cases are not passes.
- Separately authorized reporter-specific nextest command selects the **same one unique case** with native-tests/private mcpdiagnostic/retries0/thread1/no-testsfail: **one PASS6.810s**,0failure/error/XMLskip/retry. Genuine JUnit **`ff64e0c40547145a28223304455e55973f24e809420742080fccf302f602c6cc`**, UUID **`2b95310d-9221-42ba-8c04-1b52933c5288`**. Actual inventory has exactly that one unignored matching case;847 excluded cases are not passes or XML-skipped testcases.

These are **two methods executing one unique MCP case**, not two unique tests,103 cases or a full platform/workspace pass. Native Darwin arm64/RustCargo1.96.0/nextest0.9.140/native019/OpenSSL3.6.2/deployment13.3/jobs2 environment is disclosed; normal native-tests and isolated target unchanged. Cargo and nextest root binary identities differ as recorded; no unverified explanation or byte equality inferred. Zero compiler warnings in all relevant actual logs. No tests/Cargo were run by reviewer.

Independent before/after guard covers all2371 tracked inputs and records2370 other blob/mode pairs unchanged. Canonical source still has old22cd whereas isolated source has772; existing stored resp50 verdict is **not** current for the changed test. Genuine XML is available for later separately authorized correct-source once-only ingestion, not an authorization to map/ingest/fold now. No hash-only model reconciliation or final native/fullCI qualification is inferred.

## Settings, limits and STOP

Same required `openai-codex/gpt-6.1-sol` / accepted `thinking=max`, fresh native default child/no fork; host receipt e9610cfcecba31a245fc002566486b88845c17807ba0477ade41326b51d45a8f unchanged. Separate agent_type/fork_turns overrides and runtime internals are not independently exposed. Required adversarial/fix-verification method, graph-first discovery/context, memory and scoped Scryer rules/intent were followed; no provider fallback or dispatch.

Original approval/claims, model binding/folds, hosted current-head/base/rollout, non-bypass strict freshness, serial own-main CI, complete queue/release and aggregate owners remain unchanged. No source/constants/settings or original author evidence changed by reviewer. This exact diagnostic aims to preserve missing failure information, not relax acceptance. Reusable knowledge assessed: no new verified root cause warrants memory. **STOP reviewer private writes at handoff.**
