---
description: Five individually validated cleanup specifications, with retention rules, failure behavior, requirements, acceptance checks and explicit approval status.
tags:
- acceptance
- cleanup
- contract
- graph-storage
- specifications
title: Automatic graph disk cleanup — five-step specification
type: contract
---
# Automatic graph disk cleanup — five-step specification

## Identity, authority and validation status

Canonical specification for the five steps proposed in the [disk retention review](./graph-disk-retention-review-2026-10-20.md). Specification revision **0.1**, prepared 2026-10-20. Preparation ledger: Scryer `chg-cn9ja4`; it records specification work, not implementation approval. Stable step IDs are **DC-S1** through **DC-S5**, preserving the review's five-step order. Requirement and acceptance IDs below remain stable through revisions.

The wiki owns the user-facing scope, contracts, decisions, acceptance gates and evidence references in this page. Scryer remains the sole architectural authority; it owns changed architecture claims, detailed implementation briefs, prerequisites, file ownership and test assignments. This document maps existing claims and identifies where future Scryer deltas are required. It creates no competing component topology, invented implementation anchors or claims of implemented behavior.

The user requested on 2026-10-20: “for each step of the plan define its specs, validate each one of them with me before implementation.” That sequential validation instruction is preserved as history and was superseded by the explicit all-step implementation approval below. Material changes beyond the approved scope still require user validation. Specification validation is distinct from executed acceptance evidence, supported-platform qualification and deployment.

An unanswered prompt, a preselected answer, approval of another step, or earlier agreement with the overall cleanup direction is not approval of a step. Changes affecting an approved step's behavior, scope, safety or acceptance criteria require fresh validation of that affected step. Routine implementation details remain subject to existing approved contracts and Scryer rules.

| Step | Revision | Status | User decision reference | Implementation/evidence |
| --- | --- | --- | --- | --- |
| DC-S1 — Inventory and retention | 0.1 | Locally implemented and independently reviewed | Explicit all-step instruction, 2026-10-21 | 29 passing inventory tests; platform and aggregate qualification pending |
| DC-S2 — Ownership and recovery authority | 0.1 | Approved scope; public-handle amendment awaiting user decision | Explicit all-step instruction, 2026-10-21 | Guard and epoch foundations reviewed; raw-handle/collector boundaries pending |
| DC-S3 — Collection and migration | 0.1 | Partial foundations under review | Explicit all-step instruction, 2026-10-21 | 35 local tests with journal/budget fixes verified; canonical collection and other retirement categories pending |
| DC-S4 — Automatic operation and reporting | 0.1 | Scheduler primitive reviewed; integration pending | Explicit all-step instruction, 2026-10-21 | Scheduler and DTO/admission foundations reviewed; commit 44e467fa; runtime wiring pending |
| DC-S5 — Qualification and rollout gates | 0.1 | Evidence readiness underway | Explicit all-step instruction, 2026-10-21 | Two external health tests and corrected evidence audit reviewed; whole-state/native acceptance pending |

Closure owner remains the repository maintainer/user. The later implementation task owns aggregate closure and must retain these IDs rather than renumbering them.

## Implementation approval — 2026-10-21

The user explicitly instructed: “start the implementation of all steps. Each spec should be implemented by a sub-agent in fresh session. Each spec should be reviewed by another sub-agent in a fresh session.” This approves implementation of DC-S1–DC-S5 revision 0.1 together and supersedes the earlier queued one-at-a-time validation sequence. It includes the proposed 60-second interval, 100-entry/100 ms cooperative defaults and existing CLI/MCP reporting surfaces. It does not satisfy executed evidence, platform qualification or deployment gates.

The user separately selected **Support controlled migration** for pre-lease legacy state: a one-time controlled shutdown/upgrade/quiescence procedure is supported; until that condition is explicitly established, preserve state and report the reason. No actual daemon shutdown, installation upgrade or live graph deletion is part of this implementation task.

Initial implementation baseline: `512b5dc197adf5590afa5d2f7b44550dfa32d8a2` (main recovery fixes); rebased onto current main `018df35e72447fc3e014e950d1d3c6648bbd4820` before DC-S2, isolated checkout `/private/tmp/codebasegraph-disk-cleanup-7t9af0a6/repo`, branch `codex/automatic-graph-disk-cleanup`. Architecture and detailed execution briefs: Scryer change `chg-6vx5tj`; the approved same-repository recovery model was imported unchanged, preserving identities. The original preparation ledger `chg-726xze` records coordination only. Each step receives a new implementation agent and a separate new reviewer with no inherited conversation context; implementation uses GPT-6.1 Sol Medium, required reviews use GPT-6.1 Sol Max. The pre-implementation proposal review remains a starting gate.

Implementation clarifications within these contracts include whole-operation producer-use leases for low-level/native paths, exact Direct incarnation-bound retirement, incremental validation outside reader-facing state locks, and explicit default-false `confirm_legacy_quiescence` input on the existing health/MCP/CLI surface. Operator confirmation records the controlled-cohort precondition; timestamps/PIDs or ordinary health calls cannot manufacture it. No new standalone command or GUI is introduced.

## Settled scope and shared constraints

The user already selected this retention policy in the review conversation: **keep the current managed graph and its active parsing cache; retire older repository-local Direct graphs once safe ownership checks pass.** A later explicit use of a retired Direct destination will require rebuilding. This decision is reused, not reopened.

The intended result is eventual removal of unnecessary program-owned disk state while preserving graph correctness, cache reuse, active work, recoverability and supported interfaces. Approximately 50 GiB recoverable and 3.5 GiB idle usage were estimates for the observed workload. They are not a fixed quota, an upper bound for future repositories, or a promise to delete unresolved state.

Shared constraints for every step:

