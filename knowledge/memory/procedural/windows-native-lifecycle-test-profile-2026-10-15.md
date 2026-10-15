---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-maintainers
  created_at: 2026-10-15T02:12:00Z
  last_verified_at: 2026-10-15T02:12:10Z
  verified_by: codex
  review_after: null
  supersedes:
  - windows-ci-bundled-extension-feature-2026-08-13
  superseded_by: null
  sources:
  - kind: source
    reference: Cargo.toml [profile.native-tests]; crates/xtask/src/main.rs::native_test_command and build_native_binaries at d98143e2c7be72403587ecfcc15f2f7e533dc2a0
    content_hash: null
  - kind: test
    reference: 'crates/xtask/src/main.rs::tests::windows_native_tests_forward_bundled_extensions; final xtask JUnit: 31/31 passed'
    content_hash: null
  - kind: ci-log
    reference: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/38664504399/job/116193191382; release-only refresh target excluded lifecycle_faults module
    content_hash: null
  - kind: review
    reference: 'Independent GPT-6.1 Sol Max profile probe: native-tests has debug_assertions=true, PROFILE=release and target/native-tests; ordinary release has debug_assertions=false and target/release'
    content_hash: null
  - kind: primary-documentation
    reference: https://doc.rust-lang.org/cargo/reference/profiles.html#debug-assertions
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-10-15T02:12:10Z
    reason: Reviewed against the committed typed command and profile, passing target-mapping and 31-test xtask results, and the independent Max reviewer's actual Cargo profile probe; no claim of Windows fault execution is made.
description: Keep bundled extensions and deterministic fault coverage together while separating test binaries from shipped release artifacts.
tags:
- ci
- native-tests
- process-lifecycle
- windows
timestamp: 2026-10-15T02:12:00Z
title: Run Windows lifecycle faults through the native test profile
type: agent-memory
---
Use the typed `cargo run -p xtask -- native-test --target windows-x86_64` entry point for Windows runtime verification. Its `native-tests` profile inherits release optimization and enables debug assertions, which compile the lifecycle fault fixtures and deterministic readiness barriers. The `codebase-graph/bundled-windows-extensions,k-wiki/bundled-windows-extensions` features remain mandatory for the workspace runtime tests.

An ordinary `cargo test --workspace --release` run can pass while silently excluding `cfg(debug_assertions)` process regressions. Check that the native log registers the named startup, paused-process, cancellation, orphan-recovery and memory-failure cases; a green aggregate alone does not establish that coverage. Keep the test binaries in `target/native-tests`, then let `native-artifact` build and smoke ordinary `--release` binaries in `target/release`, where the fault barriers are absent. Do not replace the native test entry point with a release-only command when maintaining CI.

The 2026-10-15 repair verified profile selection with the target-mapping regression, all 31 xtask tests, and an independent Cargo probe showing debug assertions enabled only for the native test profile. This supersedes the earlier Windows CI recipe while retaining its bundled-extension invariant.