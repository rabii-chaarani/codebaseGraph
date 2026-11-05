---
description: Five fresh GPT-6.1 Sol Max reviews of DC-S1–DC-S5 revision 0.1, with no confirmed defects and explicit implementation risks and pending decisions.
tags:
- graph-storage
- cleanup
- specifications
- review
- acceptance
timestamp: 2026-10-20
title: Independent reviews of the five disk-cleanup specifications — 2026-10-20
type: review
---
# Independent reviews of the five disk-cleanup specifications — 2026-10-20

## Outcome and review boundary

Five fresh independent reviewers assessed the five steps in [Automatic graph disk cleanup](./graph-disk-cleanup-contract.md), revision 0.1, one reviewer per step. All five returned **no confirmed P0–P3 specification findings** and considered their assigned draft suitable for its scheduled individual user validation. This is not user approval, implementation clearance, proof of a working collector or rollout qualification.

The document was held unchanged throughout review. Its SHA-256 was `6a89ae66f5d089f39740d99a0b74a9b249616638601bd99511f5c925c15c7946`. The step requirements, proposed defaults and pending approval table were not edited. The user requested five new review sessions without the current context; each reviewer was spawned with **`fork_turns: "none"`**, received only a self-contained assignment and saved source references, and did not receive other reviewers' findings.

Every reviewer used **GPT-6.1 Sol at Max effort**, the required adversarial-review skill, and a read-only assignment. The coordinator's Scryer ledger was `chg-gfsr7b`. No architecture claim was changed or marked implemented. Scryer remains the authority for architecture and future execution briefs; this page records review outcomes and recommendations.

| Specification | Independent reviewer | Verdict | Confirmed findings |
| --- | --- | --- | --- |
| DC-S1 — Inventory and retention | `/root/review_cleanup_spec_1` | Suitable for user validation | None |
| DC-S2 — Ownership and recovery authority | `/root/review_cleanup_spec_2` | Suitable for user validation | None |
| DC-S3 — Collection and migration | `/root/review_cleanup_spec_3` | Suitable for user validation after prerequisite decisions | None |
| DC-S4 — Automatic operation and reporting | `/root/review_cleanup_spec_4` | Suitable for its scheduled validation after DC-S3 | None |
| DC-S5 — Qualification and rollout gates | `/root/review_cleanup_spec_5` | Suitable for its scheduled validation after DC-S1–S4 | None |

The review comments below are conditional implementation risks, verification recommendations or decisions already deliberately pending in the specification. They are not scored defects. The reviewers actively checked the shared safeguards before classifying them.

## DC-S1 — Inventory and retention

The reviewer found the read-only boundary explicit in R2, conservative unknown-state handling in R4/R8, and stale-inventory protection in R7. No missing implementation was treated as a specification defect.

Two conditions belong in the future Scryer execution brief and verification:

1. **Use observational resolution.** At source HEAD `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`, normal runtime resolution can trigger managed cleanup (`src/api/context.rs:634`; `src/storage/managed.rs:410`), Direct recovery (`src/storage/direct.rs:107`), and retirement on generation-lease drop (`src/storage/managed.rs:93`). Reusing those paths unchanged would violate R2; the spec does not require their use. Non-creating observation must also avoid side effects during interruption or handle release. Extend A2's before/after checks with recoverable journals, orphan runs and superseded generations, rather than only clean state.
2. **Do not infer full consistency from the active pointer alone.** Build/run/candidate state can change before publication (`src/storage/managed.rs:341`) while the active generation remains constant. R7 already requires moving/partial/busy reporting when coherence cannot be established. Inject artifact/run mutations with a fixed pointer to verify this condition.

The reviewer inspected source at HEAD only; the separately identified main revision was not counted as inspected or installed evidence. Checks were static, with successful graph discovery before source reads and source/hash verification. No tests or fixtures ran.

## DC-S2 — Ownership and recovery authority

The reviewer found destination-specific ownership, reader/run/containment protection, publication recovery, writer-to-state ordering, held-lease reuse and conservative historical-helper treatment coherent. The linked [worker recovery contract](./worker-recovery-regression-contract.md) already separates the relevant ownership boundaries.

Conditions and decisions:

1. **Pin baseline and legacy quiescence.** The exact implementation revision containing the approved recovery work and the procedure proving historical consumers stopped remain explicit DC-S2 decisions. Preserve-and-report is safe if proof is unavailable; it cannot be presented as completed retirement.
2. **Maintain authority through detachment.** The execution brief must define the lifetime of worker/phase/writer authority, including startup before ordinary writer admission. An observation followed by releasing ownership cannot become a later deletion permit. Exercise deterministic reader/run/phase startup between observation and detachment.
3. **Handle physical aliases conservatively.** The Direct destination key derives from database/manifest path spellings at both reviewed refs: `src/storage/layout.rs:295` at HEAD and `src/storage/layout.rs:303–318` at main `512b5dc197adf5590afa5d2f7b44550dfa32d8a2`. One spelling's lock does not automatically exclude every physical alias. Preserve existing journal keys; test aliases and defer ambiguous ownership.
4. **Retain established recovery qualifications.** Preserve managed last-generation readability while respecting blocked ambiguous Direct replay. Apply A3 consistently with already approved automatic retirement of v1 PID-only records; not all legacy metadata is permanently unknown.

The reviewer inspected both supplied source revisions. Repeated graph-MCP busy errors required the permitted repository-local graph CLI with `--no-refresh` for discovery before source inspection. The older checkout's committed Scryer model lacked main's referenced Guardian node; this supports the baseline gate and limits older-model verification. No runtime tests or mutation occurred.

## DC-S3 — Collection and migration

