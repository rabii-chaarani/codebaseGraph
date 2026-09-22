---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-22T00:00:00Z
  last_verified_at: 2026-09-22T14:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/lifecycle.rs
    content_hash: null
  - kind: test
    reference: src/adapters/cli/tests/install.rs
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-22T14:00:00+09:30
    reason: Reviewed against the PR review fixes and passing 17 install tests plus 28 lifecycle tests.
description: Lifecycle setup/reinstall and direct MCP installs must keep the managed loopback daemon available whenever local hooks are selected, independent of MCP registration transport; setup/reinstall replace prior hook ownership while uninstall cleans hook ownership independently from MCP client filtering.
tags:
- agent-hooks
- daemon
- lifecycle
- ownership
- rollback
timestamp: 2026-09-22T00:00:00Z
title: Lifecycle hook ownership changes provision the shared daemon and remove stale clients transactionally
type: agent-memory
---
When lifecycle setup or reinstall selects repository-local agent hooks, provision the existing managed loopback daemon even if MCP registration is skipped or explicitly stdio, because hook commands query the daemon's HTTP endpoint. In setup/reinstall, preserve the prior `agent_hooks.installed_clients` metadata through config replacement, snapshot both selected and previously owned hook files, remove deselected managed handlers, then install the desired set; explicit `agent_hooks=none` is a no-op that preserves files and ownership. During uninstall, derive hook cleanup from recorded ownership and fall back to all supported managed clients only when ownership metadata is absent; keep MCP registration removal filtered by `--mcp-client`.