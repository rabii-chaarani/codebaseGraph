---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-28T12:39:47+09:30
  last_verified_at: 2026-08-28T12:39:47+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/execution/parallel.rs:216-321
    content_hash: null
  - kind: source
    reference: src/artifact_store.rs:210-244
    content_hash: null
  - kind: runtime-observation
    reference: '2026-08-28 codebaseGraph full rebuild: default split failed; single-worker 2048/2560 succeeded with 3,256,287 nodes'
    content_hash: null
  - kind: source
    reference: .codebaseGraph/config.json
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-28T12:39:47+09:30
    reason: Verified against the current materialization and artifact-load budget checks in source, the successful 1.7.0 full rebuild, and the persisted repository-local configuration.
description: The codebaseGraph repository’s own full graph exceeds the default two-worker memory split; a single worker with a larger bounded budget completes the rebuild.
tags:
- codebase-graph
- materialization
- memory
- rebuild
- runbook
timestamp: 2026-08-28T12:39:47+09:30
title: Use a larger single-worker budget when rebuilding codebaseGraph itself
type: agent-memory
---
When rebuilding the codebaseGraph repository itself, the default `rust_memory_mib=384` with `max_parallelism=2` gives each worker only 192 MiB and can fail during `partition_build` or artifact reload. Use `--single-thread --rust-memory-mib 2048 --worker-memory-mib 2560` for the explicit full build, then persist the same `max_parallelism=1`, `rust_memory_mib=2048`, and `worker_memory_mib=2560` values in the repository-local `.codebaseGraph/config.json` before restarting its managed daemon. Keep the normal 384/768 defaults for smaller repositories unless their own health/build evidence requires otherwise.