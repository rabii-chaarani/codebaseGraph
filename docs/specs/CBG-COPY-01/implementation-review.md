# CBG-COPY-01 — independent final code review 01

## Outcome: initial snapshot needs corrections; final qualification remains pending

Two confirmed findings apply to the supplied snapshot: **FR-001 (P2)** loses the COPY-query versus explicit-checkpoint boundary when truncating a large diagnostic, and **FR-002 (P3)** fails strict Clippy. During this review the coordinator corrected both. I independently checked those focused source changes: FR-002 also has a passing strict-Clippy rerun; FR-001's new regression has not yet been executed. No other confirmed P0–P3 production defect survived the reviewed source traces. This is not final acceptance, user approval, supported-platform qualification, or permission to merge/deploy.

## Identity, authority and scope

- Reviewer: fresh independent RLM child; coordinator-confirmed actual model **`openai-codex/gpt-6.1-sol`**, **`thinking=max`**, default-equivalent child, bounded task prompt only, no implementing history or forked turns. Read the required `adversarial-review-engineer/SKILL.md`, its engineering lenses, and the original repository-root `AGENTS.md`.
- Worktree: `/private/tmp/codebasegraph-copy153-scope-a`; HEAD `92f3c14a7f61cf01c8e598aea8e6c3c73ccdf274`; implementation is an uncommitted diff against PR #153 baseline `5839055d9279f4475f893a54abeaa542344c1b02`.
- Approved contract: `docs/specs/CBG-COPY-01/spec.md`, revision 0.2 at `c5b62ba77d2f94b71714b5319f3b3fa215983f9f`; independently verified file SHA-256 `1a0ea5f860ce2764e1666bd6345faf9bb24929f35795255772c04380839b4dd2`. Read `approval.md`, including the explicit Scope A / PR #153 decision. The frozen spec's planning language is qualified by that separate approval; it is not a scope reopening.
- Initial source identity: `final-review-snapshot-01.json`, SHA-256 `3f657549d8b974455eb99392c412ae8fea945a1c2fe3f7fe7fa1cbaad611c038`; all seven initial source hashes matched when read.
- Scope: writer/phase one-attempt policy, configured pool, sequential final staging, diagnostics, new subprocess regression oracles, and consequential caller/ownership/search paths. No Scope B replay, native upgrade, ownership redesign, threshold increase, or new public setting was reviewed or authorized.
- Graph-first discovery used the parent's full bounded handoff `review-discovery-gates.json`: write `Module:c1e2a855e3f4de7303ea`, phase `Module:b30b0f66c988e531c46d`, run `Module:e8798576a9ba6ba106cb`, accumulator `Module:d41558c55db44da4a833`, plus explicit change-impact/runtime gates and indexed dependencies. The registered graph is the older original checkout; actual conclusions use the PR worktree source/diff, not its obsolete line ranges. Followed relevant source imports after discovery. Read registered wiki storage/recovery contracts, recalled the verified uncertain-COPY-outcome memory, and fetched Scryer rules/orientation. Their original-checkout state is not a current-worktree fingerprint or passing verdict. Linear remains pending verification.

## Confirmed findings and focused resolution

### FR-001 — P2: truncation erases the operation boundary required for a COPY failure

**Initial locations:** `src/db_writer/phase.rs:635–651,832–835,851–916`; both parent routes call the bounded reader at `248` and `335`.

**Trigger / consequence:** when the native error itself exceeds 64 KiB, a failed COPY query and a failed explicit `CHECKPOINT` can produce the same returned diagnostic. This fails Scope A §3.4 / R6 / A8's requirement to distinguish those operation boundaries; it does not establish database corruption or change the fail-closed outcome.