- Keep the active database, search sidecars, compact manifest, generation metadata, readiness marker and stable coordination/lock files.
- Preserve active source-cache keys, leased generations, live runs and unresolved publication/worker/phase evidence until their corresponding safe transitions complete.
- Do not use directory age, recorded numeric PIDs, file-name patterns, or configuration non-reference alone as deletion authority.
- Confine cleanup to registered program-owned state. Foreign directories, caller-owned low-level staging, custom external Direct destinations, source files, `target/`, `knowledge/` and generated wiki state are outside the collector's scope.
- Reject path traversal and symlink escapes. An unknown entry or uncertain identity is preserved and explained.
- Preserve existing graph/manifest formats and Direct destination identity unless a later individually validated specification expressly changes them. This proposal does not canonicalize existing destination-derived lock/journal keys.
- Keep graph queries usable during deferred cleanup. Temporary space during materialization remains legitimate.
- Separate configuration approval, local implementation, executed tests, native-platform qualification and deployment. No GUI work is proposed.

The initial review examined `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`. The [worker recovery contract](./worker-recovery-regression-contract.md) identifies merged PR #145 and test/specification work targeting `origin/main` revision `512b5dc197adf5590afa5d2f7b44550dfa32d8a2`. The older checked-out branch does not establish that those fixes exist in its source or installed daemon. DC-S2 must settle the implementation baseline and reuse the existing work; no second containment architecture is authorized here.

## DC-S1 — Inventory and retention, revision 0.1

### Problem and observable outcome

A directory total currently mixes the active graph, useful parsing cache, live build space and historical debris. Produce an inspectable inventory that explains every in-scope entry and estimates potential reclamation without changing stored state.

The inventory is a description of observations. It is not a reusable deletion permit. DC-S2 and DC-S3 must revalidate ownership and reachability at the time of any mutation.

### Scope and requirements

| ID | Specification |
| --- | --- |
| DC-S1-R1 | Observe only the current repository's configured managed root and recognized repository-local historical/Direct state whose association can be established. Do not scan other repositories or arbitrary siblings. |
| DC-S1-R2 | Inventory performs no materialization, publication replay, repair, deletion, configuration update or metadata creation. A missing installation is reported without creating directories or lock files. |
| DC-S1-R3 | Categorize entries as active graph, active parsing cache, leased generation, live build, recovery evidence, artifact temporary/replacement state, historical staging, retired-Direct candidate, coordination metadata or unknown. Each physical file is accounted for once in totals. |
| DC-S1-R4 | Give each entry a disposition: retain, potential reclamation candidate, deferred, or unknown, with a concrete reason. Active-manifest non-reference alone cannot override a live run, lease or recovery reference. |
| DC-S1-R5 | Retain the active graph bundle, active parsing cache and stable locks. Treat recognized old repository-local Direct destinations as retirement candidates under the already selected policy, subject to their own ownership checks. |
| DC-S1-R6 | Report logical bytes, filesystem-reported allocated bytes where available, file/directory counts, measurement time, scope identity, active-generation identity and scan completeness. Unknown size remains unknown rather than being reported as zero. Allocated-byte estimates do not promise an equal increase in free disk space. |
| DC-S1-R7 | If state changes during scanning, report a moving/partial observation or bounded retry; never label inconsistent totals as a coherent exact snapshot. A collector cannot delete using a stale inventory. |
| DC-S1-R8 | Do not follow symlinks. Preserve unrecognized entries. Invalid roots, unreadable metadata, missing/corrupt active pointers, ambiguous Direct mappings and permissions failures produce explicit incomplete/blocked reasons; they do not make all files reclaimable. |

Public CLI/MCP presentation belongs to DC-S4. This step defines the information contract, not a new command or GUI.

### Failure and recovery

Inventory can return a complete observation, a partial/moving observation with per-entry reasons, a busy/unavailable observation, or an invalid/missing installation result. No outcome modifies the filesystem. An interrupted scan may be retried from current state; earlier results have no deletion authority.

### Acceptance and planned verification

| ID | Given / When / Then | Existing Scryer mapping | Verification/evidence |
| --- | --- | --- | --- |
| DC-S1-A1 | Given active artifacts, old staging and a live run, when inventory runs, then every item is attributed once and active/live state is retained or deferred. | Graph Store resp-109, resp-232, resp-233, resp-234; additive inventory claim to be planned if needed | Isolated fixture and independent size/count oracle; planned |
| DC-S1-A2 | Given a missing root or a valid installation, when inventory runs or is interrupted, then the file tree, configuration and active pointer remain unchanged. | Existing coordination/confinement contracts; no invented runtime claim | Before/after tree and metadata comparison; planned |
| DC-S1-A3 | Given a pointer change, inaccessible subtree, symlink or ambiguous Direct pair, when inventory runs, then it exposes incomplete/unknown state without traversing or authorizing removal. | Graph Store resp-109; Runtime Resolver resp-144 | Deterministic mutation/error fixtures; planned |
| DC-S1-A4 | Given two unchanged observations with the same options, when inventory runs twice, then categories, reasons and totals agree apart from observation metadata. | Existing identity and deterministic behavior contracts | Repeat-fixture comparison; planned |

**DC-S1 validation requested:** Approve the read-only inventory and conservative retention behavior above, or identify changes. Recommendation: approve revision 0.1. This approval will not approve DC-S2–DC-S5.

## DC-S2 — Ownership and recovery authority, revision 0.1

### Problem and observable outcome

The program must distinguish obsolete files from files still used by a reader, writer or surviving worker. Before any destructive collection, establish verifiable authority over each destination and preserve recovery evidence whenever authority is uncertain.

### Scope and requirements

| ID | Specification |
| --- | --- |
| DC-S2-R1 | Reuse the approved process-containment and worker-recovery contracts from PR #145 and their qualification work; verify their presence on the exact selected implementation baseline. Do not duplicate the older repair or treat wiki claims as proof of deployed code. |
| DC-S2-R2 | Require the destination's exclusive writer ownership before deciding destructive eligibility. Direct retirement uses that exact Direct destination's ownership protocol; managed ownership alone is insufficient. |
| DC-S2-R3 | Respect generation reader leases, live run leases, worker/phase containment and interrupted-publication journals. Missing or unverifiable evidence beside possible execution state remains unknown. |
| DC-S2-R4 | Recover or resolve interrupted publication through its established protocol before computing authoritative references. Preserve the last committed graph while recovery cannot safely proceed. |
| DC-S2-R5 | Keep a consistent writer-to-state acquisition order. A caller that already owns the writer lease uses that ownership without reacquiring it. Contention defers maintenance instead of bypassing ownership. |
| DC-S2-R6 | Preserve stable lock files/inodes and evidence while authority is unknown. Never kill a process or authorize deletion solely from a numeric PID or elapsed age. |
| DC-S2-R7 | Return explicit proven-idle, in-use or unknown outcomes with destination-specific reasons. Recheck the relevant authority immediately before detaching/deleting state. |
| DC-S2-R8 | Historical helpers that predate current ownership protocols require a verified quiescent migration procedure. Current-lock acquisition by itself cannot authorize deletion against those helpers. If proof is unavailable, preserve state and report what is required. |

