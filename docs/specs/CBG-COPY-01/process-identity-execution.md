# CBG-COPY-01 — process-identity verification repair

## Scope and approval

[COD-98](https://linear.app/codebasegraph/issue/COD-98), Maintenance `P-COD-2`; separate from COD-52 cursor behavior. [User-approved follow-up](https://github.com/rabii-chaarani/codebaseGraph/blob/931c3ddaeccba9c3f94f75e3ae58be9e3016a4ef/docs/specs/CBG-COPY-01/approvals/process-identity-test.md) at full commit `931c3ddaeccba9c3f94f75e3ae58be9e3016a4ef` records the2026-10-27 explicit “approved” response. Reuse existing CBG-COPY-01 A3/A9 with historical ScopeA/resource-amendment approvals unchanged. Scryer `chg-k66bxx`; no new modeled responsibilities/topology.

Baseline PR157 evidence head `9e7645028013555cc78529a552e3ce0b0468c20f`. [Failed native evidence](../CBG-PLAT-01/execution/cod52-evidence/hosted-run-40449278177/summary.json) establishes55unique numericPIDs across56COPYevents, not the specific cause. Diagnostic artifact contains logs/inventories, not raw phases.jsonl; earlier Windows PID reuse remains plausible, not proven.

## Repair and preserved behavior

- Existing debug trace now contains `process_instance`:256random bits from existing getrandom dependency, generated once per helper process by process-global OnceLock, constant across calls/threads. PID remains a validated diagnostic. It is not an operating-system/security identity or product protocol.
- All three fixture PID-uniqueness comparisons use the same validated lifetime-identity predicate. Different process instances may reuse a numeric PID; multiple phase records from one process still fail. Empty/singleton collections, missing/malformed tokens and invalid diagnostic PIDs fail.
- No per-request/event token, retry, timeout, numeric-PID fallback, native/loading/containment/resource/dependency or public CLI/MCP/data-format change. All existing chunk byte/thread/index/PK/connector/search/clean-control/failure/no-replay/Managed+Direct preservation assertions remain.
- Entropy errors remain cached errors and stop the debug trace before append. Tracing-disabled ordinary builds do not initialize the token. Release builds compile out this existing test instrumentation.

## Verification and evidence

Exact reviewed source hashes in [snapshot](process-identity-evidence/implementation-snapshot.json): phase.rs `889b005587d1ce59a63ae35fdcbb1d799ae5248cc910e65cb102e23b80ffa9fa`, copy_phase_process.rs `bd6d263acba7364a2846b9cb52a669a5281026adadd7b347689e3a5bc0fdbf94`. Cursor files remain unchanged from COD-52's reviewed hashes. [Diff](process-identity-evidence/implementation-diff.patch).

Native macOS26.6.2ARM64, Rust1.96.0, unchanged cached Ladybug0.19archive SHA256 `a840ea7c538e67a8b9fc0d58495b5226dc9c0a5f2f144ff0c48c0f6ca1193ae6`. Captured rebuilt CLI SHA256 `e189db48a699a276019674f93c8698e59805ff80f0e1f7c9260089069ce7a25f`; process tests use that copied binary through CARGO_BIN_EXE_codebase-graph. Commands execute from the isolated project environment; builds serialized.

- Deterministic **3/3pass**: stable singleton calls+threads; samePID/distinctinstance accepts; duplicateinstance across events (even differentPIDs) rejects; missing/malformed/empty evidence rejects. [Log](process-identity-evidence/deterministic-tests.log), [JUnit](process-identity-evidence/deterministic-tests.junit.xml).
- Actual native COPY suite **5/5pass,0skipped**: boundedchunk/reopen/PK/connector/search/clean-control121.218s; jumbo row26.980s; Managed+Direct acknowledgement-failure/no-replay/old-bundle preservation/recovery153.320s; both identity oracle cases. [Log](process-identity-evidence/native-copy-process-tests.log), [JUnit](process-identity-evidence/native-copy-process-tests.junit.xml).
- Nearby phase unit **13/13pass**, including singleton stability, metadata/thread/protocol/scope and bounded failure diagnostics. [Log](process-identity-evidence/phase-unit-tests.log), [JUnit](process-identity-evidence/phase-unit-tests.junit.xml).
- Final `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all --check` **pass**. [Clippy](process-identity-evidence/clippy.log), [format](process-identity-evidence/format-final.log), [complete command/environment/binary provenance](process-identity-evidence/provenance.json).
- Fresh GPT-6.1Sol/High [proposal review](process-identity-evidence/proposal-review.md): proceed, no blocking finding. Fresh GPT-6.1Sol/Max [final review](process-identity-evidence/final-review.md) finds no confirmed P0–P3; final evidence addendum records exact source/results without inferring native Windows execution.
- Native Windows follow-up and current-head CI remain pending. Deterministic/Mac results are not Windows execution.

Earlier broad local1110case command is explicitly **incomplete, not passing**:606completed passes and a fixture failure requiring capturedshippingCLI, then stopped before follow-up builds. [Record](process-identity-evidence/prior-local-full-run-incomplete.json), [limits](process-identity-evidence/prior-local-full-run-incomplete.md). Hosted exact-old-source Mac843pass/2ignored is separate. No missing CLI prerequisite was bypassed; current process verification captures the actual CLI.

## Reconciliation and remaining gates

Governing Scryer claims checked; debug trace has no symbol claim and surrounding release-resources claim is unchanged. Configured model/graph are older-root orientation; unavailable graph MCP reported, target CLI graph discovery used. No exact-worktree model/verdicts fabricated, unrelated drift untouched. Memory assessment: no filler memory for transientCI cause; preserve evidence and exact process identity semantics here.

COD-98 localimplementation only, qualification/review/closure pending. COD-52 remains In Review. No merge, deployment, service restart, otherMaintenance scope or aggregateclosure. Final native cases, strictchecks, final source/evidence review and Mac/Windows head-bound CI must be reconciled before completion.
