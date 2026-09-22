---
description: Repository-local hooks that inject bounded, advisory graph context into supported coding-agent loops.
resource: repository-architecture
tags:
- architecture
- agents
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

## Invariants

Hooks are advisory and fail open within three seconds. They never approve permissions, deny tools, block the agent, rebuild the graph, or open another graph database. Repository watchers own refresh. Event input, prompt input, and injected context are bounded; raw prompts and transcripts are not persisted. Copilot's local delivery cache contains only a prompt hash, bounded result, delivery state, and timestamp, and stale entries expire after 24 hours.

Project-local hooks require client or workspace trust. Installation reports trust and reload guidance but does not bypass host policy. A malformed event, unavailable daemon or executable, stale/unknown graph, or repository identity mismatch produces a warning or no-op rather than cross-repository context.

Related: [Public Operations and Runtime Paths](./operation-paths.md), [Graph Runtime Architecture](./graph-runtime.md), [Graph Freshness and Recovery](./graph-freshness-recovery.md).