### Failure and recovery

Busy ownership defers the attempt. Unknown containment blocks conflicting writes and destructive cleanup according to the existing recovery contract; it does not erase evidence to force progress. Attempts follow the already approved bounded recovery behavior and allow paced retries. The exact procedure for legacy quiescence remains an explicit DC-S2 decision.

### Acceptance and planned verification

| ID | Given / When / Then | Existing Scryer mapping | Verification/evidence |
| --- | --- | --- | --- |
| DC-S2-A1 | Given a paused owned loader, when its outer worker dies and cleanup/writing is requested, then evidence remains until teardown is verified and no conflicting publication occurs. | Worker node-208, approved Guardian claims from PR #145, Graph Store resp-109/232 | Reuse/extend native recovery regressions on the selected baseline; planned |
| DC-S2-A2 | Given an active managed reader or Direct reader/writer, when maintenance tries retirement, then protected files survive and the attempt is deferred. | Graph Store resp-109, resp-233 | Separate-process lease fixture; planned |
| DC-S2-A3 | Given unverifiable evidence or a reused unrelated PID, when maintenance runs, then it preserves unknown state and never signals the unrelated process. | Existing approved worker/Guardian legacy and unknown-containment contracts | Existing native cases plus collector integration; planned |
| DC-S2-A4 | Given a caller already holding writer ownership or a historical helper using old state, when collection is requested, then there is no self-deadlock or unsafe legacy deletion. | Graph Store resp-109 | Ownership-order and legacy-consumer fixtures; planned |

**Decisions for later DC-S2 validation:** Use a current main baseline containing PR #145 and the relevant regressions; identify the verified one-time quiescence procedure for historical state. Recommended fallback is preserve-and-report when that procedure cannot prove safety. These are not approved by DC-S1.

## DC-S3 — Collection and migration, revision 0.1

### Problem and observable outcome

Recover disk occupied by unnecessary program state, including the namespaces missed by current cleanup, without deleting active inputs or losing the ability to recover from interruption.

### Scope and requirements

| ID | Specification |
| --- | --- |
| DC-S3-R1 | Collect unreferenced valid artifact keys only after recomputing current references under DC-S2 authority. Preserve active/live-run/recovery references and any unknown necessary state. |
| DC-S3-R2 | Recover or remove abandoned artifact temporaries and replacement backups. A valid backup of a missing referenced artifact must be reconciled before it can be classified disposable. Invalid/ambiguous backup evidence is preserved with a reason. |
| DC-S3-R3 | Recover abandoned modern workspaces through their established journals and collect them only after the associated execution is proven empty. Continue lease-aware retirement of superseded generations. |
| DC-S3-R4 | Run an idempotent migration for recognized old native-staging files and snapshots once legacy quiescence is proven. Unrecognized content is retained and reported. No age-based grace period is introduced for proven unnecessary state. |
| DC-S3-R5 | Retire approved former repository-local Direct destinations as coherent database/manifest/sidecar/cache bundles. Keep stable ownership files. A later explicit request must receive a clear rebuild-required outcome; explicit rebuilding may create a fresh destination through the normal writer protocol. Exact public wording/schema is validated with DC-S4. |
| DC-S3-R6 | Persist recoverable retirement state before destructive work. An interrupted collection resumes idempotently; it cannot make a partial Direct bundle appear committed or revive partially deleted state as a valid graph. |
| DC-S3-R7 | Keep reader-facing state-lock sections short. Detach only proven-unreferenced state, then perform bounded deletion outside the reader-facing critical section. Before detachment fails, preserve the original state; after durable detachment, retry the recorded deletion. |
| DC-S3-R8 | A filesystem failure records remaining work and preserves the primary operation's result/error. Preserve active graph readability, caller-owned outputs and foreign data. Do not evict referenced active artifacts to satisfy a disk quota. |

### Failure and recovery

An unavailable writer/reader lease defers work. Unknown references, corrupt authority or foreign paths preserve state. Filesystem failures leave explicit retryable retirement work. Cancellation stops at a recoverable boundary; restart resumes safely. Backup reconciliation follows verified checksums and existing artifact/schema validation rather than trusting a filename.

### Acceptance and planned verification

| ID | Given / When / Then | Existing Scryer mapping | Verification/evidence |
| --- | --- | --- | --- |
| DC-S3-A1 | Given obsolete keys and synthetic temporary/replacement residue, when ownership is idle and maintenance completes twice, then unnecessary bytes disappear and active cache reuse survives. | Graph Store resp-232, resp-234; additional collection claims if needed | Artifact interruption/recovery fixtures; planned |
| DC-S3-A2 | Given a missing active artifact with a usable replacement backup, when recovery runs, then usable required state is restored/preserved before backup removal. | Graph Store resp-234 | Replacement-boundary fixtures; planned |
| DC-S3-A3 | Given historical staging and an approved old Direct destination, when migration completes, then both are retired coherently and current managed query results remain unchanged. | Graph Store resp-108/109/232/233; lifecycle node-136; proposed retirement policy needs Scryer mapping | Legacy/Direct fixtures and current graph comparison; planned |
| DC-S3-A4 | Given process death at each retirement boundary or a permissions failure, when maintenance restarts, then it converges without double publication, unsafe deletion or loss of the active graph. | Graph Store resp-108, resp-109, resp-232 | Journal-boundary process fixtures; planned |
| DC-S3-A5 | Given many eligible files and a concurrent graph reader, when recursive deletion runs, then readers can proceed and external sentinels remain unchanged. | Graph Store resp-109 and confinement contract | Deterministic paused-collector/concurrent-reader fixture; planned |

