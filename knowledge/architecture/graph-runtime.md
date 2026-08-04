---
description: Component boundaries and dependency direction inside the transport-neutral graph runtime.
resource: repository-architecture
tags:
- architecture
- graph-runtime
- components
- rust
timestamp: 2026-08-04
title: Graph Runtime Architecture
type: architecture
---
# Graph Runtime Architecture

The Graph Runtime is the product executable and embeddable library. It exposes one transport-neutral operation model while keeping command parsing, MCP negotiation, graph construction, graph storage, and presentation in separate components.

## Layered component map

| Layer | Components | Accountability |
| --- | --- | --- |
| Process and adapters | Process Bootstrap, CLI Adapter, Repository Lifecycle Adapter, CLI Materialization Adapter, Repository Refresh Adapter, MCP Server Adapter, Command Request Mapper | Select an interface, translate external input into public requests, and frame results without changing product semantics. |
| Public boundary | Public API Contracts, Public API Facade, Unified API Core, Catalog Provider, Response Presenter | Define stable requests and responses, register operations once, dispatch them, and present typed or compact block output. |
| Runtime preparation | Request Normalizer, Repository Runtime Resolver | Apply canonical defaults, reject invalid requests before execution, and resolve one repository/source/graph/configuration/manifest context. |
| Application services | Graph Read Service, Materialization API, Repository Lifecycle Service, Repository Refresh Service | Execute graph reads, builds, installation lifecycle, and refresh behavior independently of transport. |
| Build pipeline | Source Scanner, Execution Planner, Semantic Enricher, Graph Writer | Convert source snapshots into validated, enriched, deterministically merged graph rows. |
| Persistence | Graph Store | Persist nodes and relationships and coordinate safe concurrent readers and writers. |

## Dependency direction

The adapters depend inward on the Public API Facade and Public API Contracts. The facade delegates to the Unified API Core. The core resolves runtime context and normalization before dispatching to application services. Application services may depend on the build pipeline and Graph Store; storage and pipeline components do not depend on CLI or MCP details.

```text
CLI / MCP / embedded client
        -> Public API Facade
        -> Unified API Core
        -> normalize + resolve repository runtime
        -> registered application operation
        -> Response Presenter
        -> transport framing
```

## Public boundary

`CodebaseGraphApi::execute_operation` in `src/api/facade.rs` is the stable library entry point. It delegates exactly once to the Unified API Core. The operation registry is authoritative for dispatch and MCP tool generation, preventing CLI, MCP, and embedded APIs from acquiring separate behavior catalogs.

The core owns three cross-cutting duties:

- execute every operation against a consistent repository context;
- normalize and validate requests before side effects;
- map internal failures into stable public errors.

## Read and write separation

The Graph Read Service reads health and metadata, performs ranked search and relationship traversal, and executes bounded read-only statements. `validate_read_only_statement` rejects empty, compound, or write-capable statements before `execute_read_only_query` reaches the Graph Store.

Graph writes enter through the Materialization API and [Materialization Pipeline](./materialization-pipeline.md). The Graph Writer assembles deterministic updates; the Graph Store applies schema, deletions, and staged rows while serializing writers and retrying transient lock failures.

## Refresh behavior

The Repository Refresh Service supports continuous and one-shot refresh. It filters and coalesces filesystem events into bounded batches, resolves canonical repository context, normalizes refresh options, and reuses incremental materialization. Transient failures are classified and retried with bounded backoff.

## Source evidence

| Boundary | Current implementation evidence |
| --- | --- |
| Process selection | `src/bin/codebase-graph.rs`; Process Bootstrap symbol `run_process_args`. |
| Public facade and core | `src/api/facade.rs`; `src/api/core.rs`. |
| Graph reads | `src/api/graph_read.rs`. |
| Request preparation | `src/api/normalization.rs` and repository-runtime resolution under `src/api`. |
| Materialization orchestration | `src/api/materialization.rs`; `src/execution/run.rs`. |
| Refresh | `src/api/refresh.rs` and CLI watch adapters under `src/adapters/cli/watch`. |
| Storage | `src/staging_writer`; `src/db_writer`. |

See [Public Operations and Runtime Paths](./operation-paths.md) for request flow and [Repository Ownership Map](./repository-map.md) for change-oriented navigation.