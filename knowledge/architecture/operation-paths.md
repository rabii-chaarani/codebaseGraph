---
description: How CLI, MCP, and embedded calls share one operation catalog, runtime resolution, storage recovery, and execution path.
resource: repository-architecture
tags:
- api
- architecture
- cli
- mcp
- runtime
- storage
timestamp: 2026-08-25
title: Public Operations and Runtime Paths
type: architecture
---
# Public Operations and Runtime Paths

All Graph Runtime clients share one public operation path. Transport code translates and frames; product semantics live behind the Public API Facade.

## Canonical request path

```text
external input
  -> adapter-specific parsing
  -> typed public request
  -> CodebaseGraphApi facade
  -> CLI/embedded: local Unified API Core
  -> MCP: authenticated loopback route to repository owner
  -> owner Unified API Core
  -> operation descriptor resolution
  -> canonical request normalization and validation
  -> repository runtime resolution and storage recovery
  -> application service
  -> typed public response or public error
  -> compact presentation and transport framing
```

## Interfaces

| Interface | Adapter responsibility | Shared behavior |
| --- | --- | --- |
| CLI | Parse product commands, map command options, choose stable exit codes, and write human or machine output. | Calls public operations; does not own graph or storage semantics. |
| MCP stdio | Negotiate MCP messages and serve newline-delimited requests over standard streams. | Tool specifications derive from public operation metadata; the process routes repository operations to the elected owner. |
| MCP HTTP | Serve MCP requests from one repository-scoped managed loopback daemon. | Every compatible local harness registers the same endpoint; the daemon uses the same MCP dispatch and repository coordinator as stdio. |
| Embedded Rust API | Accept typed operation requests and return typed or block-form results. | Enters directly at the Public API Facade. |

## Managed local MCP daemon

`auto` selects one Streamable HTTP daemon per canonical repository for every loopback-capable harness. The daemon holds `mcp-daemon.lock` before it initializes the listener, API core, coordinator, or watcher, so concurrent starts cannot create duplicate owners. Setup persists its loopback URL, service ID, and transport version while retaining the stdio launch command for explicit compatibility mode.

The daemon is supervised by launchd, a systemd user service, or Windows Task Scheduler with persistent, throttled restart policies. On macOS, launchd owns the configured IPv4 loopback listening socket and the daemon activates that inherited descriptor; a client connection therefore supplies real launch demand and recovers the daemon even when the GUI launchd domain is on-demand-only. Runtime state records its PID, repository fingerprint, endpoint, version, optional start timestamp, and a rotating control token in the repository state directory. A separate mode-0600 `mcp-daemon-failure.json` atomically retains only the latest bounded startup, runtime, or provisioning failure; it never records the control token. `mcp daemon status` combines endpoint health with best-effort service-manager state, manifest drift, controller/runtime version drift, the latest failure, and a directly executable recovery action. `mcp daemon start` is an idempotent reconciler: it returns unchanged only when the healthy runtime identity, endpoint, version, loaded service, and rendered manifest all match, otherwise it reprovisions the same repository service. Health is loopback-only and shutdown requires the control token. Intentional stop requests authenticated shutdown and immediately stops or unloads the supervisor before the bounded wait, preventing automatic restart from racing cleanup; a surviving same-repository process is then terminated through its process group and only dead matching state is removed.

Codex, Claude Code, repository `.mcp.json`, GitHub Copilot/VS Code, LM Studio, Hermes, OpenClaw, and generic local adapters render their native HTTP form with the same URL and no command field. Registration is preflighted before mutation, only recognized managed stdio entries migrate automatically, and file-backed multi-client changes roll back together. Copilot Studio, Microsoft 365 Copilot, and other cloud-brokered connectors remain manual public-HTTPS targets; lifecycle code never exposes the loopback listener or provisions a tunnel.

## Repository coordinator path

For a given repository storage root or Direct destination pair, one MCP process acquires `coordinator.lock` and owns the Public API Core. It accepts a versioned, length-bounded JSON frame over loopback and authenticates every request with the capability token stored in mode-0600 `coordinator.json`. The owner overwrites the invocation's repository selector with its canonical configured selector before execution.

