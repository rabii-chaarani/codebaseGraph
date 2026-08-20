---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-14T15:15:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/adapters/mcp/stdio.rs:9-22
    content_hash: null
  - kind: source
    reference: src/api/refresh.rs:35-137, 703-805, 1001-1060
    content_hash: null
  - kind: source
    reference: src/staging_writer/accumulator.rs:17-198
    content_hash: null
  - kind: source
    reference: src/staging_writer/connectors.rs:19-91
    content_hash: null
  - kind: source
    reference: src/api/catalog_support.rs:43-83
    content_hash: null
  - kind: source
    reference: src/db_writer/write.rs:16-37
    content_hash: null
  - kind: runtime-profile
    reference: macOS ps/vmmap/sample observations on 2026-08-14; sampled MCP refresh stacks included StagingAccumulator::finish and Ladybug FTSIndex::finalize
    content_hash: null
  history: []
description: Every stdio MCP instance starts its own watcher; multiple instances for one repository repeat full generation materialization and amplify connector/FTS peak memory.
tags:
- mcp
- memory
- refresh
- materialization
- fts
- concurrency
- profiling
timestamp: 2026-08-14T15:15:00+09:30
title: Duplicate MCP refreshers create serialized high-memory rebuild convoys
type: agent-memory
---
A live macOS diagnosis of package version 1.3.1 found that MCP idle memory was small (roughly 18–33 MiB RSS), while refresh-triggered generation builds reached multi-gigabyte RSS and 7.5–10.8 GiB physical-footprint peaks. The key amplifier was process multiplicity: every `mcp start` instance unconditionally created an auto-refresh thread, so multiple Codex tasks watching the same repository all accepted the same filesystem event. The cross-process writer lock serialized publication but did not elect one refresher or deduplicate already-consumed events; this formed a convoy in which one MCP process built while peers waited, then peers repeated generation materialization.

Stack samples identified two dominant allocation stages. Rust-side `StagingAccumulator::finish` materialized all nodes, edges, edge connectors, and two connector rows per edge in nested hash maps with duplicated owned strings. Ladybug-side bulk `COPY` maintained FTS indexes that had been created before data import; `FTSIndex::finalize`, `DocInfo`, and regex construction dominated another sampled peak. A 436,361-node/1,392,295-edge generation therefore expanded far beyond its ~1.1 GiB database size while building. The watch filter also accepted any non-excluded path rather than only supported source files, so generated `.kwiki`, `.scryer`, `.astro`, and `.DS_Store` changes could initiate the expensive pipeline.

Reusable diagnostic: distinguish idle-server memory from refresh memory, count MCP instances per repository, sample the active refresh thread, inspect staged run journals for a writer-lock convoy, and check generated-path exclusions before treating the symptom as a persistent heap leak.