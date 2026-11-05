---
description: Adversarial findings, measured disk reachability, selected retention policy, and a safe automatic cleanup proposal at revision 6aa5ba8.
tags:
- artifacts
- cleanup
- graph-storage
- lifecycle
- review
title: Graph disk retention review and cleanup proposal — 2026-10-20
type: review
---
# Graph disk retention review and cleanup proposal — 2026-10-20

## Decision and status

Review and proposal only. No program source was changed and no graph, staging, cache, database, journal, or lock was deleted. The user selected this retention policy on 2026-10-20: **keep the current managed graph and its active parsing cache; retire older repository-local Direct graphs**. Explicitly requesting a retired Direct destination later will require a rebuild.

Scryer remains the authority for architecture and implementation briefs. This review used change `chg-138pdr` for investigation, Graph Store `node-88` and Materialization API `node-56`. Existing applicable claims are `resp-108` (complete publication), `resp-109` (reader/writer coordination), `resp-232` (abandoned workspace recovery), `resp-233` (lease-aware generation retirement), `resp-234` (validated partition reuse), and `resp-238` (complete candidate assembly). The cleanup proposal is not an implemented or qualified architecture claim. Detailed implementation ownership, interfaces, prerequisites and claim assignments belong in Scryer when implementation is requested.

## Review boundary and evidence

A fresh independent reviewer used **GPT-6.1 Sol, Max effort**, following the adversarial-review-engineer skill. The reviewed runtime/test source is HEAD `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5` on `codex/fix-k-wiki-diagnostics`. Source and tests matched HEAD during this investigation. Pre-existing wiki edits and untracked work were preserved.

The scope is graph state lifecycle: managed and Direct layouts, artifact writing/collection, staging ownership, publication, abort/no-op/recovery, leases, and relevant existing tests. This is not a whole-repository correctness or security audit.

The coordinator measured filesystem allocated blocks and classified content by active-manifest reachability. The snapshot was taken at 11:11:59 am Australia/Darwin on 2026-10-20. The active pointer was unchanged throughout the scan, no scanned files vanished, and a new build was in progress. Counts and sizes are observations, not a permanently stable quota or a guarantee that every old file is immediately safe to delete.

Local evidence:

- Inventory: `/private/tmp/codebasegraph-disk-review-2026-10-20.json`, SHA-256 `9d8c6bcc9e916a617cb1557ccf1e23323f7f566a7b5685c26d73a811a9701151`.
- Source manifest: `/private/tmp/codebasegraph-disk-review-source-snapshot-2026-10-20.json`, SHA-256 `006420a7f9fcac48ffc30f8faf7b3e73c8e2500f8a994bf43be502f5254d6364`.
- Isolated reviewer harness: `/private/tmp/codebasegraph-storage-review-wihkqic4/review.rs` and `result.json`; harness SHA-256 `d68e65801ad9e7da286a2b9859ab1d7974698b74c20f17c07063a6c60c3d49d7`.

Temporary evidence paths are local artifacts, not portable release qualification.

## Why these directories exist

| State | Allocated size | Meaning and disposition |
| --- | ---: | --- |
| `native-staging/run-*` | 49.395 GiB | 1,328 old staging directories, approximately 139,930 files. Directory modification dates are in August. They contain generated import JSON/CSV. Migrate and reclaim only after proving legacy ownership is quiescent. |
| Other `native-staging` contents | About 6 MiB | Old source snapshots and incidental directory metadata. Include recognized program-owned snapshot state in migration; do not treat arbitrary unknown content as disposable. |
| Active generation, database and search sidecars | 1.264 GiB | Current graph bundle. Retain atomically with its manifest/readiness/metadata. |
| Artifacts referenced by the active manifest | 2.255 GiB | 323 active source-partition keys. Useful, regenerable parsing data required to preserve current reuse behavior. Retain under the selected policy. |
| `storage/artifacts/.tmp-*` | 44.08 MiB | 54 abandoned temporary directories. Current artifact-key GC never enumerates them. |
| Other valid artifact keys at snapshot | 236 KiB | Only two keys; a live build may own them. Do not infer garbage from absence in the active manifest alone. |
| `storage/runs/run-*` at snapshot | 2.861 GiB | An in-flight staged run, including staging/candidate state. Size changes during normal full-generation construction. Preserve while live; recover on proven abandonment. |
| `.direct-artifacts-cf7528fe0cf5be9b` | 759.4 MiB | Separate cache for the former repository-local Direct database/manifest pair. Retire as a destination bundle under the accepted policy, using its own ownership lock. |
| Former root `manifest.json` | 140.6 MiB | Legacy schema-2 manifest containing 201 files. It accompanies the old Direct destination, not the active compact managed manifest. Include it in coherent retirement. |