Follower MCP processes do not resolve and open Ladybug databases. They retain only the coordinator route and reconnect once after failure. A one-second monitor initiates deterministic election when the endpoint disappears; operating-system lock release provides takeover within five seconds.

Health responses are produced by the owner and attach coordinator PID, endpoint, refresh ownership, memory limits, worker PID, pending state, high-water marks, spill bytes, and last no-op or error information.

## Operation registry

The Unified API Core registers product operations in one authoritative catalog. Operation identifiers, request schemas, handler dispatch, output metadata, and MCP tool generation therefore evolve together. A transport must not maintain a parallel list of product behavior.

## Repository runtime resolution

Every repository-scoped operation resolves one `RepoRuntime`: source root, configuration, manifest context, and either a managed storage-v2 root or explicit Direct-mode database and manifest paths.

Managed reads resolve `active.json` and lease its generation for the entire operation. Direct reads recover any interrupted paired publication before opening their destinations. Runtime entry also recovers abandoned managed runs and retries pending retirement.

Config schema v3 supplies a managed `storage_root`, refresh policy and backend, and bounded materialization defaults: 768 MiB worker RSS, 384 MiB Rust working state, 32 MiB spill chunks, and parallelism two. Schema-v2 remains readable and receives these defaults. The legacy semantic-enrichment field remains readable but is normalized to disabled. Schema-v1 deserialization remains available for reads, but the resolved runtime is not writable until explicit reinstall.

## Graph read path

Health, schema, helper catalogs, architecture catalogs, search, context, and raw query operations dispatch from the owner core to the Graph Read Service. For MCP, this makes Ladybug memory repository-central rather than client-local. Search uses a generation-owned disk BM25 sidecar when backend metadata is present; older generations fall back to native Ladybug FTS. Both paths apply deterministic lexical/entity ranking. Context expands selected relationship profiles. Raw statements are parameterized, single-statement, read-only, and result-bounded.

Health reports storage format, writability, active generation, reused and rebuilt artifacts, pending runs, cleanup status, physical/logical database sizes, and refresh ownership/coalescing/no-op state.

## Lifecycle and refresh paths

Repository installation, reinstallation, client registration, and removal are coordinated by the Repository Lifecycle Service. Continuous or one-shot refresh is coordinated by the Repository Refresh Service, which invokes the same Materialization API used by explicit builds. MCP refresh and explicit coordinator builds execute in one isolated Materialization Worker; standalone CLI builds use the same canonical pipeline directly. Under the default `leader` policy, one cross-process lock holder owns the watcher and followers remain read-only standbys; `off` starts MCP without refresh. Refresh-only materialization may return `database_written = false` after the writer lock proves the active generation already consumed the change.

For schema-v1 state, search, context, query, and health remain available. Build, watch, refresh, and install return `legacy_storage_requires_reinstall`. Reinstall moves the legacy state without copying it, restores it after any pre-activation failure, and deletes it immediately after successful v2 activation and validation; there is no grace-period copy.

## Failure boundaries

- Request-shape and operation-rule violations fail during preparation.
- Repository selection and storage-format failures fail during runtime resolution.
- A failed candidate build, memory-budget termination, coordinator death, orphan-worker reap, or publication failure preserves the active generation.
- A managed daemon is registered only after its identity, repository fingerprint, initialization, and required tool schemas verify; failed provisioning removes a newly created service and restores changed registrations.
- Daemon failure snapshots are bounded, atomically replaced, repository-owned, and advisory: inability to record diagnostics never hides the primary service failure.
- Structured memory failures report the phase, configured limit, accounted bytes, and observed RSS.
- Cleanup errors are reported separately and never hide the primary build error.
- Application failures are translated once into stable public errors.
- CLI exit codes and MCP protocol errors are framing choices at the edge, not distinct product errors.
- Query validation blocks mutation before the Graph Store is invoked.

## Adding or changing an operation

Update the public contracts and authoritative registry first, keep the handler transport-neutral, then let CLI/MCP adapters translate to it. Verify the operation through the facade and at least one transport contract. Storage lifecycle changes must preserve [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md). If responsibility or dependency direction changes, update Scryer and this architecture set together.

Related: [Graph Runtime](./graph-runtime.md), [Materialization Pipeline](./materialization-pipeline.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [Architecture Invariants](./invariants.md).