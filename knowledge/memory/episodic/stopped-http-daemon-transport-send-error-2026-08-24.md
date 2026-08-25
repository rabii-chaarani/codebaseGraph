---
agent_memory:
  created_at: 2026-08-24T16:06:20+09:30
  history:
  - actor: codex
    at: 2026-08-25T10:33:50+09:30
    from: candidate
    reason: Verified against the implemented bounded failure snapshot, service-manager status, manifest/version reconciliation, full locked workspace tests, and the 2026-08-25 isolated macOS launchd socket-activation crash/stop/start smoke.
    to: active
  kind: episodic
  last_verified_at: 2026-08-25T10:33:50+09:30
  owner: codex
  review_after: null
  scope: repository
  sources:
  - content_hash: null
    kind: runtime-observation
    reference: 2026-08-24 launchctl, lsof, daemon status, and MCP initialize/tools-call smoke
  - content_hash: null
    kind: source
    reference: src/adapters/mcp/daemon.rs
  - content_hash: null
    kind: source
    reference: .codex/config.toml and .codebaseGraph/config.json
  - content_hash: null
    kind: documentation
    reference: https://learn.chatgpt.com/docs/extend/mcp
  - content_hash: null
    kind: test
    reference: tests/http_daemon_process.rs
  - content_hash: null
    kind: runtime-observation
    reference: 2026-08-25 isolated launchd socket activation crash, stop, start smoke
  status: active
  superseded_by: null
  supersedes: []
  verified_by: codex
  version: 1
description: Effective MCP configuration can be correct while graph tool calls fail before JSON-RPC because the repository LaunchAgent is loaded but inactive.
tags:
- daemon
- diagnostics
- http
- launchd
- mcp
- socket-activation
- transport
timestamp: 2026-08-25T10:33:50+09:30
title: A stopped managed HTTP daemon surfaces as an MCP transport send error
type: agent-memory
---
On 2026-08-24, `codebase_graph/graph_context` failed in Codex with an HTTP send error for `http://127.0.0.1:42422/mcp`. The effective project-scoped Codex registration, repository setup config, and LaunchAgent all agreed on port 42422, ruling out endpoint drift for this repository. There was no TCP listener, `.codebaseGraph/mcp-daemon.json` was absent, and `launchctl print` showed the service loaded but not running with last exit code 1. Relaunching the existing LaunchAgent restored the listener; MCP `initialize` and a real `tools/call` for `graph_context` both returned HTTP 200.

Diagnose this class of failure at the transport boundary first: compare the failing URL with effective `codex mcp list` output and `.codebaseGraph/config.json`, check `mcp daemon status`, the state file, the listener, and platform service state. If configuration agrees but no listener exists, start the managed daemon and retry before investigating graph operation code. The pre-fix launchd manifest did not persist stdout/stderr and daemon status did not expose the platform last-exit reason, so that historical exit cause was unrecoverable; the hardened runtime now retains one bounded private failure snapshot and reports best-effort supervisor state. Also compare the service binary version with the repository version, because the LaunchAgent runs the installed binary rather than the worktree build.

Implementation verification on 2026-08-25 established the missing macOS recovery mechanism. The GUI launchd domain reported `domain in on-demand-only mode`, leaving both unconditional `KeepAlive` and `StartInterval` requests pending after SIGKILL. A launchd-owned `Sockets` entry for the repository's IPv4 loopback port, adopted by the daemon through `launch_activate_socket`, supplies real connection demand. In an isolated LaunchAgent smoke, killing PID 89165 and then probing status caused launchd to start PID 89196; intentional `mcp daemon stop` unloaded the job and `start` restored it. Persistent HTTP daemons in an on-demand-only launchd domain therefore require socket activation rather than timer or keepalive policy alone.