The Direct cache suffix exactly matches `DirectLayout::destination_key` for this repository's `.codebaseGraph/codebaseGraph_graph.ldb` and `.codebaseGraph/manifest.json`. `src/storage/layout.rs:280-305` derives cache roots from the database/manifest destination. Managed builds use `storage/artifacts` instead (`src/api/materialization.rs:846-870`). Merely being unreferenced by the managed configuration does not by itself prove a Direct destination has no live explicit reader or writer.

The initial roughly 6.5 GiB managed-storage measurement combined the active generation, its useful parsing cache, and transient build space. It was not 6.5 GiB of redundant graph copies. At the measured workload, normal quiescent storage is approximately **3.52 GiB**, plus small coordination metadata. Temporary build capacity remains necessary. Roughly **50 GiB** is potentially recoverable historical/temporary state after safe migration and Direct retirement; the exact result depends on ownership checks and any changed active inputs.

## Confirmed findings

### P2 — Historical staging is outside the current cleanup namespace

**Current locations:** `src/api/materialization.rs:285-290`, `src/storage/layout.rs:94-99`, `src/storage/managed.rs:410-423`.

The old pipeline allocated a unique `native-staging/run-<pid>-<timestamp>-<sequence>` directory. At historical revision `e4f5c95^`, `src/execution/run.rs:121-122` reached `staging_run.cleanup()` only after successful graph writing. Earlier errors returned through `?`; `src/staging_writer/run_directory.rs:10-41` had no Drop cleanup, and explicit cleanup ignored deletion errors. Forced process termination also bypassed cleanup.

The current API redirects actual staging to a leased run workspace and cleans that workspace. Its managed janitor visits `storage/runs`, not the old root. Therefore upgrading the pipeline does not collect previously stranded directories; ordinary current cleanup cannot reclaim the observed historical staging. This is confirmed by the source trace plus the measured directory layout. The individual historical event that stranded each directory is not known.

**Remedy:** A one-time, idempotent legacy-state migration integrated with storage maintenance, with verified quiescence and confined deletion. Current writer locks alone cannot prove that older helpers without those locks have stopped. Report an explicit blocked reason when proof is unavailable.

**Verification:** Seed the historical layout, interrupt legacy-compatible staging, migrate, restart during collection, and verify eventual reclamation while preserving query results and every live/current workspace.

### P2 — Artifact collection permanently overlooks interrupted temporary writes

**Locations:** `src/artifact_store.rs:156-190,273-315,447-473`; `src/api/materialization.rs:1165-1188`.

Artifact writes create a root-level `.tmp-<key>-<nonce>` directory before publishing by rename. Process death or selected fallible sync/comparison operations can leave it behind. Replacement also briefly creates `.replaced-*` backups. The only artifact collector iterates `list_keys()`, which explicitly skips root-level temporary directories and accepts only valid hexadecimal artifact directory names beneath prefixes. Replacement leftovers are also invisible to that enumeration.

The isolated harness used verbatim production enumeration, deletion, GC, confinement and sync functions with minimal stand-in types. It retained the active key and removed an obsolete valid key, but left both synthetic `.tmp-*` and `.replaced-*` residue after **two** successful collections. It also rejected a symlinked artifact root while preserving an external sentinel. This validates collection behavior; it did not inject a crash into the shipped binary.

**Remedy:** Give transient artifact state an explicit storage-owned lifecycle and recover or collect proven-abandoned temp/replacement state under the same destination ownership protocol as its writers. If a referenced artifact is missing, reconcile a matching replacement backup before deciding that backup is disposable.

**Verification:** Interrupt each write/replacement transition, then run maintenance repeatedly. Assert temporary bytes converge to zero once ownership is released, active artifacts remain usable, and live writers and foreign/symlinked paths survive.

### P2 — Aborted artifacts are not retried by ordinary recovery or unchanged refresh

