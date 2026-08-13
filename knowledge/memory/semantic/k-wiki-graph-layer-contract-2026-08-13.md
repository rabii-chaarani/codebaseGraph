---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-13T03:15:00+09:30
  last_verified_at: 2026-08-13T03:16:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: commit
    reference: '738aba8 fix(k-wiki): set graph request layers'
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31662915277, cargo clippy job 94331303022
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-13T03:16:00+09:30
    reason: Verified against the current API contract, adapter call sites, CI failure log, and successful exact local Clippy run.
description: Direct Rust construction of graph search and context requests must set the required layer explicitly.
tags:
- api-contract
- graph-layer
- k-wiki
- rust
timestamp: 2026-08-13T03:15:00+09:30
title: k-wiki graph API calls use the semantic layer
type: agent-memory
---
`SearchRequest` and `ContextRequest` require an explicit `layer` when built directly in Rust. The k-wiki graph-context adapter is an internal semantic consumer, so its search and context requests must set `layer` to `"semantic"`; serde defaults do not apply to Rust struct literals.