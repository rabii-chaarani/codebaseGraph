---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-10-14T23:07:10Z
  last_verified_at: 2026-10-14T23:07:36Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/materialization_worker.rs:312-375,627-646 and src/db_writer/phase.rs:78-145 at 55abb2522d410580d96861cd0894127ec7c87206; these files match v3.0.2
    content_hash: null
  - kind: isolated-reproduction
    reference: '/private/tmp/graph-refresh-orphan-pid-k_3p3nb8/descendant-review-result.json: owned phase loader remained alive/stopped and adopted by PID 1 after its materialization worker was killed; complete=true'
    content_hash: null
  - kind: primary-documentation
    reference: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/wait4.2.html
    content_hash: null
  - kind: review
    reference: 2026-10-15 required GPT-6.1 Sol Max follow-up; process identity, bounded fault injection, and descendant cleanup verified
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-14T23:07:36Z
    reason: Reviewed against the required GPT-6.1 Sol Max source trace and the saved isolated fault-injection result; coordinator independently verified the relevant source guards, reproduction outcomes, and equality of these files to v3.0.2. The entry distinguishes confirmed failure behavior from the unproven historical Qunys cause.
description: Killing or cancelling the outer materialization worker does not terminate its native database loader child.
tags:
- cancellation
- mcp
- process-lifecycle
- refresh
timestamp: 2026-10-14T23:07:10Z
title: Database phase children survive termination of the refresh worker
type: agent-memory
---
The 2026-10-15 live-update review confirmed a descendant cleanup gap in an isolated macOS fault fixture. The materialization supervisor kills and reaps only its direct child. That worker can itself spawn a `ladybug-write-phase-v1` loader, whose launch path has no parent-death watchdog or shared cancellation mechanism. The fixture paused an owned loader to model an in-flight stalled phase, terminated only the outer materialization worker, and verified that the loader remained alive with PPID 1. All fixture descendants were explicitly terminated afterward. The existing parent-pipe watchdog protects the outer worker, not this nested loader. After outer-worker termination, the loader is no longer tracked by `worker.json` or the parent RSS supervisor; repeated failed attempts can overlap orphaned loader work and consume resources needed for live updates. The loader operates on a candidate and cannot independently publish the active generation. This is a confirmed process-cleanup defect, not proof of the earlier Qunys incident. A repair should bind descendant lifetime to the owning build and verify cancellation/crash cleanup during a real database phase.