**Locations:** `src/api/materialization.rs:323-337,482-503,1003-1004,1053-1057`; `src/storage/managed.rs:410-423`; `src/api/context.rs:634-668`.

Artifact GC is called during successful finalization. Abort cleans candidates/workspaces but not shared artifact keys already written by the failed attempt. The runtime janitor recovers runs and retired generations without collecting artifacts; unchanged-refresh acknowledgement finishes without an artifact sweep. Consequently a failed build, or interruption after publication but before artifact GC, can leave obsolete keys until another fully successful publication. Health and no-op refresh can leave them indefinitely and do not account for them as artifact cleanup pending.

Normal successful finalization does collect obsolete valid keys while retaining writer ownership; that safeguard defeats a claim that all current parsing artifacts leak. Most measured artifacts are current and should remain.

**Remedy:** Schedule recoverable artifact maintenance independently of successful publication, including startup and terminal build paths, with paced retry for deferred work and honest category-level pending status.

**Verification:** Fail after creating new artifacts, perform only recovery/health and unchanged refresh thereafter, and require eventual cleanup without a source edit or forced rebuild. Repeat for interruption after activation and before GC.

## Safety conditions and counterevidence

- Current generation retirement is lease-aware. `src/storage/managed.rs:718-771` excludes the active generation and requires an exclusive generation lease before deletion. Exactly one generation existed at the measured snapshot.
- The current source does **not** implement all guardian/containment guarantees described in the architecture documentation. See [Graph Freshness and Recovery](./graph-freshness-recovery.md) for intended recovery behavior and the later [worker recovery regression contract](./worker-recovery-regression-contract.md) for merged-repair qualification. Outer-worker termination can leave database-loader descendants. Expanding cleanup without proving those descendants empty can erase files still in use. Treat containment repair/verification as a prerequisite for automatic abandoned-run destruction, not as an already-present safeguard.
- Direct readers hold the destination's shared writer lease; retirement must acquire that **Direct destination's** exclusive lease, not just the managed-store lease. Keep stable lock files/inodes, including zero-byte coordination files.
- The public low-level materialization pipeline can receive caller-owned staging paths and returns COPY paths. Never turn an arbitrary `request.staging_dir` or user-specified database parent into a recursive deletion root. Automatic collection is limited to registered program-owned layouts.
- Old directory dates, missing PIDs and filename patterns are not sufficient deletion authority. Unknown ownership, versions or containment preserve state and expose a retryable reason.
- Large recursive removal must not run while holding the state lock needed by graph readers. Establish eligibility with short critical sections and delete eligible tombstoned state in bounded, restartable batches.

## Proposed plan

1. **Define one storage inventory and retention calculation.** Classify active bundles, leased retired generations, live runs, journal/containment evidence, active/live artifact references, temporary writes and approved retired destinations. Reconcile interrupted publication before computing reachability. Preserve active graph and source-cache behavior. Reject symlinks, traversal and unknown ownership.

2. **Establish deletion authority before broadening collection.** Restore or verify whole-build containment and persisted proof across crash/cancel/recovery. Coordinate every supported writer to an automatically collected root. Maintain writer-to-state lock ordering and avoid reacquiring a writer lease already held by the caller. Acquire each retired Direct destination's own exclusive ownership.

3. **Add recoverable collection and legacy migration.** Collect obsolete valid artifacts, proven-abandoned `.tmp-*` and dead run state; reconcile replacement backups before collecting `.replaced-*`, preserving a usable backup when its referenced destination is missing. Retire the approved old Direct database, manifest, sidecars and artifact cache coherently. Sweep recognized historical `native-staging` state only after a version-aware, verified quiescent migration. Use a same-root tombstone/retirement journal so interrupted deletions retry. Never remove lock inodes or promote old candidates accidentally.

4. **Make maintenance automatic and observable.** Run bounded maintenance at startup, build completion, failure/cancellation, unchanged refresh and paced coordinator maintenance. A busy writer defers it. Cleanup failure preserves the primary build error and records retryable pending work. Cleanup must not trigger another graph build. Expose allocated bytes by category, reclaimable bytes, pending bytes and reasons such as live lease, live writer, unknown containment or filesystem error. No fixed total-size cap may delete necessary active inputs.

