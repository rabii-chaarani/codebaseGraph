---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: candidate
  owner: codebaseGraph maintainers
  created_at: 2026-08-18
  last_verified_at: null
  verified_by: null
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: source
    reference: src/coordinator.rs
    content_hash: null
  - kind: source
    reference: src/materialization_worker.rs
    content_hash: null
  - kind: source
    reference: src/db_writer/phase.rs
    content_hash: null
  - kind: test
    reference: tests/coordinator_process.rs::twenty_mcp_clients_share_one_coordinator_worker_and_take_over
    content_hash: null
  - kind: wiki
    reference: architecture/graph-storage-lifecycle
    content_hash: null
  history: []
description: Durable ownership and crash-recovery boundary for repository-scoped MCP graph access and materialization.
tags:
- architecture
- coordinator
- mcp
- memory
- recovery
- worker
timestamp: 2026-08-18
title: MCP graph access uses one repository coordinator and kill-safe workers
type: agent-memory
---
# MCP coordinator and worker ownership boundary

For each managed storage root or Direct database/manifest pair, exactly one `coordinator.lock` holder owns the Public API Core and all MCP Ladybug database access. Other MCP processes route bounded authenticated frames over loopback and must not open the graph database.

MCP refreshes and coordinator-triggered explicit builds run through one `worker.lock` holder. The supervisor writes a versioned request, records build ID and PID in `worker.json`, and creates a start gate before the child executes. The child also inherits a parent-owned control pipe; coordinator death closes the pipe and terminates the child. A successor acquires the released lock, waits for the recorded PID to exit, removes only matching state and confined worker workspaces, then relies on ordinary run-journal recovery.

Ladybug write phases remain short-lived nested children. Their 25 ms supervisor accounts materialization-parent plus phase-child RSS together, so the configured worker ceiling covers graph-load memory as well as Rust orchestration state. A crash, budget kill, or lost result cannot publish an incomplete candidate; publication reconciliation accepts only an already active validated generation.