---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-20T00:00:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/adapters/mcp/stdio.rs:9-22
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/protocol.rs:6-74
    content_hash: null
  - kind: runtime-observation
    reference: '2026-08-20 ps process tree: multiple codebase-graph mcp start instances remained children of Codex app-server'
    content_hash: null
  history: []
description: The server intentionally has no idle or per-request shutdown path; host-side connection lifecycle determines process cleanup.
tags:
- mcp
- stdio
- lifecycle
- processes
timestamp: 2026-08-20T00:00:00+09:30
title: CodebaseGraph stdio MCP servers require transport closure to exit
type: agent-memory
---
`codebase-graph mcp start` is a long-lived stdio server: it blocks in its request loop until the input stream reaches EOF and has no idle timeout, per-tool-call exit, or supported shutdown request. Consequently, the MCP host must reuse a server session or close its stdio transport when its task/session ends. Materialization workers are separate short-lived child processes and are supervised independently.