# Interface API Facade Enforcement

## Requirements Summary

- Every production CLI and MCP adapter must invoke product behavior through `CodebaseGraphApi`.
- Adapters may consume public request, response, configuration, and observer contracts re-exported from `crate::api`, but must not import API implementation modules or legacy protocol types directly.
- CLI and MCP transports must remain peer adapters; process-level transport selection belongs outside both adapters.
- Command subadapters must not import sibling behavior; shared input translation belongs to a neutral command request mapper.
- The Scryer change `chg-1` is the authoritative architecture plan and must be folded only after code and tests satisfy it.

## Acceptance Criteria

1. Production files under `src/adapters/` contain no imports from `crate::api::refresh`, `crate::api::materialization`, `crate::api::normalization`, or `crate::protocol`.
2. CLI watch execution in `src/adapters/cli/watch/command.rs` invokes a `CodebaseGraphApi` facade method and receives only public refresh summary data.
3. MCP auto-refresh in `src/adapters/mcp/refresh.rs` obtains a refresh-enabled `CodebaseGraphApi` instead of a `RefreshState`.
4. MCP tool schema generation in `src/adapters/mcp/tools.rs` derives required fields from operation metadata returned by `CodebaseGraphApi`.
5. CLI materialization owns its command-line option shape and constructs a public `MaterializationRequest` without importing `api::materialization`.
6. Process transport routing moves out of `src/adapters/cli/dispatch.rs`; the CLI and MCP adapters no longer import one another.
7. Repository boundary tests fail on any new direct adapter dependency on API internals, protocol data, product execution services, a peer transport adapter, or sibling command behavior.
8. Existing CLI watch, materialization, MCP stdio/HTTP, lifecycle, and API boundary tests pass.
9. Scryer reports a structurally clean model, no pending work for `chg-1`, no drift, and no broken or untested anchors.

## Implementation Steps

1. Extend the public contracts in `src/api/contracts.rs:151` with refresh watch configuration, observer, and summary types. Re-export them from `src/api/mod.rs:8`.
2. Extend `CodebaseGraphApi` in `src/api/facade.rs:23` with facade methods for watch execution, refresh-enabled API construction, and shared interface metadata.
3. Refactor `src/api/refresh.rs:480` to consume public watch contracts and translate native materialization results into public summaries before notifying adapters.
4. Move CLI materialization option ownership from `src/api/materialization.rs:16` to `src/adapters/cli/materialization_input.rs`, and build `MaterializationRequest` in a neutral command request mapper shared by build and watch parsing.
5. Refactor `src/adapters/cli/watch/command.rs:5`, `src/adapters/mcp/refresh.rs:2`, and `src/adapters/mcp/tools.rs:2` so behavior flows only through `CodebaseGraphApi`.
6. Move process transport selection from `src/adapters/cli/dispatch.rs:30` into `src/bootstrap.rs`, and update `src/bin/codebase-graph.rs:1`.
7. Strengthen `src/api/boundary_tests.rs:74` to enforce facade-only adapter behavior, peer transport isolation, and sibling command-adapter isolation.
8. Run formatting, targeted regression tests, the full test suite, and Scryer validation; then fold `chg-1` with implementation and test anchors.

## Risks and Mitigations

- Moving watch contracts can accidentally change CLI output. Preserve the existing output fields and verify existing watch integration tests byte-for-byte.
- Replacing refresh state with a refresh-enabled facade can lose health synchronization. Keep the same shared `Arc<RefreshState>` inside `ApiCore` and verify MCP health output.
- Moving CLI option ownership can change defaults. Copy existing defaults and parsing behavior before deleting adapter imports.
- Transport bootstrap movement can alter blocking stdio/HTTP behavior. Preserve command dispatch branches and run MCP stdio and HTTP tests.
- Scryer's dependency audit may infer false direct links from ambiguous Rust call resolution. Do not add skip-stage links unless source imports or resolved calls confirm them.

## Verification

- `cargo fmt --check`
- `cargo test api::boundary_tests`
- `cargo test watch_once_runs_single_refresh_and_exits`
- `cargo test watch_auto_backend_refreshes_after_probe_resolution`
- `cargo test mcp_stdio_serves_tools_and_tool_errors`
- `cargo test mcp_http_handles_initialize_list_call_and_protocol_errors`
- `cargo test materialize_empty_project_from_native_request`
- `cargo test`
- Scryer `validate_model`, `get_pending`, `get_drift`, and `get_health`
