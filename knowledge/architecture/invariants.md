---
description: Durable constraints for transport neutrality, immutable graph generations, run recovery, safe knowledge publication, and evidence integrity.
resource: repository-architecture
tags:
- architecture
- constraints
- decisions
- graph-storage
- invariants
timestamp: 2026-08-20
title: Architecture Invariants
type: architecture
---
# Architecture Invariants

These constraints are the shortest durable test for whether a change still fits the repository architecture. They summarize Scryer responsibilities and current graph evidence; changing one is an architecture decision, not a local refactor.

## Runtime invariants

1. **All product interfaces share one operation model.** CLI, MCP, and embedded clients translate to public typed requests and must not implement separate product semantics.
2. **The operation registry is authoritative.** Dispatch, schemas, catalog output, and MCP tool generation derive from one registered operation catalog.
3. **Repository context is canonical per operation.** Source root, storage mode, configuration, and manifest selection are resolved once and reused by the handler.
4. **Validation precedes execution.** Canonical defaults and operation rules are applied before side effects or storage access.
5. **Graph reads are bounded and non-mutating.** Raw statements are single, parameterized, read-only, and result-limited; adapters never bypass the Graph Read Service.
6. **Materialization has one pipeline.** Explicit builds, setup, lifecycle refresh, and watch refresh converge on Source Scanner -> bounded Execution Planner -> deterministic Graph Writer -> Search Index Builder -> isolated database loading -> Graph Store.
7. **Execution state is bounded.** Required source snapshots, partition workers, spill buffers, merge fan-in, search construction, and database phases have explicit fallible limits.
8. **Output is deterministic across execution modes.** Parallel parsing and external merging preserve stable identities, merge priority, and collection order.
9. **Relationships satisfy the ontology.** Parsed relationship endpoints are validated before persistence; retired semantic inference is not part of production materialization.
10. **Managed graph generations are immutable.** Every mutation builds a fresh self-contained candidate; the active database is never partition-deleted, appended to, or replaced in place.
11. **Publication is one atomic pointer change.** A candidate database, manifest, metadata, and readiness marker are validated before `active.json` advances under the state lock. Failure preserves the prior active generation.
12. **Writer and reader lifetimes are explicit.** One exclusive writer lock covers the complete mutation. Each read holds a shared lease on exactly one generation until all database access is complete.
13. **Retirement is lease-aware and immediate.** Superseded generations have no timed retention; they are deleted after the last reader releases, with retryable failures visible as `cleanup_pending`.
14. **Run ownership is durable.** Every build has a leased, journaled `RunWorkspace`; explicit finish or abort reports cleanup errors, and later runtime entry deterministically recovers abandoned work.
15. **Cleanup is confined and primary errors survive.** Cleanup rejects symlinks and escaping paths, is idempotent, and never masks the failure that caused abort.
16. **Artifacts optimize parsing, not persistence correctness.** Raw partitions are content-addressed across every invalidation dimension, compact manifest v5 carries only publication metadata, and all partitions are externally assembled deterministically.
17. **Legacy state is read-only until explicit reinstall.** Schema-v1 reads remain available; mutations return `legacy_storage_requires_reinstall`. Successful reinstall deletes renamed legacy state immediately after validated v2 activation.
18. **Refresh orchestrates rather than reimplements.** Event filtering, batching, recovery, and retry wrap generation-backed materialization instead of duplicating indexing logic.
19. **Refresh ownership and admission are bounded.** One nonblocking refresh lease holder creates the watcher; followers remain read-only. Event churn collapses to one bounded dirty state, overflow forces a full rescan, CodebaseGraph-owned roots are never admitted, and only refresh intent may close an unchanged writer session without publication.
20. **MCP graph and local transport access have one repository owner.** One coordinator lease holder owns the API core and every MCP Ladybug open. One service-managed Streamable HTTP daemon holds the repository daemon lock and gives every loopback-capable harness the same endpoint; concurrent starts cannot initialize a second listener, watcher, or API. Stdio remains explicit compatibility mode, and cloud connectors require a separately deployed public HTTPS endpoint.
21. **Materialization workers cannot outlive supervision.** One worker lease covers request creation through result reconciliation. The child begins only after durable identity and a start gate, exits when its parent control pipe closes, and a successor reaps the recorded PID before cleanup or new work.

## Knowledge invariants

22. **Curated source is distinct from generated state.** `knowledge/` is authored intent; `.kwiki/` is disposable projection state; `.codebaseGraph/` is source-graph state.
23. **OKF consumption is forward-compatible.** Unknown types, extensions, and links remain consumable and visible while required conformance errors are reported separately.
24. **Wiki publication is generation-atomic.** Failed compilation or rendering preserves the last valid projection, and stale concurrent work cannot replace newer output.
25. **Rendering treats bundle content as untrusted.** Markdown, HTML, links, fragments, and resource identifiers are sanitized before publication.
26. **Authoring is confined and concurrency-safe.** Writes stay beneath configured bundle roots, reject traversal and escaping links, use atomic replacement, and reject stale content hashes.
27. **Wiki preview HTTP is a read-only local boundary.** It binds locally by default, applies restrictive browser headers, and does not become an alternate authoring surface; this is distinct from the authenticated MCP Streamable HTTP transport.
28. **Graph context is optional.** The wiki calls only the Graph Runtime public API; graph failure returns explicit degraded context without blocking curated knowledge.
29. **Stable identities and URLs outlive implementation refactors.** Concept IDs, directory projections, backlink targets, and published routes remain deterministic.

## Verification invariants

30. **Release checks exercise packaged behavior.** The separate Release Verifier validates repository policy, versions, workflows, CLI artifacts, and MCP negotiation as shipped.
31. **Intent and implementation evidence remain separate.** Scryer is the authored responsibility model; the codebase graph and tests are evidence of the current implementation. Neither silently substitutes for the other.
32. **Architecture documentation records durable boundaries, not transient counts.** Snapshot metrics may qualify confidence, but responsibilities, dependencies, failure modes, and recovery rules are what this wiki preserves.
33. **Idle storage has a measurable acceptance state.** With no readers, managed v2 has exactly one generation, no run directories, no pending cleanup, stable graph results, and churn size within the greater of 10% or 8 MiB above a clean-control rebuild.

## When an invariant changes

Update the Scryer model first for changed responsibilities or links, obtain the required architecture sign-off, implement and test the change, reconcile anchors and drift, then update the affected wiki concepts. If code evidence contradicts the wiki without an approved intent change, treat it as drift rather than rewriting the invariant around the implementation.

Related: [Repository Architecture Overview](./overview.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), [Public Operations and Runtime Paths](./operation-paths.md), and [Repository Ownership Map](./repository-map.md).