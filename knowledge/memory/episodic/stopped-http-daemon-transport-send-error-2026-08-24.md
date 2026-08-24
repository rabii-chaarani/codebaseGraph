---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-24T16:06:20+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: runtime-observation
    reference: 2026-08-24 launchctl, lsof, daemon status, and MCP initialize/tools-call smoke
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/daemon.rs
    content_hash: null
  - kind: source
    reference: .codex/config.toml and .codebaseGraph/config.json
    content_hash: null
  - kind: documentation
    reference: https://learn.chatgpt.com/docs/extend/mcp
    content_hash: null
  history: []
description: Effective MCP configuration can be correct while graph tool calls fail before JSON-RPC because the repository LaunchAgent is loaded but inactive.
tags:
- daemon
- diagnostics
- http
- launchd
- mcp
- transport
timestamp: 2026-08-24T16:06:20+09:30
title: A stopped managed HTTP daemon surfaces as an MCP transport send error
type: agent-memory
---
On 2026-08-24, `codebase_graph/graph_context` failed in Codex with an HTTP send error for `http://127.0.0.1:42422/mcp`. The effective project-scoped Codex registration, repository setup config, and LaunchAgent all agreed on port 42422, ruling out endpoint drift for this repository. There was no TCP listener, `.codebaseGraph/mcp-daemon.json` was absent, and `launchctl print` showed the service loaded but not running with last exit code 1. Relaunching the existing LaunchAgent restored the listener; MCP `initialize` and a real `tools/call` for `graph_context` both returned HTTP 200.

Diagnose this class of failure at the transport boundary first: compare the failing URL with effective `codex mcp list` output and `.codebaseGraph/config.json`, check `mcp daemon status`, the state file, the listener, and platform service state. If configuration agrees but no listener exists, start the managed daemon and retry before investigating graph operation code. The current launchd manifest does not persist stdout/stderr and daemon status does not expose the platform last-exit reason, so the historical exit cause may be unrecoverable; add durable service diagnostics when hardening this path. Also compare the service binary version with the repository version, because the LaunchAgent runs the installed binary rather than the worktree build.