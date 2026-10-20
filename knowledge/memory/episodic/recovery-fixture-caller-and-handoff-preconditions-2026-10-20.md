---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-10-20T01:58:09Z
  last_verified_at: 2026-10-20T01:58:09Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: review-comment
    reference: https://github.com/rabii-chaarani/codebaseGraph/pull/152#discussion_r4320594256
    content_hash: null
  - kind: git-source
    reference: 6d603f0:src/adapters/cli/materialization/command.rs; src/api/facade.rs; src/api/core.rs
    content_hash: null
  - kind: executed-control
    reference: docs/testing/worker-recovery-evidence-2026-10-20.json review_followup.automatic_only_control
    content_hash: null
  - kind: source
    reference: tests/refresh_recovery_process.rs::legacy_worker_pid_recovery
    content_hash: 158e6795af9b9dd0b812f9ae7df579694c15238b4a0de5c36cca6a603fc63b5c
  - kind: independent-review
    reference: 2026-10-20 fresh GPT-6.1 Sol Max source/control review independently verified the exact standalone caller trace and both frozen trial results.
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-20T01:58:09Z
    reason: Reviewed against the exact 6d603f0 standalone/default/coordinator caller source, the preserved automatic-only mutant and two actual failing trial logs, and independent fresh GPT-6.1 Sol Max verification. The record distinguishes a falsified original-vacuity hypothesis from adopted fixture hardening.
description: PR152 controls distinguished the standalone CLI route from coordinator isolation and made the legacy-state handoff explicit.
tags:
- process-identity
- recovery
- review
- testing
timestamp: 2026-10-20T01:58:09Z
title: Verify recovery fixture callers and handoff preconditions
type: agent-memory
---
A PR #152 review proposed that successful manual publication made the stale-PID regression vacuous by retiring version-1 ownership through the isolated explicit worker. On revision `6d603f0`, the actual fixture entry point was standalone CLI `build` → `CodebaseGraphApi::new()` → default `ApiCore`, whose `isolate_materialization` flag is false. Coordinator-owned API construction sets that flag true. The helper's cleanup behavior therefore did not establish that the standalone fixture consumed its injected fault.

A validation-only runtime rejected v1 recovery only in the automatic worker path, preserving manual publication. Both the original and hardened fixtures reached the legacy-recovery assertion after manual success and failed with v1 retained and the automatic-only marker. This falsified the driver's initial expectation that the original test would pass; it did not demonstrate an existing false positive. The raw expectation failure and actual trials were retained.

The adopted hardening serializes the legacy record once, restores it after manual publication, and asserts full equality immediately before daemon startup while retaining writer-exclusion, process-survival and eventual-content checks. For multi-stage recovery regressions, trace the exact client constructor and execution mode, then assert the failing state at the operation boundary. Distinguish useful future-path hardening from a reproduced current defect, and preserve negative controls that contradict a review premise.