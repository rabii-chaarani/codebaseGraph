---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-08T05:30:12Z
  last_verified_at: 2026-09-08T05:34:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/context.rs:232-286 at f938670
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/daemon.rs:169-181,282-300 at f938670
    content_hash: null
  - kind: source
    reference: src/api/refresh.rs:1347-1355,1452-1460,1546-1553 at f938670
    content_hash: null
  - kind: runtime-observation
    reference: 2026-09-08 LoopAI direct HTTP graph_health and isolated config-only versus explicit-root MCP reproduction with refresh disabled
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/graph-freshness-recovery.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-08T05:34:00Z
    reason: Reviewed against current source and direct LoopAI health; independently reproduced config-only cwd selection versus explicit root with the installed 1.7.0 binary.
description: A config-only managed daemon can inherit cwd as source root and permanently lose its refresher while continuing to serve a healthy old graph.
tags:
- configuration
- mcp
- refresh
- repository-root
timestamp: 2026-09-08T05:30:12Z
title: Config-only MCP can select correct storage but refresh the wrong repository
type: agent-memory
---
In codebase-graph 1.7.0, LoopAI graph health reported a readable managed graph but runtime repo_root=/ and refresh backend=failed with permission denied under /Library. The selected config had the correct repository root. Root cause: daemon startup passed only --config, while resolve_runtime chose cwd before reading config and never adopted config.repo_root; configured storage was still selected correctly. An isolated config-only MCP launched from an unrelated temporary cwd reproduced that split identity; adding --repo-root selected the correct root with the same graph. Startup reconciliation then failed before watcher registration, and the detached refresh thread exited permanently while status retained enabled=true and former leader identity. When diagnosing stale graphs, inspect effective source root and structured refresh state as well as transport/database health. A manual graph rebuild or restart of the same config-only daemon does not fix this cause. Canonical repository binding and supervised refresh recovery are required; see the freshness recovery proposal for the unimplemented design.