---
description: Component boundaries and dependency direction inside the transport-neutral graph runtime.
resource: repository-architecture
tags:
- architecture
- components
- graph-runtime
- rust
timestamp: 2026-08-18
title: Graph Runtime Architecture
type: architecture
---
# Graph Runtime Architecture

The Graph Runtime is the product executable and embeddable library. It exposes one transport-neutral operation model while keeping command parsing, MCP negotiation, graph construction, graph storage, and presentation in separate components.

## Layered component map

| Layer | Components | Accountability |
| --- | --- | --- |
| Process and adapters | Process Bootstrap, CLI Adapter, Repository Lifecycle Adapter, CLI Materialization Adapter, Repository Refresh Adapter, MCP Server Adapter, Repository Coordinator, Command Request Mapper | Select an interface, elect one repository-scoped MCP owner, translate external input into public requests, and frame results without changing product semantics. |
| Public boundary | Public API Contracts, Public API Facade, Unified API Core, Catalog Provider, Response Presenter | Define stable requests and responses, register operations once, dispatch them, and present typed or compact block output. |
| Runtime preparation | Request Normalizer, Repository Runtime Resolver | Apply canonical defaults, reject invalid requests, resolve schema-v1 versus storage-v2 state, and select Managed or Direct storage mode. |
| Application services | Graph Read Service, Materialization API, Materialization Worker, Repository Lifecycle Service, Repository Refresh Service | Execute graph reads, isolated generation builds, installation lifecycle, and refresh behavior independently of transport. |
| Build pipeline | Source Scanner, Execution Planner, Graph Writer, Search Index Builder, Database Phase Runner | Revalidate inputs, reuse or rebuild raw partitions, externally merge deterministic rows, build disk-backed search, and load a candidate within hard memory limits. |
| Persistence | Graph Store | Own immutable generation publication, read leases, abandoned-run recovery, retirement, direct-mode recovery, and partition artifacts. |

## Dependency direction

Adapters depend inward on the Public API Facade and Public API Contracts. The facade delegates exactly once to the Unified API Core. The core resolves runtime context and normalization before dispatching to application services. Application services may depend on the build pipeline and Graph Store; storage and pipeline components do not depend on CLI or MCP details.

```text
CLI / embedded client -> Public API Facade -> Unified API Core
MCP stdio or HTTP -> Public API Facade -> repository coordinator loopback route
                                      -> owner Unified API Core
        -> normalize + resolve repository runtime
        -> registered application operation
        -> Response Presenter
        -> transport framing
```

## Public boundary

`CodebaseGraphApi::execute_operation` in `src/api/facade.rs` is the stable library entry point. The operation registry is authoritative for dispatch and MCP tool generation, preventing CLI, MCP, and embedded APIs from acquiring separate behavior catalogs.

The core owns three cross-cutting duties:

- execute every operation against a consistent repository context;
- normalize and validate requests before side effects;
- map internal failures into stable public errors.

## Central MCP ownership and process isolation

All MCP processes for one managed storage root or Direct destination pair contend for one nonblocking `coordinator.lock`. The holder writes a mode-0600 loopback endpoint and random token to `coordinator.json`, owns the Public API Core, and is the only MCP process that opens Ladybug databases. Followers keep only the bounded route state, retry the owner on connection failure, and independently attempt takeover. Their monitor detects owner death and operating-system lock release permits takeover within five seconds.

Refresh and coordinator-triggered explicit materialization use the same versioned Materialization Worker protocol. The owner writes request/result files under one worker workspace, holds `worker.lock`, drains bounded newline-delimited progress, and samples RSS every 25 ms. A parent-owned pipe and persisted `worker.json` identity prevent an orphan from continuing after coordinator death: the child exits when the pipe closes, and the next owner reaps the recorded PID and recovers abandoned run journals before starting another worker. Standalone CLI builds remain short-lived and execute the canonical pipeline directly.

