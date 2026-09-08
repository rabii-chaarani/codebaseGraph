---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-08T08:10:30Z
  last_verified_at: 2026-09-08T08:11:50Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/34200148261/job/101976833482?pr=115
    content_hash: null
  - kind: source
    reference: src/api/refresh.rs::run_refresh_leader at fac7b23 and the native startup label fix
    content_hash: null
  - kind: test
    reference: api::refresh::tests::native_refresh_startup_reports_native_after_reconciliation (red before fix, green after)
    content_hash: null
  - kind: test
    reference: tests/coordinator_process.rs::twenty_mcp_clients_share_one_coordinator_worker_and_take_over (debug serial pass)
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-08T08:11:50Z
    reason: Reviewed against the failed Linux job's populated metrics/startup backend, the current native observer call chain, the failing-before/passing-after forced-native regression, and the passing original debug coordinator test.
description: A startup phase label overwrote native monitoring identity and made a successful Linux coordinator startup fail readiness checks.
tags:
- ci
- native
- observability
- refresh
timestamp: 2026-09-08T08:10:30Z
title: Preserve selected backend identity through startup reconciliation
type: agent-memory
---
PR #115's Linux job failed twenty_mcp_clients_share_one_coordinator_worker_and_take_over with a readable graph and published worker metrics, but refresh.backend remained startup. run_refresh_leader selected native, then passed the phase label startup into the refresh observer, which overwrote backend during successful reconciliation. Without a follow-up native event, the next periodic reconciliation was 30 seconds away while readiness validation waited 10 seconds. Startup reconciliation must carry the selected native/poll backend; lifecycle progress belongs in its separate status fields. The targeted regression forces native mode and a reconciliation interval longer than its startup deadline, and asserts backend immediately after first successful reconciliation without a source edit. It failed with the old label and passed after preserving native. Auto-only tests can conceal backend-specific startup defects through fallback or subsequent events; do not fix this class by weakening readiness predicates or extending their timeout.