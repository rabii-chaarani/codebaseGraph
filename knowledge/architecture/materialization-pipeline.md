---
description: Source-to-graph execution flow, contracts, determinism, and failure boundaries.
resource: repository-architecture
tags:
- architecture
- materialization
- pipeline
- graph-indexing
timestamp: 2026-08-04
title: Graph Materialization Pipeline
type: architecture
---
# Graph Materialization Pipeline

Materialization converts repository source snapshots into a persistent source graph. Full builds, incremental builds, setup-time builds, and refreshes share the same pipeline; callers vary only in how they select the repository and requested change set.

## Pipeline

```text
public materialization request
  -> canonical repository runtime
  -> Source Scanner
  -> Execution Planner
  -> Semantic Enricher
  -> Graph Writer
  -> Graph Store
  -> repository manifest
```

### 1. Prepare the request

The Unified API Core normalizes public options and resolves one canonical source root, graph location, configuration, and manifest context. Invalid operation combinations fail before execution.

### 2. Discover source changes

The Source Scanner discovers supported files as snapshots and compares them with the previous manifest. Planning can report rebuild, delete, skip, and ignored paths without materializing graph rows.

### 3. Build self-contained execution plans

The Execution Planner parses each snapshot with its selected language profile. Tree-sitter-backed programming-language parsing and Markdown parsing both produce normalized graph rows. Partitions are self-contained, can be built concurrently, and are collected in stable order. Relationship endpoints are checked against the ontology allowlist.

### 4. Enrich cross-file semantics

The Semantic Enricher collects unresolved calls, references, and type annotations across partitions, selects compatible targets, and records both inferred relationships and their evidence. Unresolved or lower-confidence relations retain diagnostic metadata instead of silently disappearing.

### 5. Merge deterministic graph rows

The Graph Writer merges changed partitions, preserves shared identities, creates connectors only when endpoints exist, and avoids replacing populated fields with duplicate empty values. Node and edge ordering remains deterministic across execution modes.

### 6. Persist atomically within the graph boundary

The Graph Store applies schema, safe deletion statements, and staged rows. A write-intent guard serializes writers and supports stale-lock recovery. Transient database lock failures use bounded retry. Readers remain coordinated through the same store boundary.

### 7. Publish the manifest

After a successful graph update, the Materialization API writes the new repository manifest. The manifest is the basis for later incremental source-change planning.

## Contracts carried through the pipeline

| Contract | Purpose |
| --- | --- |
| Materialization input | Source root, graph target, configuration, previous manifest, language profiles, and execution options. |
| Source snapshot and manifest diff | Immutable source payload plus rebuild/delete/skip decisions. |
| Execution plan / partition | Parsed, self-contained node and relationship rows for one source snapshot. |
| Enriched plan | Cross-file semantic relationships, evidence, and fallback diagnostics. |
| Graph node and edge rows | Deterministically mergeable persistence payload. |
| Materialization result | Graph summary, manifest changes, rebuilt entries, and database-write outcome. |

## Important invariants

- Removing source files after scanning does not invalidate a self-contained execution plan.
- Parallel planning must preserve stable collected output.
- Only ontology-approved relationship endpoints enter the graph.
- Incremental deletion targets replaced partitions and superseded incoming rows, not unrelated graph state.
- The manifest advances only after the graph write succeeds.
- Refresh is an orchestrator of this pipeline, not a second implementation of indexing.

## Source evidence

| Stage | Verified symbol and path |
| --- | --- |
| Pipeline orchestration | `execute_materialization_pipeline` and `execute_scanned_materialization` in `src/execution/run.rs`. |
| Parallel plan building | `build_execution_plan` in `src/execution/parallel.rs`. |
| Parsing and row creation | `src/parser` and `src/syntax_materializer`. |
| Semantic enrichment | `enrich_semantics` in `src/semantic_enrichment/mod.rs`. |
| Deterministic staging | `write_graph_rows` in `src/staging_writer/writer.rs`. |
| Database application | `write_database` and concurrency helpers under `src/db_writer`. |

Related: [Graph Runtime](./graph-runtime.md) and [Architecture Invariants](./invariants.md).