The reviewer found current-reference recomputation, preservation of required backups, durable retirement-before-destruction, bounded detachment, stable lock protection and conservative confinement sufficient to defeat the principal unsafe-cleanup hypotheses.

Conditions and verification refinements:

1. **Bind resumed deletion to the retired incarnation.** If old Direct retirement is interrupted and the same destination is explicitly rebuilt, an old continuation must not delete the fresh files. Durable coherent detachment already prohibits that outcome. Add a scenario that interrupts retirement, rebuilds the pair, resumes old deletion twice and verifies the new bundle stays readable. During DC-S3 validation, settle whether explicit rebuilding ends that destination's prior retirement eligibility.
2. **Reconcile overlapping Direct registrations.** Database and manifest overrides can be selected independently (`src/api/context.rs:518` at HEAD), while coordination identity hashes the pair. Different pairs can share a physical database yet derive different locks. The review did not establish that the future registry accepts overlapping pairs. The existing ambiguous-identity safeguards remain controlling. Detect overlap/aliases or preserve-and-report; verify that one pair's retirement cannot damage another supported pair.
3. **State the permission-repair precondition in convergence evidence.** Read A4 in two stages: persistent denial preserves data and reports pending work; after permissions are restored, retries converge. R8 already permits that safe outcome. This is an acceptance-clarity recommendation.

The reviewer statically inspected artifact enumeration/replacement/validation, Direct identity/leases and managed retirement at HEAD and the separate main comparison. Artifact validation code was identical between the supplied refs, supporting policy feasibility without proving a future collector. No tests, reproductions or temporary files were created.

## DC-S4 — Automatic operation and reporting

The reviewer found trigger coverage, failed/no-op-only convergence, coalescing, writer exclusion, no rebuild loops, truthful pending/unknown output and durable shutdown continuation coherent.

Conditions and user decisions:

1. **Define the cooperative budget's scope.** R3 explicitly proposes a deletion-batch budget while R1/R4 require bounded progressing maintenance. The future brief should cover scanning, enumeration, classification and recoverable continuation as well as unlinking; otherwise foreground work could still be dominated by an unbounded stage. Use controlled-clock fixtures with many entries, an inaccessible subtree and eligible entries late in traversal. The stated soft-budget caveat remains valid.
2. **Separate pure observation from requesting maintenance.** Define which existing reporting paths merely return observations, request work, or wait for a bounded attempt. R2 of DC-S1 remains mutation-free. Check coordinator shutdown during scan/deletion and restart without another source edit. At HEAD, the current coordinator's execution admission and shutdown behavior are relevant (`src/coordinator.rs:174,884`).
3. **Validate representative public output.** Before implementation approval, supply CLI/MCP examples for success, partial observation, primary build failure plus cleanup pending, and retired-Direct rebuild-required results. Define unavailable allocation values, existing `cleanup_pending` scope and per-attempt versus cumulative reclamation estimates. Preserve compatible typed and compact output. The interval, batch defaults, configuration and public schema remain user decisions, not review findings.

Coordinator and API-core source inspected at HEAD had identical Git blobs at the separately pinned main ref. Graph discovery initially encountered busy responses and later succeeded. No tests, reproductions or runtime changes occurred.

## DC-S5 — Qualification and rollout gates

The reviewer found whole-state clean-control comparison, content equivalence, cache reuse, deferred-state protection, fair progress, traceable executed evidence, individual approval and separate deployment gates coherent.

Conditions and decisions:

1. **Freeze the complete matrix and proposed churn workload.** Include more eligible entries than one batch, a persistently blocked entry ahead of them, failure/no-op-only triggers and recovery after contention disappears. This makes the existing fairness promise falsifiable.
2. **Bind native evidence precisely.** Identify the selected source revision, actual target/profile, named test registration and executed reports. The curated [release verification contract](./release-verification.md) supplies native target/profile context; no current platform execution was qualified by this review.
3. **Resolve actual deployment and quiescence.** Repository refs and passing tests do not identify the installed daemon/helper set or prove that old consumers stopped. Compatible installation, coordinated restart and historical quiescence remain rollout gates.

This reviewer inspected the specification, related contracts, Scryer claims and graph context, but no implementation source. No tests, fault injection or runtime operations ran.

## Source, method and limitations

- HEAD verified at dispatch: `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`.
- Separate `origin/main` reference: `512b5dc197adf5590afa5d2f7b44550dfa32d8a2`.
- All five reviewers checked the identical specification hash and reported it unchanged at completion. No revision was assumed to identify an installed binary.
- All five performed static specification reviews. Some inspected scoped source; DC-S5 did not. No runtime tests, new benchmarks, load tests, actual process faults, rebuilds, daemon changes, spec edits or approval changes were performed.
- Busy graph discovery was a tool limitation, not a product finding. Source discovery followed repository rules; the DC-S2 reviewer used the documented fallback.
- No reviewer found new verified durable knowledge requiring a separate memory record. Existing storage/recovery memory already covers the applicable principles; conditional implementation comments were not promoted to established facts.
- Local machine-readable coordinator consolidation: `/private/tmp/codebasegraph-cleanup-spec-reviews-20261020-3ivqt6lr/reports.json`. Dispatch settings are alongside it in `dispatch.json`. These are temporary local evidence, not portable runtime qualification.

## Handoff

Keep the five step-validation gates in the [canonical specification](./graph-disk-cleanup-contract.md). Reviewer suitability does not populate the user-approval column. DC-S1 remains the first user validation; later decisions and proposed defaults are still pending. No specification revision was made as part of these reviews.

Carry the conditional safeguards and refined acceptance cases into the appropriate future Scryer execution briefs and user decisions. If they require a material contract change, revise and revalidate the affected step. The final implementation still requires the repository's proposal/final-review, executed-test, native-qualification and closure gates; these five static specification reviews do not substitute for those later gates.
