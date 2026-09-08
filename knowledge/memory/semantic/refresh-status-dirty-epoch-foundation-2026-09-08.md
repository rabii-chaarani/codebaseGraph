---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-08T06:00:00Z
  last_verified_at: 2026-09-08T07:22:07Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/refresh.rs:1167-1610
    content_hash: null
  - kind: test
    reference: src/api/refresh.rs:1960-2045
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/graph-freshness-recovery.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-08T07:22:07Z
    reason: Coordinator reviewed the epoch invariant against commits 45e1cc1 and 920600f and the passing refresh_status_acknowledges_only_the_epoch_captured_by_a_refresh and scheduler_waits_for_the_announced_blocked_retry_deadline tests. The source/test line references reflect the original foundation revision; current symbol evidence preserves the claim.
description: RefreshStatus now reports explicit task liveness and freshness while acknowledging only the epoch captured when a reconciliation began.
tags:
- freshness
- lifecycle
- reconciliation
- refresh
timestamp: 2026-09-08T06:00:00Z
title: Refresh status preserves dirty epochs across in-flight success
type: agent-memory
---
The refresh status contract keeps transport/configuration availability separate from task liveness. `RefreshState` reports `task_alive`, a lifecycle `state`, `effective_root`, successful reconciliation time, pending age, retry deadline, and dirty/reconciled epochs. `mark_refreshing` captures the current dirty epoch; `mark_refreshed` acknowledges that captured epoch only, so edits received during a build remain pending. Terminal `failed` status clears leader and worker identities while preserving pending work. Supervisors can use `mark_dirty`, `acknowledge_reconciliation`, `mark_retrying`, `mark_blocked`, and `mark_stopped` for later recovery orchestration.