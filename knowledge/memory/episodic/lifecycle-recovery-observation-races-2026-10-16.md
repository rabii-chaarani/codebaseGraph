---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-10-16T05:00:00Z
  last_verified_at: 2026-10-16T05:22:00Z
  verified_by: repository-agents
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38848080891/job/116803254633
    content_hash: null
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38848080891/job/116803254559
    content_hash: null
  - kind: git-commit
    reference: 2b07a52:tests/refresh_recovery_process.rs
    content_hash: null
  - kind: git-commit
    reference: 2b07a52:tests/support/lifecycle_faults.rs
    content_hash: null
  - kind: executed-test
    reference: '2026-10-16: cargo test --locked -p codebase-graph --test refresh_recovery_process observation -- --nocapture; 2 passed'
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-10-16T05:22:00Z
    reason: Reviewed the historical observations against both original native job logs and the test-only diff at 2b07a52d713560f60b5fe8aea74c2bd56509cfb2. Strict workspace Clippy, formatting, and both retry regression tests pass. The note preserves the uncertainty about the Windows transport cause and explicitly does not claim platform qualification; active graph readability and 64 no-op reconciliations are reported directly by the macOS failure.
description: Native fault-recovery tests can miss a completed generation or encounter an interrupted read-only coordinator observation.
tags:
- ci
- lifecycle
- recovery
- test-synchronization
timestamp: 2026-10-16T05:00:00Z
title: Lifecycle recovery checks need observable reconciliation windows
type: agent-memory
---
PR #150's failed native run exposed two harness sensitivities while testing recovery, outside the renderer changes under review. The macOS cancellation test timed out requiring a simultaneous `freshness=current`, `pending=false` health snapshot before querying the pending symbol; its last status reported a readable active graph and 64 successful no-op reconciliations with a 100 ms reconciliation interval. The Windows guardian-loss test instead stopped on a `graph_health` response containing `coordinator_transport_error` and OS error 10054. The Windows log establishes the interrupted observation, not its underlying transport cause.

Keep periodic reconciliation slow enough in lifecycle fault fixtures to observe completion while ordinary polling still detects edits. If an eventual-recovery test repeats a read-only health/search observation after a coordinator transport error, bound that retry window and preserve the final failure. Do not replay mutation requests, HTTP failures, or unrelated tool errors under this exception. The repair at `2b07a52` uses a 5-second fixture interval and a 2-second retry window, with regressions for read-only recovery, deadline exhaustion, and non-replay cases.

Retain the actual recovery assertions: owned descendants exit, invalid cancellation capabilities are rejected, the prior generation survives interruption, and both the pending edit and a later independent edit publish automatically. This lesson concerns test synchronization and observation; it does not establish a production transport repair or waive native platform qualification.