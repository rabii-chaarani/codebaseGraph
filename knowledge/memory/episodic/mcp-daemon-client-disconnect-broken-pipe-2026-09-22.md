---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-22T07:25:01Z
  last_verified_at: 2026-09-22T07:25:30Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: runtime-observation
    reference: .codebaseGraph/mcp-daemon-failure.json recorded accept_loop Broken pipe (os error 32) after hook installation
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/daemon.rs
    content_hash: null
  - kind: test
    reference: tests/http_daemon_process.rs::one_http_daemon_serves_multiple_sessions_and_rejects_duplicate_owner
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/graph-runtime.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-22T07:25:30Z
    reason: Reviewed against the live accept_loop Broken pipe failure, the narrow daemon response-write fix, and the passing real-process disconnect/session-stability regression.
description: A hook-side deadline can close an HTTP socket before graph_search completes; response-side BrokenPipe is connection-local and must not end the accept loop or discard other MCP sessions.
tags:
- broken-pipe
- daemon
- hooks
- mcp
- recovery
- sessions
timestamp: 2026-09-22T07:25:01Z
title: Hook timeouts must not terminate the managed MCP daemon
type: agent-memory
---
A repository-local hook may hit its three-second deadline while the daemon is still preparing an MCP response. The client then closes its TCP connection, and the server write can return `BrokenPipe` or the platform-equivalent reset. Treat every accepted connection's response-write failure as connection-local: ignore it and continue the managed daemon accept loop. Only listener acceptance failures are process-fatal; authenticated shutdown remains intentional even if its acknowledgement cannot be delivered.

If response-write failure escapes the loop, launchd can restart the daemon with an empty in-memory session table. Existing Codex requests then fail with `-32002: MCP session is not initialized`. Verify the fix with a real daemon process: initialize a session, perform repeated abortive disconnects, assert stable PID, reuse the original session, confirm unknown session IDs still return `-32002`, and confirm authenticated shutdown exits.