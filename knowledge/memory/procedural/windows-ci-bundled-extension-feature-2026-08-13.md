---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-13T00:00:00+09:30
  last_verified_at: 2026-08-13T00:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: '6eb6a88 fix(ci): bundle Windows Ladybug extensions'
    content_hash: null
  - kind: commit
    reference: 'c637df0 fix(ci): bundle Windows test extensions'
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31664643339, native package (windows-2022)
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31665467806, cargo test (windows-2022)
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-13T00:00:00+09:30
    reason: Verified against the CI fixes in commits 6eb6a88 and c637df0 and the successful Windows package and workspace-test jobs in GitHub Actions run 31667751952.
description: Windows package and test jobs need the bundled extension Cargo feature when they execute codebase-graph.
tags:
- cargo
- ci
- github-actions
- ladybug
- windows
timestamp: 2026-08-13T00:00:00+09:30
title: Windows CI must enable bundled Ladybug extensions for every runtime surface
type: agent-memory
---
On Windows GitHub Actions runners, the production binary and workspace tests need the Cargo feature `bundled-windows-extensions` whenever they execute codebase-graph. The feature causes Ladybug extensions to be bundled rather than resolved from unavailable system locations. Keep the Windows native-package build and smoke test **and** the Windows `cargo test --workspace --locked --release` command feature-aligned; enabling it only for packaging leaves tests failing offline with missing Ladybug extension JSON.