Graph reads do not wait for a build-wide in-process lock. They continue leasing the previous immutable active generation until candidate validation and atomic publication advance `active.json`.

## Read and write separation

The Graph Read Service reads health and metadata, performs ranked search and relationship traversal, and executes bounded read-only statements. `validate_read_only_statement` rejects empty, compound, or write-capable statements before `execute_read_only_query` reaches the Graph Store.

A managed read resolves `active.json` under a shared state lock and holds a shared lease on that generation for the complete database operation. This lease, rather than a stale timestamp, prevents retirement while a reader is active.

Graph writes enter through the Materialization API and [Materialization Pipeline](./materialization-pipeline.md). The bounded pipeline releases each partition after use, stages deterministic sorted runs, builds a generation-owned disk search sidecar, and runs Ladybug loading in an RSS-supervised child. Semantic enrichment is retired from production; its legacy options are accepted only for compatibility. The Graph Store holds the exclusive writer lock for the complete mutation, validates the reopened candidate and sidecar, and atomically publishes its generation pointer. It never applies source deltas to the active database.

## Storage and recovery boundary

The Graph Store owns the complete lifecycle described in [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md):

- managed generation and run-workspace layout beneath the configured `storage_root`;
- atomic `active.json` publication under the state lock;
- lease-aware retirement with retryable `cleanup_pending` state;
- journal-driven abandoned-run recovery and path-confined cleanup;
- content-addressed raw partition artifacts and garbage collection;
- checksummed paired publication recovery for explicit Direct-mode paths.

The Materialization API requests these operations but does not publish paths itself. The Repository Runtime Resolver selects and recovers the appropriate storage mode before reads or writes. The Repository Lifecycle Service enforces schema-v1 read compatibility, typed mutation rejection, and reinstall rollback or immediate legacy deletion.

## Refresh behavior

The Repository Refresh Service supports continuous and one-shot refresh. Continuous refresh is a cross-process elected role: one nonblocking `refresh.lock` holder performs a manifest catch-up before creating the watcher, while followers remain read-only and retry election with deterministic jitter. Install schema v3 defaults to `refresh.policy = leader`; `off` starts a watcher-free MCP runtime.\n\nThe leader admits supported source and rebuild-triggering configuration events, never admits CodebaseGraph-owned state or storage roots, and collapses churn into one dirty signal plus a bounded path set. Path-count, byte-count, or watcher-channel overflow becomes one full-rescan marker. After the materialization writer lock is held, refresh intent may discard an unchanged candidate without publishing a generation; explicit builds retain their publication semantics. Transient failures are classified and retried with bounded backoff.

## Source evidence

| Boundary | Current implementation evidence |
| --- | --- |
| Process selection | `src/bin/codebase-graph.rs`; Process Bootstrap symbol `run_process_args`; internal materialization worker dispatch in `src/bootstrap.rs`. |
| MCP ownership and routing | `src/coordinator.rs`; facade routing in `src/api/facade.rs`. |
| Worker supervision | `src/materialization_worker.rs`; worker state and control paths in `src/storage/layout.rs`. |
| Public facade and core | `src/api/facade.rs`; `src/api/core.rs`. |
| Graph reads | `src/api/graph_read.rs`. |
| Request preparation | `src/api/normalization.rs` and repository-runtime resolution under `src/api`. |
| Materialization orchestration | `src/api/materialization.rs`; `src/execution/run.rs`. |
| Refresh | `src/api/refresh.rs` and CLI watch adapters under `src/adapters/cli/watch`. |
| Generation storage, locking, artifacts, and recovery | Storage lifecycle and writer modules under `src/db_writer`, with deterministic staging under `src/staging_writer`. |

See [Public Operations and Runtime Paths](./operation-paths.md) for request flow, [Architecture Invariants](./invariants.md) for the governing constraints, and [Repository Ownership Map](./repository-map.md) for change-oriented navigation.