**Confirmed by complete static trace:** the child prepends `COPY statement ... failed:` or `checkpoint after ... failed:` to the native error. `execute_phase_file` returns that string through `bootstrap.rs:54–62`; `src/bin/codebase-graph.rs:1–4` prints it. The parent reader seeks to only the last 64 KiB, discarding either prefix. `isolated_phase_error` restores `phase=copy`, ordinal, target and limits, but the `LadybugWritePhase::Copy` request cannot identify which internal operation failed. Equal sufficiently long native tails therefore lose the distinction in both guarded and inherited routes. The byte cap and UTF-8 truncation themselves are bounded and correct. The old baseline also lost this boundary: this is an incomplete approved diagnostic fix, not a newly introduced replay defect.

**Minimum remedy / verification:** keep a short stable boundary independently of the native prefix, for example append `operation=COPY-query` or `operation=explicit-CHECKPOINT` after the native cause. Preserve the actual native tail and the existing 64 KiB cap; COPY-query includes possible engine-internal checkpoint uncertainty and must not imply rollback. Exercise the actual error-formatting helpers, bounded reader and assembler with the same oversized UTF-8 native cause, asserting distinct boundaries, preserved metadata, marker and cap.

**Focused source recheck:** coordinator changed actual query/checkpoint mappings to `copy_query_failure` / `checkpoint_failure`, appending exactly those suffixes. New `oversized_native_error_preserves_query_vs_explicit_checkpoint_boundary` runs both helpers through a cursor, bounded reader and assembled error with a >64 KiB cause. At phase SHA-256 `1500b94723ada235301c44f6e9c27ffcddeb3f13f03862de84a54b100ca0be7e`, the original causal loss is corrected by source inspection: the final printed error and both truncators preserve its suffix. **Affected execution/Clippy/final-snapshot verification is still pending.**

### FR-002 — P3: strict workspace Clippy rejects the helper after the test module

**Initial location:** `src/db_writer/write.rs:348–463`.

The new `run_schema_query` follows `#[cfg(test)] mod tests`. `clippy-01.log` records `clippy::items_after_test_module`, fatal under the required `-D warnings`. This is an introduced, bounded build-gate failure, not a database correctness defect.

**Resolution verified:** coordinator moved the identical helper above the test module, without an allow or semantic change. I compared the actual old/new text; the only change is relocation and surrounding whitespace. Current write SHA-256 `09598352b80e42f2c242e992b6377f57eee82d0acd3a0e308ddcf601f04d16c0`. `clippy-02.log` ends successfully for strict workspace Clippy. This resolves FR-002 on that helper-move snapshot; the subsequent FR-001 phase edit still needs its affected checks.

## Safeguards and attempted falsifiers

- **No same-candidate replay:** isolated `run_phase` makes one call with `request.buffer_pool_bytes`; fallback invokes `write_database_once` once. All schema/COPY/index/post-COPY results propagate with `?`; no buffer-pool classifier or mutating retry remains. Native query helpers now accept only actual `Ok`; generic `Err` mapping cannot silently suppress any of the former existing-object substrings. The first error stops scheduling; no candidate deletion/repair was added here.
- **Ordering:** first successful repeated-target COPY/checkpoint precedes its early hash-index/checkpoint. Other node indexes finish before connector COPY; a failing phase returns before the next mutation. This preserves the baseline sequence instead of replacing its detector. In-process and isolated paths use the same early-table logic.
- **Resources:** direct request pool and threads pass through unchanged; production writer retains one native thread and its existing calculator. Both failure routes now seek before reading, consume at most 64 KiB, decode within constant bounded storage, and cap assembled metadata plus UTF-8-safe native tail. Successful exits do not ingest stderr. Typed guardian/inherited RSS error branches and containment barriers are retained. No claim is made that a bounded file caps native index memory or that pool allowance equals resident RSS.
- **Staging:** the production call now passes the same existing spill bytes to both limits. Existing JSON/CSV sinks account for framing/header, reject an oversized framed row with `MemoryBudgetExceeded`, rotate deterministically and produce one COPY per file. The search reader already enumerates contiguous base/suffixed JSON files. Retained/cloned COPY-plan scaling is pre-existing and explicitly requires resource qualification; no evidence supports inventing a new streaming subsystem in this review.
- **Ownership/publication:** actual materialization caller aborts on pipeline error before finalization/publication (`src/api/materialization.rs:316–328`). Storage execution owns cleanup and appends cleanup failures after the primary error (`505–525`); Direct cleanup consults the existing phase containment registry (`1449–1450`). Existing newer lease, workspace, guardian and unknown-containment boundaries remain. Source inspection does not substitute for the pending native ownership matrix.
- **Real subprocess coverage:** `Fixture::build` invokes the normal Cargo-provided `codebase-graph build` executable, not a library fallback. Bootstrap registers the same executable; distinct phase PIDs and the post-success trace prove actual helper execution. The debug fault hook runs only after `execute_phase` returns successfully, including explicit checkpoint and handle drop, before reporting success. The failed trace contains schema then first COPY only. The follow-up build is an independent invocation, not Scope B replay.

