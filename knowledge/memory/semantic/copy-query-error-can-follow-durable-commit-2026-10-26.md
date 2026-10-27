---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: quarantined
  owner: repository-maintainers
  created_at: 2026-10-26T01:51:56.635065+00:00
  last_verified_at: 2026-10-26T01:52:27.126513+00:00
  verified_by: repository-maintainers
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: upstream-source
    reference: https://github.com/LadybugDB/ladybug/blob/c934f673b6b1c5b680bdae3295cbd909b5855cef/src/transaction/transaction_manager.cpp#L119-L173
    content_hash: 5f18e441c0b9e423fd22fb0f4f845be73fe4e125e08a617e7d230a8f51f7ea0b
  - kind: upstream-source
    reference: https://github.com/LadybugDB/ladybug/blob/c934f673b6b1c5b680bdae3295cbd909b5855cef/src/main/client_context.cpp#L591-L664
    content_hash: 89d40244bab1de83e10c4489cf55305945e85bdb77551d32ed54ef47f48cbb2a
  - kind: upstream-pull-request
    reference: https://github.com/LadybugDB/ladybug/pull/784
    content_hash: null
  - kind: upstream-pull-request
    reference: https://github.com/LadybugDB/ladybug/pull/763
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-maintainers
    at: 2026-10-26T01:52:27.126513+00:00
    reason: 'Verified narrow engine commit/checkpoint ordering against pinned 0.19.0 sources and upstream PR #784; incident cause remains explicitly unproven.'
  - from: active
    to: quarantined
    actor: investigation-agent
    at: 2026-10-26T03:28:39.046130+00:00
    reason: Upgrade evaluation found bulk COPY row data is not WAL-backed. The previous title/wording could imply durable COPY after a commit record; actual persistence after checkpoint failure is uncertain. Preserve the no-blind-retry lesson but qualify durability explicitly.
description: A post-commit checkpoint error does not establish COPY rollback, so same-candidate replay needs durable-outcome proof.
tags:
- checkpoint
- copy
- ladybug
- recovery
- retry
timestamp: 2026-10-26T01:51:56.635065+00:00
title: Ladybug COPY errors can follow a durable commit
type: agent-memory
---
A Ladybug COPY query error does not prove the COPY rolled back. In verified 0.19.0 source, COPY forces a checkpoint, transaction commit writes/publishes the WAL commit first, and the checkpoint runs afterward. CheckpointException clears the transaction rather than rolling the committed COPY back. Upstream PR #784 explicitly documents that durable data may exist after this error and requires close/reopen recovery to determine the result.

When deferred primary-key indexes are in use, blindly replaying that COPY on the same candidate can then hit the non-empty/no-index guard before the normal post-success index phase. Adding a hash index changes load capability but does not make replay idempotent. For rebuildable disposable candidates, use a pristine candidate retry boundary; otherwise recover and establish the durable outcome before replay. Preserve the original attempt error.

This is a verified engine contract and safe-retry lesson, not a diagnosis of the 2026-10-26 DocumentationSource incident. The original attempt history was unavailable. Isolated ordinary precommit pool failures rolled back and replayed successfully; do not generalize that all COPY errors leave rows. PR #763 (0.19.1+) permits suppressing automatic COPY checkpoints but keeps defaults unchanged and does not make replay idempotent. Reverify the exact engine/native archive and extension ABI when upgrading.