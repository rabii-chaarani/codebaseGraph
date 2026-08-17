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
timestamp: 2026-08-17
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
  -> CodebaseGraphApi::execute_operation
  -> Unified API Core
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
| MCP stdio | Negotiate MCP messages and serve newline-delimited requests over standard streams. | Tool specifications derive from public operation metadata. |
| MCP HTTP | Serve MCP requests over HTTP and enforce configured authentication and bind rules. | Uses the same MCP dispatch and public operations as stdio. |
| Embedded Rust API | Accept typed operation requests and return typed or block-form results. | Enters directly at the Public API Facade. |

## Operation registry

The Unified API Core registers product operations in one authoritative catalog. Operation identifiers, request schemas, handler dispatch, output metadata, and MCP tool generation therefore evolve together. A transport must not maintain a parallel list of product behavior.

## Repository runtime resolution

Every repository-scoped operation resolves one `RepoRuntime`: source root, configuration, manifest context, and either a managed storage-v2 root or explicit Direct-mode database and manifest paths.

Managed reads resolve `active.json` and lease its generation for the entire operation. Direct reads recover any interrupted paired publication before opening their destinations. Runtime entry also recovers abandoned managed runs and retries pending retirement.

Config schema v3 supplies a managed `storage_root`, refresh policy and backend, and bounded materialization defaults. Schema-v2 remains readable and receives v3 defaults for missing fields. Schema-v1 deserialization remains available for reads, but the resolved runtime is not writable until explicit reinstall.

## Graph read path

Health, schema, helper catalogs, architecture catalogs, search, context, and raw query operations dispatch from the core to the Graph Read Service. Search reads native full-text indexes and applies lexical/entity ranking. Context expands selected relationship profiles. Raw statements are parameterized, single-statement, read-only, and result-bounded.

Health reports storage format, writability, active generation, reused and rebuilt artifacts, pending runs, cleanup status, physical/logical database sizes, and refresh ownership/coalescing/no-op state.

## Lifecycle and refresh paths

Repository installation, reinstallation, client registration, and removal are coordinated by the Repository Lifecycle Service. Continuous or one-shot refresh is coordinated by the Repository Refresh Service, which invokes the same Materialization API used by explicit builds. Under the default `leader` policy, one cross-process lock holder owns the watcher and followers remain read-only standbys; `off` starts MCP without refresh. Refresh-only materialization may return `database_written = false` after the writer lock proves the active generation already consumed the change.

For schema-v1 state, search, context, query, and health remain available. Build, watch, refresh, and install return `legacy_storage_requires_reinstall`. Reinstall moves the legacy state without copying it, restores it after any pre-activation failure, and deletes it immediately after successful v2 activation and validation; there is no grace-period copy.

## Failure boundaries

- Request-shape and operation-rule violations fail during preparation.
- Repository selection and storage-format failures fail during runtime resolution.
- A failed candidate build or publication preserves the active generation.
- Cleanup errors are reported separately and never hide the primary build error.
- Application failures are translated once into stable public errors.
- CLI exit codes and MCP protocol errors are framing choices at the edge, not distinct product errors.
- Query validation blocks mutation before the Graph Store is invoked.

## Adding or changing an operation

Update the public contracts and authoritative registry first, keep the handler transport-neutral, then let CLI/MCP adapters translate to it. Verify the operation through the facade and at least one transport contract. Storage lifecycle changes must preserve [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md). If responsibility or dependency direction changes, update Scryer and this architecture set together.

Related: [Graph Runtime](./graph-runtime.md), [Materialization Pipeline](./materialization-pipeline.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [Architecture Invariants](./invariants.md).