## Executed evidence read; remaining evidence limits

I ran **no builds, tests or native probes**; executions below belong to the coordinator and their retained logs. Read-only Git inspection and source/hash checks were performed.

| Evidence | Supported conclusion / limitation |
| --- | --- |
| `focused-unit-02.log` and JUnit | 72/72 pass, including actual duplicate-DDL native result propagation in both helpers, phase stop ordering, bounded reader instrumentation, UTF-8/context cap, framing/oversize/rotation, staging determinism and existing search chunks. These precede the FR-001 fix; the runner schedule is not native fault injection. |
| `xtask-tests.log` | 42/42 pass. Does not qualify native execution or a platform by itself. |
| `native-copy-01.log` | One preservation case passed; the other failed an invalid `HasReturnType` fixture expectation before that oracle was corrected. Retain as a failed execution, not a pass or a proven product regression. |
| `native-copy-02.log` | Corrected suite **2/2 pass** through normal CLI/phase subprocesses. Reopen checks all 1,400 expected Function labels, per-ID PK lookup, scan/PK parity for repeated targets, and genuine pinned-native `PRIMARY_KEY_SCAN_NODE_TABLE` evidence; exact Module–Defines–Function endpoint rows, first/last searchable terms, and observed per-COPY files ≤1 MiB. Managed and Direct real-COPY acknowledgement failures preserve DB/manifest/search hashes and prior content; later independent builds succeed. |
| Native02 resource output | Successful normal configured pool is demonstrated, separately from budget failure: database-write sampled high water `130073800`, parse `23124382`, search `1048544`, staging `2087283` bytes for the 1,400-function fixture. This does not measure a fully resident 384 MiB pool, all control-state costs, per-child phase overhead, or every representative workload. |
| `clippy-01.log` / `clippy-02.log` | Initial strict lint failure; helper-only fix rerun passes. New diagnostic fix requires fresh affected checks. |

**Acceptance evidence not yet complete at this report:**

1. The reviewed native oracle does not yet compare all same-source chunked results with an unchunked clean-control build (A9), independently expected IDs/JSON metadata for every node/edge target (A3), or prove every named repeated edge-node condition solely from its assertions. It does have substantial scan/PK/endpoint evidence; do not relabel it a count-only test. Coordinator plans a clean-control extension; that new source needs focused review and execution.
2. The new two-case suite does not establish the whole explicit-checkpoint/index/cancellation/RSS/secondary-cleanup preservation matrix in A5/A8 or no residual failed-run/extra generation condition in A6. Existing lifecycle/resource/churn cases can supply relevant evidence once traced and executed; their setup/barrier coverage must not be overstated as a failed in-flight COPY. The existing RSS fixture allocates before native database opening and must stay distinct from the normal successful profile.
3. Source passes configured pool directly, but the current new policy test uses only one tiny sentinel pool; A7's below/equal/above-256 request matrix and its measured resource/control-plan/phase evidence require explicit reconciliation. Safe typed budget abort alone does not satisfy normal workload success.
4. Full workspace, doctests, final format/strict Clippy after fixes, native-tests profile, and Windows x64 qualification remain pending. A full run executing an earlier source snapshot cannot silently cover later edits. The ordinary native-tests profile's debug fault coverage is required; release-only test exclusion is not a pass.
5. Scryer worktree evidence reconciliation, durable evidence/handoff, Linear synchronization, and owner acceptance remain with the coordinator. The original-root service cannot truthfully fingerprint this uncommitted worktree. No model/tracking mutation was done by this reviewer.

