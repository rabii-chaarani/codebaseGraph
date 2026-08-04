---
description: OKF source, projection, search, rendering, authoring, and graph-context architecture.
resource: repository-architecture
tags:
- architecture
- k-wiki
- okf
- knowledge
- projection
timestamp: 2026-08-04
title: Knowledge Wiki Architecture
type: architecture
---
# Knowledge Wiki Architecture

The Knowledge Wiki is a deployable OKF knowledge publishing service. It treats curated Markdown as source, compiles deterministic repository-local projections, and exposes read, search, diagnostic, rendering, and controlled authoring operations through one transport-neutral API.

## End-to-end publication flow

```text
configured OKF bundle roots
  -> Bundle Reader
  -> Conformance Validator
  -> Knowledge Compiler
  -> Projection Store
  -> Concept Search + Wiki Renderer
  -> CLI / MCP / read-only HTTP
```

## Component map

| Concern | Components |
| --- | --- |
| Public operation boundary | Wiki Public API and `LocalWikiService` dispatch versioned read, authoring, validation, build, search, and diagnostic operations. |
| Source consumption | Bundle Reader discovers bundle roots, parses concepts and reserved files, preserves producer extensions, and rejects paths that escape configured roots. |
| Conformance | Conformance Validator separates required OKF 0.1 semantics from advisory guidance and retains consumable unknown types, extensions, and links. |
| Compilation | Knowledge Compiler derives stable concept identities, directory projections, backlinks, fragments, citations, diagnostics, recent changes, and synthesized navigation. |
| Persistence | Projection Store atomically publishes complete projections, caches unchanged content, retains the last valid generation after failure, and keeps state under `.kwiki`. |
| Discovery | Concept Search indexes identity, metadata, headings, and body text; exact title and identity matches outrank body-only matches; filters cover bundle, type, and tag. |
| Presentation | Wiki Renderer sanitizes Markdown, HTML, links, and resources; preserves script-free reading; supports accessible navigation and stable URLs. |
| Refresh | Refresh Coordinator rebuilds affected projection artifacts, prevents stale concurrent publication, and can coordinate a shared source-change stream with the Graph Runtime. |
| Interfaces | Wiki CLI Adapter, Wiki Agent Adapter, and Wiki HTTP Server translate to the Wiki Public API. |
| Controlled writes | Bundle Authoring creates and populates validated bundle-relative pages with atomic writes and stale-content protection. |
| Optional implementation context | Graph Context Adapter calls only the Graph Runtime public API and returns explicit degraded results if graph operations are unavailable. |

## Source versus generated state

| State | Authority | Rule |
| --- | --- | --- |
| `knowledge/` | Curated repository intent | Edit only through authored Markdown or the controlled k-wiki authoring operations. |
| `.kwiki/` | Generated normalized projections and static site state | Disposable and rebuildable; never edit as source. |
| `.codebaseGraph/` | Generated source-graph database, indexes, manifests, and configuration | Owned by the Graph Runtime; separate from wiki projection state. |

## Trust boundary

- Only configured bundle roots are readable or writable.
- Absolute paths, traversal, escaping symlinks, reserved-file misuse, stale revisions, and conflicting destinations are rejected.
- Browser routes are read-only and bind locally by default with restrictive security headers.
- Rendering sanitizes untrusted Markdown and HTML before publication.
- Public errors omit absolute host paths, source contents, secrets, and raw parser failures.
- Graph context is optional; graph failure must not block concept reading or publication.

## Graph Runtime integration

The wiki queries graph health, search, and context through the Graph Runtime's public Rust API. It does not import graph storage or materialization internals. This is an intentional container boundary: curated concepts remain useful without an available graph, while graph context can enrich implementation-oriented views.

## Publication and recovery

A successful compilation publishes a complete versioned projection atomically. An unchanged source is a cache hit. Generation tokens prevent older concurrent work from replacing newer state. Compilation or rendering failure leaves the previous valid projection readable and surfaces diagnostics.

## Source evidence

| Boundary | Current implementation evidence |
| --- | --- |
| Public wiki facade and operation registry | `crates/k-wiki/src/api`; `crates/k-wiki/src/service.rs` (`LocalWikiService::execute`). |
| Bundle reading and conformance | `crates/k-wiki/src/bundle`; `crates/k-wiki/src/conformance`. |
| Compilation and projection | `crates/k-wiki/src/compiler`; `crates/k-wiki/src/projection`; projection model in `crates/k-wiki/src/model.rs`. |
| Search and graph context | `crates/k-wiki/src/search`; graph-context adapter under `crates/k-wiki/src`. |
| Rendering and HTTP | `crates/k-wiki/src/render`; HTTP adapter under `crates/k-wiki/src/adapters`. |
| Authoring and refresh | `crates/k-wiki/src/authoring`; `crates/k-wiki/src/refresh.rs`. |

Related: [Repository Architecture Overview](./overview.md), [Repository Ownership Map](./repository-map.md), and [Architecture Invariants](./invariants.md).