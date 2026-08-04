---
description: Durable constraints that preserve transport neutrality, deterministic indexing, safe knowledge publication, and evidence integrity.
resource: repository-architecture
tags:
- architecture
- invariants
- decisions
- constraints
timestamp: 2026-08-04
title: Architecture Invariants
type: architecture
---
# Architecture Invariants

These constraints are the shortest durable test for whether a change still fits the repository architecture. They summarize Scryer responsibilities and current graph evidence; changing one is an architecture decision, not a local refactor.

## Runtime invariants

1. **All product interfaces share one operation model.** CLI, MCP, and embedded clients translate to public typed requests and must not implement separate product semantics.
2. **The operation registry is authoritative.** Dispatch, schemas, catalog output, and MCP tool generation derive from one registered operation catalog.
3. **Repository context is canonical per operation.** Source root, graph path, configuration, and manifest selection are resolved once and reused by the handler.
4. **Validation precedes execution.** Canonical defaults and operation rules are applied before side effects or storage access.
5. **Graph reads are bounded and non-mutating.** Raw statements are single, parameterized, read-only, and result-limited; adapters never bypass the Graph Read Service.
6. **Materialization has one pipeline.** Explicit builds, setup, lifecycle refresh, and watch refresh converge on Source Scanner -> Execution Planner -> Semantic Enricher -> Graph Writer -> Graph Store.
7. **Execution plans are self-contained.** Later stages do not depend on source files remaining present after scanning.
8. **Output is deterministic across execution modes.** Parallel parsing, enrichment, and merging preserve stable identities and collection order.
9. **Relationships carry evidence and satisfy the ontology.** Cross-file inference records evidence or fallback diagnostics, and relationship endpoints are validated before persistence.
10. **Manifest publication follows graph success.** Failed writes do not advertise a newer repository manifest.
11. **Graph concurrency is coordinated at the store.** Writers are serialized with stale-lock recovery; transient lock failures use bounded retry; readers use the same storage boundary.
12. **Refresh orchestrates rather than reimplements.** Event filtering, batching, and retry wrap incremental materialization instead of duplicating indexing logic.

## Knowledge invariants

13. **Curated source is distinct from generated state.** `knowledge/` is authored intent; `.kwiki/` is disposable projection state; `.codebaseGraph/` is source-graph state.
14. **OKF consumption is forward-compatible.** Unknown types, extensions, and links remain consumable and visible while required conformance errors are reported separately.
15. **Wiki publication is generation-atomic.** Failed compilation or rendering preserves the last valid projection, and stale concurrent work cannot replace newer output.
16. **Rendering treats bundle content as untrusted.** Markdown, HTML, links, fragments, and resource identifiers are sanitized before publication.
17. **Authoring is confined and concurrency-safe.** Writes stay beneath configured bundle roots, reject traversal and escaping links, use atomic replacement, and reject stale content hashes.
18. **HTTP is a read-only local preview boundary.** It binds locally by default, applies restrictive browser headers, and does not become an alternate authoring surface.
19. **Graph context is optional.** The wiki calls only the Graph Runtime public API; graph failure returns explicit degraded context without blocking curated knowledge.
20. **Stable identities and URLs outlive implementation refactors.** Concept IDs, directory projections, backlink targets, and published routes remain deterministic.

## Verification invariants

21. **Release checks exercise packaged behavior.** The separate Release Verifier validates repository policy, versions, workflows, CLI artifacts, and MCP negotiation as shipped.
22. **Intent and implementation evidence remain separate.** Scryer is the authored responsibility model; the codebase graph and tests are evidence of the current implementation. Neither silently substitutes for the other.
23. **Architecture documentation records durable boundaries, not transient counts.** Snapshot metrics may qualify confidence, but responsibilities, dependencies, failure modes, and recovery rules are what this wiki preserves.

## When an invariant changes

Update the Scryer model first for changed responsibilities or links, obtain the required architecture sign-off, implement and test the change, reconcile anchors and drift, then update the affected wiki concepts. If code evidence contradicts the wiki without an approved intent change, treat it as drift rather than rewriting the invariant around the implementation.

Related: [Repository Architecture Overview](./overview.md), [Public Operations and Runtime Paths](./operation-paths.md), and [Repository Ownership Map](./repository-map.md).