These are qualification limits/open evidence items, not additional scored product defects. No original incident root cause, stronger native durability guarantee, exhaustive integrity validation, or platform success is inferred.

## Snapshot hashes

Initial snapshot hashes below remain the identity of the main review. The two focused corrections above are separately identified; all other source files still matched their initial hashes at focused recheck.

| File | Initial SHA-256 |
| --- | --- |
| `src/db_writer/write.rs` | `e185526cfd2a31115a53117b9b089734b4d2eab41a02710c18266b8741900ab7` |
| `src/db_writer/phase.rs` | `0220a1a8cd6c301d4ad27f1c800d6ed014a32a111584ceb6d7a28ebc91e8fa13` |
| `src/execution/run.rs` | `0c2adc995840d52503545e7918e76e785ad41705320eb03c36844fb229489752` |
| `src/staging_writer/accumulator.rs` | `13737811c1dd9cb827573bf4ca7233265c393c3dbdc02214ceca9a3814199d4e` |
| `src/staging_writer/tests.rs` | `39360ae193bd29bc6a2e72ab9b188569a952944afa1b0f686224d9a96a8dc942` |
| `src/staging_writer/writer.rs` | `08d8cfbb0ec534f09e5ad7c13e5e90cb1ab397b80b51aae1356cb397431e2a55` |
| `tests/copy_phase_process.rs` | `a17912256764f14a4723e1611f1dd37fe9bc6625f6984ffb2ad2e478f3f8aef9` |

**Baseline comparison / assumptions / side effects:** the baseline-adaptation GREEN verdict remains valid: no new owner, topology, platform, or Scope B premise is needed. The actual integrated implementation exposed the two findings above; this report does not retroactively turn the proposal review into runtime approval. Assumed only the explicitly approved PR baseline and pinned native provenance supplied in `native-inputs.json`; read source and execution rather than treating recalled historical Linux/older-checkout claims as current support. **No constants changed by this reviewer.** No repository source, Git state, dependency, service, model, memory, tracking, or daemon writes were performed; the sole output is this requested report. Reusable knowledge was assessed: existing uncertain-COPY-outcome memory already covers the durable lesson; pending implementation qualification warrants no filler memory.

**Next review gate:** execute the focused diagnostic fix checks, reconcile named acceptance gaps, and provide the exact final source/evidence snapshot for same-context focused verification. Maintain the user's approval and release/platform decisions separately.


## Focused final-source addendum — snapshot 03

After the initial review, I read and independently compared the new clean-control regression and verified every current source hash against the coordinator's snapshot-03 manifest. Only the two already-reviewed production corrections and the test-oracle extension differ from snapshot 01:

- `src/db_writer/write.rs`: `09598352b80e42f2c242e992b6377f57eee82d0acd3a0e308ddcf601f04d16c0` — identical helper relocated above tests; FR-002 resolved as described above.
- `src/db_writer/phase.rs`: `1500b94723ada235301c44f6e9c27ffcddeb3f13f03862de84a54b100ca0be7e` — actual COPY-query/checkpoint format helpers append distinct stable suffixes and the new oversized-tail regression exercises both through the bounded reader/assembler. **FR-001 source correction passes focused independent inspection.** Its execution remains pending, not passing.
- `tests/copy_phase_process.rs`: `ca61337099747e3356c4308bfcba7c6b73f8d2a604bef6808569509465567c3e` — the chunked build's read-only logical snapshot is now compared to a second normal CLI build from the same paths/content, fresh candidate, using the existing 32 MiB spill value. It compares all sorted node labels/IDs/native string representations (intended to include JSON properties), all sorted connector labels/endpoints, and both sampled search result vectors. Original independent 1,400-label/PK/endpoint expectations remain intact. Handles close before the second build; it is an independent fresh-candidate invocation, not replay of a failed phase.

