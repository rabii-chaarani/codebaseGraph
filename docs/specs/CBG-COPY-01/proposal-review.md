# CBG-COPY-01 — Independent adversarial proposal review

## Recommendation: proceed with specified conditions — PLAN ONLY

Scope A is a supportable minimum correctness fix. Remove same-candidate mutation replay, retain existing ownership/publication boundaries, and use the already implemented bounded sequential-file path. **Before freezing the approval candidate, resolve APR-001 and APR-002 below and incorporate the supplied APR-003 caveat.** These are narrow contract/acceptance changes, not grounds for requiring Scope B, an engine upgrade, a streaming-plan framework, or a full production integrity validator.

This review is not user approval, implementation permission, executed qualification, or rollout approval. No plan-level architectural blocker was established. The repository owner must still choose the scope and clean-main baseline and approve the exact committed specification/full SHA. Scope B remains excluded.

## Reviewed decision and evidence boundary

- Frozen proposal: `spec-r0.1.md`, revision 0.1, SHA-256 **`54d51d60096d1038485f3d335fc25911bac4d092afbfaee034fff3e2697c64a2`**. Canonical `docs/specs/CBG-COPY-01/spec.md` matched that hash during review.
- Requested commitment: approval of the Scope A delivery plan, including all-mutating-phase fail-closed behavior, one configured request pool, and final JSON/CSV file bounds. Owner: repository maintainer/user. No approval record or implementation date is supplied.
- Source baseline: supplied HEAD `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`. The coordinator reports unchanged writer/staging/run/resource files against local `origin/main` `ca115690b226384164cf364458a0e3255b8908d3`. I additionally read that local main's phase file with read-only `git show`; its lifecycle differs, and both narrow phase concerns below remain present there. Then-current implementation main is deliberately not selected.
- Supporting artifacts read: `context.json`, `authority-and-contracts.json`, `loader-analysis.md`, `publication-analysis.md`, `proposed-review-amendment.md`, and the additive portable `docs/specs/CBG-COPY-01/investigation-evidence.md`. The latter two are **not** part of the frozen proposal hash.
- Required review skill and technical-feasibility, architecture/integration, operating-model, and staged-risk lenses were applied, including quantitative checks. Reviewer assignment is the required fresh/default GPT-6.1 Sol / Max configuration; no substitute reviewer was used.

## Findings

### APR-001 — State how native “already exists” errors fit the all-mutation failure rule

**Priority:** Material; resolve before approval freeze. **Evidence type:** Established coverage/contract gap; safety consequence remains conditional, not an observed engine failure.

**Proposal claim:** §3 Scope A.1 (line 46), R1 (line 76), §5 step 3 (line 93): any schema/hash/post-COPY/checkpoint error stops the candidate. A1 (line 103) injects an error at the phase-runner boundary.

**Verified evidence:** `src/db_writer/write.rs:55–67,376–393` and `src/db_writer/phase.rs:213–216,254–260,305–322` pass schema/post-COPY SQL through `query_ignoring_existing`. That helper converts *any* native error containing `already exists`, `exists already`, or `already installed` into `Ok`. Local main preserves this at `phase.rs:457,502,563–581`. Consequently, merely deleting the retry loop can still let later statements/checkpoints/phases run after such a native `Err`; a runner-level injected error does not exercise this conversion.

**Strongest counterevidence:** Idempotent catalog/extension setup is an existing intentional path. This review did not demonstrate a real mutating fault carrying those strings or show that an ordinary already-present catalog object is unsafe. Official v0.19.0 `install_extension.cpp:23–29` also reports the ordinary already-installed case as a successful output message, not necessarily an exception. Therefore do not label every harmless no-op as corruption or claim this caused the incident.

**Remedy / resolution evidence:** Explicitly say whether Scope A removes/narrows this blanket error-to-success mapping or retains a precisely justified no-op exception. Successful idempotent SQL/result messages can remain successful. Do not treat a raw mutating error as proven harmless solely because a substring matches. Add a small query-result-level regression for schema/post-COPY error propagation in both isolated and fallback paths, plus the intended successful idempotent setup case. No general SQL parser or full-graph runtime validator is required. If an error exception changes the selected behavior, obtain the owner's decision rather than silently weakening R1.

### APR-002 — Bound diagnostic ingestion, not only the returned stderr tail

**Priority:** Material; resolve before approval freeze. **Evidence type:** Established implementation gap and incomplete planned resource oracle; no stderr-induced OOM was executed or observed.

**Proposal claim:** §2 diagnostic evidence row (line 34), Scope A.4 (line 49), R6 (line 81), §5 step 3, and A8 (line 110) promise bounded original context under the existing child-error budget.

**Verified evidence:** Old `phase.rs:135` calls `fs::read` on the complete stderr file; only `bounded_child_error` at `340–343` trims the last 64 KiB. Local main still does complete reads at `phase.rs:210,281`, then trims at `598–601`. These reads occur after child supervision finishes, including successful exits. Thus a short final error string does not imply bounded diagnostic allocation. With sufficiently long native stderr, ingestion can exhaust parent memory or obscure the very failure Scope A intends to preserve. A8 can pass a returned-length assertion while missing this mechanism.

