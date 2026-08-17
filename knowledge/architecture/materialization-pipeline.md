---
description: Generation-backed source-to-graph execution, artifact reuse, validation, publication, and failure boundaries.
resource: repository-architecture
tags:
- architecture
- artifacts
- generations
- graph-indexing
- materialization
- pipeline
timestamp: 2026-08-18
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
  -> bounded Execution Planner + durable raw-partition artifacts
  -> deterministic external staging merge
  -> disk-backed search sidecar
  -> isolated Ladybug database phases
  -> validate candidate generation
  -> Graph Store atomic publication
```

### 1. Prepare and recover

The Unified API Core normalizes public options and resolves one canonical source root, storage mode, configuration, and manifest context. The Graph Store runs its janitor before normal work, completes deterministic recovery where possible, and rejects invalid operation combinations before execution.

### 2. Discover stable source snapshots

The Source Scanner hashes file metadata before retaining content. Required sources are copied into the run workspace with hash verification; an unstable file is retried up to three times. Planning reports rebuild, delete, reuse, and ignored paths without modifying graph state.

### 3. Reuse or build raw partitions

The Execution Planner computes an artifact key from repository identity, relative path, content hash, language, parser/profile/ontology versions, and artifact schema version. A fixed-size worker pool reserves memory fallibly, emits partitions in stable order, persists each raw artifact, and releases it. A source or artifact that cannot fit the working budget returns a structured `memory_budget_exceeded` failure.

### 4. Assemble deterministic candidate rows

Partitions are reloaded one at a time. Length-prefixed sorted runs merge nodes, edges, connectors, and endpoint types by deterministic keys. Shared identities keep the existing first-nonempty merge behavior, connector endpoints are resolved by merge join, and unique node and edge counts are computed during the final stream. Output chunks never require a graph-sized in-memory collection.

Semantic enrichment is retired from the production pipeline. Legacy configuration and request fields remain readable for compatibility but normalize to disabled and do not affect materialization identity.

### 5. Build the search sidecar

New generations build a generation-owned `disk_bm25_v1` sidecar from externally sorted term postings, document lengths, and bounded metadata tables. It is checksummed and validated with the candidate. Generations without search-backend metadata remain readable through their legacy Ladybug FTS indexes.

### 6. Build and validate the database

Ladybug work is ordered into pre-COPY schema, bulk COPY, and post-COPY indexes. Database phases run in an isolated child with a 25 ms RSS supervisor; the child is terminated before the configured worker ceiling is exceeded. The Ladybug buffer pool starts at 256 MiB and may retry COPY/index pool exhaustion at 320 and 384 MiB, while the configured worker RSS limit remains the hard authority.

Managed mode writes the database, compact manifest v5, metadata, sidecar siblings, and readiness marker into a fresh candidate. The database is closed, reopened read-only, and checked before publication. Existing v4 generations remain readable; their next write performs one bounded full rebuild.

### 7. Publish atomically

The Graph Store rejects a stale-base candidate, then atomically replaces and fsyncs `active.json` under the exclusive state lock. Database, compact manifest, metadata, readiness marker, and search sidecar advance as one generation. A failure or killed database phase preserves the prior active generation.

Explicit Direct mode uses the same candidate principle beside the requested destinations. Checksummed journals recover the database, manifest, and sidecar rename sequence before the next read or write.

### 8. Finish and collect

The run journal records publication and explicit workspace cleanup. Artifact garbage collection removes entries not referenced by the active manifest or a live run. Cleanup errors remain visible as `cleanup_pending` and never mask the primary materialization error.

## Contracts carried through the pipeline

| Contract | Purpose |
| --- | --- |
| Materialization input | Source root, storage target, refresh intent, memory limits, configuration, active manifest, and execution options. |
| Source snapshot and manifest diff | Verified source snapshot plus rebuild, delete, reuse, and ignored decisions. |
| Artifact key and raw partition | Durable parse output with every invalidation dimension encoded in its identity. |
| Compact manifest v5 entry | Path, content hash, language, partition ID, artifact key, row counts, and timestamp. |
| Candidate generation | Self-contained database, manifest, metadata, readiness marker, sidecar files, and validation evidence. |
| Materialization result | Active generation, graph summary, artifact counts, memory high-water marks, spill bytes, pending runs, and cleanup status. |

## Important invariants

- Source snapshots and working buffers are bounded and fallibly reserved.
- Parsing, staging, search construction, and database loading preserve deterministic output.
- Only ontology-approved relationship endpoints enter the graph.
- The active database is never partition-deleted, appended to, or replaced in place.
- Publication advances every generation-owned artifact atomically.
- A failed build, killed child, or publication failure preserves the previously active generation.
- A live generation lease delays retirement; later runtime entries retry deletion.
- Legacy manifests force one bounded complete rebuild and schema-v1 storage rejects mutation until explicit reinstall.
- Refresh orchestrates this pipeline rather than implementing a second indexing path.

## Source evidence

| Stage | Verified symbol and path |
| --- | --- |
| Pipeline orchestration | `execute_materialization_pipeline` and `execute_scanned_materialization` in `src/execution/run.rs`. |
| Bounded execution | `build_execution_plan` in `src/execution/parallel.rs` and `src/artifact_store.rs`. |
| Deterministic spill and merge | Modules under `src/staging_writer`. |
| Disk-backed search | Modules under `src/search_index`. |
| Isolated database phases | `src/db_writer/phase.rs`, `src/db_writer/rss.rs`, and `src/db_writer/write.rs`. |
| Publication and recovery | Storage lifecycle modules under `src/db_writer` and `src/storage`. |

Related: [Graph Runtime](./graph-runtime.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [Architecture Invariants](./invariants.md).