The extension directly addresses the missing same-source control comparison in evidence item 1. It neither weakens an assertion nor changes production behavior. **Its native `CAST(n AS STRING)` binding/representation and successful comparison are unexecuted at this addendum**; schema-wide introspection, every-property semantics, and all intended JSON content must not be claimed merely from reading the query. A binding failure would be a fixture failure to correct and rerun, not evidence that the product loses graph data. Native02's existing 2/2 pass does not cover this later source.

**Current conclusion:** no unresolved confirmed source defect remains after the focused fixes, but final review/qualification is **pending the affected executed checks and the named acceptance evidence**, including the coordinator's full/native/resource/platform gates. No further source change is requested by this focused inspection. This addendum supersedes the initial report's statement that the control extension is only planned; all other limitations remain. No additional build/test/probe or repository/service write was performed by this reviewer.


## Focused execution addendum — findings resolved, final qualification still pending

Read `diagnostic-final-01.log` and its JUnit: **14/14 pass**, including `oversized_native_error_preserves_query_vs_explicit_checkpoint_boundary`, bounded reader/cap tests, native query-result propagation, and no-replay schedule. Read `workspace-doctest.log`: **3/3 pass**. Read `clippy-final.log`: strict workspace check finishes successfully after the suffix/control edits. **FR-001 and FR-002 are now resolved by focused source inspection and affected passing execution.** These executions are coordinator-run; I started none.

A later test-only addition changes `src/db_writer/write.rs` to SHA-256 `d1625f1bd5239eea6eb7dd2b85244ce93f2df13d1503007b6cbe37dddd782542`. I compared it with the previously reviewed write source: the sole additional change is `phase_requests_preserve_low_and_high_configured_pools`, checking phase-request serialization for **128/256/384 MiB**, 768 MiB worker budget and one thread. This is a proportionate test of the already-reviewed direct pass-through; no production semantics changed. Its passing execution is **not** in the 14-case diagnostic log and remains pending at this addendum. The earlier snapshot-03 manifest does not silently identify that later file; the current test/control/phase hashes otherwise remain as recorded above.

**Pool-matrix follow-up verified:** `writer-pool-matrix-final.log` and JUnit record **15/15 pass**, explicitly including `phase_requests_preserve_low_and_high_configured_pools` (case 14), against the same current write SHA-256 `d1625f1bd5239eea6eb7dd2b85244ce93f2df13d1503007b6cbe37dddd782542`. This closes the narrow pool-test execution gap above; it is serialization/pass-through evidence, not measured RSS at each pool.

**Optimized native clean-control follow-up verified:** independently read completed `native-profile-copy-final.log`: **2/2 pass** in the optimized `native-tests` profile, zero ignores/filtered cases, elapsed **220.65 s**. The current test source hash remains `ca61337099747e3356c4308bfcba7c6b73f8d2a604bef6808569509465567c3e`; all seven current source hashes match the reviewed corrections/pool-test snapshot. The same-source chunked-versus-32-MiB-control logical node/ID/native property-string comparison, all connector endpoints and sampled search-vector comparison execute successfully. Native `CAST(n AS STRING)` binds and produces equal compared results. The original expected 1,400 Function labels, all PK checks, genuine PK operator, early index sequence, connector expectations, file-byte bounds, and Managed/Direct post-COPY preservation/fresh-invocation checks remain in the passing suite. This **closes acceptance-evidence gap 1 at the fixture level**; it does not add production schema introspection or establish a general every-property integrity validator. Sampled chunked database-write high water is `129582232` bytes, distinct from the configured pool or deliberate RSS-abort evidence.

**Final reviewer state at handoff:** no unresolved confirmed P0–P3 source finding. Full workspace, existing lifecycle/resource evidence and supported-platform qualification are still pending. A running run is not passing; the original full-suite snapshot cannot cover later source edits by assumption. Proceed with those checks and evidence reconciliation within approved Scope A; do not declare full acceptance/closure from this review alone. No constants changed or other writes were made by this reviewer.
