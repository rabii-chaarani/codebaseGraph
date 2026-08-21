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
  - kind: source
    reference: src/storage/atomic.rs
    content_hash: null
  - kind: test
    reference: tests/coordinator_process.rs
    content_hash: null
  - kind: test
    reference: cargo test --locked --test coordinator_process twenty_mcp_clients_share_one_coordinator_worker_and_take_over
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-21T16:35:00+09:30
    reason: Verified from two consecutive debug process-test failures with abandoned .worker.json.tmp.1 files from prior PIDs, the PID-scoped implementation, its unit test, two focused debug passes, and the complete workspace suite.
description: A process-local sequence alone can collide with an abandoned atomic-write temp file after coordinator takeover.
tags:
- atomic-write
- coordinator
- process
- storage
- test
timestamp: 2026-08-21T16:33:39+09:30
title: Atomic temp filenames must be unique across coordinator processes
type: agent-memory
---
The coordinator process integration test reproduced `failed to record materialization worker state: File exists (os error 17)` after killing an owner during takeover. `write_json_atomically` named temporary files with only a process-local sequence, so a replacement coordinator could reuse `.worker.json.tmp.1` while the killed process's incomplete file remained. Atomic temp paths shared by multiple processes must include a writer identity such as PID in addition to the per-process sequence. Verify this invariant with a focused unit test and rerun the takeover process test in debug mode, where the slower writer makes the race easier to reproduce.