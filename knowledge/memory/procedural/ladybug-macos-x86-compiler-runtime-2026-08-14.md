---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-08-14T15:42:00+09:30
  last_verified_at: 2026-08-14T15:43:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: ci-log
    reference: GitHub Actions run 31773525988 job 94684123400, undefined ___cpu_model on macos-x86_64
    content_hash: null
  - kind: source
    reference: build.rs link_macos_x86_compiler_runtime
    content_hash: null
  - kind: ci-log
    reference: GitHub Actions run 31774057855, all native targets and required check successful
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-08-14T15:43:00+09:30
    reason: Verified against the failed macOS Intel linker command and the subsequent hosted CI run where macOS Intel, Linux, Apple Silicon, Windows, publish verification, and the required aggregate all succeeded.
description: Ladybug's macOS x86 static archive needs LLVM's CPU-model runtime in Rust's nodefaultlibs final link.
tags:
- ci
- ladybug
- linker
- macos
- release
- x86_64
timestamp: 2026-08-14T15:42:00+09:30
title: Link LLVM compiler runtime for Ladybug on macOS Intel
type: agent-memory
---
When linking the prebuilt Ladybug 0.18.3 static archive for `x86_64-apple-darwin`, the archive can reference `__cpu_model`. Rust invokes the final C linker with `-nodefaultlibs`, so Clang does not automatically add the compiler runtime and the native artifact build fails with an undefined `___cpu_model` symbol. For macOS x86_64 only, resolve the active Clang resource directory through `xcrun clang --print-resource-dir`, add its `lib/darwin` directory to the native link search, and statically link `libclang_rt.osx.a`. Do not add this link to Linux, Apple Silicon, or Windows. Verify the repair with the hosted `macos-x86_64` native artifact build and smoke test, then confirm every other native matrix target remains green.