**Decisions for later DC-S3 validation:** Confirm the bounded, recoverable migration scope and behavior when a retired Direct destination is accessed again. Actual live deletion remains outside specification preparation.

## DC-S4 — Automatic operation and reporting, revision 0.1

### Problem and observable outcome

Unnecessary files must be reclaimed without requiring another successful graph build or a manual source edit. Users must be able to distinguish useful disk use from deferred or blocked cleanup.

### Scope and requirements

| ID | Specification |
| --- | --- |
| DC-S4-R1 | Schedule maintenance at coordinator startup, after build success/failure/cancellation, after unchanged refresh, and periodically while the coordinator runs. With no long-lived coordinator, ordinary runtime entries request bounded maintenance through the same protocol. |
| DC-S4-R2 | At most one collector acts on a destination at a time. Coalesce duplicate triggers; do not compete with an active writer or create a graph rebuild because cleanup changed internal files. |
| DC-S4-R3 | Proposed schedule: a 60-second background maintenance interval; event triggers request an immediate eligible attempt. Proposed deletion batch: up to 100 entries or a 100 ms cooperative work budget, then yield. A single filesystem operation may overrun that soft budget; this is not a hard syscall or completion deadline. Exact defaults require DC-S4 validation. |
| DC-S4-R4 | Retry deferred work on later triggers/intervals without requiring source edits. Track and resume progress so repeated bounded attempts eventually visit every eligible entry. Do not busy-loop on blocked or failing paths. |
| DC-S4-R5 | Proposed reporting surface: additive storage-maintenance fields on existing health/materialization reporting, available through existing CLI/MCP operation paths. No new GUI or standalone command is assumed. Existing response fields and error meanings remain compatible unless a specific change is separately approved. |
| DC-S4-R6 | Expose observation time/completeness, logical/allocated bytes by category, potential reclaimable bytes, deferred/unknown bytes, last attempt/outcome, reclaimed allocation estimates, and concrete blocked reasons. Do not report a live build as confirmed garbage or report unknown space as zero. |
| DC-S4-R7 | A cleanup failure must not mislabel an already committed generation as unpublished. Preserve the primary build failure and report cleanup pending separately. Do not silently erase cleanup failures. |
| DC-S4-R8 | New maintenance configuration/response contracts, defaults and compatibility behavior require Scryer planning and explicit DC-S4 validation before implementation. Do not log control capabilities or file contents as disk diagnostics. |

### Failure and recovery

A busy destination waits for a later trigger. A failed deletion remains pending with a reason and continuation state. An invalid scope fails closed for that scope. Inventory freshness and an executing collector are distinct observable states. Maintenance stops cleanly with the coordinator and resumes from durable state on next entry.

### Acceptance and planned verification

| ID | Given / When / Then | Existing Scryer mapping | Verification/evidence |
| --- | --- | --- | --- |
| DC-S4-A1 | Given only failed builds followed by no-op refreshes, when maintenance opportunities occur, then eligible artifacts converge without a successful publication or source edit. | Materialization resp-231/270; Refresh resp-147/268; proposed maintenance trigger claims required | Controlled-clock lifecycle/integration tests; planned |
| DC-S4-A2 | Given repeated simultaneous triggers and an active writer, when the scheduler runs, then attempts coalesce, yield and later resume with no overlapping collector/writer or rebuild loop. | Refresh resp-266/267/268; Graph Store resp-109 | Scheduler and process interleavings; planned |
| DC-S4-A3 | Given a permission failure, unknown containment or live lease, when health is read, then pending work and reason are distinguishable from retained active space; recovery later clears the specific reason. | Refresh resp-269; public-contract node-96; new storage reporting claims required | CLI/MCP contract fixtures; planned |
| DC-S4-A4 | Given a committed generation and a later cleanup failure, when the operation responds, then its committed publication remains truthful and cleanup is separately pending. | Graph Store resp-108; Materialization resp-238 | Fault injection after commit; planned |

**Decisions for later DC-S4 validation:** Automatic triggers; proposed 60-second interval; proposed cooperative batch defaults; existing CLI/MCP reporting versus a separately scoped dedicated command; compatibility semantics. None is approved by DC-S1.

## DC-S5 — Qualification and rollout gates, revision 0.1

### Problem and observable outcome

A cleaner must be proven to reduce unnecessary whole-state disk usage while preserving graph behavior and safe recovery, rather than passing tests that inspect only the database file.

### Scope and requirements

| ID | Specification |
| --- | --- |
| DC-S5-R1 | Trace every accepted requirement and criterion to applicable Scryer claims, real implementation anchors, a verification method and executed evidence. Do not invent claims or label configured/attached tests as passing executions. |
| DC-S5-R2 | Extend whole-state convergence coverage beyond database size: valid keys, temporary/replacement state, runs, retired generations, tombstones, legacy staging and Direct retirement. Use an isolated clean-control fixture with the same final source state. |
| DC-S5-R3 | Proposed churn qualification: 20 supported source updates plus failures, unchanged refreshes and repeated maintenance. At verified idle, retain one active generation, current cache and necessary coordination files, with no known collectible residue. Blocked foreign/unknown state must be reported and excluded from a successful clean-state claim. |
| DC-S5-R4 | Prove graph/search/context equivalence and preservation of unchanged artifact reuse. Compare expected reachable content, not only counts or empty directories. |
| DC-S5-R5 | Exercise process death at temporary-write, replacement, publication, detachment and deletion boundaries; reader/writer contention; cancellation; permissions failures; stale inventory; missing/corrupt authority; symlink/foreign paths; historical helper ownership; and retries without new edits. Faults control only fixture-owned processes. |
| DC-S5-R6 | Use existing native CI platform gates and lifecycle profiles. Local macOS observations, extracted-function checks, default compilation, native Windows qualification and aggregate closure remain separate evidence. New required regressions run normally, without ignore or expected-failure treatment. |
| DC-S5-R7 | Before any implementation, obtain the repository-required fresh GPT-6.1 Sol Max proposal review and resolve decision-blocking findings. Before completion, obtain the required fresh final integrated review, fix every confirmed P0–P3 defect, and reverify affected paths. This specification turn does not claim either implementation gate is complete. |
| DC-S5-R8 | Record exact source revision, binary/environment identity, commands, JUnit evidence, outcomes and limitations. Complete required formatting/build/static/test checks without unresolved compiler warnings, then reconcile Scryer and wiki outcomes. |
| DC-S5-R9 | Rollout requires a compatible verified binary/helper set and truthful inventory before collection. Selection of installation/restart/live migration actions is validated separately with the actual operational context. Specification preparation neither updates an old daemon nor deletes this installation's data. |

