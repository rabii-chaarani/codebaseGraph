---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-20T09:32:00+09:30
  last_verified_at: 2026-10-20T09:32:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: reviewed-revision
    reference: 6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5
    content_hash: null
  - kind: merged-pull-request
    reference: https://github.com/rabii-chaarani/codebaseGraph/pull/145
    content_hash: null
  - kind: main-revision
    reference: 512b5dc197adf5590afa5d2f7b44550dfa32d8a2
    content_hash: null
  - kind: runtime-observation
    reference: 2026-10-20 Qunys MCP initialize/graph_health; /private/tmp/codebase-graph-watcher-review-20261020/observation-232455.json
    content_hash: null
  - kind: architecture
    reference: Scryer Build Guardian node-z6qhch and legacy recovery claim resp-xqsh3f in merged PR145 model
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-20T09:32:00+09:30
    reason: Reviewed against exact GitHub PR145 merge metadata, matching main revision, the same-repository approved Scryer model, and captured live v3.0.2 MCP runtime evidence.
description: The watcher investigation reviewed an older checkout and v3.0.2 daemon although recovery fixes already existed on main.
tags:
- deployment
- recovery
- review
- worktrees
timestamp: 2026-10-20T09:32:00+09:30
title: Align the reviewed branch, merged recovery work and running daemon
type: agent-memory
---
The 2026-10-20 watcher follow-up found that the reviewed checkout at 6aa5ba8 and the running Qunys v3.0.2 daemon still had PID-only recovery and direct-child termination, while PR #145 had already merged the guardian-based fixes and their Scryer contracts into main. Main was release 3.0.4 at 512b5dc. Wiki text describing containment was valid for that merged work, not proof that the older checkout or live daemon implemented it. Before creating new remediation claims, compare the exact reviewed revision, relevant merged work and live daemon version; reuse the existing claim identities when the repair already exists. A manual graph build updates derived generations but does not upgrade or restart the daemon.