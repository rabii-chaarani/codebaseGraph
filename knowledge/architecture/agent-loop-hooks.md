---
description: Repository-local hooks that inject bounded, advisory graph context into supported coding-agent loops.
resource: repository-architecture
tags:
- agents
- architecture
- hooks
- mcp
- runtime
title: Agent-Loop Hooks
type: architecture
---
# Agent-Loop Hooks

Agent-loop hooks connect repository-local coding-agent lifecycle events to the existing Graph Runtime without creating a second graph owner.

## Supported clients

The first adapters target Codex, Claude Code, GitHub Copilot CLI, and Copilot in VS Code. Codex writes `.codex/hooks.json`, Claude writes `.claude/settings.json`, and the Copilot CLI/VS Code adapter writes `.github/hooks/codebase-graph.json`. Copilot cloud-agent events are a successful no-op because the local loopback daemon is unavailable there.

Repository setup selects hooks with `--agent-hooks <auto|none|codex|claude|github-copilot|all>`. The dedicated `agent-hooks install`, `remove`, `verify`, and `run` commands manage or execute the integration. Configuration merges preserve unrelated settings and managed entries are idempotent and removable.

## Agent-loop path

```text
agent lifecycle event
  -> client-specific hook envelope
  -> Agent Hook Adapter
  -> fixed repository setup identity
  -> managed loopback MCP daemon
  -> graph_health
  -> bounded semantic graph_search
  -> advisory client context
```

`SessionStart` checks graph health and reports repository identity and freshness. Each non-empty user prompt performs a health check followed by a semantic, slim search limited to five matches and one context level. Agents request `graph_context` explicitly for deeper dependencies, call graphs, runtime, docs, or change-impact analysis.

## Transport budget

Connection establishment, request writes, and response reads share one elapsed deadline. Hook `graph_health` and `graph_search` requests send their remaining per-call budget in `X-CodebaseGraph-Timeout-Ms`, capped at 900 ms; ordinary MCP callers omit this metadata. The server subtracts receive time and carries the remaining budget to the repository coordinator.

There is no graph execution queue. A busy executor returns `graph_busy`; an expired request returns `deadline_exceeded` and never dispatches if it has not started. Tool calls are not retried. An already-running native read retains the execution slot until completion even after the hook times out. This bounds abandoned work; it does not guarantee successful context delivery within the budget under refresh contention. That latency work remains in STAB-08/10.

## Invariants

Hooks are advisory and fail open within three seconds. They never approve permissions, deny tools, block the agent, rebuild the graph, or open another graph database. Repository watchers own refresh. Event input, prompt input, and injected context are bounded; raw prompts and transcripts are not persisted. Copilot's local delivery cache contains only a prompt hash, bounded result, delivery state, and timestamp, and stale entries expire after 24 hours.

Copilot cache consumers serialize each claim with a nonblocking exclusive OS file lock held through rename, reading, and marking delivery. A contending hook returns no context immediately and leaves the pending cache entry available for a later hook. The lock is shared across the repository's cache consumers, so it adds one stable `.claim.lock` file rather than one retained lock per session. Closing the handle or exiting the process releases the lock. The file itself is never removed on release or by cache expiry: replacing its filesystem identity would let concurrent processes lock different files under the same path. Rename alone is not the concurrency guard.

Project-local hooks require client or workspace trust. Installation reports trust and reload guidance but does not bypass host policy. A malformed event, unavailable daemon or executable, stale/unknown graph, or repository identity mismatch produces a warning or no-op rather than cross-repository context.

Related: [Public Operations and Runtime Paths](./operation-paths.md), [Graph Runtime Architecture](./graph-runtime.md), [Graph Freshness and Recovery](./graph-freshness-recovery.md).
