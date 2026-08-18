---
description: Immutable generation storage, recoverable run workspaces, artifact reuse, legacy policy, and the operator recovery runbook.
resource: repository-architecture
tags:
- architecture
- generations
- graph-storage
- recovery
- runbook
timestamp: 2026-08-18
title: Graph Storage Lifecycle and Recovery
type: architecture
---
# Graph Storage Lifecycle and Recovery

Graph storage format v2 prevents unbounded embedded-database growth by replacing in-place graph mutation with immutable generations. It also makes build cleanup recoverable: every mutation is represented by a journaled run workspace, and publication changes one atomic active-generation pointer.

## Managed layout

A configured `storage_root` owns all mutable graph state:

```text
storage_root/
  active.json
  generations/
    gen-<id>/
      database/
      search sidecar siblings
      manifest.json
      metadata.json
      READY
      lease.lock
  runs/
    run-<id>/
      journal.json
      lease.lock
      staging/
      candidate/
  artifacts/
    <content-addressed raw partitions>
  writer.lock
  state.lock
```

`active.json` is the only publication pointer. A generation is eligible for activation only after its database, manifest, metadata, and readiness marker are durable and its reopened read-only database passes schema, count, and artifact validation.

## Storage invariants

1. **Managed updates never mutate the active database.** Every build writes a fresh, self-contained candidate generation.
2. **Publication is atomic.** The publisher holds the exclusive writer lock for the mutation and the exclusive state lock while replacing and fsyncing `active.json`.
3. **Failure preserves the active graph.** The current generation is not modified, retired, or deleted before the candidate is validated and publication succeeds.
4. **Readers lease exactly one generation.** Resolving the active pointer uses the shared state lock; the read operation then holds a shared generation lease until all database access is complete.
5. **Retirement is lease-aware and has no time-based retention.** A superseded generation is deleted as soon as its final reader releases the lease. The last reader, publisher, and later runtime entries all retry retirement. Failure remains visible as `cleanup_pending`.
6. **Only one managed generation remains at idle.** With no readers and no cleanup error, the active generation is the only generation directory.
7. **Paths are confined.** Cleanup accepts only expected descendants of the managed root, rejects symlinks and traversal, and never follows links into user data.
8. **Direct paths remain transactional.** Explicit `--db` and `--manifest` targets use adjacent shadow files plus a checksummed recovery journal so the pair cannot remain half-published after a crash.

These rules replace the stale-file write-intent heuristic and all in-place partition deletion or replacement. The compatibility `atomic_rebuild` request field remains accepted but does not re-enable in-place mutation.\n\n## Refresh ownership\n\n`refresh.lock` is independent from `writer.lock` and `state.lock`. Its nonblocking exclusive holder is the only process allowed to create a repository watcher. Followers do not materialize and retry election every second with up to 250 ms of deterministic jitter; operating-system lock release enables takeover without a persisted leader record. On acquisition, the new leader reconciles the active manifest before it begins watching.\n\nManaged storage places the lock under `storage_root`. Direct storage derives a destination-scoped lock from the explicit database and manifest pair. Lock files reject symlinks. Refresh candidates still acquire the ordinary writer lock for the complete mutation, and an unchanged refresh may close its write session without publishing after comparing against the latest active manifest.

## Run workspace lifecycle

Each build owns a `RunWorkspace` whose durable journal advances through:

`created -> staged -> candidate_ready -> publishing -> published`

A failure records `failed`; a cleanup failure records `cleanup_pending`.

Normal completion calls explicit `finish`; error paths call explicit `abort`. Both report cleanup errors without masking the primary build error. `Drop` is only a best-effort fallback.

On later runtime entry, the janitor takes the run lease before acting. It removes unlocked pre-publication and published workspaces, completes or rolls back an interrupted publication deterministically from the journal, and leaves locked live runs untouched. Repeated recovery and cleanup are idempotent.

## Durable partition artifacts

Raw source partitions are stored content-addressably. An artifact key includes repository identity, relative path, content hash, language, parser version, profile version, ontology version, and artifact schema version.

A materialization run revalidates source hashes, reuses valid unchanged artifacts, rebuilds missing or corrupt entries, and reloads each partition only for its current bounded pass. Compact manifest v5 retains path, content hash, language, partition ID, artifact key, row counts, and timestamp; readable v4 generations receive one bounded full rebuild on their next write. Artifact garbage collection retains entries referenced by the active manifest or a live run and removes everything else.

Artifact reuse reduces parsing work. It does not make graph database files incremental or mutable.

## Legacy storage policy

Schema-v1 installations remain readable for search, context, query, and health. Build, watch, refresh, and install return the typed `legacy_storage_requires_reinstall` error.

Reinstall renames the legacy state without copying it, builds and validates storage v2, and restores the legacy state if failure occurs before activation. After successful activation and validation it deletes the renamed legacy state immediately. There is no rollback grace period and no automatic migration during ordinary mutation commands.

## Health and operator signals

Health and materialization output expose:

- storage format and writability;
- active generation;
- reused and rebuilt artifact counts;
- pending run count and cleanup status;
- physical and logical database sizes;
- parsing, staging, search, and database phase high-water marks plus spill bytes;
- configured Rust and worker RSS limits;
- refresh role, leader process, pending state, coalesced and overflow counts, deduplicated refreshes, and the latest no-op reason.

A healthy idle managed store reports format v2, one active generation, zero run directories, and `cleanup_pending = false`.

## Recovery runbook

1. Confirm the process reporting refresh role `leader`, then quiesce the repository watcher and any long-lived readers before reinstalling or investigating retirement.
2. Run health and record the storage format, active generation, pending runs, cleanup status, and physical/logical sizes.
3. If a v2 run or publication was interrupted, enter the runtime through health or another repository operation. The janitor will acquire unlocked run journals and recover them before normal work continues.
4. If `cleanup_pending` remains true, confirm no process holds the run or retired-generation lease, then enter the runtime again. Do not manually delete a locked workspace or generation.
5. For a schema-v1 repository, use explicit reinstall. Verify the new active generation and graph queries before restarting the watcher; successful reinstall deletes the renamed legacy state immediately.
6. Confirm the idle store contains exactly one generation and no run directories. On Unix, also confirm no deleted legacy database file remains open.
7. Restart the MCP watcher against the v2 configuration and repeat health plus search/context/query smoke tests.

If cleanup rejects a path or symlink, preserve the reported path and investigate the managed root rather than bypassing confinement. This design intentionally does not depend on a Ladybug fork, truncate patch, or dependency upgrade.

## Verification expectations

Lifecycle regressions cover failure at parsing, staging, search construction, isolated database loading, database validation, and publication; abandoned-run recovery; symlink rejection; lease-delayed retirement; stale-base rejection; schema-v1 write rejection; reinstall restoration and immediate legacy deletion; direct-mode paired recovery; artifact invalidation and corruption; deterministic clean-rebuild equivalence; and artifact garbage collection.

The churn acceptance test performs 10–20 updates. At idle it must leave one managed generation and no run directories, preserve graph results, and keep final physical database size within the greater of 10% or 8 MiB above a clean-control rebuild.

Related: [Graph Runtime](./graph-runtime.md), [Materialization Pipeline](./materialization-pipeline.md), [Public Operations and Runtime Paths](./operation-paths.md), and [Architecture Invariants](./invariants.md).