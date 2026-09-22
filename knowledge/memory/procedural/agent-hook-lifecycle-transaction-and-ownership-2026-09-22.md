---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: codex
  created_at: 2026-09-22T00:00:00Z
  last_verified_at: 2026-09-22T12:00:00+09:30
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/api/lifecycle.rs
    content_hash: null
  - kind: source
    reference: src/api/contracts.rs
    content_hash: null
  - kind: source
    reference: src/api/context.rs
    content_hash: null
  - kind: test
    reference: src/adapters/cli/tests/install.rs
    content_hash: null
  - kind: test
    reference: src/api/lifecycle.rs
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-22T12:00:00+09:30
    reason: Reviewed against the implemented lifecycle/config code and passing lifecycle, process, Clippy, and full workspace tests.
description: Lifecycle setup, reinstall, and MCP install reconcile repository-local hooks only after successful preflight, snapshot hook/MCP targets for rollback, and persist advisory ownership after success; direct MCP installs union existing hook clients.
tags:
- agent-hooks
- installer
- lifecycle
- mcp
- rollback
timestamp: 2026-09-22T00:00:00Z
title: Agent-hook lifecycle integration preserves ownership and registration transactions
type: agent-memory
---
Repository lifecycle requests carry an additive `agent_hooks` policy defaulting to `auto`; setup config schema v3 carries an optional advisory ownership record with `format_version`, `policy`, and `installed_clients`. Setup and reinstall replace desired ownership, while direct `mcp install` unions selected clients with existing ownership. Hook reconciliation uses the configured MCP descriptor command, and lifecycle paths snapshot project hook targets plus file-backed MCP registration targets before mutation, restoring them when later hook or registration work fails. Native client CLI registration remains best-effort on rollback and is reported as a caveat. Verification attaches hook readback alongside MCP transport verification.