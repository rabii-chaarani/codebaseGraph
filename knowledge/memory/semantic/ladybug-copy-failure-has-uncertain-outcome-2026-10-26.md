---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-26T03:28:39.081786+00:00
  last_verified_at: 2026-10-26T03:28:39.111415+00:00
  verified_by: investigation-agent
  review_after: null
  supersedes:
  - copy-query-error-can-follow-durable-commit-2026-10-26
  superseded_by: null
  sources:
  - kind: upstream-pull-request
    reference: https://github.com/LadybugDB/ladybug/pull/1204
    content_hash: null
  - kind: upstream-pull-request
    reference: https://github.com/LadybugDB/ladybug/pull/1361
    content_hash: null
  - kind: upstream-source
    reference: https://github.com/LadybugDB/ladybug/blob/4ca22377b85a33a10247410fe3944a5224b1169d/src/storage/wal/records/copy_table_record_replay.cpp
    content_hash: null
  - kind: isolated-native-test
    reference: /private/tmp/ladybug-022-evaluation-dp_8zy5d/no-close-observations.json
    content_hash: bd49c9cedd051e41ea68cab99426dc61f6a8f40c9ef61e8d6e5a77d7f6286cdf
  history:
  - from: candidate
    to: active
    actor: investigation-agent
    at: 2026-10-26T03:28:39.111415+00:00
    reason: Reviewed against pinned0.22 source, merged PR1204, openPR1361 and isolated official-release abrupt-exit observations; conditional COPY persistence is stated explicitly.
description: Bulk COPY commit, checkpoint completion and durable recovery are distinct; neither blind replay nor disabling checkpoints is safe.
tags:
- checkpoint
- copy
- ladybug
- recovery
- retry
timestamp: 2026-10-26T03:28:39.081786+00:00
title: Ladybug COPY failures have an uncertain outcome
type: agent-memory
---
Treat a failed Ladybug COPY/checkpoint as an uncertain operation outcome, not proof of rollback or durability. Transaction commit can precede checkpoint failure, but bulk COPY does not log row payloads to the WAL. The released 0.22 error text promising durable data is too broad; upstream PR #1361 corrects that wording but was unmerged when verified. Recovery/checkpoint progress determines which rows survive restart.

An isolated official0.22.0 native probe confirmed COPY with auto_checkpoint=false can report success yet lose its rows on abrupt exit before an explicit checkpoint. Therefore disabling automatic checkpoints is not a durability or replay-safety fix. Adding an index or skipping duplicate keys also does not resolve an uncertain prior outcome. For rebuildable candidates, restart from a fresh candidate after uncertainty; for retained databases, preserve recovery files and establish the recovered outcome before replay.

This is a verified safe-retry lesson, not proof of the initial DocumentationSource incident's cause. Ordinary precommit pool failures can roll back successfully. Qualify the exact native version and failure boundary rather than generalizing that all failed COPYs leave rows or that a transaction commit makes bulk COPY durable.