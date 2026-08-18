---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-17T10:08:37+09:30
  last_verified_at: 2026-08-17T10:08:37+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions release run 31980750823 job 95246933454
    content_hash: null
  - kind: source
    reference: .github/workflows/release.yml
    content_hash: null
  - kind: test
    reference: crates/xtask/src/main.rs workflow_policy_rejects_unconditional_release_publication
    content_hash: null
  - kind: upstream-action
    reference: googleapis/release-please-action action.yml at 45996ed1f6d02564a971a2fa1b5860e934307cf7
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-17T10:08:37+09:30
    reason: Verified against failed run 31980750823, the v1.4.1 tag target, associated PR metadata for release and ordinary commits, and the pinned action's documented skip-github-release input.
description: A later green main commit can cause release-please to tag an older merged release PR whose own CI failed unless ordinary runs disable GitHub Release creation.
tags:
- ci
- github-actions
- release
- release-please
timestamp: 2026-08-17T10:08:37+09:30
title: Gate release-please tag creation on the release merge's own successful CI
type: agent-memory
---
When release-please is invoked after a merged release PR whose push CI failed, a later successful main CI run may create the pending tag at the older release-merge SHA. A post-action exact-SHA check prevents artifact and crate publication but cannot undo the tag or GitHub Release that release-please already created. Before invoking release-please, classify whether the triggering CI SHA is the merge commit of a release-please PR. Pass `skip-github-release: true` for ordinary commits so they may manage release proposals without tagging; enable tag creation only for a successful current-tip release-merge CI run. Retain the post-action main-tip, release classification, and release-SHA checks as defense in depth.