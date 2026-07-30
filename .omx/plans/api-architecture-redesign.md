# API and Architecture Redesign

Scryer change: `chg-1`

Rationale: Centralize every CLI, MCP, and library operation behind a stable Public API and a linear materialization pipeline.

## Requirements Summary

- Developers may use the CLI or embed the Public API directly.
- Developers using MCP interact through an MCP Host, which invokes the MCP Server Adapter.
- The CLI and MCP adapters must invoke the Public API Facade before any product operation.
- The Public API Facade must delegate transport-neutral execution to the Unified API Core.
- The Unified API Core must own operation registration, repository runtime resolution, error normalization, graph reads, catalogs, response presentation, repository lifecycle operations, and materialization dispatch.
- Typed responses and compact block responses are equally supported public formats. Block output is not a legacy or compatibility-only format.
- The materialization engine must be linear:

  `Materialization API -> Source Scanner -> Execution Planner -> Semantic Enricher -> Graph Writer -> Graph Store`

- The Execution Planner must receive scanned source snapshots in its input and carry all source data required by later phases. No later phase may rescan the repository.
- Existing CLI flags, MCP tool names, exit codes, graph semantics, and block output must remain stable unless separately approved.

## Planned Architecture

The Scryer plan keeps one `Graph Runtime` container because the executable and embedded library are one runtime boundary. Components are organized into three modules:

- `Transport Adapters`: CLI Adapter, Repository Lifecycle Adapter, CLI Materialization Adapter, Repository Refresh Adapter, MCP Server Adapter.
- `Unified Public API`: Public API Facade, Public API Contracts, Unified API Core, Graph Read Service, Catalog Provider, Response Presenter, Repository Lifecycle Service.
- `Materialization Engine`: Materialization API, Source Scanner, Execution Planner, Semantic Enricher, Graph Writer.

`Graph Store` remains the persistence boundary used by Graph Read Service and Graph Writer.

## Acceptance Criteria

1. CLI graph, lifecycle, refresh, and materialization commands call `CodebaseGraphApi::execute_operation` and do not import graph-read, materialization, catalog, or persistence services directly.
2. MCP tool calls enter through `mcp_call_tool_result`, map to an `OperationRequest`, invoke the Public API Facade, and map only the returned response or error into MCP protocol types.
3. `OperationRegistry` is the single authoritative operation catalog. Adding an operation requires one descriptor and one handler registration; MCP tool metadata is generated from that catalog.
4. Public request and response types contain no CLI or MCP protocol types.
5. Every supported operation can return typed output or compact block output through `OutputFormat`.
6. Search, context, query, metadata, and materialization block outputs remain byte-for-byte compatible with existing snapshots.
7. `ApiError.code` values are stable and transport-neutral; CLI exit codes and MCP failures are mapped only in their adapters.
8. Repository selection is resolved once into `RepoRuntime` before an operation handler runs.
9. Materialization passes a self-contained plan from Source Scanner to Execution Planner to Semantic Enricher to Graph Writer.
10. No filesystem source read occurs after Source Scanner completes.
11. Only Graph Writer submits materialized graph updates to Graph Store.
12. Existing full and incremental materialization tests produce the same nodes, relationships, manifests, diagnostics, and deterministic ordering.
13. Scryer validation is clean and all implemented `chg-1` claims are folded with source anchors and attached tests.

## Implementation Steps

### 1. Establish Public API Contracts

Create `src/api/contracts.rs` and define the planned Scryer symbols:

- `RepoSelector`, `NodeRef`, and `RepoRuntime` selection inputs.
- `OperationRequest` with Health, Search, Context, Query, Materialize, Catalog, Setup, Reinstall, Uninstall, and Refresh variants.
- Typed request records for search, context, query, materialization, lifecycle, and refresh operations.
- `OutputFormat::{Typed, Block}`.
- `OperationResponse` and `ApiError`.

Move engine-only materialization types out of the public contract. The current starting declarations are `NativeSyntaxMaterializationRequest` in `src/protocol.rs:7` and `NativeSyntaxMaterializationResponse` in `src/protocol.rs:151`.

Verification:

- Serialization round trips for every public data type.
- Exhaustive operation and output-format matching.
- Compile-time checks that public contracts do not depend on `src/cli/**`.

### 2. Build the Unified API Core

Create `src/api/core.rs` and `src/api/context.rs`:

- Implement `OperationDescriptor`, `OperationRegistry`, `register_operations`, `resolve_operation`, and `dispatch_operation`.
- Implement `resolve_runtime` so repository, graph, and manifest paths are selected once.
- Register graph reads, catalog reads, lifecycle operations, refresh, planning, and materialization behind transport-neutral handlers.
- Reject duplicate operation identifiers and unsupported surface exposure.

The first graph-read handlers wrap the existing operations at `src/cli/graph/search.rs:16`, `src/cli/graph/search.rs:88`, and `src/cli/graph/query.rs:114`.

Verification:

- Registry uniqueness and deterministic ordering tests.
- Runtime selection tests for repository defaults and explicit graph paths.
- Unknown operation and invalid request tests with stable `ApiError.code` values.

### 3. Add the Public API Facade

Create `src/api/facade.rs`:

- Implement `CodebaseGraphApi`.
- Implement `execute_operation` as the only public execution entry point.
- Validate the public request, delegate once to Unified API Core, and return `OperationResponse`.
- Keep transport formatting out of the facade.

