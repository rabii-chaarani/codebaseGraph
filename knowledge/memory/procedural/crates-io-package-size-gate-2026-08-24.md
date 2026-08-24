---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-24T10:00:00+09:30
  last_verified_at: 2026-08-24T10:01:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions release run 32539115832 job 96948078212
    content_hash: null
  - kind: source
    reference: Cargo.toml and src/db_writer/extensions.rs
    content_hash: null
  - kind: source
    reference: .github/workflows/ci.yml and .github/workflows/release.yml
    content_hash: null
  - kind: test
    reference: crates/xtask/src/main.rs crate_size_validation_enforces_the_registry_limit and workflow_policy_rejects_missing_crate_size_gates
    content_hash: null
  - kind: documentation
    reference: docs/release.md and knowledge/architecture/release-verification.md
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-24T10:01:00+09:30
    reason: Verified against official crates.io/Cargo documentation, exact before-and-after .crate byte measurements, SHA-256 comparison of all ten decompressed extension assets, a successful Cargo publish dry-run, runtime JSON/FTS tests, workflow policy tests, strict Clippy, and the complete workspace suite.
description: Measure the compressed .crate by exact bytes, losslessly precompress bundled native assets, and enforce the registry limit in CI and Release.
tags:
- assets
- cargo
- ci
- crates-io
- packaging
- release
timestamp: 2026-08-24T10:00:00+09:30
title: Keep the crates.io source package below 10 MiB
type: agent-memory
---
crates.io limits the compressed `.crate` upload to 10 MiB (10,485,760 bytes); the unpacked source-tree size is not the enforced value. `codebase-graph` v1.5.0 initially produced a 13,748,813-byte archive because ten bundled Ladybug extension binaries dominated the package.

Store the same extension bytes as XZ streams in the repository and publish only build-required sources through Cargo's `include` allowlist. Decompress the platform-selected JSON/FTS stream before seeding the Ladybug cache. Verify every compressed stream against the original official binary SHA-256, run real JSON and FTS loading tests, and run `cargo publish --dry-run --locked` so the extracted source package compiles.

After Cargo creates the archive, run the xtask exact-byte gate in both CI and Release. The corrected archive measured 9,734,316 bytes, leaving 751,444 bytes below the limit. Keep the gate because future source or asset growth can consume that headroom.