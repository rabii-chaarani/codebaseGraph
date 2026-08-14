---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-14T15:05:00+09:30
  last_verified_at: 2026-08-14T15:06:00+09:30
  verified_by: codex
  review_after: null
  supersedes:
  - ladybug-release-extension-cache-variants-2026-08-13
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 31770156177 job 94674205658
    content_hash: null
  - kind: source
    reference: Cargo.toml and Cargo.lock pin lbug 0.18.3
    content_hash: null
  - kind: source
    reference: lbug 0.18.3 build.rs removes bundled component relinking and bundled CMakeLists.txt declares LBUG_EXTENSION_VERSION=0.18.1
    content_hash: null
  - kind: test
    reference: db_writer::extensions::tests::supports_prebuilt_and_source_built_ladybug_extension_cache_versions
    content_hash: null
  - kind: test
    reference: db_writer::tests::native_writer_loads_json_staging_through_ladybug_copy and adapters::cli::tests::graph::graph_search_reads_native_fts_indexes
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-14T15:06:00+09:30
    reason: Verified against the failing CI linker command, the lbug 0.17.1 and 0.18.3 build scripts, the 0.18.3 bundled CMake extension version, official 0.18.1 extension binaries, a successful crates.io dry-run, and passing JSON/FTS runtime tests.
description: lbug 0.18.3 fixes the 0.17.1 source-link fallback but requires matching 0.18.1 extension binaries and cache paths.
tags:
- ci
- crates-io
- extensions
- ladybug
- linker
- release
timestamp: 2026-08-14T15:05:00+09:30
title: Align Ladybug source linking and extension ABI when upgrading lbug
type: agent-memory
---
When `cargo publish --dry-run` verifies the crates.io tarball without an external `LBUG_LIBRARY_DIR`, `lbug 0.17.1` can fall back to a source build that whole-archives both `liblbug.a` and its component archives, producing duplicate symbols. Upgrade to `lbug 0.18.3` or later with the source-link fix, and keep its native extension ABI aligned: the bundled Ladybug source in 0.18.3 declares `LBUG_EXTENSION_VERSION=0.18.1`, so vendored JSON/FTS binaries and the seeded `.lbdb/extension/0.18.1/<platform>` paths must come from the official 0.18.1 extension repository. Never copy older extension bytes into the new cache directory; JSON may appear to work while FTS fails at load time with unresolved C++ symbols. Verify both `cargo publish --dry-run --locked` and focused JSON/FTS runtime tests after the upgrade.