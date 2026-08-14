---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-14T10:45:00+09:30
  last_verified_at: 2026-08-14T10:46:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: a8d54d536cee9a5ed7d322939156df8e17733da7
    content_hash: null
  - kind: source
    reference: .github/workflows/release.yml
    content_hash: null
  - kind: source
    reference: crates/xtask/src/main.rs
    content_hash: null
  - kind: source
    reference: docs/release.md
    content_hash: null
  - kind: source
    reference: knowledge/architecture/release-verification.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-14T10:46:00+09:30
    reason: Verified against commit a8d54d536cee9a5ed7d322939156df8e17733da7 and the updated release workflow, xtask policy checks, and release documentation.
description: Automatic workflow_run releases need exact triggering-run identity plus a post-release-please freshness guard to keep artifact promotion and crate publication aligned with current-tip CI.
tags:
- ci
- github-actions
- provenance
- release
- workflow_run
timestamp: 2026-08-14T10:45:00+09:30
title: workflow_run releases must bind publication to the triggering CI run and SHA
type: agent-memory
---
For automatic `workflow_run` releases, carry `github.event.workflow_run.id` and `github.event.workflow_run.head_sha` through the release workflow, revalidate that run as a successful `push` of `.github/workflows/ci.yml` on `main`, and promote only its retained `native-*` artifacts. Keep manual `rebuild-if-missing` recovery separate, and add a post-`release-please` check that stops publication if `main` advanced or `release-please` targeted a different SHA.