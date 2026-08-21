---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: candidate
  owner: codebaseGraph maintainers
  created_at: 2026-08-20T00:00:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/adapters/mcp/daemon.rs
    content_hash: null
  - kind: source
    reference: src/api/lifecycle.rs
    content_hash: null
  - kind: test
    reference: tests/http_daemon_process.rs
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/operation-paths.md
    content_hash: c359640fbaaaa6b0a68dcf2d0a887831beafb67837e851fca959e1485430fc4f
  history: []
description: The durable ownership and registration contract for local MCP transports.
tags:
- mcp
- http
- daemon
- lifecycle
- ownership
timestamp: 2026-08-20T00:00:00+09:30
title: Local MCP harnesses share one managed repository daemon
type: agent-memory
---
CodebaseGraph defaults loopback-capable MCP harnesses to one repository-scoped Streamable HTTP daemon. The daemon must acquire the repository daemon lock before initializing its listener, API, coordinator, or watcher; every compatible harness registers the same persisted loopback URL without a command field. Platform user services supervise restart and reaping, while authenticated shutdown plus bounded forced termination handles lifecycle removal. Stdio remains an explicit compatibility transport. Cloud-brokered connectors are manual public-HTTPS targets and must never receive or expose the loopback endpoint.