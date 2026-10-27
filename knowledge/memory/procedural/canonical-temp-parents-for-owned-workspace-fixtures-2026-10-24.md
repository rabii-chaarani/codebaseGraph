---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-24T09:45:53Z
  last_verified_at: 2026-10-24T09:48:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: git-source
    reference: 073f0fb91e8a866d31e5f01a07a20898c3eb2370:src/storage/inventory.rs::real_path
    content_hash: null
  - kind: executed-baseline
    reference: /private/tmp/pr153-native-ci-baseline-macos.log; exact Managed recovery case fails before changes
    content_hash: null
  - kind: executed-comparison
    reference: /private/tmp/pr153-native-ci-canonical-temp-probe.log; same baseline binary, TMPDIR=/private/tmp, exact case passes
    content_hash: null
  - kind: executed-junit
    reference: /private/tmp/pr153-native-ci-nextest/ci-pr153/junit.xml; 31/31 passing
    content_hash: 637fc9f29282bc598c24793ff402d88722b480debac405e49eb6b0512662e14a
  - kind: source-snapshot
    reference: PR153 isolated CI worktree src/storage/managed.rs
    content_hash: 8090dfa8926c954b569f84eb9db507f09fee04f16c6e5659abe352b5b239937c
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-24T09:48:00Z
    reason: Reverified the exact baseline failure, same-binary canonical-parent comparison, 31-case passing JUnit hash, unchanged fixed fixture source hash, and the baseline real_path invariant. The procedure preserves ownership checks and explicitly limits native evidence to Darwin ARM64.
description: macOS temporary paths can cross /var symlinks and leave strict origin factories untracked even though primary graph creation succeeds.
tags:
- graph-storage
- macos
- ownership
- testing
timestamp: 2026-10-24T09:45:53Z
title: Canonicalize temporary parents before testing owned workspace creation
type: agent-memory
---
When a regression requires genuine workspace registration or terminal-retirement authority, resolve the existing temporary parent with `std::env::temp_dir().canonicalize()` before creating the new fixture namespace. On macOS the default temporary path may start at `/var`, while the actual directory is under `/private/var`. The existing `real_path` ownership check intentionally rejects symlinked ancestors; a successful primary write does not prove optional origin registration succeeded.

PR #153 CI run 40074045438 exposed six such fixture failures. The exact baseline Managed-recovery case failed with the default temporary path and passed with an otherwise unchanged binary when its temporary parent was `/private/tmp`. Canonical-parent fixes then passed the six original failures and the surrounding 31-case native regression selection. Preserve the ownership checks and assert that fixtures obtained real `.workspace.binding`/owner scope before testing retirement. This evidence is Darwin ARM64; Windows/Linux qualification remains separate.

The behavior remains within the approved [disk cleanup contract](../../architecture/graph-disk-cleanup-contract.md), especially DC-S2 ownership evidence and DC-S5 qualification.