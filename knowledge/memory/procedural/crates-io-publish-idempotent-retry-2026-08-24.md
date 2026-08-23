---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-24T09:11:11+09:30
  last_verified_at: 2026-08-24T09:12:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/32539115832/job/96948078212
    content_hash: null
  - kind: source
    reference: .github/workflows/release.yml
    content_hash: null
  - kind: test
    reference: crates/xtask/src/main.rs workflow_policy_rejects_missing_crate_publish_retry
    content_hash: null
  - kind: documentation
    reference: docs/release.md and knowledge/architecture/release-verification.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-24T09:12:00+09:30
    reason: Verified against the linked Actions log, current crates.io version state, the updated release workflow, the workflow-policy regression, shell-level retry scenarios, and passing release/workspace tests.
description: Retry transient or ambiguous cargo publish failures only within a bounded loop, treating exact-version registry visibility as authoritative success.
tags:
- cargo
- ci
- crates-io
- publishing
- release
- retry
timestamp: 2026-08-24T09:11:11+09:30
title: Make crates.io publish retries version-aware and idempotent
type: agent-memory
---
GitHub Actions release run 32539115832 successfully packaged and verified `codebase-graph` v1.5.0, then crates.io returned HTTP 503 for the actual upload. `CARGO_NET_RETRY` did not retry that publish request, and the exact version remained absent from the registry.

Wrap `cargo publish --locked` in a bounded retry loop. Before the first attempt and after every failed response, query `https://crates.io/api/v1/crates/<crate>/<version>` with a descriptive User-Agent. Treat exact-version visibility as success because crates.io versions are immutable and a client can lose a successful upload response. Otherwise back off between attempts and preserve the final Cargo exit status. Protect the workflow shape with a policy regression.

A `Finished dev profile` line during `cargo publish` is normal package verification of the source crate; it is not the distributed native binary. Native GitHub Release archives must continue to come from the separately verified `cargo build --release` artifact path.