5. **Qualify convergence and failure safety before rollout.** Extend existing lifecycle, artifact-reuse and generation-churn verification to whole-state allocated disk usage and the cases below. First offer a reviewable inventory/dry-run report, then exercise collection in isolated repositories. Only after acceptance should the approved migration run against this installation. Windows and macOS native lifecycle qualification remains distinct from a local function harness.

## Acceptance and evidence plan

| Criterion | Applicable existing claim | Observable check | Evidence status |
| --- | --- | --- | --- |
| Active graph and search remain identical across cleanup | resp-108, resp-109 | Search/context/query equivalence before/after; active bundle checksums where unchanged | Planned |
| Idle state contains one generation, no abandoned run/temp/replacement/legacy state | resp-232, resp-233 | Churn plus crash fixtures, repeated maintenance and whole-state allocated-byte comparison to a clean control | Planned |
| Current cache reuse survives cleanup | resp-234 | Unchanged source uses retained valid artifacts without reparsing | Planned |
| Concurrent work and reader leases are protected | resp-109, resp-233 | Paused writer/reader, active-pointer transition and destination-retirement interleavings | Planned |
| Interrupted cleanup converges without a new source edit | resp-232 | Process death around artifact/publish/tombstone steps; health/no-op-only/maintenance retries | Planned |
| No deletion outside registered roots | resp-109 plus established confinement contract | Symlink, traversal, foreign metadata, arbitrary low-level staging and missing-root fixtures | Harness verifies artifact root symlink rejection only; remaining cases planned |
| Retired Direct destinations follow the selected policy safely | New policy selected 2026-10-20; Scryer contract mapping required before implementation | Own-destination lock, coherent bundle retirement, later explicit old-destination rebuild behavior | Policy selected; implementation/qualification pending |
| Pending space is reported truthfully | Existing lifecycle context; additive observability claims require Scryer review | Failed-only builds, permission failures and unknown containment expose category/reason and later clear on recovery | Planned |

The existing generation-churn test `tests/generation_storage.rs:178-250` checks one generation, zero runs, graph queryability and database-size convergence, but does not account for the entire state tree or historical and temporary artifact namespaces. The artifact GC unit test `src/api/materialization.rs:1401-1440` checks active/stale valid keys only. These tests are useful counterevidence for normal generation leaks but do not disprove the findings above.

## Executed checks and limitations

- Filesystem allocated/logical-size inventory and active-manifest reachability classification completed read-only.
- Source/history traces and source-to-Scryer mapping completed; current program source/tests were unchanged.
- Isolated extracted-function harness compiled with `rustc --edition=2021 -D warnings` and exited 0. Synthetic temporary/replacement leftovers survived two collections; the active key survived, obsolete key was deleted, and the symlink escape was rejected.
- No production crash/cancel injection, graph rebuild, load test, full suite or cross-platform qualification was performed for this review. No current-process attribution is claimed from metadata; process listing was sandbox-restricted. A final optional graph dependency query encountered a server transport failure after source discovery; it does not change the cited source traces.
- The executing installed daemon binary was not conclusively bound to this source revision. On-disk observations are reported separately from current-source findings. Installed config/Cargo metadata can be stale and are not binary provenance.
- Existing [storage lifecycle intent](./graph-storage-lifecycle.md), [materialization pipeline](./materialization-pipeline.md) and the [worker recovery contract](./worker-recovery-regression-contract.md) remain canonical context. No implementation has satisfied the proposed acceptance checks yet.

## Handoff

Recommend implementing the cleanup work with the selected retention policy, while treating legacy/Direct deletion authority and descendant containment as required safeguards. Preserve existing architecture and closure identities. Before implementation, author any necessary new policy/observability claims and detailed execution briefs in Scryer, obtain sign-off for changed intent, and follow the repository's large-implementation review and qualification gates. This review does not approve arbitrary directory deletion or claim those changes are already delivered.


## Specification validation follow-up — 2026-10-20

The [five-step cleanup specification](./graph-disk-cleanup-contract.md) defines stable steps DC-S1–DC-S5, observable requirements, failure/recovery behavior and acceptance checks. The user requested individual validation before implementation. Revision 0.1 of DC-S1 is proposed first; all five step approvals remain pending until explicitly recorded in that contract. The earlier retention-policy selection remains settled and does not approve the new step specifications. Scryer remains the authority for architecture and detailed implementation briefs; no implementation or live cleanup follows merely from authoring these drafts.
