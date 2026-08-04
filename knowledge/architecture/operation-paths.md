---
description: How CLI, MCP, and embedded calls share one operation catalog and runtime execution path.
resource: repository-architecture
tags:
- architecture
- api
- mcp
- cli
- runtime
timestamp: 2026-08-04
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
  -> repository runtime resolution
  -> application service
  -> typed public response or public error
  -> compact presentation and transport framing
```

## Interfaces

| Interface | Adapter responsibility | Shared behavior |
| --- | --- | --- |
| CLI | Parse product commands, map command options, choose stable exit codes, and write human or machine output. | Calls public operations; does not own graph semantics. |
| MCP stdio | Negotiate MCP messages and serve newline-delimited requests over standard streams. | Tool specifications derive from public operation metadata. |
| MCP HTTP | Serve MCP requests over HTTP and enforce configured authentication and bind rules. | Uses the same MCP dispatch and public operations as stdio. |
| Embedded Rust API | Accept typed operation requests and return typed or block-form results. | Enters directly at the Public API Facade. |

## Operation registry

The Unified API Core registers product operations in one authoritative catalog. Operation identifiers, request schemas, handler dispatch, output metadata, and MCP tool generation therefore evolve together. A transport must not maintain a parallel list of product behavior.

## Repository runtime resolution

Every repository-scoped operation resolves one `RepoRuntime`: source root, graph location, configuration, and manifest context. This prevents setup, graph reads, materialization, and refresh from interpreting repository selection differently.

## Graph read path

Health, schema, helper catalogs, architecture catalogs, search, context, and raw query operations dispatch from the core to the Graph Read Service. Search reads native full-text indexes and applies lexical/entity ranking. Context expands selected relationship profiles. Raw statements are parameterized, single-statement, read-only, and result-bounded.

## Lifecycle and refresh paths

Repository installation, reinstallation, client registration, and removal are coordinated by the Repository Lifecycle Service. Continuous or one-shot refresh is coordinated by the Repository Refresh Service, which invokes the same Materialization API used by explicit builds.

## Failure boundaries

- Request-shape and operation-rule violations fail during preparation.
- Repository selection failures fail during runtime resolution.
- Application failures are translated once into stable public errors.
- CLI exit codes and MCP protocol errors are framing choices at the edge, not distinct product errors.
- Query validation blocks mutation before the graph store is invoked.

## Adding or changing an operation

Update the public contracts and authoritative registry first, keep the handler transport-neutral, then let CLI/MCP adapters translate to it. Verify the operation through the facade and at least one transport contract. If the change alters responsibility or dependency direction, update Scryer and this architecture set together.

Related: [Graph Runtime](./graph-runtime.md), [Materialization Pipeline](./materialization-pipeline.md), and [Architecture Invariants](./invariants.md).