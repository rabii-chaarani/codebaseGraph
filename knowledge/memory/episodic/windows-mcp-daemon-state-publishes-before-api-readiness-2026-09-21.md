---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-21T00:00:00Z
  last_verified_at: 2026-09-21T01:15:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 35546568815 job 106173444208
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/daemon.rs:326-341
    content_hash: null
  - kind: source
    reference: src/coordinator.rs:346-399
    content_hash: null
  - kind: source
    reference: tests/http_daemon_process.rs:468-506
    content_hash: null
  - kind: source
    reference: src/coordinator.rs:182-200
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-21T01:15:00Z
    reason: Verified against GitHub Actions run 35546568815 job 106173444208, daemon/coordinator startup ordering, the implemented retryable-only bounded readiness probe, and passing focused plus full HTTP daemon release integration tests.
description: The managed daemon state file can appear before the coordinator owner has finished building its API, so the first MCP graph_health call may return a retryable startup error on slow Windows runners.
tags:
- ci
- daemon
- mcp
- startup
- testing
- windows
timestamp: 2026-09-21T00:00:00Z
title: Windows HTTP daemon tests must probe graph readiness after state publication
type: agent-memory
---
In PR #117 CI run 35546568815, `tests/http_daemon_process.rs::config_only_daemon_from_unrelated_cwd_tracks_source_changes` failed on Windows at its first `graph_health` assertion after waiting only for `.codebaseGraph/mcp-daemon.json`. `serve_mcp_daemon` publishes that file after `start_configured_api` returns, but `CoordinatorClient::start_owner` writes coordinator state and spawns a thread that constructs the owner API before entering its accept loop. Therefore the daemon state file is an endpoint/process publication marker, not a graph-operation readiness barrier. The narrow test fix is a bounded graph_health readiness probe after MCP initialize: retry only MCP errors whose structured transport payload marks `retryable: true`, fail immediately on non-retryable runtime/storage errors, and include the final response on timeout. A 20-second deadline aligns the daemon start timeout. This preserves a real failure signal while synchronizing the test with the documented asynchronous startup boundary.
