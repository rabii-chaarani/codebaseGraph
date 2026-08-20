---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: codebaseGraph maintainers
  created_at: 2026-08-21T00:00:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/lifecycle.rs
    content_hash: null
  - kind: documentation
    reference: README.md
    content_hash: null
  - kind: runtime-observation
    reference: 2026-08-21 Codex app-server process tree and daemon health verification
    content_hash: null
  history: []
description: How to activate the shared HTTP daemon for an existing harness registration without killing host-owned children.
tags:
- mcp
- migration
- stdio
- http-daemon
- runbook
timestamp: 2026-08-21T00:00:00+09:30
title: Binary upgrades do not migrate loaded stdio MCP registrations
type: agent-memory
---
When an installed harness still launches `codebase-graph mcp start`, replacing the CodebaseGraph binary is insufficient: the harness configuration remains stdio and a running host may have cached it. Rerun `codebase-graph mcp install` for each exact client, scope, name, and repository config with `--mcp-transport http-daemon`. Verify the resulting registration has a loopback `url` and no `command`, and verify `mcp daemon status` reports one healthy PID for the repository. Then restart the harness to release its existing host-owned stdio children and reload the URL. Do not kill those children during migration. Multiple registration names may share the same repository URL and daemon PID.