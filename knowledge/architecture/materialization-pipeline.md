---
description: Generation-backed source-to-graph execution, artifact reuse, validation, publication, and failure boundaries.
resource: repository-architecture
tags:
- architecture
- graph-indexing
- materialization
- pipeline
- generations
- artifacts
timestamp: 2026-08-06
title: Graph Materialization Pipeline
type: architecture
---
# Graph Materialization Pipeline

Materialization converts repository source snapshots into a fresh persistent source-graph generation. Full builds, incremental builds, setup-time builds, and refreshes share the same pipeline; callers vary only in repository selection and requested change set. “Incremental” describes planning and artifact reuse, not mutation of the active database.

## Pipeline

```text
public materialization request
  -> canonical repository runtime
  -> recover abandoned runs / direct publication
  -> Source Scanner
  -> Execution Planner + durable raw-partition artifacts
  -> Semantic Enricher across all partitions
  -> Graph Writer
  -> validate candidate generation
  -> Graph Store atomic publication
```

### 1. Prepare and recover

The Unified API Core normalizes public options and resolves one canonical source root, storage mode, configuration, and manifest context. The Graph Store runs its janitor before normal work, completes deterministic recovery where possible, and rejects invalid operation combinations before execution.

### 2. Discover source changes

The Source Scanner discovers supported files as immutable snapshots and compares them with the active manifest. Planning reports rebuild, delete, reuse, and ignored paths without modifying graph state. Every source hash is revalidated before an artifact can be reused.

### 3. Reuse or build raw partitions

The Execution Planner computes an artifact key from repository identity, relative path, content hash, language, parser/profile/ontology versions, and artifact schema version. A valid matching raw partition is reused; a missing or corrupt entry is rebuilt.

Tree-sitter-backed programming-language parsing and Markdown parsing both produce normalized graph rows. Partitions are self-contained, may be built concurrently, and are collected in stable order. Relationship endpoints are checked against the ontology allowlist.

### 4. Enrich the complete graph

The Semantic Enricher runs across every reused and rebuilt partition. It resolves calls, references, and type annotations, records inferred relationships with evidence, and retains diagnostic metadata for unresolved or lower-confidence relations. Reusing raw partitions never skips global semantic enrichment.

### 5. Assemble deterministic candidate rows

The Graph Writer combines all partitions, preserves shared identities, creates connectors only when endpoints exist, and avoids replacing populated fields with duplicate empty values. Node and edge ordering remains deterministic across execution modes and equivalent to a clean rebuild.

### 6. Build and validate a candidate generation

Managed mode writes the database and manifest into a fresh run candidate. The database is closed, reopened read-only, and checked for schema, counts, artifact references, and repository metadata before metadata and `READY` are made durable. The active generation remains untouched throughout this work.

### 7. Publish atomically

The Graph Store rejects a stale-base candidate, then atomically replaces and fsyncs `active.json` under the exclusive state lock. The manifest is part of the newly active generation; graph and manifest therefore advance together. The prior generation is retired and removed as soon as its final read lease is released.

Explicit Direct mode follows the same candidate principle beside the requested database and manifest destinations. A checksummed journal recovers the paired rename sequence before the next read or write.

### 8. Finish and collect

The run journal records `published`, explicit workspace cleanup runs, and artifact garbage collection removes entries not referenced by the active manifest or a live run. Cleanup errors are visible as `cleanup_pending` and never mask the primary materialization error.

## Contracts carried through the pipeline

| Contract | Purpose |
| --- | --- |
| Materialization input | Source root, storage target or explicit Direct paths, configuration, active manifest, language profiles, and execution options. |
| Source snapshot and manifest diff | Immutable source payload plus rebuild, delete, reuse, and ignored decisions. |
| Artifact key and raw partition | Durable parse output with every invalidation dimension encoded in its identity. |
| Enriched plan | Cross-file semantic relationships, evidence, and fallback diagnostics over the complete repository. |
| Candidate generation | Self-contained database, manifest v2, metadata, readiness marker, and validation evidence. |
| Materialization result | Active generation, graph summary, reused/rebuilt artifact counts, pending runs, cleanup status, and physical/logical sizes. |

## Important invariants

- Removing source files after scanning does not invalidate a self-contained execution plan.
- Parallel planning preserves stable collected output.
- Only ontology-approved relationship endpoints enter the graph.
- The active database is never partition-deleted, appended to, or replaced in place.
- Publication advances the database and manifest as one generation.
- A failed build or publication preserves the previously active generation.
- A live generation lease delays retirement; the last reader and later runtime entries retry deletion.
- Legacy manifests force one complete artifact rebuild and schema-v1 storage rejects mutation until explicit reinstall.
- Refresh orchestrates this pipeline rather than implementing a second indexing path.

## Source evidence

| Stage | Verified symbol and path |
| --- | --- |
| Pipeline orchestration | `execute_materialization_pipeline` and `execute_scanned_materialization` in `src/execution/run.rs`. |
| Parallel plan building | `build_execution_plan` in `src/execution/parallel.rs`. |
| Parsing and row creation | `src/parser` and `src/syntax_materializer`. |
| Semantic enrichment | `enrich_semantics` in `src/semantic_enrichment/mod.rs`. |
| Deterministic staging | `write_graph_rows` in `src/staging_writer/writer.rs`. |
| Candidate writing, validation, publication, and recovery | Storage lifecycle and database writer modules under `src/db_writer`. |

Related: [Graph Runtime](./graph-runtime.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [Architecture Invariants](./invariants.md).