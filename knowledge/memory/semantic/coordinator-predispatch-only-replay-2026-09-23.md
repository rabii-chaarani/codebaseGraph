---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-09-23T01:32:44Z
  last_verified_at: 2026-09-23T01:32:44Z
  verified_by: codex
  review_after: null
  supersedes:
  - coordinator-frame-timeout-retry-2026-08-21
  superseded_by: null
  sources:
  - kind: source
    reference: src/coordinator.rs::CoordinatorClient::send_command_with_recovery and send_to_state_with_context
    content_hash: null
  - kind: test
    reference: src/coordinator/tests.rs::ambiguous_operation_disconnect_is_not_replayed_or_retryable
    content_hash: null
  - kind: test
    reference: src/coordinator/tests.rs::caller_deadline_does_not_release_the_real_execution_permit
    content_hash: null
  - kind: test
    reference: 'target/nextest/stab01/junit.xml: all 48 focused regressions passed on macOS ARM64'
    content_hash: null
  - kind: wiki
    reference: knowledge/architecture/graph-runtime.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-23T01:32:44Z
    reason: Reviewed against the implemented non-replay and permit-retention paths and passing deterministic plus real-process tests in the 48-test focused run.
description: An ambiguous coordinator disconnect must never replay graph work; only explicit pre-dispatch failures are safely retryable.
tags:
- coordinator
- deadlines
- mcp
- retry
- stability
timestamp: 2026-09-23T01:32:44Z
title: Coordinator retries require proof that execution did not begin
type: agent-memory
---
Coordinator retries require explicit evidence that the operation was not dispatched. A server-side request-frame receive failure is safe to retry on the same owner within the remaining budget; ordinary authentication rejection can refresh routing. An operation write/read failure after connection establishment is ambiguous and must be non-retryable: it may represent running or completed work, including mutations. Never replay even once on that ambiguity. Hook tool calls also do not retry busy or deadline failures. Transport timeout is not native cancellation: keep the single execution permit occupied until the real operation finishes, even after its caller has gone away. Otherwise repeated hooks can accumulate abandoned work behind an apparently free transport slot. This supersedes the older advice allowing one replay after an ambiguous disconnect.