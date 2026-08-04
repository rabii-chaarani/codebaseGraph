---
description: System context, container boundaries, principal flows, and evidence model for codebaseGraph.
resource: repository-architecture
tags:
- architecture
- system-context
- codebase-graph
- scryer
timestamp: 2026-08-04
title: Repository Architecture Overview
type: architecture
---
# Repository Architecture Overview

`codebaseGraph` is a local repository knowledge platform. It indexes source repositories into a queryable code graph and publishes curated OKF knowledge as a navigable wiki. Its architecture deliberately separates transport adapters, transport-neutral application services, deterministic build pipelines, and repository-local generated state.

## System context

| Participant | Relationship to the system |
| --- | --- |
| Developer or agent | Runs commands, embeds the Rust API, queries graph tools, and reads or authors wiki concepts. |
| MCP host | Invokes graph and wiki operations through MCP JSON-RPC. |
| Source repository | Supplies source snapshots, configuration, manifests, and curated OKF Markdown. |

## Deployable containers

| Container | Responsibility | Primary boundary |
| --- | --- | --- |
| [Graph Runtime](./graph-runtime.md) | Builds, refreshes, queries, and manages repository-local source graphs. | Rust library plus CLI and MCP process surfaces. |
| [Knowledge Wiki](./knowledge-wiki.md) | Maintains and publishes OKF bundles, search projections, backlinks, diagnostics, and optional graph context. | Rust service with CLI, MCP, and read-only HTTP surfaces. |
| Release Verifier | Enforces repository release policy and smoke-tests packaged command and MCP behavior. | Rust `xtask` executable. |

## Principal flows

1. **Public operation flow:** a CLI, MCP, or embedded client submits a typed request to the public facade; the unified API core resolves repository context, normalizes the request, dispatches a registered operation, and presents a typed or compact result.
2. **Graph materialization:** source discovery feeds a self-contained execution plan, semantic enrichment, deterministic graph-row assembly, and the embedded graph store. See [Materialization Pipeline](./materialization-pipeline.md).
3. **Graph reading:** health, search, context, catalog, and bounded read-only query operations go through the graph read service; transport adapters never query storage directly.
4. **Repository refresh:** filesystem changes are filtered and coalesced into bounded batches, then reuse the incremental materialization path with bounded retry for transient failures.
5. **Wiki publication:** curated Markdown is discovered, validated, compiled into a normalized projection, atomically published under `.kwiki`, indexed, and rendered. Graph context is optional and degrades explicitly when unavailable.

## Architectural stance

- **One semantic path across transports.** CLI, MCP, HTTP, and embedded clients translate at the edge and delegate to transport-neutral services.
- **Source and projections are distinct.** Repository source, graph state, curated `knowledge/`, and generated `.kwiki/` state have separate ownership and recovery rules.
- **Builds are deterministic and incremental.** Stable ordering, manifest differences, validated relationship endpoints, and atomic publication make outputs reproducible.
- **Reads are bounded and non-mutating.** Raw graph queries are validated as single read-only statements and have bounded results.
- **Failure preserves useful state.** Transient storage and refresh failures use bounded retry; failed wiki compilation preserves the last valid projection.

## Evidence and freshness

This documentation reconciles two repository-local evidence layers on 2026-08-04:

- The Scryer intent model defines 201 architecture nodes and 224 responsibilities across the system, with all 157 anchorable leaf responsibilities mapped. Its health report also records 28 silent anchors and 52 asserted-only links, so responsibility wording is authoritative intent while those mapping gaps remain confidence qualifiers.
- The codebase graph contained 502,118 materialized nodes at the time of review. Graph search and context confirmed the current entry points, public facade, materialization stages, graph reads, wiki service, and renderer paths.

Counts describe the reviewed snapshot, not a permanent invariant. When code and this wiki diverge, consult Scryer for intended responsibilities and the codebase graph for current implementation evidence, then update the durable decision here.

## Reading map

- [Graph Runtime](./graph-runtime.md)
- [Public Operations and Runtime Paths](./operation-paths.md)
- [Materialization Pipeline](./materialization-pipeline.md)
- [Knowledge Wiki](./knowledge-wiki.md)
- [Repository Ownership Map](./repository-map.md)
- [Architecture Invariants](./invariants.md)