### Failure and recovery

A failed or incomplete gate leaves qualification pending. Preserve reports and identify precondition/setup failures separately from demonstrated product failures. Do not weaken assertions or conceal failing runs. If a required reviewer/platform is unavailable, report that exact missing gate and keep the affected closure pending under repository rules.

### Acceptance and planned verification

| ID | Given / When / Then | Authority | Verification/evidence |
| --- | --- | --- | --- |
| DC-S5-A1 | Given the same final source workload, when churn and clean-control builds reach verified idle, then the collector leaves only allowed state and equivalent graph behavior. | Existing Graph Store/Materialization contracts and accepted DC-S1–S4 | Whole-state/native integration reports; planned |
| DC-S5-A2 | Given every listed interruption/ownership case, when recovery and repeated maintenance run, then required data and external sentinels survive and eligible work eventually clears. | Approved recovery contracts and accepted DC-S2/S3 | Native lifecycle JUnit reports; planned |
| DC-S5-A3 | Given a claimed completed implementation, when its evidence is audited, then each requirement has a real anchor, executed passing evidence, reviewed snapshot and honest platform limits. | Scryer closure rules; AGENTS review/CI/Git workflow | Traceability/evidence audit and independent reviewer verification; planned |
| DC-S5-A4 | Given a candidate rollout against this repository, when inventory and binary identity are checked, then live migration uses the approved policy and the proven ownership protocol. | Accepted DC-S1–S4 plus actual deployment context | Deployment inventory, identity, before/after query and disk evidence; planned |

**Decisions for later DC-S5 validation:** The complete test matrix and proposed 20-update convergence fixture; required platform qualification and rollout boundaries. Approximately 3.5 GiB is not a pass/fail threshold; the same final workload and approved reachable state are the oracle.

## Handoff and next validation

All five specifications are approved for implementation by the explicit instruction recorded above. Per-step implementation, independent review, executed acceptance evidence, supported-platform qualification and deployment remain separate states. This approval does not claim any implementation already passed its checks. Revisit affected requirements if later work needs material changes to approved scope.

Detailed implementation signatures, file ownership, worker slices and any added architecture claims remain with the future Scryer plan. This contract intentionally does not duplicate execution briefs or invent symbols for documentation-only work. The established Git/PR workflow applies when implementation changes are requested and completed; unrelated existing workspace changes remain preserved.

## Executed local evidence — DC-S1

The read-only inventory foundation is committed as `bb3c716e899c` on current main. Its implementation and tests live in `src/storage/inventory.rs`; Scryer `node-2yhnem` (`resp-a133nw`, `resp-hd92fb`) and `node-hdwgka` have been anchored, supplied with 29 actual test attachments and the executed JUnit report. No destructive maintenance or live installation changes occurred.

A separate fresh GPT-6.1 Sol Max reviewer accepted the frozen inventory SHA-256 `2d773c8ffbfd5e701b5af9cf73dd668477f157a1792e8a333fabe1f1e60b0428` after independently compiling the exact source, running all 29 unit cases and nine boundary probes. Confirmed review findings were corrected and verified: scope binding, active Direct classification, physical aliases/overlapping roles, configured external storage without local state, raw Direct address spelling, and static symlinks hidden by lexical parent traversal. Final reviewed JUnit run `cc7cbef3-35cf-4d41-a6d4-e7743f43036d` has SHA-256 `abba30344a85b32b9bdec3084dae421643a7a430c779aef743b07e6bcae1750d`.

After integration with main, `cargo nextest run --offline --locked --lib --config-file /private/tmp/codebasegraph-disk-cleanup-7t9af0a6/nextest.toml --profile cleanup-s1 -E 'test(storage::inventory::tests)' --test-threads 1` again passed all 29 selected tests (run `467d04e0-f3a8-4d37-ab03-ea8a7216e0e9`). Evidence snapshots are retained under `/private/tmp/codebasegraph-disk-cleanup-7t9af0a6/evidence/s1-fix-round-3`; independent probe evidence is under `/private/tmp/codebasegraph-cleanup-s1-review-r3-048vl1pl`.

Inventory currently reports a partial observation when compatible artifact-producer ownership cannot be established. It does not create locks or cohort metadata. Later ownership integration must supply a validated, noncreating observation path. This is local inventory qualification only: native Windows behavior, hostile concurrent ancestor replacement, large-cache resource behavior and final integrated cleanup remain unqualified. Unused foundation warnings must be resolved through the planned production callers before aggregate completion.

## DC-S2 public native-handle decision — pending

Implementation review identified a contract boundary omitted from revision 0.1: the public Rust `db_writer::open_ladybug_database` and `open_ladybug_database_with_limits` helpers return raw `lbug::Database` handles. They can open both recognized Direct destinations and managed generation files without retaining a program-owned use lease. Native read-only file handles are not a sufficient general retirement exclusion. This is a confirmed missing lifetime proof; actual query damage has not been demonstrated by a runtime test.

The repository maintainer/user has been asked to choose the public compatibility boundary. The recommendation is a guard-owning database wrapper that retains database-use ownership for its full lifetime and supports ordinary borrowed access to the native database. Rust callers with explicit native return types or consuming-type APIs may require updates; CLI/MCP response contracts remain unchanged. A raw-return alternative requires controlled downtime of every raw-handle consumer throughout coherent database detachment, including upgraded library clients. Excluding database bundles from automatic retirement is the other safe fallback; obsolete database space would then remain. Staging/artifact cleanup can proceed independently where its existing ownership proofs hold.

