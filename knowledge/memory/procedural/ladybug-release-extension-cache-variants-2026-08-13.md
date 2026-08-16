---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: superseded
  owner: codex
  created_at: 2026-08-13T00:00:00+09:30
  last_verified_at: 2026-08-13T00:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: ladybug-0183-link-and-extension-alignment-2026-08-14
  sources:
  - kind: commit
    reference: 'ba7ed2f fix(release): seed both Ladybug extension caches'
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31669249757, native release artifact smoke tests for Ubuntu and Windows
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31670460113, native package (windows-2022)
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31671399427, native package (ubuntu-latest) and native package (windows-2022)
    content_hash: null
  - kind: source
    reference: lbug 0.17.1 CMakeLists.txt defines LBUG_EXTENSION_VERSION=0.17.0
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-13T00:00:00+09:30
    reason: Verified against the distinct 0.19.0 source-built and 0.17.0 prebuilt Ladybug cache requests in the failing logs, and the successful Ubuntu and Windows native-package smoke tests in run 31671399427.
  - from: active
    to: superseded
    actor: codex
    at: 2026-08-14T15:07:00+09:30
    reason: The repository now uses lbug 0.18.3 consistently for source and prebuilt modes, so both require ABI-compatible 0.18.1 extension binaries instead of seeding 0.17.0 and 0.19.0 copies.
description: Prebuilt and source-built Ladybug libraries can resolve the same bundled extensions from different cache-version paths.
tags:
- ci
- extensions
- github-actions
- ladybug
- offline
- release
timestamp: 2026-08-13T00:00:00+09:30
title: Seed both Ladybug extension cache versions for offline release artifacts
type: agent-memory
---
For offline packaged-artifact smoke tests, `preseed_ladybug_extensions` must seed both `.lbdb/extension/0.17.0/<platform>/...` and `.lbdb/extension/0.19.0/<platform>/...`. The prebuilt Ladybug 0.17.x libraries used in Windows CI request the 0.17.0 cache, while a source-built release library may request 0.19.0. Seeding only one path lets the other build mode attempt a network download of `json` during the artifact smoke test. Keep `LADYBUG_EXTENSION_CACHE_VERSIONS` and its focused test aligned with these verified runtime requirements.