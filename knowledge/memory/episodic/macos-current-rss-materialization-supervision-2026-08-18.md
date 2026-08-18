---
agent_memory:
  created_at: 2026-08-18T00:00:00+09:30
  history: []
  kind: episodic
  last_verified_at: null
  owner: codebaseGraph maintainers
  review_after: null
  scope: repository
  sources:
  - content_hash: null
    kind: source
    reference: src/db_writer/rss.rs
  - content_hash: null
    kind: source
    reference: src/db_writer/phase.rs
  - content_hash: null
    kind: benchmark_artifact
    reference: /private/tmp/codebasegraph-pr3-footprint.NmAgjn
  - content_hash: null
    kind: benchmark_artifact
    reference: /private/tmp/codebasegraph-pr3-footprint-repeat.hn6WRh
  - content_hash: null
    kind: benchmark_artifact
    reference: /private/tmp/codebasegraph-pr3-mimalloc-repeat.mAV6vB
  status: candidate
  superseded_by: null
  supersedes: []
  verified_by: null
  version: 1
description: Reference-build root causes and repeatability evidence for the macOS worker memory supervisor.
tags:
- benchmark
- ladybug
- macos
- materialization
- memory
- rss
timestamp: 2026-08-18
title: macOS supervision must use composable physical footprint
type: agent-memory
---
The macOS release gate exposed two distinct accounting failures. First, `proc_pid_rusage(...).ri_resident_size` retained an earlier sequential peak and falsely reported 587,137,024 bytes for a parent whose graph-building state had already been released. Second, replacing it with current task RSS removed that error but summing parent and Ladybug-child RSS still double-counted their clean shared runtime mappings; a repeat build was falsely killed at 803,307,520 bytes from a 464,961,536-byte parent plus a 338,345,984-byte child. The stable boundary uses `ri_phys_footprint`, which is current and composable across processes, and routes Rust graph-building allocations through mimalloc with forced collection before Ladybug starts. With semantic enrichment disabled and `disk_bm25_v1` enabled, two consecutive clean builds produced byte-identical manifests for 510,031 nodes and 1,104,464 edges. They completed in 139.04 and 131.57 seconds with database-phase high-water marks of 565,102,272 and 568,051,368 bytes, below the 768 MiB ceiling. macOS supervision must therefore enforce additive physical footprint, not cumulative resident usage or summed raw RSS.