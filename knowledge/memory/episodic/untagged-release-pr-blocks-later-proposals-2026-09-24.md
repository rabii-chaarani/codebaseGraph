---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-24T03:29:06Z
  last_verified_at: 2026-09-24T03:29:06Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35823493116
    content_hash: null
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35826294802
    content_hash: null
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/35949576180
    content_hash: null
  - kind: pull-request
    reference: https://github.com/rabii-chaarani/codebaseGraph/pull/124
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T03:29:06Z
    reason: 'Verified the identical outstanding-untagged-PR warning in three subsequent successful Release runs, successful originating CI runs, and PR #124''s current pending label.'
description: After a release is skipped, later green ordinary commits cannot even create the next release proposal until the outstanding release is resolved.
tags:
- ci
- failure-handling
- release
- release-please
timestamp: 2026-09-24T03:29:06Z
title: An untagged merged release PR blocks subsequent release proposals
type: agent-memory
---
Release-please refuses to build a new release PR while a merged release PR remains untagged. After version 2.0.0 release PR #124 was skipped by the current-tip guard, later successful main CI for PRs #123, #114, and #125 reached release-please with skip-github-release=true. In all three Release logs it found PR #124 and emitted 'There are untagged, merged release PRs outstanding - aborting'. The workflows still completed successfully with no release-created output.

The resulting block has two parts: ordinary runs cannot tag the older release, and release-please will not propose a subsequent release while that older merged PR has autorelease: pending. Preventing the original freshness race alone does not clear an already blocked repository. Recovery must explicitly resolve the outstanding release and preserve its exact-SHA CI provenance; do not remove the pending label or mark it tagged merely to silence the warning. Add a regression covering subsequent proposal creation after successful publication, and report outstanding merged releases clearly instead of presenting the run as a successful publication.