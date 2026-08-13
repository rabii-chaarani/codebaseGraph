---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-13T15:17:11+09:30
  last_verified_at: 2026-08-13T15:17:11+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: .github/workflows/ci.yml:170-280
    content_hash: null
  - kind: source
    reference: .github/workflows/release.yml:121-276
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions release run 31669249757
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-13T15:17:11+09:30
    reason: Verified against the CI and release workflow recipes and GitHub Actions release run 31669249757, where Ubuntu and Windows artifact smoke failed after CI packaging had passed.
description: Native package CI did not reproduce the Linux and Windows release build recipe, allowing release-only offline-extension failures.
tags:
- artifacts
- ci
- cross-platform
- github-actions
- ladybug
- release
timestamp: 2026-08-13T15:17:11+09:30
title: CI and release artifact recipes diverged across Ladybug link modes
type: agent-memory
---
The native-package CI matrix and release matrix must use one artifact recipe per target. In the reviewed workflow, CI used prebuilt Ladybug libraries on every target, while release source-built Ladybug on Linux and Windows. The different libraries resolved runtime extensions from different cache-version paths, so CI passed but the v1.4.0 release artifact smoke failed on Ubuntu and Windows while attempting network downloads. Reuse or promote the exact CI-built, fully packaged artifacts for release; do not maintain separate target recipes.