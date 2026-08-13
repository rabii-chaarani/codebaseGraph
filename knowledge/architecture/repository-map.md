---
description: Directory-level ownership map and starting points for architecture-oriented changes.
resource: repository-architecture
tags:
- architecture
- ownership
- repository-map
- source-layout
timestamp: 2026-08-13
title: Repository Ownership Map
type: architecture
---
# Repository Ownership Map

This map connects durable architecture responsibilities to current source locations. Paths are implementation evidence and may move; component responsibilities and dependency direction should remain stable across refactors.

## Top-level map

| Path | Architectural owner |
| --- | --- |
| `src/bin/codebase-graph.rs` | Graph Runtime process entry point. |
| `src/adapters/` | CLI and MCP transport translation and process-facing behavior. |
| `src/api/` | Public contracts, facade, unified operation core, catalogs, presentation, graph reads, lifecycle, normalization, repository runtime, materialization, and refresh coordination. |
| `src/execution/` | Materialization planning and scan-to-write orchestration. |
| `src/parser/` | Language-profile parsing support and normalized syntax inputs. |
| `src/syntax_materializer/` | Syntax-tree traversal and graph node/relationship emission. |
| `src/semantic_enrichment/` | Cross-file resolution, evidence, semantic metadata, and fallbacks. |
| `src/staging_writer/` | Deterministic row accumulation, merge, connector creation, and staging. |
| `src/db_writer/` | Embedded graph-store writes, deletion planning, concurrency, retry, and extension setup. |
| `crates/k-wiki/` | Knowledge Wiki container: OKF API, source reading, validation, compilation, projection, search, rendering, authoring, refresh, and transports. |
| `crates/xtask/` | Release Verifier container, native artifact contract, and release gate. |
| `.github/actions/setup-native/` | Target-specific native dependency preparation. |
| `.github/workflows/native.yml` | Reusable cross-platform native test, build, package, and smoke execution. |
| `tests/` and module-local test modules | Cross-boundary integration tests and component-level regression coverage. |
| `knowledge/` | Curated repository intent consumed by k-wiki. |
| `.kwiki/` | Generated wiki projection and static-site state. |
| `.codebaseGraph/` | Generated source-graph database, indexes, manifests, and configuration. |

## Graph Runtime change guide

| Change shape | Start here | Follow dependencies toward |
| --- | --- | --- |
| Add or alter a public operation | `src/api` contracts, registry/core, and facade | Request normalization, runtime resolution, application service, presenter, then adapters. |
| Change CLI behavior | `src/adapters/cli` | Public contracts and facade; do not bypass them. |
| Change MCP behavior | MCP adapter under `src/adapters` | Public operation registry and facade; tool schemas derive from operation metadata. |
| Change source discovery | Source Scanner / materialization support in `src/api` and `src/execution` | Execution Planner and manifest contracts. |
| Change parsing or graph ontology emission | `src/parser` and `src/syntax_materializer` | Execution plan row contracts, semantic enrichment, and ontology validation. |
| Change cross-file resolution | `src/semantic_enrichment` | Evidence metadata and deterministic row staging. |
| Change persistence | `src/staging_writer` and `src/db_writer` | Graph schema catalog, deletion safety, readers/writers, retry, and manifest publication. |
| Change watch behavior | refresh services in `src/api` and watch adapters under `src/adapters/cli` | Incremental Materialization API; never create a second indexing pipeline. |

## Knowledge Wiki change guide

| Change shape | Start here | Preserve |
| --- | --- | --- |
| Add a wiki operation | `crates/k-wiki/src/api` and `service.rs` | Single operation catalog, typed results, and transport-neutral semantics. |
| Change OKF consumption | bundle reader and conformance modules | Unknown-extension retention, reserved-file semantics, and configured-root boundary. |
| Change normalized knowledge | compiler, model, and projection modules | Stable identities, deterministic ordering, backlinks, diagnostics, and atomic generation publication. |
| Change search | search module | Exact identity/title priority, stable ordering, snippets, and facets. |
| Change rendering | render module | Sanitization, accessibility, script-free navigation, and stable URLs. |
| Change authoring | authoring module | Bundle-relative validation, atomic writes, stale-write rejection, and extension preservation. |
| Change graph integration | graph-context adapter | Use only the Graph Runtime public API and preserve explicit degraded behavior. |

## Release verification

The Release Verifier is intentionally separate from runtime code. It selects platform-aligned tests, produces deterministic release-ready archives, records exact-SHA provenance, validates complete artifact sets, and smoke-tests packaged CLI and MCP behavior through subprocesses. CI and release call the same reusable native workflow; release promotes only the complete artifact set from successful CI for the exact tag commit.

## Architecture discovery workflow

Before reading source for an architecture change, locate the governing files and relationships through the repository codebase graph. Read Scryer for the intended container/component responsibility and inherited directives. Use source only as implementation evidence, then reconcile durable architectural changes back into Scryer and this wiki.

Related: [Graph Runtime](./graph-runtime.md), [Knowledge Wiki](./knowledge-wiki.md), [Native Release Verification](./release-verification.md), and [Architecture Invariants](./invariants.md).