---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-21T14:34:08+09:30
  last_verified_at: 2026-08-21T14:34:08+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 32440497451 job 96650083565
    content_hash: null
  - kind: source
    reference: src/coordinator.rs
    content_hash: null
  - kind: test
    reference: coordinator::tests::request_receive_failure_is_retryable_before_dispatch
    content_hash: null
  - kind: test
    reference: coordinator::tests::retryable_receive_failure_retries_the_same_owner
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/graph-runtime.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-21T14:34:08+09:30
    reason: Verified against GitHub Actions run 32440497451 job 96650083565, the implemented coordinator retry path, two deterministic regressions, 20 stress repetitions, and the complete workspace test suite.
description: macOS reports socket read timeouts as EAGAIN; pre-dispatch coordinator receive failures must be retryable without forcing owner election.
tags:
- ci
- coordinator
- macos
- mcp
- retry
- transport
timestamp: 2026-08-21T14:34:08+09:30
title: Retry coordinator frame receive timeouts before dispatch
type: agent-memory
---
GitHub Actions CI run 32440497451 failed on macOS ARM in `coordinator::tests::concurrent_clients_share_one_repository_coordinator`. The owner timed out reading a request frame and macOS surfaced `Resource temporarily unavailable (os error 35)`; the server encoded that pre-dispatch transport condition as a non-retryable `coordinator_protocol_error`, so a healthy client failed after route refresh.

Treat request-frame receive failures as explicitly retryable because no operation was dispatched. Retry those replies against the same live owner within a bounded window; refresh the route for authentication failures, and keep only one replay for ambiguous disconnects that may occur after dispatch. Apply a read timeout to Ping only. Do not apply the short ping timeout to operation replies, because valid materialization and lifecycle requests may run much longer and replaying them after a client-side timeout can duplicate side effects.