Further feasibility review established that the existing native helpers support clean readonly databases in unwritable parents and do not reject paths merely for being symlinks or hardlinks. The current sibling use-lock implementation would introduce permission and alias restrictions as well as a return-type change. The latest consolidated question replaces both earlier questions: (1) a strictly guarded wrapper, explicitly accepting that some formerly valid readonly/alias opens may fail; (2) a wrapper preserving those behaviors through additional coordination design and qualification, retaining unproven incarnations; or (3) raw returns with controlled all-consumer downtime for database retirement. The strict wrapper is recommended for the automatic-cleanup goal, but remains unapproved. No answer or preselected option is approval. The earlier controlled legacy migration choice only established shutdown/upgrade of old daemons/helpers; it does not establish that upgraded clients holding raw handles have stopped. No public return-type change or dependent database retirement may proceed until this decision is resolved. Scryer `node-2xwzve` retains the ownership claim; detailed execution and affected file ownership remain in Scryer rather than duplicated here. Closure owner remains the repository maintainer/user.


## Executed local evidence — partial DC-S2–DC-S5 foundations

DC-S2's guard foundation is committed as `c0741ce137e09dac309ac72f5a2c2243a92ef070`. A separate fresh GPT-6.1 Sol Max reviewer verified the corrected snapshot SHA-256 `0e2bde1fe2c37b664aa864175c0b95a1f9297f6f0a64094295aed98c44af2dc3`; actual focused/regression runs passed 128 and 127 executions, respectively. JUnit hashes are `6118b539934bbec11a16cacaf410fe10b12aea7d4b9e17a5ef600dc1d7941876` and `ddccd6f40a368bcacd87f7c1c826028dde700841371e4d54fd6c27e45a11be92`. Findings corrected and independently rechecked include existing-root reads blocked by producer ownership, dangling worker evidence, artifact versus destination cohort provenance, Windows link-count checks, and a reproduced Unicode-alias exclusion bypass. Parent-family database guards now conservatively prevent alias bypass while preserving original Direct address keys. This is a guard-foundation checkpoint, not whole DC-S2 acceptance; reference mutation/seal integration, the user decision above and native-platform evidence remain pending.

DC-S3's frozen expanded partial snapshot `ac4f8016de83c8a3cd156d24cd7b2b3d24f2d816fb64b3b45318f207b2464cf8` passed 31 real-library unit executions on Darwin arm64; JUnit SHA-256 `e00f773cf313ef6575f4fdcf79486dc865dd19ee634888b5b4cf3e1d427e0627`. It contains exact-incarnation retirement primitives, cooperative validation and retained traversal foundations. Separate Max review then identified four defects: post-publication cleanup error propagation, torn marker replay, insufficient retained-memory accounting, and housekeeping outside zero/single-entry allowances. The marker and budget fixes are independently verified at snapshot `1e4f8e9063133924204c712634203c6ff9f7aaaebf7fbf056148b725c7756f65`, with 35 actual passing tests (JUnit SHA-256 `8b6a70e681e8fa0ec013cfb0d0dc1deb85064163db66de579cd9e2871f653965`). Progress publication now binds and resumes only its own preparation inode, syncs complete contents before rename, and charges independent unlinks. Arbitrary non-prefix corruption stays unknown; power-loss convergence is unproven. The post-publication artifact-error finding is now independently verified as fixed in the S4 API/protocol snapshot (17 real pipeline/contract executions passed, including Direct/Managed readability and a Direct postcommit cleanup fault). The mixed Managed recovery/cleanup handoff and decoder/resource defects remain pending. No database/generation/run/legacy/Direct retirement is qualified by these primitive tests. Scryer owns the private reference-epoch and bounded transition prerequisites; no stale inventory or advisory parser result authorizes deletion.

DC-S4's scheduler primitive source SHA-256 `480df68088b7ab2e01936c9e07733a8a70b1380eda38a65c929bfebc297ac9bf` passed separate fresh GPT-6.1 Sol Max review with no confirmed P0–P3 findings: 10 actual real-library nextest cases and seven additional independent boundary probes. Independent JUnit SHA-256 `8fa9207d515e7be607d98f1f9e799edc0970899e21bace664d9e971a270bd414` records run `ca7ff1b0-0683-4ec5-8cc1-b671fc13e83f`. Atomic dispatcher integration, real lifecycle triggers, reporting and collector budgets are separate pending checks. The primitive alone does not establish automatic cleanup. The later additive health DTO/default-false confirmation/CLI normalization and real dispatcher-admission slice passed the same independent reviewer with no confirmed P0–P3 findings. Its independent 44-test union includes 32 focused and 16 coordinator cases with four overlapping cases; JUnit SHA-256 `e93dd8463718f7ace48d05c5e38bcee59b8b6a7bdf5a755ff26df0515dcd7846`. Verification used accepted HEAD plus exact frozen S3/S4 prerequisites, not in-flight epoch code. Eleven independent files are committed as `44e467fa9ea1d0a9604a05a41cd34e763bb1c131`; the scheduler module awaits its collector dependency. The additive native materialization-response field and narrow post-publication correction are included in the combined foundation below. Lifecycle wiring, confirmation consumption, compact measurements and the mixed Managed recovery/cleanup handoff remain pending.

DC-S5 has prepared [the acceptance/evidence matrix](../../docs/testing/graph-disk-cleanup-evidence.json), archived actual foundation JUnit reports and an integrity audit. Counts are test executions across possibly overlapping selections, not unique scenarios or integrated acceptance. The full churn/control, fault/restart, graph equivalence, native-platform and final integrated review gates remain pending. Rust 1.82 qualification is additionally blocked by the unchanged baseline lockfile: pinned clap 4.6.1 requires edition 2024/Rust 1.85, and pinned tree-sitter-language 0.1.8 declares Rust 1.90. Initial offline registry-cache failure was diagnosed separately. No dependency downgrade, manifest change or advertised MSRV change is included; current actual local executions use Rust 1.96. The baseline dependency mismatch does not prove the new source fails on 1.82, and it cannot be reported as a passing 1.82 check.


## Remaining integration gates

