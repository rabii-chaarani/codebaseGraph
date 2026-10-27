---
agent_memory:
  created_at: 2026-10-24T02:51:30Z
  history:
  - actor: codex
    at: 2026-10-24T02:52:00Z
    from: candidate
    reason: Verified current Managed source SHA256 and both actual lease-gap instances, plus shipping atomic-generation and ten-update physical convergence in passing32-case final256 JUnit; source/native/binary stability all true. Historical255 counterexample is preserved separately.
    to: active
  kind: procedural
  last_verified_at: 2026-10-24T02:52:00Z
  owner: repository-maintainers
  review_after: null
  scope: repository
  sources:
  - content_hash: 33af3c8a49ce45043b26a404fd78923bc728820af6314ca1c8c3eb135ddd584b
    kind: source
    reference: src/storage/managed.rs::recover_runs_locked_typed, source capture 256
  - content_hash: 4109a5af4ca854d8a293dacc74ea8d360a6df88a234a0896802cb094c24d30a8
    kind: executed-test
    reference: docs/testing/graph-disk-cleanup-v2-evidence/pre-platform-fixes-256.tar.gz:checkpoints/256/nextest/s5-regressions256/junit.xml; 32 passed including both managed_read_recovery_does_not_recreate_a_partially_retired_run_lease instances and shipping generation convergence
  - content_hash: null
    kind: runtime-observation
    reference: 'docs/testing/graph-disk-cleanup-v2-evidence/implementation-checkpoints-255.tar.gz: standalone atomic-generation failure after replacement lease inode; capture 256 matching case passed'
  status: active
  superseded_by: null
  supersedes: []
  verified_by: codex
  version: 1
description: Existing-parent opens can still create files and invalidate a pending retirement's inode binding.
tags:
- disk-cleanup
- leases
- recovery
- storage
timestamp: 2026-10-24T02:51:30Z
title: Managed recovery must not recreate a cooperatively removed lease
type: agent-memory
---
When ordinary Managed recovery acquires a workspace lease, open only an existing file. Disabling parent-directory creation does not disable file creation: `try_open_locked_in_existing_parent` could recreate `lease.lock` after a cooperative retirement grant removed it. The recorded removal then rejected the replacement inode and could not finish.

Use `try_open_existing_locked` for that recovery acquisition. A missing lease must not be recreated as evidence or ownership. Verify the interleaving with a genuine recorded Removing workspace, a lease-deleted/rmdir-deferred gap, and a still-Publishing sibling that forces required `recover_for_read`: the sibling must recover, the missing lease must remain missing, and the original removal must resume to an absent root and durable Idle. Also exercise repeated shipping health calls until physical generation/run/cache convergence.

This was verified in final capture 256 on Darwin arm64: both library and standalone storage regression instances passed, alongside the actual atomic-generation and ten-update convergence cases. Source and binary/native inputs remained unchanged across the 32-case targeted run. Linux/Windows native qualification remains separate.