---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-10-15T07:00:53+09:30
  last_verified_at: 2026-10-15T07:02:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/execution/run.rs:35-56,120-150,172-197 at 55abb2522d410580d96861cd0894127ec7c87206; paths identical in v3.0.2
    content_hash: null
  - kind: source
    reference: src/storage/layout.rs:324-330 and src/storage/managed.rs::begin_write at v3.0.2 (8ba4c316770cba00b9ca8d967aebf2e1ee8670b3)
    content_hash: null
  - kind: runtime-observation
    reference: 2026-10-15 Qunys generation 24545-1792012183188288000-1 published_at_ms=1792012681914; read-only MCP graph_health and File-node discovery; independent 341-file SHA-256 comparison
    content_hash: null
  - kind: test
    reference: '2026-10-15 focused review on HEAD55abb252: 18 refresh unit tests and 2 refresh_recovery_process tests passed; runtime paths verified equivalent to Qunys v3.0.2'
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-15T07:02:00+09:30
    reason: Reviewed against the required independent GPT-6.1 Sol Max source trace, verified Qunys generation timestamps, read-only MCP health and graph discovery, and the independent 341-file SHA-256 comparison. The entry explicitly leaves the earlier incident's cause unproven.
description: The Qunys refresh review verified that whole-generation publication can take minutes despite a 30-second reconciliation interval.
tags:
- diagnostics
- freshness
- mcp
- refresh
timestamp: 2026-10-15T07:00:53+09:30
title: Large graph publication can appear stale while automatic refresh is healthy
type: agent-memory
---
During the 2026-10-15 Qunys automatic-refresh review, a generation took approximately 498.7 seconds from candidate creation to atomic publication. The reviewed Darwin refresh path in v3.0.2 retains the previous readable generation while the execution pipeline stages the complete graph, constructs search files, and loads the replacement database; partition reuse does not remove this whole-generation publication work. The 30-second reconciliation interval schedules checks and is not a completion deadline. A subsequent independent SHA-256 comparison of all 341 graph-discovered manifest files matched the current source with no missing entries and a stable active generation. This establishes that minutes of apparently old query results can coexist with a working refresher on a large graph; it does not establish the cause of the user's earlier incident. Preserve refresh health and worker/pending/error state before a manual build, and distinguish a running build from stopped monitoring or blocked retries before choosing a repair.