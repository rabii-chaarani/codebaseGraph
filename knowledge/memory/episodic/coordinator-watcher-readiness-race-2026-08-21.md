---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-21T15:26:45+09:30
  last_verified_at: 2026-08-21T15:26:45+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 32449306778 job 96674608770
    content_hash: null
  - kind: source
    reference: tests/coordinator_process.rs::twenty_mcp_clients_share_one_coordinator_worker_and_take_over
    content_hash: null
  - kind: source
    reference: src/api/refresh.rs::run_refresh_leader
    content_hash: null
  - kind: test
    reference: cargo test --locked --release --test coordinator_process twenty_mcp_clients_share_one_coordinator_worker_and_take_over
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-21T15:26:45+09:30
    reason: Verified against the Windows CI failure log, run_refresh_leader startup ordering, the test synchronization change, four focused release-mode passes, and the full workspace suite.
description: Coordinator startup metrics can precede watcher readiness, so process tests must synchronize on a native or polling backend before triggering a file change.
tags:
- ci
- coordinator
- refresh
- testing
- windows
timestamp: 2026-08-21T15:26:45+09:30
title: Wait for the refresh watcher before process-test source edits
type: agent-memory
---
GitHub Actions run 32449306778 job 96674608770 failed on Windows in `twenty_mcp_clients_share_one_coordinator_worker_and_take_over` after the startup generation had published and resource metrics were visible. The test then edited `src/lib.rs`, but `run_refresh_leader` performs startup reconciliation before installing and probing its watcher. On the slower Windows runner, the test observed the startup metrics and wrote during that readiness gap; no watch event was delivered, and the generation-change wait expired.

When a process test triggers a source edit after startup reconciliation, require both completed worker metrics and `refresh.backend` equal to `native` or `poll`. Do not treat `phase_high_water_marks` alone as watcher readiness, and do not repair this failure by merely extending the generation timeout because a missed event will not arrive later.