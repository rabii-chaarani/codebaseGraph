---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-24T04:35:52Z
  last_verified_at: 2026-10-24T04:35:52Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/process_lifecycle/unix.rs::Contained::exited at255302957a2a4265fed7de0d112888aa504fd251
    content_hash: bbe2c2cc599f9472047faaad821eacb813653082bb16f713118636d7717093a5
  - kind: executed-test
    reference: docs/testing/graph-disk-cleanup-v2-evidence/platform-corrections-257.tar.gz:checkpoints/257/nextest/s5-platform_fixes257/junit.xml
    content_hash: ae3dad3133c42d9ae3e7e9aa0592424631370ae34d7c010a6fb13696fb42a3fd
  - kind: executed-test
    reference: docs/testing/graph-disk-cleanup-v2-evidence/platform-corrections-257.tar.gz:checkpoints/257/nextest/s5-darwin_failure257/junit.xml
    content_hash: 46a097bdd7eb19adc58424be9284fbe99711f0e2b2af3b7f49297de95e52a22d
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-24T04:35:52Z
    reason: Reverified source hash and exact257 JUnit for both actual stopped-child cases and the original owner-loss regression; syscall probes show stoppedcode5 is excluded while exited/killedclasses remain terminal and peeks stay nonreaping.
description: Darwin returned a stopped child from a WEXITED peek; PID presence alone caused premature guardian teardown.
tags:
- darwin
- process-lifecycle
- unix
- waitid
timestamp: 2026-10-24T04:35:52Z
title: Unix exit peeks must require an actual siginfo exit class
type: agent-memory
---
Classify an owned child's `waitid` result using `si_code`, not merely a nonzero `si_pid`. A native Darwin probe returned `CLD_STOPPED` with `WEXITED | WNOHANG | WNOWAIT` while the child remained stopped. Treating that observation as an exit sent the guardian down its normal teardown path and killed the stopped worker without an owner-loss or budget event.

`Contained::exited` now accepts only `CLD_EXITED`, `CLD_KILLED` or `CLD_DUMPED` as terminal. Preserve `WNOWAIT` so peeks do not consume the status. Verify actual stop detection, repeated nonterminal peeks, resume/exit with repeated nonreaping peeks, and cancellation of a separately stopped owned group. Retain the original end-to-end owner-loss/stopped-loader regression; do not hide this failure by accepting a vanished worker or loosening its stopped-state barrier.

The actual-OS regression passed in both library and standalone storage harness, and the original Darwin owner-loss case passed on source capture257. This records observed Darwin behavior and a portable exit-class invariant; it is not a Linux/Windows whole-platform qualification claim.