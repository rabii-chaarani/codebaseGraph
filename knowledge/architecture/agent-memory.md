---
description: Repository-scoped durable agent memory contract, lifecycle, recall rules, and trust boundaries.
tags:
- architecture
- agents
- memory
- knowledge
title: AI Agent Memory
type: architecture
---
# AI Agent Memory

Repository memory is durable, curated knowledge for agents. It is stored as typed OKF concepts and remains separate from session history, raw conversation logs, and task scratch state.

See also [Knowledge Wiki Architecture](knowledge-wiki.md) and [Public Operations and Runtime Paths](operation-paths.md).

## Memory classes

- **Semantic** memory records stable repository facts, terminology, invariants, and ownership.
- **Episodic** memory records distilled outcomes from repository work when the outcome remains useful beyond one task.
- **Procedural** memory records verified runbooks, workflows, and operating constraints.

All v1 memory is repository-scoped. User, agent, organization, and cross-repository scopes are intentionally excluded.

## Source layout and contract

Memory concepts live at `memory/{semantic|episodic|procedural}/<memory-id>.md` inside the configured OKF bundle and declare `type: agent-memory`.

The `agent_memory` extension carries:

- schema version and memory kind;
- repository scope and lifecycle status;
- owner, creation time, review time, and verifier;
- structured provenance sources;
- supersession relationships;
- transition history.

A record request cannot choose its initial status. The server always creates a `candidate`.

## Lifecycle

Allowed transitions are:

```text
candidate   -> active | quarantined
active      -> superseded | quarantined
quarantined -> candidate
superseded  -> terminal
```

Activation records the verifier, verification time, and review reason. Supersession requires an active replacement in the same bundle, and the replacement must declare the prior memory in `supersedes`. Superseded source remains intact for audit; v1 has no physical memory deletion.

Invalid transitions are rejected before authoring. Transition writes use the controlled authoring boundary and its stale-write check.

## Recall behavior

`wiki_memory_recall` requires a bundle and query text, accepts optional memory-kind filters, and returns at most 20 results. It reuses deterministic concept-search ranking.

Default recall includes only valid, active memory. Candidate, quarantined, superseded, malformed, cross-bundle, and kind/path-mismatched records are excluded.

## Public operations

- `wiki_memory_record` writes a provenance-bearing candidate.
- `wiki_memory_recall` reads valid active memory.
- `wiki_memory_transition` applies the governed lifecycle.

The same operations exist in the transport-neutral Rust and HTTP contracts. MCP preserves their read/write annotations.

## Trust and safety

Recalled memory is advisory. It never overrides current instructions, authorization, or repository policy. Mutable code facts must be checked against the current codebase graph and source.

Do not store raw sessions, secrets, credentials, personal data, or copied tool output. Distill durable knowledge, cite repository provenance, quarantine suspicious content, and never automatically re-ingest recalled memory as new memory.

Malformed agent-memory metadata produces recommended-profile diagnostics but does not break baseline OKF consumption or conformance. Unknown OKF types and extensions remain forward-compatible.

## Deliberate v1 exclusions

V1 does not add embeddings, a vector database, model calls, background consolidation, automatic expiry, secret scanning, cross-repository memory, raw session storage, or physical deletion.