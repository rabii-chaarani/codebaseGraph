---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: codex
  created_at: 2026-08-25T12:39:22+09:30
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: Cargo.toml package include = ["src/**", ...]
    content_hash: null
  - kind: runtime-observation
    reference: 2026-08-25 cargo publish dry-run for PR branches 101-104
    content_hash: null
  - kind: pull-request
    reference: https://github.com/rabii-chaarani/codebaseGraph/pull/101
    content_hash: null
  history: []
description: Ignored Finder metadata under src/** can make cargo publish dirty and contaminate exact package measurements even when git status is clean.
tags:
- cargo
- packaging
- release-verification
- clean-worktree
timestamp: 2026-08-25T12:39:22+09:30
title: Verify Cargo packages from a clean worktree when include globs are broad
type: agent-memory
---
When `Cargo.toml` uses a broad package allowlist such as `src/**`, ignored untracked files beneath that tree can still make `cargo publish --dry-run` report a dirty package. During the web-language PRs, local `src/.DS_Store` and `src/adapters/.DS_Store` triggered this condition even though ordinary `git status` was clean.

For release-representative verification, preserve user metadata and run `cargo publish --dry-run --locked` from a clean detached worktree at the exact commit. Then pass the produced `.crate` to the xtask size gate. When verifying a version already present on crates.io, Cargo may retain the archive under `target/package/tmp-crate/`; use the path Cargo actually produced rather than assuming the top-level package path.