**Strongest counterevidence:** The displayed native tail is already bounded; normal logs may be small. The proposal adds no indefinite raw-log retention, and main's containment-controlled cleanup must remain intact. This finding does not require a new log-retention policy or claim a current incident exploited stderr size.

**Remedy / resolution evidence:** Require a bounded file-tail/window read using the existing budget before allocating the diagnostic, on both relevant main execution routes. Bound the assembled returned context as well; reserve room for parent-known phase/target/pool metadata rather than allowing added prefixes to defeat the budget. Extend A8 to assert bounded ingestion and preserved primary/secondary ordering for oversized stderr, not just short displayed output. Keep native log retention and deletion under the existing owner/containment contract. A small helper is sufficient; no logging subsystem or increased constant is needed.

### APR-003 — Incorporate the supplied COPY-control-list caveat

**Priority:** Improvement; apply before final handoff. **Evidence type:** Established scaling fact; resource failure remains conditional.

**Proposal locator:** Scope A.3 (line 48), R3 (line 78), and §7 chunk-overhead risk (line 121). Supplied amendment: `proposed-review-amendment.md`.

**Verified evidence:** `accumulator.rs:695,798,825,942` retains one string per output file; `400–406` joins those lists; `staging_writer/writer.rs:30` clones the list. File/COPY count therefore grows with output volume divided by chunk size. The new file bound is not a cardinality-independent bound on Rust/control state.

**Counterevidence and remedy:** The frozen draft already disclaims native-memory fit, keeps resource failure terminal, and anticipates more child/checkpoint overhead. There is no measured control-list blocker. Apply the supplied amendment and include statement/file count and retained/control RSS in the existing resource qualification. Keep the existing list design unless evidence demonstrates a blocker and the user approves wider scope. For R3's budget evidence, reuse existing Graph Writer `node-81` / `resp-273` (“Stage graph rows within the configured memory budget”), returned by scoped Scryer orientation; no invented claim is needed.

## Owner choices and execution gates — not defects or executed passes

| Item | What remains to decide/prove | Consequence |
| --- | --- | --- |
| Availability policy | User explicitly accepts one attempt for **all** mutating phases and fallback writes. An immediate build may fail where old adaptive execution succeeded. Existing pending-refresh scheduling remains unchanged; a later ordinary invocation is not Scope B. | Required scope decision before implementation. No evidence requires automatic candidate reconstruction now. |
| Pool policy | Select one configured pool from the start, or the explicitly described 256 MiB-start fail-closed alternative. A failed resource qualification must return to the owner; it must not silently raise limits or implement Scope B. | Required behavior choice, not an inferred approval. |
| Baseline and ownership | Choose clean then-current main, preserve its lifecycle fixes, reread source anchors and relevant Scryer drift. Distinguish Managed generation identity from Direct's recreated candidate at its established destination-scoped spelling; “fresh” need not mean a new Direct filename. | Old-checkout source does not establish current teardown safety. Unknown containment blocks destructive cleanup and new mutation. No loader-level arbitrary-path reset. |
| Actual isolated fault path | A2 must reach an authenticated fixture barrier **after real scratch mutation/checkpoint**, then lose/fail acknowledgement. Record launched helper identity, mutation evidence, one invocation, no later mutation/publication, and prior-active preservation. | A pre-open readiness failure or in-process fallback does not pass this criterion. A deterministic engine disk-full reproduction is not required. |
| Reopen correctness | A3/A9 must reopen in a separate process and compare the expected fixture's complete ID set, genuine PK-index lookups, exact connector endpoints, and cross-chunk search results. Establish that the selected query really uses the intended lookup path for the pinned native version. | Counts alone, or query syntax without a valid lookup oracle, do not establish the required regression proof. This remains a targeted fixture, not a production full-ID scan. |
| RSS and supported workload | Run at least the existing representative successful/default profile under unchanged limits, as well as A7's deliberate budget-abort case. Record effective pool, parent/child high-water, statement/file count, and phase overhead. No invented performance SLA is needed. | “Correctly aborts over budget” alone does not establish that normal builds remain usable. Results are pending execution, not a reason to reject a bounded implementation plan now. |
| Approval and qualification | Resolve findings, freeze/commit spec, record real user decision/full SHA and Scryer sign-off; synchronize Linear when dispatch becomes available. Execute normal affected/native/platform checks and independent final review. | Drafts, this recommendation, attached tests, local passes, and merges do not replace these distinct gates. |

### RSS arithmetic and counterevidence

Default configuration is worker **768 MiB**, Rust **384 MiB**, spill **32 MiB** (`src/api/context.rs:251–253`). The request calculator selects `min(768−384,384) = 384 MiB` (`protocol.rs:169–190`). In the inherited phase branch the 16 MiB supervisor headroom makes its threshold **752 MiB** (`phase.rs:104–106`; local main `171–173`). If the parent actually retained 384 MiB and native pages used another 384 MiB, their 768 MiB sum would exceed that threshold by 16 MiB before other overhead. The guarded main branch instead passes the existing worker ceiling to the guardian (`main phase.rs:238–247`); qualify the actual selected path.