The private epoch-only producer foundation and the independent S4 DTO/dispatcher slice passed focused fresh Max proposal starting reviews. Bounded preparation and collector sealing remain separate gates: validate the observed epoch before the collector's own invalidation, retain exact transition authority, and release all maintenance-owned guards on pending/error. Repeatedly restarting an oversized idle registry scan does not establish convergence. The current whole-envelope decoder's memory estimate, automatic partial-value destruction and synchronous join do not establish bounded cancellation.

Unreferenced canonical artifacts and recognized unreferenced root temporaries require both destination and entry-origin proof, strict shape and current complete references. Root ownership, a hexadecimal directory name or a plausible envelope header alone does not establish entry origin; foreign, corrupt or unverified legacy content stays unknown. Decoding every graph row is not itself a deletion permit, and avoiding that decode does not waive entry ownership. Usable referenced replacement recovery requires separate validation. A resident cursor or parser stack alone cannot establish progress across repeated transient CLI processes. Durable logical discovery/reference progress must be implemented and verified, or the affected convergence qualification remains pending; no new daemon, silent routing change or reduced acceptance scope has been approved.

The narrow S5 production contract run passed two external tests with zero skips (run `9c9f4e71-b6aa-425a-8ed1-910eb25df0e2`, JUnit SHA-256 `8baac7efb57e9919f20d5ae9c019fa688fbf8a9ffb1b566053bfb02e90abe6c6`). Its fresh independent Max review found evidence-audit and source-provenance gaps, with original execution archives clean. Both narrow findings are independently verified as corrected. A fresh capture records 145 source identities, including the actual parser, unchanged before/build/after execution; binary SHA-256 `eeac63536eaf120f28533f201720cab4ee7769755d6eaafaa81ba3c069092a76`; run `5f3b8e10-9d6f-414b-a4ec-6a910655d899` passed two tests with zero skips. The evidence audit now checks actual named-case outcomes, exact external case/run identity and all outcome counts. Ten audit regressions passed, including skipped-evidence rejection and lossless archival tamper checks. Accepted narrow evidence commits are `9eaa8de` and `a313b4d`; raw probe bytes remain unchanged inside a checked deterministic gzip archive. These checks do not qualify collection or aggregate closure.

A reviewed reusable lesson is recorded in [the Unicode-alias ownership memory](../memory/episodic/database-use-unicode-alias-parent-family-2026-10-21.md): filename normalization/hashing does not reproduce filesystem equivalence, and Direct addressing must remain distinct from physical lifetime coordination.


## Proposed DC-S3/DC-S4 latency amendment — awaiting user validation

**Status: proposed, not approved; revision 0.1 still governs implementation.** A fresh GPT-6.1 Sol Max proposal review found this alternative coherent, subject to memory and admission prerequisites. This does not approve a reduced safety boundary or establish implementation readiness. The maintainer/user owns this decision under the repository rule requiring validation of acceptance-criteria changes.

The revision-0.1 all-work cooperative budget includes discovery, reference preparation, validation, disposal, admission and mutation. Across transient CLI processes, preserving that budget requires durable logical discovery/reference progress and resumable validation; an opaque Serde stack, directory ordinal or work ID is insufficient. A private catalog/index design is under review, not implemented. Legacy bootstrap also needs an explicit compatible protocol: relocating unknown children is not authorized by a recognized prefix name.

The proposed amendment changes **DC-S4-R3/R4**, with corresponding verification for **DC-S3-R7/R8**, **DC-S4-A1/A2** and **DC-S5-A1/A2**. Existing stable identities, retention policy, confinement, ownership, quiescence, recovery and primary-result contracts remain. No public raw-native-handle choice is implied.

| Option | Observable latency | Engineering and qualification consequence |
| --- | --- | --- |
| Keep revision 0.1 | All work cooperates with the shared 100-entry/100-ms attempt; individual filesystem calls can still overrun. | Implement durable discovery/reference progress and resumable parsing/disposal, including a separately reviewed legacy-bootstrap protocol. |
| Proposed mutation budget | Advisory discovery/reference preparation and at most one payload validation/disposal may exceed 100 ms. Authoritative admission and mutations get a fresh 100-entry/100-ms cooperative budget afterward. | Removes strict-latency-driven iterator/parser-stack persistence. Standalone CLI/direct API runtime entries that request maintenance—health, graph query, materialization and refresh paths—may wait for full preparation and one validation before returning/exiting. No invocation, cancellation-completion or exit deadline is promised. |

If selected, the exact proposed contract is:

- Advisory preparation and validation hold **no** destination writer, state, database-family-exclusive, artifact-exclusive or coordinator execution authority. Bound the number of jobs and the combined retained/temporary memory for references, decoded objects, indexes, buffers, results and disposal against the configured memory ceiling. The existing encoded-byte heuristic is not proof of this bound.
- Start a separate 100-entry/100-ms cooperative budget before fresh authoritative admission. Count independent filesystem entries; a directory containing many files is not one deletion entry. Large admission scans cannot be hidden under authority; current phase/containment proof still needs bounded certification or compaction.
- Reacquire actual guards and revalidate current epoch, cohort, configuration, containment and candidate identity. State-lock sections contain fixed-size checks and recoverable transitions; recursive deletion remains outside them. Stop starting mutation work once the allowance expires.
- In a persistent coordinator, health enqueues work and returns cached/current status. Advisory validation and disposal run without retaining a request/execution permit. Only the short mutation phase acquires the real dispatcher permit. Real graph reads and writer admission must remain possible while validation is paused.
- Transient standalone runtime entries listed above may finish a full advisory pass and one validation/disposal; repeated invocations use retained retirement journals and small durable selection metadata. A blocked first candidate must not monopolize every invocation. Longer preparation must not permanently consume every mutation allowance.
- Cancellation stops new work and checks available interruption points, then completes unwinding/disposal. Acknowledging cancellation is distinct from completed shutdown. Disposal or an in-flight filesystem operation can delay completion; workers cannot escape controller shutdown.
- Keep the original active graph/cache, unknown/foreign preservation, legacy operator confirmation, exact-incarnation journals and primary build-result guarantees. CPU and scan-I/O contention remain possible even while graph admission is available. No new daemon or silent routing change is introduced.

