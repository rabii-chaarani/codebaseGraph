# COD-22 execution brief — revision 3, verified draft delivery and retention decision

Work: `CBG-C4-01/F02/S03` / [COD-22](https://linear.app/codebasegraph/issue/COD-22), parent COD-6, M1/P1. Status: **C22-D01/D02 approved; local implementation, independent execution/final review and publication verified; draft PR190 open; historical executable-retention disposition pending human decision before handoff/closure**. Maintainers retain COD-6/milestone aggregate closure. This file is the execution record under the scoped Git/bootstrap authority; it grants no additional scope. Earlier immutable brief2 remains the actual pre-code assignment at `b4d075d05b203d1dd7e54b946cac94f944810df6`.

## Authority and immutable inputs

- Approved canonical specification 0.3 / R01–R12 / A01–A18: `66dff2564e76ad06cdf172332e2da318d150fc73:docs/specs/CBG-C4-01/spec.md`, with its exact incorporated C16/C18/C1/Q1 contracts. [Actual Scope A/B human decision](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/approvals/cod19-approval.md) and scoped activation remain `ba5c7c543bd711b663e1dfc92d4bd58497ffb557`.
- Approved compiler grammar/input supplement: `9b4384378c03e05a458f8f94291c8bba4d82c7c3:docs/specs/CBG-C4-01/cod21-contracts.md`; actual record at `f2129e7dac64191ca61d842e6773ffb1a104668d:docs/specs/CBG-C4-01/approvals/cod21-approval.md`.
- Root minimum Rust 1.98 / explicit build and verification Rust 1.98.1: existing maintainer decision at `1ed5620073e5e0a60cef6425daaca2cb20c469d9:docs/specs/CBG-C4-01/approvals/cod21-rust-minimum.md`. Do not select the older local stable alias or rewrite other members' minima.
- [COD-22 supplement 0.1](https://github.com/rabii-chaarani/codebaseGraph/blob/8635297ff7f25bfd745d959f0ec7917982afa802/docs/specs/CBG-C4-01/cod22-contracts.md): exact domains, equivalence/payloads and linked-content protocol were actually approved at full SHA `8635297ff7f25bfd745d959f0ec7917982afa802`. [Actual C22-D01/D02 decision](../approvals/cod22-approval.md) records the question locator and exact answer. Reuse approved C16 behavior without reopening it; commit/status/review/preselection/elapsed time are not approval.

Only activated CBG-C4 scope uses this bridge. Unrelated Scryer models, directives, pending work and closure owners remain untouched. No Scryer claim/sign-off/runtime dependency is invented. Native YAML authority and trust/acceptance/hosted enforcement still require COD-37/46/47 and their independent human decisions.

## Readiness, baseline and graph limits

Live CodebaseGraph Extension Linear was consulted, including completed/archived records and dependencies. COD-22's sole blocking issue COD-21 is Done for C21-A01–A06; COD-21 itself depends on completed COD-20. COD-25 remains blocked by COD-24, so its backlog position is not readiness. One issue only is selected here: COD-22.

Actual product prerequisite snapshot is published, independently verified/reviewed COD-21 head `e9bcaf0cb21e4865ffe54387833954940b7c7d58`, tree `679a30d04f1395a84f5f0fb5355ac3d96cef113c`, [draft PR189](https://github.com/rabii-chaarani/codebaseGraph/pull/189), open/unmerged into dev `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`. COD-20 PR188/frozen worktree remains unchanged. Product implementation branched from the exact COD-21 head; no merge is inferred. [Draft PR190](https://github.com/rabii-chaarani/codebaseGraph/pull/190) targets dev and discloses included prerequisite source while those PRs remain unmerged.

This preparation uses isolated `/private/tmp/codebasegraph-cod22-spec-20261107` on `codex/cod-22-contracts`, based on preserved specification branch head `ca5c213a8ce4e0f874d1e2cf0251b3fc82c1ddae`. That branch carries canonical approvals/briefs, not a new pilot baseline. Preserve original main `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, its intentional MCP-authored wiki/memory edits and every predecessor worktree/evidence location. Main's 4.2.1 remains the selected source pilot; inherited dev 4.2.0/manifests/dependencies do not acquire pilot qualification through this work.

Read AGENTS and referenced RTK; recalled repository memory; started k-wiki with list-bundles then concept search/read; consulted compiler/authority/spec context and Linear. Active memory preserves essential-only PRs and Rust policy. Graph MCP is available in this session. Semantic search/context and architecture-query catalog were consulted; exact File query for `src/architecture` returned zero rows. Graph-discovered `src/lib.rs` (`Module:41664f532caa8578527c`) locates the existing module export; explicit dependency/change-impact context was requested. Graph indexes original main, lacking unmerged COD-20/21 source and manifest coverage. Frozen prerequisite Git paths/diffs and actual source qualify this gap; no rebuild/install or another project's graph is used.

## PC-01/02 placement, affected callers and preserved contracts

Extend the transport-neutral Native Architecture compiler owner. The existing compiler returns an opaque complete typed model, sorted captured raw closure, effective policy and verified optional Git provenance. Its complete-output canonical cache includes raw bytes/policy/provenance for admission; using it wholesale as an intent or evaluation identity would be incorrect.

Add separate identity and linked-artifact qualification services beneath `architecture::compile`. Reuse existing strict typed vocabulary, cooperative policy/context and immutable Git capture. Narrow internal shared helpers may serve both paths; preserve one authoritative Git-capture implementation and the existing compile signature/complete-output bytes. No new cross-component dependency/cycle, k-wiki parser dependency, public graph writer, storage/schema/lease change, extraction inference, source node alias, operation catalog, adapter or accepted-state producer is in scope.

Immediate consumers are library callers of `architecture::compile`; future COD-23/29/32/37 consume the typed constituents/qualified locators. Requirements/test documents retain original locations/IDs/revisions; their internal semantics/execution/trust qualification remain downstream. A compiled or content-qualified artifact never becomes a VerifiedReview, VerifiedMaintainerAction, full evaluation identity, passing test or acceptance receipt.

## Workflow, ownership and proposed service signatures

Select **delegated** workflow: root coordinates and owns documentation/tracking/Git; `/root/cod22_implementation` is the one complete technical owner (native default / gpt-6.1-terra / medium / fork_turns:none). This changes the preparation's direct proposed mode before any product writer/process exists; root's code ownership is quiescent and revoked before assignment. The owner investigates, locally designs within settled contracts, implements and checks the complete deliverable. Independent verification/review retain separate agents and the exact repository settings below. Git actions are serialized by root; all workers/checkers share the workspace and must preserve others' edits, report blockers and never dispatch nested agents.

Root owns `docs/specs/CBG-C4-01/cod22-contracts.md`, `docs/specs/CBG-C4-01/execution/COD-22.md` and `approvals/cod22-approval.md`. The supplement remains byte-identical to the approved subject. Product worktree `/private/tmp/codebasegraph-cod22-20261107`, branch `codex/cod-22-identity`, started at exact prerequisite e9bcaf0. The implementation-ready brief2 was committed/published before root's GO; the technical owner is now quiescent and retains bounded fix ownership pending final handoff.

Exact product ownership for `/root/cod22_implementation` after those gates:

- `src/architecture/compile/mod.rs`: additive module export and private validated root-file index in CompiledModel, captured from the existing reader's root path before it is discarded. Identity uses that index into the already retained sorted closure; no root-path reconstruction or change to the existing complete-output byte shape.
- New `src/architecture/compile/identity.rs`, `identity/canonical.rs`, `identity/artifacts.rs`: identity orchestration, canonical hashing/normalization and reference inventory/qualification respectively. One owner controls the entire deliverable; additional files require an exact recorded reassignment before edits.
- `src/architecture/compile/output.rs`: bounded internal reuse of canonical traversal/encoding only, preserving existing complete-output representation/admission and stack-safe behavior.
- `src/architecture/compile/source.rs`: expose a narrow internal verified immutable artifact capture helper using existing Git plumbing; no duplicate Git implementation, broadened loader policy or working-tree mode.
- New `tests/architecture_identity.rs`: observable identity/reference/failure acceptance coverage; fixtures remain self-contained temporary local repositories or inline values in that file.

Cargo.toml/Cargo.lock, primitive models/schema, compile grammar/validators, existing tests, lib/API/adapters/storage/config/workflows, Scryer, wiki and predecessor evidence are outside product ownership. Existing sha2/serde_json dependencies suffice. Required additional paths/contract changes return to the coordinator before edits. Broader restructuring is not silently included.

Settled service entrypoints under `architecture::compile::identity`:

```rust
pub fn identify(
    model: &CompiledModel,
    context: &ExecutionContext,
) -> Result<CompiledIdentity, CompileError>;

pub fn qualify_artifacts(
    references: &[ArtifactRef],
    repositories: &[RepositoryBinding],
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<QualifiedArtifacts, CompileError>;
```

Success types have private fields/immutable getters and no unchecked deserializer/constructor. `CompiledIdentity` exposes the supplement's exact logical fields; `QualifiedArtifacts` exposes complete entries with exact original ref/tree/blob/captured bytes. Logical canonical serialization is available for exact result-byte accounting/tests, with no derived cache entering itself. Reuse CompileError and deterministic C16 diagnostics; source path/ref subject is retained where available, never fabricated as an authored span. Hashing retains policy cancellation/deadline/error behavior.

## Acceptance, checks and failure behavior

Implement only C22-A01–A06, mapped in the supplement to R01/R09/R10/R11/R12 and bounded A01/A12–A14 contributions. No full A12 result-integrity/invalidation, A13 human authorization, A14 durable evaluation table, Q1/pilot, scenario execution, native-platform or release qualification is claimed. COD-6 closure remains with its owner.

Meaningful checks include independent canonical byte/hash vectors; typed equalities under file moves/set reorder and negative ordered-array/ID controls; root/import/comment/path/provenance/policy invalidation; exact nested ArtifactRef inventory and opaque original reference retention; immutable Git repository/commit/path/digest and missing/corrupt/symlink/submodule/conflict controls; resource output/evidence bounds, deep traversal, cancellation/deadline and all-or-nothing/no-mutation behavior. Run existing compiler/model/schema/public-compatibility radius, formatting and unsuppressed strict Clippy using explicit Rust/Cargo 1.98.1. Record failures as failures, not superseded greens. Tests configured/attached are not executed evidence.

No partial successful identity/qualification, implicit fetch, fallback to mutable bytes, loose digest authentication, candidate policy broadening, invented resource cap/default, automatic replay or accepted/source mutation. Missing required artifact content stays unavailable for the dependent evaluation, never empty PASS. Every test/review is bound to the final implemented snapshot; rerun affected checks and focused review after relevant changes.

## Storage, evidence, delivery and pending gates

Before preparation, df showed approximately 13 GiB free; at actual approval it showed 7.6 GiB. At that preflight no Rust build or cleaner had started. Configured cargo-reclaim covers established repository/worktree/cache roots, balanced mode, whole-target off and a four-hour recent-write window; retained COD-21 temporary evidence is outside those unattended roots. Root inspected the running service/latest plan (zero failures and no delete candidates); no cleaner, target/cache deletion or global policy/toolchain change was performed. Builds used new issue-owned normal target `/private/tmp/codebasegraph-cod22-20261107/target`, explicitly sequentially reused for this issue only, with `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`. This is not default-profile or dependency-hermetic qualification. Preserve this target and all frozen copies; do not reuse/delete COD-21's retained target or unrelated/shared caches. Future large builds require a fresh df/reclaim-plan check and new ownership where applicable.

Keep session logs and session-specific evidence outside the eventual implementation PR; use retained external session artifacts plus PR/Linear summaries. Immutable specification/approval/brief history is preserved separately. PR contents contain only essential source/substantive tests and any explicitly required deliverable contracts; no session-evidence dump or unrelated wiki edits.

Independent `verification`: native default / gpt-6.1-terra / medium / fresh prompt-only session, actually executing relevant evidence. Required fresh `final_review`: native default / gpt-6.1-sol / xhigh / fork_turns:none, using `/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md` and its engineering lenses; AGENTS settings override the skill's Max default. Review integrated change against the exact baseline, C22 criteria and executed evidence, explicitly checking placement/dependencies/shared contracts/callers/replacement cleanup/failure behavior. Fix every confirmed P0–P3 issue and obtain focused final-snapshot verification. Conditional risks/open questions remain separate.

For this proposed contract package, the same required independent review method/settings inspect the two-file documentation delta before the exact published subject is presented for decision. Independently read back remote bytes/approval references; do not describe document review as implemented product verification. No implementation PR handoff or COD-22 Done state follows from this preparation.

Actual decision is now recorded. Root commits/publishes this brief/approval record, creates the isolated product worktree, synchronizes Linear and signals GO before code. After product acceptance/checks/review, root may commit/push the essential branch and open a draft PR to dev, with prerequisites, canonical approved revisions, Linear and real evidence/limitations. Independent publication verification checks the final remote snapshot. Reconcile relevant wiki knowledge and explicitly assess reusable memory without filler. Only after the one issue's authorized PR handoff, open a fresh coordinator chat for the next actually ready issue.

## Final implemented and executed snapshot

Published/reviewed/tested head: **`d3f1dd5b9147543b84460a832381df67387588b9`**, tree **`87a261f6fd2f4e42f82e088aecd26ac50907cf0b`**, [PR190](https://github.com/rabii-chaarani/codebaseGraph/pull/190). Exactly seven COD-22 source/test files changed over e9bcaf0 (1,853 insertions/46 deletions). The PR against dev contains 29 essential paths, including 22 predecessor blobs unchanged from e9bcaf0; no specification/session-evidence/log/XML files. Manifests/dependencies were not changed by COD-22. The transport-neutral services, one shared canonical writer/verified Git reader, private validated root index and opaque successes are the implemented boundaries described above.

Independent `/root/cod22_verify`, native default/gpt-6.1-terra/medium/fresh prompt-only, executed final Rust/Cargo 1.98.1 on macOS 26.6.2 arm64; Git 2.52.0. The SHA-256 init probe succeeded, so both SHA-1 and SHA-256 fixture branches ran. All commands used the explicit target/profiles above and `TMPDIR=/private/tmp/cod22-qualified-verify-20261107/tmp`. No command is represented as execution on main/pilot or another native platform.

Executed checks: `cargo +1.98.1 fmt --all --check`; unsuppressed `cargo +1.98.1 clippy --all-targets --keep-going -- -D warnings`; `cargo +1.98.1 test --test architecture_identity` (12), `--test architecture_compile` (16), `--test architecture_model` (9), `--lib architecture::compile` (28), `--lib schema` (15). **80 passing tests, zero failures/ignored** in this selected final scope. Cargo JSON binds the actual package path and execute-time binaries; source/manifests/diff hashes match before/after, with clean Git status. Final diff SHA-256 over e9bcaf0 is `88d418d6cdbfb90817cfce658368c5d128710bd6344f3dc447a3a2639be9044e`.

| Criterion | Final implementation/test evidence |
| --- | --- |
| C22-A01 | Shared iterative intent emitter and canonical normalization; `independent_canonical_oracles_and_raw_closure`, `nonempty_intent_matches_independent_canonical_payload`, set/text/ID controls, `moving_declarations_between_documents_preserves_intent`, nested operation-order and deep-scenario controls. |
| C22-A02 | Captured closure/raw-byte hashing, complete policy and pinned/inline provenance payloads; independent closure/policy oracles, `path_and_policy_changes_invalidate_the_expected_constituents`, `inline_and_git_closure_match_but_provenance_differs`. |
| C22-A03 | Typed nested inventory preserving exact locators; `nested_inventory_deduplicates_typed_references_and_preserves_operation_order` and `waiver_authorization_references_survive_nested_transform_inventory`; arbitrary strings do not become references. |
| C22-A04 | Exact configured local-Git capture, type/hash verification and structured missing/integrity distinction; `pinned_git_qualification_and_failure_boundaries` and `unreadable_commit_tree_and_blob_are_distinct_from_missing_objects`. Valid/missing/inaccessible, wrong digest/type, symlink/gitlink, corrupt/truncated/unknown headers, warning config, literal metacharacter paths, diagnostics and all-or-nothing controls executed in both object formats. |
| C22-A05 | Streaming descriptor/result metering, admitted-model-bounded record hashing, fallible owned vectors, chunked hash checkpoints and iterative scenario traversal; exact/minus-one evidence/result limits, `identity_result_size_is_exact_even_with_policy_digest_changes`, cancellation/deadline and 300-level deep traversal passed. OOM/timing injection is not claimed. |
| C22-A06 | Final independent 80-test/fmt/strict-Clippy execution, fresh Sol/xhigh review plus focused fix verification, and independent exact remote publication readback passed. The separate historical binary-retention decision below still blocks final handoff/closure. |

Fresh `/root/cod22_final_review`, native default/gpt-6.1-sol/xhigh/fork_turns:none, used adversarial-review-engineer and its engineering lenses on integrated fa1aad against e9bcaf0, then focused corrected snapshots through final d3f1dd5. It explicitly covered PC-01–14 placement/dependencies/contracts/callers, replacement cleanup, canonical identities, typed references, failure/resources/cancellation, coherence and evidence. **No confirmed P0–P3 defects remain in final d3f1dd5.** The original malformed Git-stream classification P2 and the harmless-stderr-warning P2 are resolved, including the failed-probe malformed-header branch. Earlier passing tests that omitted those branches are not substituted for final regression evidence.

Earlier fixture/lint failures and the verifier's initial omitted-TMPDIR run remain retained, with corrected executions and their original meaning. The first technical-owner attempt returned orientation only/no edits/tests; the same owner resumed and completed the full deliverable. Final verification was rerun after every relevant code correction. No failed checkpoint is described as passing.

Independent `/root/cod22_contract_publication`, native default/gpt-6.1-terra/medium, read back exact branch/commit/tree, all 29 remote PR filenames/blob hashes and full bytes of all seven COD-22 files; all match local Git. PR190 is open/draft/unmerged at the named head/base, latest mergeable=true/mergeable_state=blocked. Exact-head combined status is pending with zero statuses; check-runs and all-event workflow-runs counts are zero. Current CI PR branch filters exclude dev; there is no hosted passing-green evidence or merge authorization.

## Preserved final artifacts and limits

The independent verifier copied/hash-verified binaries immediately after each invocation before any later build, preserving original loader paths and references. Original reports/copies are `/private/tmp/cod22-qualified-verify-20261107`; a byte-verified durable copy of all 31 evidence files (excluding only temporary fixture/cache directory) is `/Users/rabii/.codex/visualizations/2026/11/07/01a1b4b9-3a29-7201-ae67-0e7ece607d3d/cod22-session-evidence/qualified-d3f1dd5`. Root's separate copy manifest records exact checksums; no original was moved or relabeled. Binaries are stored by hash under `frozen-binaries/`:

| Artifact | Execute-time/preserved SHA-256 |
| --- | --- |
| CLI | `0e50fdc5e931007b8dfd910399678cc0e2f818e7e078fd14a5231a806936aa5b` |
| Identity API tests | `0fdbbde7d63d2f8f04eb2675b2e6070dddddc94e56d4b39b456b008d07e9dd17` |
| Compiler API tests | `dee4353c492ba1e4c94f2c6437d6a41869d046d20ff56db56a63313418e860f2` |
| Model tests | `cafad8c8230998a29de82d49fbf136c750885dc3b59aa86cbda568e48f2e107d` |
| Library tests | `268d5b368d60d59685555db279967a234c5711fd262746f91f16ef84e31daf79` |
| OpenSSL libssl | `26ecfd43c742914a142c8b414b1f308cefe0531fcc6e38b6d45021649850e758` |
| OpenSSL libcrypto | `ef2239cec921003b54b61968f00489271e2a2805d7d1b4e69d632423f38efe1e` |

The nested Cellar libcrypto path matches the same content. Other linked dependencies are system libraries/frameworks, recorded through otool; loader paths were not modified and relocated replay is not claimed. The target, frozen copies, earlier reviewed 268808f/6bd1053 copies and predecessor evidence remain retained outside configured cleanup roots. Final processes are quiescent.

Remaining qualification: other platforms, exact minimum patch 1.98.0, default debug/incremental profiles, dependency-hermetic rebuilding, injected OOM, mid-operation/mid-sort timing and concurrent object/tree replacement, full hosted enforcement, Q1/pilot sufficiency, complete evaluation identities/result integrity, native trust/accepted state, scenarios, release and aggregate closure retain separate gates. Standard serde fixed-depth internal allocation remains inherited; new identity emission is per record and new untrusted descriptor/result accounting is streamed. No unlimited/deployment default or weakened evidence obligation was introduced.

## Historical intermediate-binary retention exception — decision pending

The coordinator authorized the P2 correction build before copying execute-time artifacts of rejected intermediate **`fa1aadab88d8dd95870685247fd3ab4624156fb5`**. It overwrote that snapshot's CLI and three integration test executables. Its old library executable (`d9796dbd7840bb65b518101ca31885b01a2256d92aa0eaf20db98c995cf63279`) survived and was copied/hash-checked read-only under external `frozen-fa1aada`. That Git source, independent Cargo/hash receipts/logs and reviewer reproduction remain unchanged. The four overwritten executables are **not preserved or recoverable from those receipts**, and corrected binaries are not labeled as old execution. The user was informed when discovered.

The audit is `/Users/rabii/.codex/visualizations/2026/11/07/01a1b4b9-3a29-7201-ae67-0e7ece607d3d/cod22-session-evidence/fa1aada-retention-audit.json`. All later reviewed snapshots' execution artifacts and dependencies were copied/hash-verified before subsequent builds, including the entire final d3f1dd5 set above. Final independent full execution/review/publication do not retroactively repair the historical loss or authorize a retention exception.

**Proposed bounded decision:** accept the explicit historical fa1aad four-executable retention gap for COD-22 draft PR handoff/issue closure, relying on the fully preserved and independently verified final d3f1dd5 evidence. This accepts no assertion that old binaries were recovered and waives no final artifact, trust, merge, release, platform or aggregate gate. **Alternative:** keep handoff/closure blocked while the maintainer determines another retention disposition. A real human answer identifying this record/final head is required; no approval record exists yet. Root must record the actual answer later, not infer it from technical review or the original C22-D01/D02 approval.

Current unsatisfied gates: human historical retention disposition, curated knowledge reconciliation and final authorized handoff/issue closure. No merge, release, trust activation, integration-settings mutation or unrelated issue advancement is authorized. Open a fresh coordinator chat only after this one issue is actually finished.
