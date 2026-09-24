---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-24T01:02:24Z
  last_verified_at: 2026-09-24T01:02:24Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35940207624/job/107446231120
    content_hash: null
  - kind: source
    reference: src/adapters/mcp/dispatcher.rs::slow_reader_does_not_block_other_responses_and_is_released
    content_hash: null
  - kind: test
    reference: 'target/nextest/dispatcher-sync/junit.xml: all seven dispatcher tests passed on macOS ARM64'
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T01:02:24Z
    reason: Reviewed the CI timeout location, the premature readiness signal before fixture construction/serialization, the new first-byte synchronization, and the seven passing dispatcher tests.
description: A readiness signal sent before building and serializing a large fixture charged setup work against the unrelated client's responsiveness budget.
tags:
- deadlines
- macos
- mcp
- testing
timestamp: 2026-09-24T01:02:24Z
title: Synchronize slow-reader isolation tests after response preparation
type: agent-memory
---
macOS ARM64 CI failed slow_reader_does_not_block_other_responses_and_is_released when the fast client hit its one-second socket timeout (WouldBlock/EAGAIN). The test's readiness channel fired before the dispatcher constructed and serialized a 16 MiB fixture response, so the isolation measurement included fixture preparation. Wait for the slow client to receive the first response byte before starting the fast request, then leave the slow body unread. This synchronizes on actual transmission while preserving the fast client's one-second timeout, the existing connection-release assertion, and production deadlines. The setup read has its own bounded timeout; do not fix this class of test by relaxing the isolation threshold or adding sleeps.