---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-23T23:48:16Z
  last_verified_at: 2026-09-23T23:49:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: 'eae5ae3 fix(mcp): recover sessions after daemon restart'
    content_hash: null
  - kind: test
    reference: tests/http_daemon_process.rs::http_daemon_restart_recovers_sessions_without_reusing_old_ids
    content_hash: null
  - kind: test
    reference: 'target/nextest/stab02/junit.xml: 38 focused tests passed on macOS ARM64'
    content_hash: null
  - kind: wiki
    reference: knowledge/stability-backlog.md#stab-02-recover-sessions-after-restart
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-23T23:49:00Z
    reason: Reviewed against eae5ae3, the real-daemon restart test's ordering and PID readiness checks, and its passing result in the 38-test focused JUnit report.
description: Checking a stale ID against an empty session map misses counter-reset aliasing; create new sessions before presenting retired IDs.
tags:
- mcp
- restart
- sessions
- testing
timestamp: 2026-09-23T23:48:16Z
title: Test restart recovery after a competing client initializes
type: agent-memory
---
When testing MCP recovery after daemon restart, retain at least one old client session ID and initialize another client on the restarted daemon before presenting that old ID. A lookup against an empty map proves only rejection, not protection against IDs reused by a reset process-local counter. Assert the retired ID receives HTTP 404 even after new sessions exist, then initialize without the retired header, send notifications/initialized, and successfully call a graph tool. Repeat for clean shutdown and forced termination on the same endpoint. Wait for the replacement child's PID in daemon state rather than trusting the existence of an old state file. Keep the competing client usable and check retired IDs remain invalid after recovery.