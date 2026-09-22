---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-22T05:42:44Z
  last_verified_at: 2026-09-22T05:43:10Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/storage/managed.rs
    content_hash: null
  - kind: source
    reference: src/storage/locks.rs
    content_hash: null
  - kind: test
    reference: src/storage/test_harness.rs
    content_hash: null
  - kind: ci
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35688919584/job/106621560622
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-22T05:43:10Z
    reason: Reviewed against the Windows CI failure, the managed cleanup implementation, the non-creating lock regression, and passing refresh-recovery process tests.
description: Windows refresh recovery can expose a run-directory disappearance race during graph reads; cleanup must probe existing locks without recreating paths and ignore only NotFound transitions.
tags:
- concurrency
- managed-storage
- recovery
- refresh
- windows
timestamp: 2026-09-22T05:42:44Z
title: Treat vanished managed run workspaces as benign cleanup races
type: agent-memory
---
## Reusable procedure

When a managed graph read overlaps refresh cleanup, a `runs/run-*` directory may disappear after `read_dir` has yielded it. Windows reports this as `ERROR_PATH_NOT_FOUND` (`io::ErrorKind::NotFound`). Treat `NotFound` from the directory entry, file type, existing lease probe, or journal read as a benign completed-cleanup race, but continue propagating all other errors.

Use a non-creating lock probe for cleanup: it must never recreate a vanished run directory merely to inspect `lease.lock`. Verify with the storage-foundation regression and the real `refresh_recovery_process` daemon tests. Do not broaden retries or suppress arbitrary runtime-resolution failures.