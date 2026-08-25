---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-25T13:21:26+09:30
  last_verified_at: 2026-08-25T13:21:40+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: vendor/tree-sitter-wat/src/scanner.c
    content_hash: null
  - kind: documentation
    reference: vendor/tree-sitter-wat/UPSTREAM.md
    content_hash: null
  - kind: source
    reference: build.rs and Cargo.toml
    content_hash: null
  - kind: test
    reference: cargo test --workspace --locked and cargo publish --dry-run --locked --allow-dirty
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-25T13:21:40+09:30
    reason: Reviewed the callback claim against the pinned upstream scanner, the documented upstream and patched hashes, the local C patch, a warning-clean build, the passing full workspace suite, and the verified extracted source package.
description: Vendored Tree-sitter external scanners must return valid payload and serialization values even when upstream builds only warn.
tags:
- c
- ffi
- parser
- supply-chain
- tree-sitter
- vendoring
timestamp: 2026-08-25T13:21:26+09:30
title: Verify external-scanner callback returns when vendoring Tree-sitter grammars
type: agent-memory
---
When vendoring generated Tree-sitter grammar sources, compile the external scanner with warnings enabled and inspect every callback against Tree-sitter's scanner ABI. The pinned `g-plane/tree-sitter-wat` scanner had two payload-free callbacks that compiled but violated their C return contracts: `tree_sitter_wat_external_scanner_create` fell off a non-void function, and `serialize` returned `1` without writing a byte. This can propagate an indeterminate payload pointer or nondeterministic serialized state.

For a stateless scanner, make `create` return null/zero and `serialize` return zero. Preserve the upstream source hash and the patched vendored hash in provenance rather than presenting the local file as byte-for-byte upstream. Then verify the focused parser tests, full native workspace suite, extracted crates.io package build, and package-size gate. Recheck the callbacks and provenance whenever the grammar commit changes.