**That calculation does not prove the minimum fix infeasible or imply an instant 384 MiB RSS allocation.** Rust's configured allowance is not measured retained RSS. The writer releases unused allocator pages before the first child (`write.rs:91–94`). Official **v0.19.0** `BufferManager` creates VM regions and tracks claimed memory (`buffer_manager.cpp:77–85,336–368`); `VMRegion` uses `MEM_RESERVE` on Windows or anonymous `mmap` on Unix (`vm_region.cpp:30–76`), with physical pages claimed as used. A higher configured pool can change resident usage and metadata, but is not itself proof of a fully resident pool at startup. The draft's warning and stop condition are appropriate; measure rather than declaring either safety or infeasibility from configured ceilings. Sampled RSS enforcement is not a mathematical instantaneous-peak bound.

## Supported strengths and rejected challenges

- The strongest causal case survives: a reported mutation/checkpoint failure does not establish rollback. Stopping the complete candidate and returning to its storage owner avoids replay without introducing a new retry state machine. The existing owner already separates pipeline failure from publication (`materialization.rs:311–319,482–503,903–929`).
- The early hash-index step already exists after a successful first chunk, in both writer paths (`write.rs:103–157,248–320`). This is not a missing repeated-table detector. Failing COPY must not be followed by compensating index creation.
- Equal record/output spill limits reuse existing exact framing/header checks (`accumulator.rs:721–755,851–899`). Sequential files avoid depending on the evaluated JSON file-list behavior. The error for a single framed row that cannot fit is explicit, not silent truncation.
- Search is not a single-file blocker: independently read `search_index/build.rs:97–101,197–235`; it enumerates the base JSON file and contiguous suffixed chunks. Preserve all-chunks search parity in A9; no reader rewrite is presently justified.
- The fallback fresh-path check generally prevents a second native load after candidate creation (`write.rs:170–176`), so its retry wrapper must not be described as proven duplicate COPY execution. It can replace the original failure with “candidate database path already exists”; removing write-operation replay still has a sound purpose.
- Scope B correctly treats teardown, candidate/sidecar rebinding, immutable staged inputs and publication as owner-level concerns. Existing publication recovery is not a promise to reconstruct failed builds. The draft correctly avoids conflating Direct Prepared recovery with prepublication abandonment.
- PK/connector regressions are warranted. A new full runtime integrity validator is not required by the verified safety mechanism, and this review does not request one.

## Research performed, limits, and reusable knowledge

Read repository `AGENTS.md`, the planning/review skills and applicable lenses; discovered k-wiki bundles, recalled the existing uncertain-COPY memory, retrieved storage lifecycle context, and used scoped Scryer rules/orientation. Semantic graph discovery and explicit context preceded repository source reads. Linear was confirmed pending and had no dispatchable delivery tools; no tracking state was changed. No broad architectural-model dump was used.

On **2026-10-26**, inspected primary upstream records [issue #1406](https://github.com/LadybugDB/ladybug/issues/1406), [PR #1204](https://github.com/LadybugDB/ladybug/pull/1204), and [PR #1361](https://github.com/LadybugDB/ladybug/pull/1361), plus versioned v0.19.0 [buffer manager](https://github.com/LadybugDB/ladybug/blob/v0.19.0/src/storage/buffer_manager/buffer_manager.cpp), [VM region](https://github.com/LadybugDB/ladybug/blob/v0.19.0/src/storage/buffer_manager/vm_region.cpp), and [extension installer](https://github.com/LadybugDB/ladybug/blob/v0.19.0/src/processor/operator/simple/install_extension.cpp). #1406 remains open with an open linked fix; #1361 is unmerged/draft. Neither is a released safety guarantee. PR1204 explicitly identifies bulk COPY row payloads as absent from WAL. These sources support uncertainty and the regression oracle, not a diagnosis of the original 0.19 incident.

Read the preserved official 0.22.0 arm64 observation record and rechecked two raw-file hashes against the supplied portable provenance: `checkpoint-integrity-observations.json` = `3b9ef4730ca17b2501ff3995b7fb25f166b98abc279717dd67c7752d49cd9684`; `failed-checkpoint-confirm.log` = `1a4000b2f68bbf88dfa50de8666adfd4e556ef6efc821336c24b0042931fd1de`. The reopened scan/PK disagreement is prior executed evidence, **not a test performed in this review**. It neither proves 0.19 safe nor proves the original incident's first failure. Portable evidence retention is a useful addition to the final approval snapshot.

No builds, tests, benchmarks, fault probes, database operations, Git mutations, dependency installs, daemon changes, repository edits, model/tracking writes, or constants changes were performed by this reviewer. The report is the only file written. Resource feasibility and actual new implementation behavior remain unexecuted gates. The existing uncertainty memory already captures the reusable lesson; this read-only review warrants no new memory/model entry.
