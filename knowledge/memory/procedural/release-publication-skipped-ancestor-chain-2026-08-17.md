---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-17T14:08:43+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions Release run 31993773993 attempt 1
    content_hash: null
  - kind: source
    reference: .github/workflows/release.yml
    content_hash: null
  - kind: test
    reference: crates/xtask/src/main.rs workflow policy tests
    content_hash: null
  - kind: commit
    reference: 4ccc23cf87560d24caea9f6d12faaf8802514518
    content_hash: null
  - kind: documentation
    reference: https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-jobs
    content_hash: null
  history: []
description: GitHub propagates an intentionally skipped rebuild job through the release dependency chain unless final publishers use always() with explicit successful prerequisite guards.
tags:
- github-actions
- release
- workflow
- publication
- ci
timestamp: 2026-08-17T14:08:43+09:30
title: Release publishers must override intentional skipped ancestors
type: agent-memory
---
When automatic release promotion reuses retained CI artifacts, `rebuild-artifacts` is intentionally skipped. GitHub propagates that skipped status through the dependency chain, even after an `always()`-guarded validation job succeeds, so downstream asset and crate publishers can be silently skipped. Put `always()` in each final publisher's job-level condition, then require every direct prerequisite result to equal `success` and retain affirmative release/publication outputs. This neutralizes only the expected skipped ancestor and remains fail-closed for failed, cancelled, stale, dry-run, or unauthorized paths. Protect the topology with workflow-policy tests for both publisher jobs.