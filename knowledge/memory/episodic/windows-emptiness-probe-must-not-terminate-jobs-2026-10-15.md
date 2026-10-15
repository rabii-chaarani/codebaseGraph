---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-15T03:07:20Z
  last_verified_at: 2026-10-15T03:07:20Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/process_lifecycle/windows.rs::recover_empty at e46b2b6a468cbbe911ba94a3ad24584478e84997; src/process_lifecycle/mod.rs::recover_descriptor
    content_hash: null
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38669713929/job/116209863297; unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven
    content_hash: null
  - kind: source
    reference: Read-only probe correction at 7c368f8; src/process_lifecycle/windows.rs
    content_hash: 8dc6b6c8c873b191bad9cb41b1e6b66a88fe05b2f6c7d898bbca9d36d8af75a8
  - kind: test
    reference: src/process_lifecycle/windows.rs::tests::recovery_probe_does_not_terminate_a_nonempty_job (compiled for Windows; native execution required)
    content_hash: null
  - kind: review
    reference: Independent GPT-6.1 Sol Max focused review approved the read-only probe and owned-job test against e46b2b6; platform source hash 8dc6b6c8c873b191bad9cb41b1e6b66a88fe05b2f6c7d898bbca9d36d8af75a8
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-15T03:07:20Z
    reason: Reviewed the claim against the executed Windows failure, complete pre-authentication source trace, read-only correction, and independent Max review. The record explicitly preserves the pending native-runtime verification limit.
description: A Windows recovery probe terminated a nonempty job before guardian authentication, retiring state while a guardian lease was still live.
tags:
- failure-handling
- process-lifecycle
- recovery
- windows
timestamp: 2026-10-15T03:07:20Z
title: Keep containment probes separate from authenticated teardown
type: agent-memory
---
During the worker-lifecycle repair, native Windows CI passed 13 of 14 refresh/recovery cases but failed the unavailable-guardian case because `worker.json` had disappeared. The complete source trace showed that shared recovery called Windows `recover_empty` before authenticated guardian control; that purported probe opened the job with termination rights, killed a nonempty job, awaited zero active processes, and returned true. Recovery then removed global metadata while workspace collection still found the paused guardian's live lifetime lease.

Keep observation and teardown separate. An emptiness probe should open the existing job with `JOB_OBJECT_QUERY`, return whether `ActiveProcesses == 0`, and close its temporary handle promptly. A nonempty result must proceed to authenticated guardian cancellation; failed authentication or unavailable control preserves unknown containment and pending work. Missing jobs are empty proof, while proof mismatch, denied access and failed queries remain unknown. Opening a query handle must not extend the abandoned job's lifetime beyond the probe.

The correction adds a native Windows regression using a separately owned, suspended contained child: probing must leave its process unsignalled and job nonempty, after which the fixture explicitly tears down its own job. Do not assign the test runner itself. At recording time the correction passed actual-source Windows compilation and strict Clippy; its Windows runtime execution remains a release gate. The reusable lesson is to audit side effects in recovery helpers named as checks: mutating a containment to manufacture emptiness can bypass control authorization and erase the evidence needed for recovery.