---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-20T00:00:00+09:30
  last_verified_at: 2026-08-20T00:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 32331746250 job 96313611906
    content_hash: null
  - kind: source
    reference: src/adapters/cli/tests/watch.rs
    content_hash: null
  - kind: test
    reference: 'cargo test --locked adapters::cli::tests::watch:: -- --nocapture'
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-20T00:00:00+09:30
    reason: Reviewed against the failed Windows CI log, the shared marker rewrite in src/adapters/cli/tests/watch.rs, and the passing watch-test module after the fresh-marker change.
description: A watch-test retry that rewrites its marker can race the materialization worker's read lock on Windows.
tags:
- ci
- flaky-tests
- watch-tests
- windows
timestamp: 2026-08-20T00:00:00+09:30
title: Windows watch tests must create fresh change markers
type: agent-memory
---
`drive_watch_until_finished` must create a fresh marker path for each retry rather than truncate one shared `created.py`. In the Windows native test job, the refresh worker can still hold the prior marker open while parsing it; the next `fs::write` then fails with OS error 32. Fresh paths preserve repeated change delivery without modifying a file the active refresh may be reading.