Verification:

- An injected core spy observes exactly one dispatch per facade call.
- Public API integration tests cover every operation variant and both output formats.

### 4. Separate Catalog and Response Presentation

Create `src/api/catalog.rs` and `src/api/presenter.rs`:

- Move catalog loading from `metadata_payload` at `src/cli/format/metadata.rs:4`.
- Keep schema and query catalogs transport-neutral.
- Move typed and block presentation behind `present_operation_response`.
- Retain the existing compact serializers, beginning with `serialize_search_block` at `src/cli/format/blocks.rs:111`.
- Do not introduce a legacy payload mapper. Block output remains a first-class presentation strategy.

Verification:

- Existing block snapshots remain byte-for-byte unchanged.
- Typed results preserve all fields represented in block output.
- Catalog output is stable and operation descriptors expose allowed surfaces and formats.

### 5. Convert CLI and MCP into Adapters

Update the current command and MCP dispatch paths:

- CLI command routing remains in `src/cli/dispatch.rs`, but product operations call the Public API Facade.
- `materialize_request` in `src/cli/build/command.rs:51` becomes a facade call and command-output mapping.
- `mcp_call_tool_result` in `src/cli/mcp/tools.rs:18` maps MCP arguments to public requests and public responses back to MCP content.
- Implement `generate_mcp_specs` from public operation metadata.
- Implement `map_error_to_transport` for MCP errors; keep CLI exit-code mapping in the CLI adapter.
- Preserve MCP server startup and transport negotiation as adapter responsibilities.

Verification:

- Dependency tests fail if CLI or MCP modules directly import graph-read, materialization, catalog, or persistence modules.
- Existing CLI dispatch and MCP negotiation suites pass unchanged.
- Tool names, schemas, and protocol error behavior remain stable.

### 6. Extract Repository Lifecycle Service

Create `src/api/lifecycle.rs`:

- Move installation behavior behind `setup_repository`, `reinstall_repository`, and `uninstall_repository`.
- Keep client-specific MCP registration and configuration rendering in the Repository Lifecycle Adapter.
- Route lifecycle commands through the Public API Facade.

Verification:

- Setup, reinstall, and uninstall behavior matches current repository-state tests.
- Reinstall preserves all recoverable data covered by existing tests.
- Client configuration tests remain adapter-level tests.

### 7. Linearize Materialization

Refactor from the current orchestration in `src/execution/run.rs:12`:

- `Materialization API`: rename the public engine entry points to `execute_materialization_pipeline` and `plan_materialization`; invoke only Source Scanner.
- `Source Scanner`: rename `scan_source_state` at `src/scan.rs:16` to `scan_sources`; return source snapshots, manifest diff, selected profiles, and immutable execution inputs.
- `Execution Planner`: consolidate parsing, syntax graph construction, and partition planning. Rename `build_partitions` at `src/execution/parallel.rs:18` to `build_execution_plan`.
- `Semantic Enricher`: rename `enrich_partitions` at `src/semantic_enrichment/mod.rs:37` to `enrich_semantics`; consume only the execution plan.
- `Graph Writer`: assemble deterministic rows, connectors, manifests, and Graph Store writes through `write_graph_rows`.
- `Graph Store`: retain database concurrency, deletion, schema, and write mechanics.

Remove the old Source Parser and Graph Row Model component boundaries after their symbols are rehomed under Execution Planner.

Verification:

- A test removes or makes the source repository unreadable after scanning; planning, enrichment, and writing still complete from the scanned payload.
- Call-graph and dependency checks show one forward edge between each adjacent stage and no backward or skip-stage edge.
- Sequential and parallel plans produce identical ordered output.
- Full, incremental, deletion, semantic-enrichment, connector, and database-write suites pass.

### 8. Complete the Migration

- Remove obsolete direct links and imports after every adapter uses the facade.
- Preserve temporary internal wrappers only while callers are migrated; do not expose them as a second public API.
- Update crate exports and user-facing API documentation.
- Attach unit tests to every conditional symbol-level Scryer claim.
- Fold `chg-1` incrementally with `mark_implemented`, anchors, and tests.

Recommended fold order:

1. Public API Contracts, Unified API Core, and Public API Facade.
2. Graph Read Service, Catalog Provider, Response Presenter, and Repository Lifecycle Service.
3. CLI and MCP adapters.
4. Materialization API and each linear materialization stage.
5. Obsolete component and link deletions.

## Risks and Mitigations

- Public API becomes a generic untyped dispatcher: keep operation-specific request structs and typed handler registration.
- Registry centralization couples core to transports: descriptors declare neutral metadata and allowed surfaces; adapters generate transport schemas.
- Block output regresses during presenter extraction: retain byte-for-byte snapshot tests before moving serializers.
- Materialization plan grows too large: carry immutable shared source buffers and measure peak memory against the current pipeline.
- Lifecycle extraction changes filesystem side effects: lock existing setup, reinstall, and uninstall behavior with regression tests before moving logic.
- A second internal execution path survives migration: enforce import-boundary tests and remove direct adapter-to-service links before folding the Scryer change.

## Verification Commands

Run the repository's established formatting, lint, test, and architecture checks. At minimum:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Then run Scryer validation, inspect `get_pending { change: "chg-1" }`, and fold only claims whose code and tests are complete.
