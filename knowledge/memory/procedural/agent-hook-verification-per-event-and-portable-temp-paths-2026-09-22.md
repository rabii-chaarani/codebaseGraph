---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-22T00:00:00Z
  last_verified_at: 2026-09-22T14:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/agent_hooks.rs:640-720
    content_hash: null
  - kind: source
    reference: src/adapters/cli/agent_hooks.rs:203-220
    content_hash: null
  - kind: test
    reference: tests/agent_hooks_process.rs:350-425
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-22T14:00:00+09:30
    reason: Reviewed against per-event verification code, the missing-event regression, and the portable Windows path fix.
description: Hook verification must validate every required event independently, and CLI path tests must avoid Unix-only temporary paths.
tags:
- agent-hooks
- ci
- verification
- windows
timestamp: 2026-09-22T00:00:00Z
title: Agent-hook verification is per-event and tests use platform-native temporary paths
type: agent-memory
---
`verify_agent_hooks` reports managed-handler counts per required event and only succeeds when every event has at least one managed handler; duplicate handlers in one event cannot mask a missing event. Regression tests should remove one required event and duplicate another. For repository-local CLI path derivation tests, build expected roots from `std::env::temp_dir()` and assert `Path::is_absolute()` plus platform-neutral joins instead of hard-coding `/tmp`.