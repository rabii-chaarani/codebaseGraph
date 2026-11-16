# CBG-REBUILD-01 — Explicit rebuilds and two-slot storage

## 1. Identity, status and authority

| Field | Value |
| --- | --- |
| Work | [COD-125](https://linear.app/codebasegraph/issue/COD-125), CBG-REBUILD-01 |
| Preparation | [COD-126](https://linear.app/codebasegraph/issue/COD-126), D00 |
| Canonical specification | `docs/specs/CBG-REBUILD-01/spec.md`, proposed revision **0.1**, 2026-11-16 |
| State | Proposed contracts; no exact-revision approval, product implementation, native qualification or closure |
| Authoring base | `dev`, full SHA `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9` |
| Inspected comparison | `main`, full SHA `90e2ab6fae2767bdb158e19f0072d43568e80dbc` |
| Preparation authority | Scryer `chg-c9eh2t`; existing runtime/storage owners, not the scoped CBG-C4-01 exception |
| Closure owner | Repository maintainer/user; child completion does not close COD-125 |
| Approval record | None. A later separate record must identify the approved path/full Git SHA, model change/snapshot, exact scope and actual human decision |

The user authorized D00 execution with “Please implement the plan” following the COD-126 preparation plan in this chat on 2026-11-16. This authorizes specification/model-proposal authoring, independent review and a proposal PR to dev. It does **not** approve the final bytes of this document, future product implementation, an auto-advancing issue queue, merge, release, service restart or live migration. Do not manufacture an approval file, sign-off, implementation verdict or passing product evidence.

The two bases have identical tracked product Rust source. Their differences are a 4.2.1 release, a `cc` build-dependency update and a release-workflow dependency update. They are not interchangeable runtime/build qualification revisions. Revalidate the actual integrated baseline before each dependent issue; do not import pending branches implicitly.

### Decisions and original contracts

The chat's 2026-11-13 selections authorize these proposal directions: product-wide watcher removal; paused target queries; retained last-good snapshot; immediate failed-build disposal; managed-only storage; controlled migration; captured per-file inputs; CLI/library-only builds; snapshot-owned parse reuse; stored-snapshot reads without source rescans; two fixed slots; breaking delivery; and an explicit first build after setup/activation.

The 2026-11-16 answers to `d00_legacy_formats` and `d00_cancel_cleanup` narrow migration to **managed v2 only** and require cancellation acknowledgment **after verified teardown and synchronous cleanup**. These decisions remain directions until bound to the final committed specification/architecture revision.

Read original wiki contracts through configured k-wiki at their existing concepts: `architecture/graph-storage-lifecycle`, `architecture/materialization-pipeline`, `architecture/graph-runtime`, `architecture/graph-freshness-recovery`, `architecture/graph-disk-cleanup-contract`, `architecture/invariants`, and `architecture/agent-loop-hooks`. They keep their original identities, revisions, approvals and closure owners. This document specifies a proposed narrow replacement for the CodebaseGraph product; it does not migrate or copy the wiki specifications.

Scryer remains the authority for this runtime/storage work. COD-19's approved CBG-C4-01-only transition does not select a new authority for this issue. Future implementation must bind approved intent and a versioned per-issue execution brief; changed file paths or labels alone grant no exemption.

## 2. Problem and observable outcome

The current watcher reuses parsing artifacts but assembles complete graph rows, search files and a fresh database for a materializing update. Live monitoring, repeated builds, concurrent generation readers, run workspaces, global cache references and deferred collection create lifecycle complexity. Failed attempts can retain substantial staging; directory size alone does not establish safe deletion authority.

The new product serves one explicitly built source snapshot. Only an explicit build constructs its complete replacement. The target graph pauses during rebuilding, allowing synchronous replacement and retirement without overlapping generation readers. Parsing reuse remains an optimization. The graph remains derived data, but a failed pre-publication rebuild preserves its previous validated snapshot.

### Scope and exclusions

Include CLI/library materialization, managed graph reads, setup/lifecycle defaults, MCP read catalogs, worktree routing, trusted hooks, optional wiki graph-client composition, two-slot storage, containment/recovery, controlled v2 migration and breaking-change documentation. Preserve graph/parser/ontology/search meanings, source-selection rules, current resource policy, native-runtime integrity and unchanged transport security/non-replay contracts.

Exclude incremental database mutation, a new database backend/version, engine extraction, schema-v1 or Direct migration, automatic builds/migration, new RSS/disk ceilings, new performance stages, GUI work, unrelated wiki storage/publication, live installation changes and aggregate closure. Temporary replacement space remains necessary; this is not a zero-copy or zero-extra-disk promise.

## 3. S01 — Explicit builds and snapshot reads

| Requirement | Contract |
| --- | --- |
| S01-R01 | Only explicit CLI `build` and the existing public library materialization operation construct a graph. MCP graph tools remain read-only; no MCP build/migration tool is added. |
| S01-R02 | Setup/install/reinstall configuration, registration/activation, daemon startup/restart, queries, health, discovery and hooks never construct an initial graph or schedule a later build. An installation without a snapshot is not-ready. |
| S01-R03 | Every accepted build assembles the complete database and search bundle, including unchanged inputs. There is no partial/delta database update and no unchanged-source build no-op. |
| S01-R04 | Capture the eligible source set with per-file hash verification using existing source-selection/ignore rules. Failure to capture a required input fails the request, rather than silently producing a partial graph. Once captured, later edits do not modify or abort the input set and do not schedule another request. This is not an atomic whole-source-tree or Git-commit capture. |
| S01-R05 | Between builds, graph reads use the stored snapshot without source rescans. Report snapshot identity, build time and explicitly unchecked freshness against current source. Repository/worktree identity validation remains required and is not a content-freshness scan. |
| S01-R06 | Remove continuous native/poll watchers, event/debounce queues, refresh election, dirty epochs, periodic reconciliation, automatic build retry and watcher-only configuration/status. Preserve coordinator/daemon ownership required by remaining callers. |
| S01-R07 | Retire Direct destination arguments, raw arbitrary-path writer bypasses, partial-update selectors and watch/refresh surfaces through a breaking contract. Reject retired inputs before effects with actionable explicit-build/migration guidance; do not silently ignore or alias them to a build. |
| S01-R08 | Keep non-mutating build planning. A preview may inspect source/current cache but never creates a candidate, publishes, migrates or manufactures ownership. |

### Public request boundary

Retain CLI `build` and the library materialization entrypoint rather than adding a parallel rebuild service. Keep normal repository/worktree selection, complete-build include/exclude selection, Git-based source enumeration, search choice, progress/output and existing concurrency/spill tuning. Source selection uses one canonical selector; no request may provide a second storage identity.

Remove public `mode`, `git_diff`, `git_base`, `candidate_paths`, raw native-request paths, and custom `db_path`/`manifest_path` targets. CLI `--mode` (including `full`), `--git-diff`, `--git-base`, `--db`, `--manifest`, `--native-request`, `watch` and watch/refresh-only options receive retired-input diagnostics. Internal worker protocols may still carry private resolved paths; they are not an alternate public writer. Low-level exported materializers/writers become internal where they could bypass managed admission. Preserve genuine read catalogs and parameterized read-only query validation.

An explicit migration uses `build --migrate-storage --confirm-legacy-quiescence` or equivalent library booleans. Confirmation is valid only with migration; ordinary reads/health cannot accept administrative confirmation or perform migration. A recorded migration may resume under the same durable association and approved authority without manufacturing a new confirmation or root. No confirmation substitutes for contradictory execution/ownership evidence.

### Result and error meanings

Use the canonical operation result/presenter owner for CLI/library parity. The build outcome carries:

| Field | Meaning |
| --- | --- |
| `publication` | `not_committed`, `committed`, or `unknown`; uncertainty is never converted to success or auto-replay |
| `cleanup` | `not_needed`, `complete`, `pending`, or `blocked`; `blocked` denotes missing safety authority, `pending` a known incomplete effect |
| `previous_snapshot_id` / `candidate_snapshot_id` | Opaque target-scoped identities, nullable when not present |
| `active_snapshot_id` | The validated selected snapshot, null if selection is unresolved; never infer it from candidate existence |
| `built_at` / `captured_inputs_digest` | UTC build timestamp and deterministic captured-manifest digest for the known active snapshot; null when no active snapshot is known |
| `freshness` | `unchecked` relative to current source; not a promise of fresh source, even immediately after construction |
| error/status detail | Canonical code, phase and bounded diagnostic; preserve primary and cleanup failures separately without credentials/control capabilities |

Preserve useful graph summary and artifact reuse/rebuild observations. Do not expose private worker paths or serialize internal capabilities as product metadata. Read/status output exposes the selected snapshot facts using the same meanings; no active graph gives not-ready, and rebuild admission gives rebuild-in-progress rather than queued work or another graph's data.

Freeze canonical product codes at the application boundary: `graph_not_ready`, `rebuild_in_progress`, existing shared-admission `graph_busy`, `migration_required`, `unsupported_storage_format`, `retired_input`, `publication_unknown`, `containment_unknown`, `cleanup_pending`, `build_failed` and `cancelled`. The outcome's publication/cleanup fields remain authoritative when a primary error and secondary cleanup error coexist; a cleanup code never erases the original cause. Transport adapters preserve existing framing/status mapping rather than defining different meanings. Unknown future format and unsupported v1/Direct migration return `unsupported_storage_format` with non-destructive guidance.

CLI success requires committed publication and complete cleanup. Non-cancellation failure, unknown publication or incomplete post-commit cleanup is nonzero. Pre-commit cancellation returns cancellation after teardown/cleanup; cleanup errors remain explicit alongside it. Cancellation arriving after the commit cannot undo the selected snapshot: finish cleanup and report actual publication certainty. No caller automatically retries an ambiguous or committed operation.

## 4. S02 — Managed storage v3 and exclusive admission

| Requirement | Contract |
| --- | --- |
| S02-R01 | Configuration schema v4 selects managed storage format v3. Retain current compact manifest v5 and artifact-key/schema semantics; any required incompatibility needs a revised approved contract. New readers reject unknown versions instead of interpreting them as v2. |
| S02-R02 | Each store has two fixed slots, `a` and `b`, and a durable active selector binding slot plus a unique, never-reused snapshot identity. Slot labels are not snapshot identities. |
| S02-R03 | A populated slot owns its database, search files, source manifest, bundle metadata/readiness and parsing cache. Captured sources, staging and candidate-only diagnostics belong to the inactive slot's disposable build area. |
| S02-R04 | There is one cross-process target admission path for every supported caller. Reserve the target rebuild, close new read admission, drain admitted native accesses cancellably, then hold exclusive target-store ownership through construction, publication and cleanup. Preserve family producer serialization. |
| S02-R05 | New target reads and competing builds return truthful busy/rebuild-in-progress results without a waiting request queue. Cancellation during drain changes no snapshot. Existing native reads complete under their use guard. Health/control status remains available without opening the database. |
| S02-R06 | Readers hold target-store protection for their complete database access and validate the selected slot/identity. The new format has no generation-specific reader-retirement registry. Other targets retain their own identity and availability subject to existing shared execution admission. |
| S02-R07 | Reuse compatible raw partitions by copying and validating them into replacement-owned cache. No symlink/hardlink/shared mutable history is required for reuse. Missing/corrupt/incompatible cache rebuilds from captured source; storage-write errors fail normally. The new snapshot remains self-contained after former-slot deletion. |
| S02-R08 | Validate the complete bundle, close/reopen its database read-only and verify existing schema/count/artifact/search requirements before atomically durably selecting it. Delete the former slot and cache synchronously afterward. Healthy idle has one populated slot, the other empty/absent, no build staging, no run/generation collections and no background collector. |

### Shared layout and publication

The managed root contains a version/identity descriptor, stable coordinator/admission/use/writer controls, `active.json`, `slots/a`, `slots/b`, and minimal current-operation/containment status outside the slots. No active pointer means no published snapshot; choose `a` for the first candidate. A subsequent candidate uses the inactive slot only after prior operation state and any old contents are safely reconciled. These two slots are architecture, not a byte quota or age policy.

`active.json` schema v3 contains `schema_version`, `slot`, `snapshot_id` and `metadata_sha256`. The hash binds the slot's immutable metadata/readiness membership; metadata records the snapshot identity, source-manifest identity and the bundle validation evidence required by the existing writer. Do not replace existing validation with a hash alone or impose an unrequested complete-database hashing pass. Publication writes a same-directory temporary pointer, synchronizes it, atomically replaces the pointer and synchronizes its parent through the existing supported-platform durability mechanism. Native or synchronization errors propagate. No stronger power-loss/filesystem guarantee is claimed.

Admission and publication belong to Graph Store. CLI/MCP/library adapters do not acquire ad hoc locks or interpret slot paths. All v3 writers/recovery use this order: family producer reservation → target writer reservation → exclusive read-admission barrier → exclusive store-use guard → pointer publication. The family and writer reservations reject a competing producer without queuing. The admitted writer waits cancellably for the brief admission phase and existing readers; no new timeout is invented.

A reader tries a shared admission barrier, obtains its shared store-use guard while still holding that barrier, then releases the admission barrier and retains store-use protection through native access. A writer closes the barrier before draining store-use guards. Readers arriving at a closed barrier return rebuild-in-progress. Because every writer obtains the barrier before exclusive store-use, a reader holding the barrier cannot wait behind a writer that already owns exclusive store-use. Cancellation releases acquired guards in reverse order. Never upgrade a held read guard into a rebuild.

Migration additionally reserves the installation's stable migration control before the family reservation, then obtains existing old-root authority before new-root writer/admission/use guards. No new-root operation reaches back to acquire old-root ownership. Reuse held legacy authority rather than reacquiring its writer lock. Platform aliases and stable control-inode replacement remain ownership concerns, not string comparisons. Status reads use atomic control observations without database access; moving/inconsistent observations are reported as such, not a coherent ready snapshot.

## 5. S03 — Failure, cancellation and cleanup

| Requirement | Contract |
| --- | --- |
| S03-R01 | Retain proven native worker/phase containment. Keep minimal durable current-operation and containment evidence outside disposable slot data, so cleanup cannot erase authority needed to exclude surviving writers. |
| S03-R02 | Normal state progression is reserve/drain → build → validate → commit-started → published → cleanup → idle. One operation record binds target identity, operation identity, previous selector, candidate slot/identity, phase and required containment references. It is not an unbounded run history. |
| S03-R03 | On caught pre-commit error/cancellation, stop the complete owned process tree, verify termination, then synchronously delete replacement/cache/captured-input/staging data before returning. Preserve the old active snapshot. Cancellation does not skip teardown or interrupt deletion into a falsely clean outcome. |
| S03-R04 | Unknown containment, invalid identity, symlink/substitution, inaccessible paths or deletion failure preserve necessary evidence and report primary plus cleanup errors. Do not infer authority from age, PID or config non-reference. Do not admit another candidate while unresolved state occupies the inactive slot. |
| S03-R05 | Startup or a later explicit write may recover owned interrupted work under proper exclusion, but never retries a build or schedules source reconciliation. Read/health requests do not initiate recovery mutations. A crash cannot promise immediate cleanup while no authorized process is running. |
| S03-R06 | The active selector is the publication boundary. Once commit is known, late cancellation/cleanup failure retains the new selected snapshot and exposes cleanup status; no silent rollback or unpublished-failure report. Uncertain durability remains `unknown` until recovery establishes it. |
| S03-R07 | Cleanup is idempotent and confined. Stable controls and minimal truthful last-outcome metadata may remain, but failed bulk build data is not retained as diagnostic history after safe cleanup. No new filesystem-entry quota, timeout, TTL or background reclamation budget is introduced for v3. Existing legacy-collector safety/budget contracts remain separate. |

### Recovery and error table

| Observed state | Required recovery/outcome |
| --- | --- |
| Draining/building/validating; old pointer intact | Verify build tree terminal/empty; remove replacement and retain old snapshot. No automatic reconstruction. |
| Commit-started; pointer still names validated old snapshot | Treat candidate as uncommitted; verified teardown and replacement disposal preserve old selection. |
| Commit-started/published; pointer names validated candidate | Complete required durability/operation reconciliation, retain candidate as active and retire former data. Do not replay graph construction. |
| Missing/invalid/conflicting pointer or identity; outcome cannot be established | Preserve both slots/evidence; report recovery-required/unknown and block conflicting writes. Never guess a winner. |
| Teardown unknown | Preserve evidence and protected data; block writes and unsafe reads. Do not signal a persisted PID or remove proof. |
| Processes verified stopped, valid active selection, inactive-slot deletion fails | Report pending cleanup, retain active snapshot and allow its safe reads after admission release; block another candidate until cleanup succeeds. |
| New pointer committed; former-slot deletion fails | Report new active identity plus incomplete cleanup, not clean success. Retain the operation association for idempotent recovery. |
| No initial snapshot and build fails | Dispose safely owned failed data; graph remains not-ready rather than borrowing another graph. |

During synchronous cleanup, cancellation is recorded but does not abandon safety steps. If a filesystem effect itself fails, return the actual incomplete-cleanup outcome instead of looping indefinitely, suppressing the failure or claiming that all bytes were deleted. Future authorized recovery may retry from durable state. The last-good snapshot is preserved, not forcibly served through unresolved publisher/containment uncertainty.

## 6. S04 — Controlled managed-v2 migration

| Requirement | Contract |
| --- | --- |
| S04-R01 | Support migration of the managed-v2 generation/run layout only. Schema-v1/custom Direct state is not interpreted or retired; return fresh-managed-setup guidance while preserving it. |
| S04-R02 | Migration is an explicit CLI/library build operation. Ordinary reads/startup/builds on legacy storage report migration-required without conversion; no read-triggered maintenance, administrative confirmation or migration tool in MCP. |
| S04-R03 | Require controlled shutdown/upgrade/quiescence of old coordinators, refreshers, standalone writers and helpers plus applicable verifiable ownership/containment evidence. Human confirmation does not override live/contradictory state and does not authorize PID signalling. |
| S04-R04 | Create one separate positively owned v3 root and a durable migration association. Default target is the installation's `storage-v3` root; a collision/foreign identity is an error, not overwrite authority. Store the actual old/new root identities and source/config binding. Resume the association rather than allocating repeated roots. |
| S04-R05 | Build the complete replacement and validate its bundle before cutover. Preserve old graph/config/selector bytes and recovery evidence until the new configuration selector is durably switched. |
| S04-R06 | Cutover atomically replaces the same-directory configuration selector to schema v4/new v3 root only after new snapshot durability. A stable installation-scoped migration association outside both roots records cutover intent/identity for crash reconciliation. Never publish mixed old/new configuration or guess by path existence. |
| S04-R07 | Pre-cutover failure/cancellation performs verified teardown and synchronous failed-new-root disposal, leaving the old installation intact. The new runtime may still require migration. Restarting the old version is an explicit operator action after teardown; no automatic restart or downgrade is implied. |
| S04-R08 | After durable cutover, synchronously retire only positively proven-owned obsolete managed state. Preserve unknown/foreign entries and all custom Direct/external destinations; report incomplete migration and retain resumable evidence. Migration/recovery is idempotent and creates no second normal production writer. |

The operator sequence is: stop the old producer cohort; establish the declared quiescence precondition; invoke the explicit migration build; inspect actual publication/cleanup outcome; start only the selected compatible runtime. D00 does not execute this sequence on the current installation. Public wording distinguishes accepted operator confirmation, observed ownership, completed cutover and completed cleanup.

Controlled migration retains relevant legacy ownership, phase/publication and collector guards. It does not bypass the COD-113 empty-run/legacy-recovery constraints or use unconditional recursive deletion to escape them. A legacy obstruction may leave a validated new root plus pending old-state retirement; the response must state actual cutover certainty. Source absence, malformed legacy associations or unknown containment may keep migration incomplete.

## 7. S05 — Compatibility, authority and delivery

| Requirement | Contract |
| --- | --- |
| S05-R01 | Deliver as a breaking release. Keep one canonical transport-neutral lifecycle; no public old/new compatibility writer or implicit fallback. Retired inputs fail before effects. |
| S05-R02 | Preserve worktree explicit selection, identity, isolated state, registration/retaining detach, durable intent and family serialization. Amend live refresh, implicit initial build and target-read availability explicitly against COD-111's approved revision. No global/session selected tree or invalid-selection fallback. |
| S05-R03 | Hooks remain trusted-root, advisory, bounded and fail-open for busy/not-ready/unavailable graphs. They never activate/build. Optional wiki graph composition remains through the public read API with available/degraded/disabled behavior; curated wiki data/storage/publication is unchanged. |
| S05-R04 | Reconcile every supported caller before removing old paths/dependencies. Temporary internal development bridges need an owner and D07 removal condition; none survive integrated handoff as an alternate publisher. Update help, schemas/catalogs, library examples, installers, hooks, agent instructions and breaking/migration docs together. |
| S05-R05 | Bind exact approved specs, genuine architecture intent/briefs, file ownership and tested/reviewed source to each issue. Preserve other work, approval histories and closure owners. Assignment/PR/merge is not approval, qualification or aggregate closure. |
| S05-R06 | Independently verify/review the final relevant snapshots and qualified packaged behavior on supported current stable macOS ARM64 and Microsoft-supported Windows 11 x86_64 desktop. Other issues' macOS-only or Windows-emulation exceptions do not transfer. Resolve confirmed P0–P3; retain genuine evidence and limitations. |

### Linked-work impact and disposition

| Existing work | Conflict / retained contract | Disposition before dependent implementation |
| --- | --- | --- |
| COD-111, approved spec 0.3 at `d05df72076b5a2016b95a70dd0cbe55bd9ac9f33` | D04/R09 preserves reads during builds; R10 and related lifecycle/settings criteria require live refresh | Proposed narrow amendment adopts S01/S02/S05 for this breaking product while retaining routing/identity/isolation/intent meanings. Preserve the existing implementation branch, approvals and qualification evidence; do not rewrite the old approved file or claim its closure. Current checkout's 0.1 is not approval authority. |
| COD-113 and COD-124 | Proven legacy staged-run recovery limits and historical post-reader-release failure | Preserve their fixtures, evidence, branches and approvals. Carry applicable obstructions into S04 and new lifecycle failure tests; do not infer common causes or close those issues. |
| COD-114; COD-116/119/120/121 | Fault readiness, persisted equivalence, transport parity and optional wiki composition | Reuse real behavioral oracles, adapting watcher-specific expectations only under this approved delta. Test counts/configuration are not execution. |
| COD-68/COD-76, CBG-REUSE-01 proposal | Existing proposal preserves Direct, leases/concurrent reads and live-refresh compatibility | This product proposal is not an engine-extraction authorization or silent rewrite of that project. Record the incompatible assumptions and require its owner to reconcile its baseline/contracts before dependent extraction. Retain consumer-neutral API goals and unrelated package boundaries. |
| CBG-COPY-01 / resource amendment | Native isolation, fail-closed mutation, no uncertain COPY replay; removed workload ceilings | Preserve these contracts. Do not restore old budget-enforcement claims or change backend/native versions as incidental work. |
| COD-19 / CBG-C4-01 | Exact-scope native-authority bootstrap | Unrelated runtime/storage work remains Scryer-governed. Preserve all C4 artifacts, intent, trust/approval records and closure ownership. |

Pending Scryer claims owned by other changes are not reassigned by this document. A candidate replacement that conflicts with an already-owned pending claim remains an explicit architecture-readiness gate until resolved by its owner/user; no implemented verdict or signed baseline is inferred. The captured [unsigned architecture proposal](architecture-proposal.json) records only this preparation change's exact 17 entries; it is evidence of the draft, not a new authority. Its detailed execution record is Scryer `chg-e1cw51`. The existing 25 pending entries were read back unchanged after authoring this proposal.

## 8. Acceptance and planned verification

All product methods below are **planned**, not passing execution. The requirement ranges are inclusive; no requirement or original acceptance identity is renumbered.

| Criterion | Requirements | Given / When / Then and evidence method |
| --- | --- | --- |
| S01-A01 | S01-R01–R03, R06–R08; S05-R01/R03/R04 | Given configured/empty installations and CLI/library/MCP/hooks, when setup, activation, startup, reads or edits occur, then observe zero build/watch/queued-retry starts; explicit CLI/library builds alone publish equivalent complete graphs. Test actual public boundaries, retired inputs and no side effects. |
| S01-A02 | S01-R04/R05; S05-R02/R03 | Given captured source and a later edit, when construction/reads finish, then graph facts match captured inputs and metadata reports that snapshot with unchecked current freshness. No follow-up build/source scan/fallback. Use deterministic capture barriers and persisted facts. |
| S02-A01 | S02-R04–R06; S05-R02 | Given admitted readers and competing producers, when a target build starts, then close new admission, drain readers before construction, reject new requests without queuing and retain each selected worktree identity. Test real cross-process locks, cancellation and family admission on supported platforms. |
| S02-A02 | S02-R01–R03/R07/R08; S01-R03 | Given compatible, missing, corrupt and incompatible parse caches, when complete builds publish/reopen, then reuse/rebuild correctly, preserve deterministic persisted graph/search parity with a clean control, and retire old cache without dependency. Include a→b→a identity reuse, deletes/renames/shared entities and unsupported/corrupt formats. |
| S03-A01 | S03-R01–R04/R07 | Given capture/cache/staging/search/database/validation/pre-commit failures or cancellation, when teardown completes, then retain old graph and synchronously remove failed bulk data before response. Inject real faults, including permission/ENOSPC and cleanup failure; assert separate primary/cleanup outcomes. |
| S03-A02 | S03-R01/R02/R04–R06 | Given native owner/worker/guardian loss around durable boundaries, when recovery repeats, then select only a validated committed bundle, retain unknown proof, block conflicting writers and avoid repeated candidate allocation. Include post-commit cleanup failure and malformed/conflicting pointer identities; use deterministic process barriers, not PID/age or launch-ACK oracles. |
| S04-A01 | S04-R01–R08 | Given owned managed-v2, unsupported schema-v1/Direct, busy/unknown/foreign and interrupted-cutover fixtures, when explicit migration/recovery runs, then preserve pre-cutover old bytes, switch only to validated v3, retire only proven-owned state and resume without duplicate roots. All unsupported/custom external state stays untouched. No live migration is evidence. |
| S05-A01 | S05-R01–R06; S02-R08; S03-R04/R07 | Given the integrated final snapshot, when 20 mixed successful/failed rebuilds and packaged caller/worktree/wiki tests execute, then healthy terminal recovery leaves one populated slot/no staging/history, unresolved faults prevent new candidates, results match clean controls and no retired production path remains reachable. Record actual native/packaged qualification, independent review and limitations. |

The original preparation criteria remain distinct:

| Criterion | Preparation outcome |
| --- | --- |
| D00-A01 | Complete traced S01–S05 proposal preserving selected directions and original acceptance identities. |
| D00-A02 | Settled shared contracts, verified complete caller/retirement inventory and bounded downstream file ownership. |
| D00-A03 | Explicit resolved linked-work impacts without overwriting approvals or closure owners. |
| D00-A04 | Remotely retrievable exact approved revision and matching signed intent before dependent source changes. |

Preparation checks and incomplete readiness are reported in [evidence.md](evidence.md), never substituted for native acceptance.

Evidence identifies full source/spec revision, actual environment/toolchain/platform, exact command/method, expected/observed facts, genuine reports, relevant implementation/claim anchors and limits. Use required native-test profiles with actual fault cases enabled; a release-only run that omits them is insufficient. Preserve frozen binaries/dependencies outside cleanup roots. No extra performance stage or final-review release-candidate gate is added.

## 9. Execution and handoff

The proposed order remains D00 COD-126 → D01 COD-127 → D02 COD-128 → D03 COD-129 → D04 COD-130 → D05 COD-131 → D06 COD-132 → D07 COD-133 → D08 COD-134. This is not an approved automatic queue. One issue is active per coordinator; one current owner per file, including shared manifests/contracts/lockfiles. Exact source inventories and writable-file briefs must be verified before each implementation begins; unmerged worktree files and unavailable graph evidence are not fabricated bindings.

Detailed execution briefs belong in Scryer on the existing owning components. D01 supplies layout/admission contracts; D02 full materialization/cache/publication; D03 verified teardown/recovery; D04 caller activation/watcher removal; D05 migration; D06 retained integrations; D07 retired-path/docs coherence; D08 independent integrated qualification. Temporary internal bridges are development-only and must be removed by D07; intermediate PRs do not authorize live activation of an incomplete new lifecycle.

Coordinator/authoring uses direct ownership; implementation/independent verification assignments, if dispatched, use the repository settings. Every implemented PR requires a fresh prompt-only read-only Final Reviewer using `adversarial-review-engineer`, default agent, no conversation replay, GPT-6.1 Sol/xhigh under the user's assignment table. This explicit table overrides the referenced skill's Max default. Confirmed P0–P3 require correction, affected checks and focused final-snapshot review. No self-review or invented test verdict.

After the proposal is reviewed and remotely available, obtain human validation of this exact spec and its architecture change/snapshot. Only then write a separate approval record referencing their actual immutable identities and decision; never embed a file's own containing SHA or advance old approvals silently. Link real bindings on the dependent issues without unblocking/starting them implicitly. A child normally ends at its authorized PR, followed by a fresh coordinator chat for the next issue. Merge/release/live migration/restart and aggregate closure remain separate authority.
