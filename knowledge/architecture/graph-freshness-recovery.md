---
description: Repository identity, supervised refresh, source selection, reconciliation, and truthful freshness reporting.
resource: repository-architecture
tags:
- architecture
- graph-runtime
- refresh
- recovery
timestamp: 2026-09-08
title: Graph Freshness and Recovery
type: architecture
---
# Graph Freshness and Recovery

Continuous refresh keeps the configured repository graph convergent while preserving the previous readable generation during failures and builds. This implementation follows the 2026-09-08 LoopAI investigation and its approved recovery proposal.

## Incident and corrected identity boundary

In version 1.7.0 before this change, a config-only daemon could open the correct managed graph while choosing its source root from process cwd. LoopAI reported source root `/`, failed startup reconciliation on a protected system directory, and kept serving its old generation after the refresh thread exited. The investigation reproduced this independently with a config-only MCP process launched from an unrelated directory.

The runtime now binds explicit root/config input before discovering defaults. A configured source root is resolved relative to the selected config file when necessary; a normal `<repo>/.codebaseGraph/config.json` can identify its repository. Arbitrary config locations do not imply that their grandparent is the source root. Missing explicitly selected configs, invalid roots, and managed root/config conflicts fail explicitly.

The coordinator pins repository identity without retaining a graph read lease. Source root, selected config, managed storage, and effective direct destinations cannot silently rebind while old ownership locks remain held. Active generations are still resolved per operation. Materialization validates the binding before writing, before acknowledging an unchanged refresh, and before publication. Failure aborts the candidate and preserves the previous generation.

Direct storage retains the existing spelling of its destination paths for journal and lock recovery: those filenames historically hash the database/manifest path strings. Canonicalizing `/var/...` into `/private/var/...` at that boundary would miss existing recovery journals. Identity validation preserves this provenance. Genuinely unbound standalone native requests with explicit direct destinations retain their own source-root authority; managed or explicitly bound requests must match the selected repository.

## Refresh lifecycle

One refresh lease holder runs source monitoring and reconciliation. Native event collection starts before startup catch-up. Native, polling, and CLI watch paths also reconcile periodically, so notifications improve latency without being the sole correctness mechanism.

Pending changes survive failed attempts. Dirty epochs are recorded before relevant events enter the bounded queue; successful work acknowledges only the epoch captured by its attempt. Changes received during a build remain pending. Current materialization compares the full source manifest even for path-triggered requests.

Refresh failures remain observable and recover through supervision. Transient storage, source-change, and worker-exit errors retry with bounded delays. Persistent errors retain unsatisfied work and use paced retries, including increasing blocked backoff. Scheduling and `next_retry_unix_ms` share the same deadline. Corrected configuration is loaded before another attempt, so a failing build does not keep retrying with obsolete limits.

Dropping the last coordinator owner stops the refresh task. Registered MCP materialization workers support cancellation while waiting for ownership and while running: the child is killed and reaped, progress is drained, and worker state, workspace, and ownership are released. The embedded fallback without a registered executable remains synchronous and only checks cancellation before dispatch.

## Source selection and configuration

Scanning and monitoring share `SourceSelection` for includes, excludes, ignore patterns, and generated output. Excluded directories are pruned before descent; an unreadable `node_modules` subtree must not break a source scan. CodebaseGraph-owned state remains excluded. Generated `.kwiki`, `.scryer`, `.astro`, and `dist-*` directories are excluded by default and can be explicitly included where supported; ordinary source filenames beginning with `dist-` remain eligible.

The native callback rejects irrelevant access/read events and protected output before queue admission. Relevant overflow and OS pathless rescan notifications request a complete reconciliation. Polling tracks configuration paths separately from excluded state directories and avoids traversing symlinks outside the source tree.

Config and ignore-file fingerprints use content hashes, including for missed-notification recovery. Runtime settings and filters reload coherently, and effective limits appear in status. Explicit command-line overrides retain precedence. A running service can release monitoring when config policy becomes off; initial MCP policy off retains the existing watcher-free startup behavior. Changes to repository identity require restart instead of retargeting an active owner.

Install schema v3 remains compatible. Refresh configuration supports:

```json
{
  "refresh": {
    "policy": "leader",
    "backend": "auto",
    "reconcile_interval_ms": 30000
  }
}
```

The interval must be positive. Backends are `auto`, `native`, and `poll`; auto may fall back to polling. The source-snapshot consistency check is retained: a source that changes after its hash was captured produces the stable `source_changed` error and a fresh scan is required.

## Health contract

Existing `ok` continues to mean graph readability. It does not claim freshness. Health adds `freshness`, `refresh_readiness`, and `refresh_health`, while retaining the raw `refresh` details.

- `current` means a readable graph with a live, running task, successful reconciliation, and no newer known dirty epoch, as of that reconciliation.
- `pending` means newer known work remains.
- `overdue` means the configured reconciliation interval elapsed.
- `unknown` covers absent, disabled, standby, stopped, unreconciled, errored, or otherwise unverified refresh state.

The active generation publication time is reported as `active_generation_published_at_unix_ms`, read from the same leased generation metadata as its identity; unknown and direct-mode timestamps remain null.

The default block output includes refresh state, liveness, effective root, pending work, successful reconciliation time, retry schedule, and failure reason. An old publication time alone never establishes staleness: a successful no-op reconciliation may prove an old generation is still current.

## Verification and operational recovery

Regression coverage includes unrelated-cwd config-only daemon startup with persistent MCP create/rename/delete queries; direct recovery journal compatibility; native-request source provenance; excluded unreadable directories; changes during source snapshots; pathless rescan and configuration fingerprints; failed work and retry deadlines; worker cancellation and lock waits; and real HTTP budget failure followed by config repair without another source edit. Process tests also exercise live exclusions and command-line override precedence.

Use the rebuilt runtime for operational recovery, retain the current managed graph, restart the affected daemon, and verify effective root, successful reconciliation, visible source changes, and no pending failures. Reinstalling or deleting the graph is not the repair. A manual rebuild alone cannot correct an old daemon's source-root or refresh-lifecycle behavior.

See [Graph Runtime Architecture](./graph-runtime.md), [Materialization Pipeline](./materialization-pipeline.md), [Graph Storage Lifecycle and Recovery](./graph-storage-lifecycle.md), and [the incident memory](../memory/episodic/config-root-loss-stops-refresher-2026-09-08.md).
