# COD-108 — independent MCP diagnostic verification

**Local checks PASS with zero compiler warnings. This is a diagnostic-message change, not a runtime fix. The hosted Mac failure remains unresolved and blocks full CI acceptance.**

## Authority and snapshot

This bounded step remains under C103-R04/R05, C103-A04/A05, Design8 and brief `chg-cw9n3h`. Approved revision: `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; approval record: `0e6008a1f46679a86aba85ac513beb05af94b0c0`. Engineer owns empty diagnostic ledger `chg-c6x8xk`; this verification neither changes nor closes it. No new intent or claims were created.

The tested project is `/private/tmp/codebasegraph-cod108-20261101`, HEAD **0413e543668f3e08003aa6a0d4ead350f49a61cc**, HEAD tree **5141d876f604bea22efaea7224f47d50b3a2b81a**, with exactly one tracked change:

- `src/adapters/cli/tests/mcp.rs`: **19,557 bytes**, SHA256 **772e2d7c7ad22cc4155875b5cdbf5a480ab688891a7a29ebd563ea6e7349164a**.
- All **2,370 other tracked blob/mode pairs** match HEAD. All **2,371 current input bytes/modes** match before and after the checks.
- Canonical `/Users/rabii/Projects/Repositories/codebaseGraph` remains clean at the same HEAD/tree, with the **old** MCP test SHA256 **22cd4cef51368a0f3050641e7e437d7fbc4d798e6337b87c3bf1b446cf3081bc**. This expected source difference is not relabelled as equality.

Replacing only the new assertion message with the old assertion yields the entire canonical test file byte-for-byte. The comparison remains `assert_eq!(responses[2]["result"]["isError"], false)`. The added message pretty-prints that existing response on failure. Every request, fixture, runtime action and remaining assertion is unchanged. Production CI classifier, CLI, YAML and release documentation bytes are unchanged.

## Executed checks

| Actual check | Outcome | Scope |
| --- | --- | --- |
| `cargo fmt --all --check` | PASS | Formatting only |
| Exact native Cargo case below | PASS, 1 case, 0 failed/ignored | Other four harnesses execute 0 cases; 960 filtered cases are not passes |
| `cargo clippy --offline --locked -p codebase-graph --lib --tests --all-features -- -D warnings` | PASS, 0 warnings | Root lib/test lint compilation, **not test execution** |
| Focused nextest inventory | PASS | 848 listed, exactly 1 matching unignored case; discovery is not execution |
| Focused nextest run | PASS, genuine JUnit: 1 case, 0 failures/errors/skips/retries | 847 other cases excluded by selection, not passing or XML-skipped cases |

The one exact Cargo invocation was:

```text
cargo test --workspace --offline --locked --profile native-tests --lib --bins --examples adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors -- --exact --nocapture
```

Actual PID27801 exited 0. The named case passed in **6.76s**; compilation finished in **57.93s**. Four other harnesses ran zero tests. This invocation was not retried.

After read-only `locate` confirmed that this test is attached to existing **resp-50** on `serve_mcp_stdio`, Engineer authorized one additional reporter-specific nextest execution. The inventory selected only:

```text
adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors
```

The actual reporter command was:

```text
cargo nextest run --offline --locked -p codebase-graph --lib --cargo-profile native-tests --config-file '/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-mcp-diagnostic-independent/nextest.toml' --user-config-file none --profile mcpdiagnostic -E 'test(=adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors)' --test-threads 1 --no-tests fail
```

Actual PID28516 exited 0. The one case passed in **6.810s**. Real nextest run UUID **2b95310d-9221-42ba-8c04-1b52933c5288** produced `mcp-diagnostic-junit.xml`, SHA256 **ff64e0c40547145a28223304455e55973f24e809420742080fccf302f602c6cc**. XML has exactly the named testcase, one pass, no failure/error/skipped/rerun nodes. No XML conversion, fabrication or ingestion occurred.

These are **two executions of one unique MCP test**, not two unique tests, 103 tests or a full workspace/platform qualification. The reporter-specific run is not a retry to erase the earlier hosted failure.

## Native environment and binary provenance

Existing Darwin arm64 environment: Rust/Cargo **1.96.0**, nextest **0.9.140**, native LBUG **0.19.0**, OpenSSL **3.6.2**, deployment target **13.3**, build jobs **2**. No installs, dependency/library/config changes, custom features, RUSTFLAGS, resource policy, target override or alternate cache/recipe were added. The configured `native-tests` profile and default isolated target remain unchanged. The private nextest profile sets the authorized reporter path, retries0 and one test thread.

Actual root test binaries:

- Cargo: `codebase_graph-51c13b6bd229e743`, SHA256 **c4c87df5af91037d376c632bc5eff01017e9c77b2b8cb8978b5700bcd177a006**.
- Nextest: `codebase_graph-3b349a504aecf5e2`, SHA256 **b7d5562a3842d5261bbc685aad01c1d9dd6ba692a76cf23ed86545e21e5a3466**.

Both actual root fingerprint JSON files report `default,bundled-windows-extensions` and empty rustflags. The different harness IDs/hashes are disclosed, not treated as byte-equal or assigned an unverified cause. Full compiler logs, all five Cargo harness hashes, actual nextest inventory/build metadata, native library hashes, dependency records and fingerprint payloads are adjacent. Every actual check log has zero compiler-warning lines.

## Interpretation and pending evidence

Both local runs passed, so the failure-only diagnostic message was **not emitted**. No actual failing id3 response was captured. The message-only scope is proven by the exact diff and full-file reversal; no forced failure, response mutation, second probe or dynamic failure-payload claim was made.

The actual hosted Mac job **124897498528**, run **41323156535/attempt1**, source **1c1730dfd06f238a6f6546e7e842a3988e52d61e**, still failed because `graph_health.isError` was true. Its full error body was not logged. This local success neither identifies that cause nor proves flakiness, baseline failure, unrelated failure or a fix.

Canonical Scryer still has the old test source. Its stored resp-50 record is an older pass with test fingerprint `8397b34714b30faa`; it must not be called current against isolated772. The new genuine JUnit is ready for **later, separately authorized, once-only ingestion after canonical source matches**. No maps, ingestion, fold, sign-off or canonical source transition occurred. All four captured authored model/plan/verdict/history files remain byte-exact. Original103 evidence was not re-ingested.

## Preserved history, next owner and stop

Verified unchanged: hosted Mac failure report **8a018… /22 closed files**, old unchanged local baseline **42c3… /14**, corrected Windows portable pass **a9c8… /16**, and scoped model report **683c1799…**. Old Windows failure history and Backend author artifacts were not rewritten. None of these observations clears a different failure.

Engineer and the SAME focused Reviewer own final review/evidence binding. Correct canonical source and explicit ingestion remain pending. Fresh hosted FULL CI on a reviewed diagnostic commit requires its later grant. Full COD108 acceptance/handoff, COD110, merge and release remain blocked/pending. Post-protected-main rollout and pending architectural folds stay separate.

Accepted settings: existing openai-codex Evidence Verifier, `inherit/inherit`, no model/provider/effort override, substitution or delegation; runtime model metadata is unexposed. No production constants changed. This run-specific step does not warrant a new reusable memory claim.

**STOP all Cargo/execution/private writes after this handoff.** No source/Git/API/CI/model/tracker/wiki/settings mutation, watcher, polling, sleep, rerun or cancellation was performed. Only authorized normal build/test temporary outputs and owned private evidence were written. Actual commands, raw stdout/stderr, result, genuine XML and closed hash manifest are adjacent.
