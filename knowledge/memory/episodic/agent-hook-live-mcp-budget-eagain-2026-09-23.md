---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-09-23T10:03:29+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/agent_hooks.rs:24,1020-1073,1110-1145
    content_hash: null
  - kind: source
    reference: src/mcp_client.rs:47-86
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/daemon.rs:398-462
    content_hash: null
  - kind: test
    reference: tests/agent_hooks_process.rs:455-524
    content_hash: null
  - kind: runtime-observation
    reference: '2026-09-23 live v1.8.1 hook reproduction and timed loopback MCP calls: initialize ~0.0005s, graph_health ~2.5s, graph_search ~4.36s'
    content_hash: null
  history: []
description: Codex SessionStart and UserPromptSubmit hooks can fail open on macOS when real graph operations exceed the 900 ms per-call socket budget, even though registration verification and mocked hook tests pass.
tags:
- agent-hooks
- mcp
- macos
- timeout
- diagnostics
timestamp: 2026-09-23T10:03:29+09:30
title: Live agent hooks can exhaust the per-call MCP budget during repository refresh
type: agent-memory
---
On codebase-graph v1.8.1 in the codebaseGraph repository, live Codex `SessionStart` and `UserPromptSubmit` hooks returned `failed to read managed MCP daemon response: Resource temporarily unavailable (os error 35)`. The installed daemon and direct HTTP health endpoint were healthy. Timed live MCP calls showed `initialize` at about 0.5 ms, `graph_health` at about 2.5 s, and the hook-shaped semantic `graph_search` at about 4.36 s while native refresh was active. Hook execution has a 3 s total deadline and caps session opening and every tool call at 900 ms. The TCP client applies that value as `SO_RCVTIMEO`; macOS reports expiry as EAGAIN/os error 35. Both events call `graph_health`, and prompt submission additionally calls `graph_search`, so the live operations exceed their individual ceiling. The retry classifier does not recognize `Resource temporarily unavailable`, but retrying cannot overcome the current 900 ms ceiling. Registration verification is insufficient evidence of live hook success because it uses malformed input only to assert fail-open behavior, and the positive hook integration test uses an immediate fake HTTP server rather than the real daemon under refresh load. The daemon processes connections serially, so a timed-out client can briefly leave later probes queued while the already-dispatched graph operation finishes; the daemon remains alive and recovers.