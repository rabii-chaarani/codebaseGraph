---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-21T16:33:39+09:30
  last_verified_at: 2026-08-21T16:35:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 32452550067 job 96683575111
    content_hash: null
  - kind: source
    reference: src/api/refresh.rs
    content_hash: null
  - kind: test
    reference: src/adapters/cli/tests/watch.rs Windows verbatim path regressions
    content_hash: null
  - kind: test
    reference: cargo test --workspace --locked
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-21T16:35:00+09:30
    reason: Verified against both repeated Windows CI logs, the absolute-path-only normalization defect in src/api/refresh.rs, Windows-gated source/probe regressions, four focused release process passes, and the complete workspace suite.
description: Windows notify events may use \\?\ paths that must be normalized before source-root, protected-root, and probe-directory comparisons.
tags:
- ci
- notify
- paths
- watch
- windows
timestamp: 2026-08-21T16:33:39+09:30
title: Normalize Windows verbatim notify paths before watch filtering
type: agent-memory
---
GitHub Actions runs 32449306778 and 32452550067 repeatedly timed out waiting for `active.json` to advance in `twenty_mcp_clients_share_one_coordinator_worker_and_take_over`, even after the test waited for `refresh.backend` to report `native` or `poll`. The watcher could be healthy while silently discarding the source event because Windows `notify` may emit an absolute verbatim path such as `\\?\C:\repo\src\lib.rs`; the filter compared that path against an ordinary source root and normalization was only attempted for relative paths.

Normalize both incoming watcher paths and comparison roots before `strip_prefix`, configuration-path checks, protected-root checks, and native-probe directory classification. Keep Windows-only tests for both a verbatim source event and a verbatim probe event. Watcher backend readiness is useful test synchronization, but it is not evidence that a real source event survives path filtering.