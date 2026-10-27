---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-10-14T23:06:20Z
  last_verified_at: 2026-10-14T23:07:36Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/materialization_worker.rs:65-69,128-136,505-545,650-657 at 55abb2522d410580d96861cd0894127ec7c87206; identical in v3.0.2 (8ba4c316770cba00b9ca8d967aebf2e1ee8670b3)
    content_hash: null
  - kind: source
    reference: src/api/core.rs:69-84,266-275,961-986; src/adapters/cli/materialization/command.rs:17-21
    content_hash: null
  - kind: isolated-reproduction
    reference: '/private/tmp/graph-refresh-orphan-pid-t6586i3h/review-result.json: blocked before default manual build, manual exit 0, stale record retained, next auto attempt blocked again, record-only removal restores current'
    content_hash: null
  - kind: primary-documentation
    reference: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kill.2.html
    content_hash: null
  - kind: review
    reference: 2026-10-15 required GPT-6.1 Sol Max read-only adversarial follow-up; numeric-PID-only guard and same-version source paths verified
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-14T23:07:36Z
    reason: Reviewed against the required GPT-6.1 Sol Max source trace and the saved isolated fault-injection result; coordinator independently verified the relevant source guards, reproduction outcomes, and equality of these files to v3.0.2. The entry distinguishes confirmed failure behavior from the unproven historical Qunys cause.
description: A default manual build bypasses the automatic worker recovery gate and can refresh the graph without repairing continued live updates.
tags:
- mcp
- process-identity
- recovery
- refresh
timestamp: 2026-10-14T23:06:20Z
title: A reused PID in stale worker state can block automatic refresh
type: agent-memory
---
The 2026-10-15 live-update review reproduced an automatic-refresh recovery failure using an isolated persisted-state fixture. `worker.json` records only a protocol version, build ID, and numeric worker PID. `recover_orphan_worker` treats any live process with that PID as the former worker, waits five seconds, and returns an error without clearing the record. After a crashed worker's PID is reused by an unrelated long-lived process, every subsequent automatic attempt can repeat the same failure. A standalone default CLI build uses the direct pipeline and bypasses this gate: in the fixture it published a fresh generation successfully, but the next completed automatic attempt remained blocked and pending. Removing only the deliberately stale fixture record allowed reconciliation to become current without another source edit. This establishes a reachable recovery defect, not the cause of the earlier Qunys incident. A successful manual build does not prove that continuous refresh recovered. A repair should bind persisted workers to a verifiable process incarnation and verify safe stale-record recovery under the worker ownership lock; it must not signal an unrelated process based only on PID.