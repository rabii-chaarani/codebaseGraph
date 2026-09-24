---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-24T00:48:52Z
  last_verified_at: 2026-09-24T00:48:52Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35935522699/job/107431938836
    content_hash: null
  - kind: source
    reference: src/agent_hooks.rs::try_lock_cache_claim, take_cached_context, purge_cache
    content_hash: null
  - kind: test
    reference: tests/agent_hooks_process.rs::copilot_cached_context_claim_lock_is_process_safe
    content_hash: null
  - kind: test
    reference: 'target/nextest/cache-claim/junit.xml: all 20 hook tests passed on macOS ARM64'
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/agent-loop-hooks.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T00:48:52Z
    reason: Reviewed against the Windows double-delivery failure, the stable nonblocking lock implementation, and passing unit plus real-process regression tests in the 20-test JUnit report.
description: Cross-process cache delivery requires explicit exclusion; preserving the lock file avoids splitting concurrent consumers across different filesystem identities.
tags:
- concurrency
- hooks
- locking
- testing
- windows
timestamp: 2026-09-24T00:48:52Z
title: Keep hook cache claim locks stable and nonblocking
type: agent-memory
---
Windows CI observed two deliveries from one Copilot cache entry while the consumer relied on rename to a unique claimed filename. Cache claims must use a nonblocking exclusive OS file lock through rename, read, and delivery marking. A contending hook returns no context without consuming the pending entry. Keep the lock file stable: closing its handle releases the lock, but deleting or replacing its pathname can let concurrent processes lock different filesystem objects. Cache expiry must therefore skip the lock file. One repository-wide claim lock bounds retained lock files; contention across sessions follows the same advisory fail-open policy. Verify with a parent-held lock and a separate hook process, delivery after release, concurrent real processes with exactly one delivery, and a purge test that preserves sentinel bytes in the lock file.