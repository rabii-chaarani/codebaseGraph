# COD-22 execution brief — revision 1, proposed contract preparation

Work: `CBG-C4-01/F02/S03` / [COD-22](https://linear.app/codebasegraph/issue/COD-22), parent COD-6, M1/P1. Status: **readiness/contract preparation; C22-D01/D02 human decisions pending; dependent product coding blocked**. Maintainers retain COD-6/milestone aggregate closure. This file is the execution record under the scoped Git/bootstrap authority; it grants no additional scope.

## Authority and immutable inputs

- Approved canonical specification 0.3 / R01–R12 / A01–A18: `66dff2564e76ad06cdf172332e2da318d150fc73:docs/specs/CBG-C4-01/spec.md`, with its exact incorporated C16/C18/C1/Q1 contracts. [Actual Scope A/B human decision](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/approvals/cod19-approval.md) and scoped activation remain `ba5c7c543bd711b663e1dfc92d4bd58497ffb557`.
- Approved compiler grammar/input supplement: `9b4384378c03e05a458f8f94291c8bba4d82c7c3:docs/specs/CBG-C4-01/cod21-contracts.md`; actual record at `f2129e7dac64191ca61d842e6773ffb1a104668d:docs/specs/CBG-C4-01/approvals/cod21-approval.md`.
- Root minimum Rust 1.98 / explicit build and verification Rust 1.98.1: existing maintainer decision at `1ed5620073e5e0a60cef6425daaca2cb20c469d9:docs/specs/CBG-C4-01/approvals/cod21-rust-minimum.md`. Do not select the older local stable alias or rewrite other members' minima.
- [Proposed COD-22 supplement 0.1](../cod22-contracts.md): exact domains, equivalence/payloads and linked-content protocol require a real decision before implementation. Reuse approved C16 behavior without reopening it. No C22 approval record exists yet; commit/status/review/preselection/elapsed time are not approval.

Only activated CBG-C4 scope uses this bridge. Unrelated Scryer models, directives, pending work and closure owners remain untouched. No Scryer claim/sign-off/runtime dependency is invented. Native YAML authority and trust/acceptance/hosted enforcement still require COD-37/46/47 and their independent human decisions.

## Readiness, baseline and graph limits

Live CodebaseGraph Extension Linear was consulted, including completed/archived records and dependencies. COD-22's sole blocking issue COD-21 is Done for C21-A01–A06; COD-21 itself depends on completed COD-20. COD-25 remains blocked by COD-24, so its backlog position is not readiness. One issue only is selected here: COD-22.

Actual product prerequisite snapshot is published, independently verified/reviewed COD-21 head `e9bcaf0cb21e4865ffe54387833954940b7c7d58`, tree `679a30d04f1395a84f5f0fb5355ac3d96cef113c`, [draft PR189](https://github.com/rabii-chaarani/codebaseGraph/pull/189), open/unmerged into dev `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`. COD-20 PR188/frozen worktree remains unchanged. New product implementation will branch from the exact COD-21 head; no merge is inferred. Any eventual PR targets dev and discloses included prerequisite source while those PRs remain unmerged.

This preparation uses isolated `/private/tmp/codebasegraph-cod22-spec-20261107` on `codex/cod-22-contracts`, based on preserved specification branch head `ca5c213a8ce4e0f874d1e2cf0251b3fc82c1ddae`. That branch carries canonical approvals/briefs, not a new pilot baseline. Preserve original main `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, its intentional MCP-authored wiki/memory edits and every predecessor worktree/evidence location. Main's 4.2.1 remains the selected source pilot; inherited dev 4.2.0/manifests/dependencies do not acquire pilot qualification through this work.

Read AGENTS and referenced RTK; recalled repository memory; started k-wiki with list-bundles then concept search/read; consulted compiler/authority/spec context and Linear. Active memory preserves essential-only PRs and Rust policy. Graph MCP is available in this session. Semantic search/context and architecture-query catalog were consulted; exact File query for `src/architecture` returned zero rows. Graph-discovered `src/lib.rs` (`Module:41664f532caa8578527c`) locates the existing module export; explicit dependency/change-impact context was requested. Graph indexes original main, lacking unmerged COD-20/21 source and manifest coverage. Frozen prerequisite Git paths/diffs and actual source qualify this gap; no rebuild/install or another project's graph is used.

## PC-01/02 placement, affected callers and preserved contracts

Extend the transport-neutral Native Architecture compiler owner. The existing compiler returns an opaque complete typed model, sorted captured raw closure, effective policy and verified optional Git provenance. Its complete-output canonical cache includes raw bytes/policy/provenance for admission; using it wholesale as an intent or evaluation identity would be incorrect.

Add separate identity and linked-artifact qualification services beneath `architecture::compile`. Reuse existing strict typed vocabulary, cooperative policy/context and immutable Git capture. Narrow internal shared helpers may serve both paths; preserve one authoritative Git-capture implementation and the existing compile signature/complete-output bytes. No new cross-component dependency/cycle, k-wiki parser dependency, public graph writer, storage/schema/lease change, extraction inference, source node alias, operation catalog, adapter or accepted-state producer is in scope.

Immediate consumers are library callers of `architecture::compile`; future COD-23/29/32/37 consume the typed constituents/qualified locators. Requirements/test documents retain original locations/IDs/revisions; their internal semantics/execution/trust qualification remain downstream. A compiled or content-qualified artifact never becomes a VerifiedReview, VerifiedMaintainerAction, full evaluation identity, passing test or acceptance receipt.

## Workflow, ownership and proposed service signatures

Select **direct** workflow: root is coordinator and sole technical/documentation writer. Do not dispatch an implementer or let independent checkers edit source. Independent verification/review retain separate agents and the exact repository settings below. Git actions are serialized by root; all workers/checkers share the workspace and must preserve others' edits, report blockers and never dispatch nested agents.

Root owns only these new preparation paths now: `docs/specs/CBG-C4-01/cod22-contracts.md` and `docs/specs/CBG-C4-01/execution/COD-22.md`. A later actual `approvals/cod22-approval.md` may record only the user's real decision. No code writer is active before the human decisions and a committed implementation-ready brief revision.

Proposed product ownership after those gates:

- `src/architecture/compile/mod.rs`: additive module export and private validated root-file index in CompiledModel, captured from the existing reader's root path before it is discarded. Identity uses that index into the already retained sorted closure; no root-path reconstruction or change to the existing complete-output byte shape.
- New `src/architecture/compile/identity.rs`, `identity/canonical.rs`, `identity/artifacts.rs`: identity orchestration, canonical hashing/normalization and reference inventory/qualification respectively. Root is one owner of this entire deliverable; smaller internal splits require an exact recorded reassignment before edits.
- `src/architecture/compile/output.rs`: bounded internal reuse of canonical traversal/encoding only, preserving existing complete-output representation/admission and stack-safe behavior.
- `src/architecture/compile/source.rs`: expose a narrow internal verified immutable artifact capture helper using existing Git plumbing; no duplicate Git implementation, broadened loader policy or working-tree mode.
- New `tests/architecture_identity.rs`: observable identity/reference/failure acceptance coverage; fixtures remain self-contained temporary local repositories or inline values in that file.

Cargo.toml/Cargo.lock, primitive models/schema, compile grammar/validators, existing tests, lib/API/adapters/storage/config/workflows, Scryer, wiki and predecessor evidence are outside product ownership. Existing sha2/serde_json dependencies suffice. Required additional paths/contract changes return to the coordinator before edits. Broader restructuring is not silently included.

Proposed service entrypoints under `architecture::compile::identity`:

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

Before preparation, df showed approximately 13 GiB free. No Rust build or cleaner was started. Before a large build, recheck free space and configured cargo-reclaim plans/status. Use a new issue-owned normal worktree target; do not reuse or delete COD-21's retained approximately 10 GiB target or unrelated/shared caches. If a disposable out-of-tree target is needed, follow AGENTS' unique rust-agent-builds rule and record its actual path. Resolve any write approval needed without changing global rustup/default/reclaim policy.

Keep session logs and session-specific evidence outside the eventual implementation PR; use retained external session artifacts plus PR/Linear summaries. Immutable specification/approval/brief history is preserved separately. PR contents contain only essential source/substantive tests and any explicitly required deliverable contracts; no session-evidence dump or unrelated wiki edits.

Independent `verification`: native default / gpt-6.1-terra / medium / fresh prompt-only session, actually executing relevant evidence. Required fresh `final_review`: native default / gpt-6.1-sol / xhigh / fork_turns:none, using `/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md` and its engineering lenses; AGENTS settings override the skill's Max default. Review integrated change against the exact baseline, C22 criteria and executed evidence, explicitly checking placement/dependencies/shared contracts/callers/replacement cleanup/failure behavior. Fix every confirmed P0–P3 issue and obtain focused final-snapshot verification. Conditional risks/open questions remain separate.

For this proposed contract package, the same required independent review method/settings inspect the two-file documentation delta before the exact published subject is presented for decision. Independently read back remote bytes/approval references; do not describe document review as implemented product verification. No implementation PR handoff or COD-22 Done state follows from this preparation.

After actual decisions: record approved path/full SHA/scope/human question reference, update/commit this brief, synchronize Linear, then code. After product acceptance/checks/review, root may commit/push the essential branch and open a draft PR to dev, with prerequisites, canonical approved revisions, Linear and real evidence/limitations. Independent publication verification checks the final remote snapshot. Reconcile relevant wiki knowledge and explicitly assess reusable memory without filler. Only after the one issue's authorized PR handoff, open a fresh coordinator chat for the next actually ready issue.

Current unsatisfied gates: C22-D01/D02 human decisions; implementation-ready brief; all product acceptance/evidence; independent final product verification/review/publication. No merge, release, trust activation, integration-settings mutation or unrelated issue advancement is authorized.
