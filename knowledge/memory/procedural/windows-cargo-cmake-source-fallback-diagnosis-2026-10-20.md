---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: candidate
  owner: repository-maintainers
  created_at: 2026-10-20T01:10:12Z
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: 'v3.0.3 (4af5db4e9dc959431a467a076a881f67d584cc37): Cargo.toml:42 and Cargo.lock:180,815-828'
    content_hash: null
  - kind: dependency-source
    reference: 'lbug 0.19.0: build.rs:179-203,290-309,384-398; inspected archive checksum matched v3.0.3 Cargo.lock and archived build script matched inspected source'
    content_hash: null
  - kind: primary-source
    reference: https://docs.rs/cmake/0.1.58/src/cmake/lib.rs.html#1091-1100
    content_hash: null
  - kind: executed-check
    reference: '2026-10-20: cargo tree --offline --locked --target x86_64-pc-windows-msvc -i cmake passed on macOS at 6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5; Windows execution not performed'
    content_hash: null
  - kind: review
    reference: Fresh read-only adversarial-review-engineer review; GPT-6.1 Sol / Max; v3.0.3 source snapshot 4af5db4e9dc959431a467a076a881f67d584cc37
    content_hash: null
  history: []
description: Distinguish native source prerequisites from prebuilt installation when a Windows Cargo build cannot launch CMake.
tags:
- windows
- installation
- cargo
- cmake
- native-build
timestamp: 2026-10-20T01:10:12Z
title: Diagnose CMake source fallback in Windows Cargo installs
type: agent-memory
---
When a Windows Cargo installation reports that `cmake` cannot be executed, first capture the preceding failing-build-script line and resolved dependency version. The message establishes an executable lookup/launch failure; it does not establish whether CMake is uninstalled, outside the process PATH, or selected by an invalid `CMAKE` override.

For released v3.0.3, the authenticated locked dependency path is codebase-graph → lbug 0.19.0 → cmake 0.1.58. Ladybug accepts supplied native library/include directories, otherwise tries a prebuilt library through `sh scripts/download_lbug.sh`, and falls back to CMake if that attempt fails. Its Windows source fallback selects Ninja and needs the C++ toolchain. Therefore do not claim that every Cargo install necessarily executes CMake, or that installing CMake alone establishes a successful Windows build. The packaged codebaseGraph Windows binary avoids this compilation path.

Check the selected tools in the affected user's shell (`Get-Command cmake`, `cmake --version`, and `ninja --version`), inspect CMake executable overrides if lookup and Cargo disagree, provide missing source-build prerequisites, reopen the terminal after PATH changes, and retry the original version/features. `RUST_BACKTRACE` adds diagnostics and `CARGO_BUILD_BUILD_DIR` reuses intermediates; neither supplies a missing executable. A source/dependency trace on macOS is not evidence of a successful Windows retry.