Required additional acceptance evidence, if approved: preparation exceeding 100 ms followed by real bounded mutation progress; no entry 101; stale epoch rejection after publication during preparation; real MCP reads/writer admission while validation/disposal is paused; repeated transient invocations converging across several batches with blocked-first candidates; enforced aggregate memory and completed cancellation/drain. Existing interruption, containment, confinement and primary-result regressions remain required. These are planned methods, not executed outcomes.

No dependent implementation may adopt this amendment before the user chooses it and the corresponding Scryer intent/briefs are updated. The independently approved epoch producer hooks, DTO/dispatcher foundation and evidence fixes can proceed under revision 0.1 while this question is pending.


## Reviewed epoch and post-publication corrections

The expanded epoch foundation's original 225-test report passed an independent exact-source rebuild. Review then reproduced an availability regression: optional cleanup admitted through a corrupt epoch control could block valid committed Managed/Direct reads. The corrected frozen owned identity `235548b4195215d9781ed885b424a34884694fa2b2e3e4d940afdcc7ca0d8b0f` passed 248 local tests, including actual runtime-resolution branches, corrupt/read-only/exhausted controls, retained residue, truthful pending status, and strict producer/journal-replay negatives. Independent fix verification passed six focused tests and six separate control-mode probes with a verified rebuilt binary. Producer and ambiguous Direct replay failures remain strict. This is epoch-foundation qualification, not complete DC-S2 or database-retirement authority.

S4's narrow post-publication snapshot preserves S2 hooks and removes authorityless artifact deletion. Only Direct workspace disposal after completed publication/journaling/readback/validation is converted to separate pending diagnostics; publication and validation failures retain their existing errors. The optional native response maintenance field defaults to None when absent. Independent rebuilt execution passed 17 tests, run `ce7bf792-cc80-48fa-ac01-54ce87aba79e`, JUnit SHA-256 `3df4dc85b9039e597ac040cfca0615d7e4e0cbb05b60b2c94ea7d321c66b6216`. The mixed Managed recovery/cleanup error path remains an explicit integration gate; DC-S4-R7 is not fully qualified.

A separate test-provenance issue was caught and corrected: an initial shared-target run reused a different test inventory despite the same selected pass count. That report is preserved but excluded from frozen-snapshot qualification. A forced root-package build from the verified reconstruction restored the expected inventory. Follow [the frozen Rust binary provenance runbook](../memory/procedural/shared-cargo-target-frozen-binary-provenance-2026-10-21.md); source hashes, compilation path, binary identity and test inventory establish different parts of the evidence chain.

The clean combined foundation is committed as `e96893e025eda512b13f2a9ba754adbac45f4a49` against main `018df35e72447fc3e014e950d1d3c6648bbd4820`. All 137 source identities match the tested frozen candidate. Formatting, whitespace and workspace/all-target checks passed; 295 affected tests passed with zero skips in run `7f5466e4-cad5-4aeb-96cb-2390256fb386` (JUnit SHA-256 `3562f1cb18f27b76ecd94f3b71770d2659b91836e24c37f9bebc7c1c2adb8e76`). The all-target check still reports 75 unused foundation warnings. Exact commands, source identities, raw execution archives and limitations are in repository artifact `docs/testing/graph-disk-cleanup-foundation-integration.json`. Unqualified S3 validator, parked-job and catalog prototypes are excluded from shipping source and preserved separately as recovery evidence.

Scryer locally folds the reference-mutation producer responsibility `node-yp5307` / `resp-ejzb4s` with verified implementation anchors and 20 attached executed tests. The combined report has been ingested against the current architecture metadata; 41 responsibilities have actual verdicts. The open change, remaining pending claims, drift scopes and unrelated missing test verdicts are retained. This local fold does not qualify collector seals, raw native handles, automatic scheduling or aggregate closure.

A separate fresh GPT-6.1 Sol Max integrated review of `e96893e` against `018df35e` confirmed one P2 availability defect: ordinary Direct reads required creating or decoding producer-overlap registration. Identical isolated storage probes passed 3/3 on the baseline and 1/3 on the reviewed snapshot. The correction is committed as `077a8d8e1e75dbca553a4d9b0f65412ca9c5def1`. Ordinary reads retain writer, resource and physical-parent-family lifetime guards while leaving missing/corrupt registration untouched; producers and actual journal replay remain strict. This restores the approved reader-availability and unknown-retention contracts without changing architectural scope.

Focused independent verification passed the three original counterexamples, four shipping registration/publication-lifecycle regressions and one valid Prepared-journal probe. The latter proves corrupt registration blocks mutation before watched bytes/identities change, then restores registration only in the fixture and replays the same untouched valid journal successfully. These are storage-admission fixtures with synthetic native bytes; no new native query or platform qualification is implied. A metadata-only correction distinguishes lexical path ordering from path-component ordering; all 137 per-file source hashes matched throughout. The reviewer verified the fix and reported no new confirmed P0–P3 code defects in the reviewed foundations.

Final combined verification against the committed source passed 299 affected test executions with zero failures/errors/JUnit skips, run `bda126c4-1f9e-4980-9545-acf34ec6de22`, JUnit SHA-256 `698d41772c871f1398e5cf926c415a93af41ef65bd73b298220323266a7e761f`. The 196 tracked Rust/manifests and 12 compiled binary identities were unchanged before/after execution. Formatting and workspace/all-target checks passed; 75 unused foundation warnings remain. The initial sandbox-blocked run (285 pass, 14 local-listener permission failures) is preserved separately; the passing rerun used authorized loopback fixtures with no source changes. The final report was ingested into Scryer for 41 actual claim verdicts; unrelated missing verdicts, pending claims and the open change remain. See repository artifact `docs/testing/graph-disk-cleanup-foundation-integration.json`, especially `final_verified_snapshot`, for distinct historical and current evidence.

The two outstanding user decisions—public native handles and the proposed latency amendment—remain unresolved. No automatic collection, legacy migration, live deletion, warning-free final check, native-platform closure or aggregate completion is claimed. The source branch is a reviewed foundation checkpoint and remains unsuitable for release until dependent implementation and qualification gates are satisfied.
