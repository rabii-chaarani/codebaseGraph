---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codebaseGraph maintainers
  created_at: 2026-08-18
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: benchmark_artifact
    reference: /private/tmp/codebasegraph-pr2-retired-semantic-final.vdMpQA
    content_hash: null
  - kind: benchmark_artifact
    reference: /private/tmp/codebasegraph-pr2-sidecar-sem-ceiling.4rYvuX
    content_hash: null
  - kind: source
    reference: src/db_writer/phase.rs
    content_hash: null
  - kind: source
    reference: src/search_index/build.rs
    content_hash: null
  - kind: test
    reference: src/execution/run.rs::retired_semantic_settings_do_not_change_materialization
    content_hash: null
  history: []
description: Benchmark evidence for retiring semantic enrichment and selecting disk-backed search.
tags:
- materialization
- memory
- benchmark
- ladybug
- search
timestamp: 2026-08-18
title: Reference materialization memory gates
type: agent-memory
---
On the reference graph (493,262 nodes, 1,067,696 edges, and 2,135,392 connector rows), native Ladybug FTS construction peaked at approximately 1.76–2.35 GiB RSS and therefore cannot satisfy the 768 MiB worker ceiling. Disabling semantic enrichment and using the generation-owned `disk_bm25_v1` sidecar completed with a database-phase high-water mark of 491,798,528 bytes and a search-builder high-water mark of 33,554,424 bytes. A diagnostic semantic-enrichment build reached 864,468,992 bytes during the Ladybug File phase and was terminated by the 768 MiB supervisor. Therefore production materialization keeps semantic enrichment retired and uses the disk sidecar for new generations; legacy generations without backend metadata continue to use Ladybug FTS.