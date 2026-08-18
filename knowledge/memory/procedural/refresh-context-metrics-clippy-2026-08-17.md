---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-17T00:00:00+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/refresh.rs
    content_hash: null
  - kind: test
    reference: cargo test --lib adapters::cli::tests::watch
    content_hash: null
  - kind: test
    reference: cargo clippy --lib -- -D clippy::too_many_arguments
    content_hash: null
  history: []
description: Collapse refresh service argument lists with small private context/metrics structs instead of allow attributes.
tags:
- rust
- clippy
- refresh
- refactor
timestamp: 2026-08-17T00:00:00+09:30
title: Use private context and metrics structs to satisfy Clippy in refresh flows
type: agent-memory
---
When `src/api/refresh.rs` grows `too_many_arguments` warnings in service-flow helpers, introduce small private value objects instead of suppressing the lint.

1. Group long-lived dependencies into a `RefreshServiceContext`-style struct that carries the state and execution plan.
2. Group watch-loop configuration and filter state into a separate runtime/context struct.
3. Group status-update counters and response-derived metrics into a dedicated metrics struct for `RefreshState::mark_refreshed`.
4. Prefer passing a `WatchChangeBatch` or equivalent aggregate into the refresh helper instead of unpacking its counts at each call site.
5. Keep the new structs private or `pub(crate)` only as needed to satisfy visibility rules, and verify with focused watch tests plus `cargo clippy -- -D clippy::too_many_arguments`.

Observed outcome: the refresh watch suite still passed after the refactor, and Clippy no longer reported the three refresh-owned `too_many_arguments` sites.