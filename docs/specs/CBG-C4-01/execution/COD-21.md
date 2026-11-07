# COD-21 execution brief — revision 4, owner reassignment

Work: CBG-C4-01/F02/S02 / [COD-21](https://linear.app/codebasegraph/issue/COD-21), parent COD-6, M1/P1. Status: actual C21-D01/D02 human decisions recorded; owner plan/signatures settled below; first implementation attempt returned incomplete; fresh owner authorized under existing Scope A after this reassignment is committed and coordinator signals GO. **Compiler acceptance is unsatisfied.** Maintainers retain aggregate closure.

## Authority, baseline and starting conditions

Reuse approved specification0.3 at full SHA `66dff2564e76ad06cdf172332e2da318d150fc73`, its pinned C16/C18/C1/Q1 contract set, and actual Scope A/B approval/scoped Git-bootstrap activation published at `ba5c7c543bd711b663e1dfc92d4bd58497ffb557`. The approved canonical/policy artifacts are unchanged. The effective ignored policy source is `/Users/rabii/Projects/Repositories/codebaseGraph/AGENTS.md`, SHA-256 `ed72f74970f8c4e2cc0f5834b787a6d0d26e47360d0ba3ea3a4eabcc0b6a7b80`; its absence in a normal worktree grants no exemption. Native YAML has not become authority. No Scryer ledger, claim or verdict is fabricated for this scoped native work.

Live Linear confirms sole blocker COD-20 is Done for its schema/types scope. PR188 remains draft/open/unmerged at `f000bc1c90c788593ad0e1825ea6d03f7f439900`, targeting dev `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`; it includes PR187's documentation prerequisites. New branch `codex/cod-21-yaml-compiler` and worktree `/private/tmp/codebasegraph-cod21-20261107` start at that published head. The selected pilot source baseline remains `90e2ab6fae2767bdb158e19f0072d43568e80dbc`; inherited dev version/dependency differences are disclosed and do not redefine that pilot. Preserve predecessor evidence/target and original main checkout's four intentional wiki edits.

Memory recall, existing wiki architecture/authority context, Linear project/issues/relations and graph discovery/context were consulted. Graph tools are available but slow and index the original main checkout: ArchitectureDocument/serde_yaml searches and src/architecture query returned no entries; manifest queries likewise have no File rows. Graph-discovered src/lib.rs (`Module:41664f532caa8578527c`) and explicit change_impact context provide the existing module boundary; branch source export traces COD-20's src/architecture/mod.rs. Qualify these graph gaps against the actual frozen branch; do not rebuild/install or substitute another project's graph.

The [compiler supplement0.1](../cod21-contracts.md) is approved for C21-D01 serialized grammar and C21-D02 immutable Git/inline inputs at full SHA `9b4384378c03e05a458f8f94291c8bba4d82c7c3`; [actual decision record](../approvals/cod21-approval.md) retains both exact answers and the question locators. Fresh Sol/xhigh review and independent Terra/medium remote readback passed that published subject before the human answers. Original Scope A approval covers settled compiler behavior, with these additional decisions covering the formerly unspecified choices. No approval is inferred from status, review, preselection, dispatch or time. Direct live-tree compilation is outside this assignment.

## Workflow and ownership

Workflow: **delegated**, one complete technical owner, default/gpt-6.1-terra/medium/fork_turns:none. Initial owner `/root/cod21_owner` stopped and explicitly relinquished all files/processes after an incomplete scaffolding attempt; no compiler acceptance was achieved. New sole owner `/root/cod21_implementation_2` inherits the exact files and settled contracts below after this reassignment is committed. Root coordinates questions/tracking and serialized Git/delivery. The owner is not alone in the workspace, must preserve other edits and report ownership/contract changes before writing. No nested dispatch. This is the second owner attempt, not a reset of evidence or acceptance history.

After the start gate, that owner alone may edit:

- `src/architecture/mod.rs` for narrow compiler exports/reuse of existing invariant types;
- new `src/architecture/compile/{mod,source,yaml,policy,constraints,scenarios,validate}.rs` (split these responsibilities further only through coordinator reassignment);
- new `tests/architecture_compile.rs` and `tests/fixtures/architecture_compile/**`;
- `Cargo.toml` and `Cargo.lock`, solely the justified direct parser dependency using the existing locked version; no unrelated updates/features/version change.

Root alone owns `docs/specs/CBG-C4-01/cod21-contracts.md`, this brief, `approvals/cod21-approval.md`, `cod21-evidence.md` and evidence under `cod21-evidence/`. The supplement stays byte-identical to the approved subject unless a new revision/decision is required. Existing COD-20 product/evidence, assets/graph_schema.json, assets/architecture_schema.json, tests/architecture_model.rs, src/lib.rs, API/adapters/storage/config/workflows, Scryer and wiki files are outside this technical assignment. Further required files return to coordinator for exact reassignment before edits.

Place parsing/source capture/typed compilation/semantic validation under the existing transport-neutral architecture owner. One canonical grammar and validator; no graph→k-wiki dependency, adapter evaluator, native agent runtime or speculative crate. Existing schema/types remain compatible. Compiler service returns complete typed model/raw closure or structured errors; no model/closure hashes or acceptance IDs/publication/storage mutation. Git object integrity checks serve immutable source capture, not COD-22's model/input hashing implementation.

### Settled owner plan and Rust boundary

The owner returned this complete plan after the actual human answers. Public service entry under `architecture::compile`:

```rust
pub fn compile(
    input: ModelInput,
    repository: Option<&RepositoryBinding>,
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<CompiledModel, CompileError>;
```

ModelInput has exactly the approved PinnedRoot/InlineFiles logical alternatives. Use an opaque struct with validated constructors, or a non-exhaustive enum of validated private-field payload wrappers; no new mutable public success-state fields. RepositoryBinding binds configured repository Text identity to a local Git directory, and is mandatory/identity-matched for pinned input, unused for inline. ExecutionContext owns a shared cooperative cancellation flag plus optional absolute Instant caller deadline; compile derives the earlier configured duration from invocation start. Full policy has all17 explicit positive u64 fields; optional overrides have corresponding Option<u64> values, must be positive and cannot raise the repository value. Retain the complete effective policy.

CompiledModel holds root, ordered architecture/constraint documents, sorted raw closure entries, effective policy and optional pinned repository/commit/tree provenance. Closure entries retain normalized path, raw bytes and optional immutable blob object ID. New types use private fields, validated constructors/builders/immutable accessors; structured deserialization invokes validation. Complete successful output has no unchecked constructor/deserializer. Existing COD-20 types/legacy APIs remain compatible.

CompileError carries a stable C16 code and ordered CompileDiagnostics (code, reason, optional subject/location). SourceLocation retains path, UTF-8 start/end byte positions and one-based line/column. Omit a file position only when no source position exists (configuration/context failure or unavailable source); no fabricated position or partial successful model. Output diagnostics sort deterministically.

Implement in order: policy/context; bounded cancellable immutable Git object source and exact inline capture; span-preserving strict libyaml event parser; complete strict constraint/scenario grammar and local typed substitutions; whole-closure validation/bounded Cartesian policy checks; complete output size accounting and acceptance/compatibility checks. Source Git plumbing uses structured command arguments, exact immutable IDs and literal paths, disables replacement objects/lazy fetch/config-driven filters/optional locks, and verifies commit/tree/blob types, contents and hashes. Bounded cancellable `git hash-object` may supply the repository's object hash format without a new SHA-1 dependency. Handle legal unusual path bytes without line-framed command injection. Own child cleanup through actual completion; no shell, network fetch or unchecked blocking/unbounded process output.

The owner owns implementation/checks within the already listed files; root owns this plan/approval/evidence and all serialized Git. Any material signature/behavior/ownership change returns to coordinator before dependent edits; routine internal layout can proceed within the settled contracts.

### Reassignment and PR-content instruction

The first owner returned only policy/diagnostic scaffolding and unconnected constraint/scenario drafts (four compile files, narrow export/parser dependency/lockfile edits). There is no compile entry, source loader, strict parser, semantic validation or test suite from that attempt. Its partial offline check passed with dead-code warnings; it is not product acceptance. All processes are quiescent and its file ownership is revoked before the new assignment. Reuse sound work but complete the entire agreed compiler and checks; do not present partial scaffolding as the deliverable.

The maintainer subsequently instructed that PRs include only essential files and exclude session-related logs/evidence. This overrides earlier instructions to add session artifacts to the implementation PR, without waiving actual verification/review or retention. Root keeps session logs/reports outside the implementation PR, summarizes results/links in tracking/PR prose and preserves approved/frozen subjects and predecessor evidence. The already published exact approval subject remains available; no history or evidence is silently destroyed. The new technical owner writes logs only to its unique external scratch directory and does not add session artifacts to product changes. Final delivery will use an essential-file diff; root serializes that integration and supplies its exact snapshot for independent checks/review.

## Scope, acceptance and failures

Deliver R01/R12 and the compile/isolation foundation of R02/A02, using C16 strict modular YAML, C4/constraint/scenario validation and explicit resources. The supplement's C21-A01–A06 map requirements/contracts to observable tests. Do not claim all canonical A01–A03 or platform/pilot qualification. COD-22 owns stable closure/model hashing; COD-29/39 own actual transformations/scenario execution; COD-30/36 own evaluation/waivers; COD-43/44 own adapters/config/catalog. Full YAML input shapes may be compiled without pretending those runtime checks have executed.

Failure paths include malformed syntax/types/versions, duplicate identities/keys/paths/refs, dangling/wrong-level parents/endpoints, contradictory rules, unsupported constructs, confined source failures, unavailable immutable objects, resource exhaustion, cancellation/deadline and input changes. Compilation is all-or-nothing and cannot mutate accepted/source generations. No automatic replay/fetch, partial PASS, invented default policy or weakened obligation.

## Verification and authorized delivery

Before any large Rust build, check df. Readiness check showed65GiB free; no cleaner launched. Use this worktree's normal `/private/tmp/codebasegraph-cod21-20261107/target`, never repurpose COD-20's target. Preserve durable evidence in the root-owned spec evidence directory outside target. Owner reports exact commands/revision/environment/results/limitations and failures; root records immutable evidence without rewriting predecessor records.

Run focused compiler fixtures, affected COD-20/legacy compatibility tests, formatting and strict relevant Clippy. Independent default/gpt-6.1-terra/medium verification executes acceptance evidence. Fresh independent default/gpt-6.1-sol/xhigh/fork_turns:none reviewer uses adversarial-review-engineer and engineering lenses on final integrated diff against the baseline, including placement/dependency/contracts/callers/replacement/failure behavior. AGENTS assignment settings override the skill's Max default. Resolve every confirmed P0–P3 defect with owner corrections, affected reruns and focused reviewer verification at the final snapshot.

Root may commit/push reviewable authorized changes and create/update a draft PR targeting dev, disclosing PR188/187 prerequisite history; independent remote publication readback is required. Proposal publication does not complete COD-21 or authorize product code. After implementation acceptance/review/handoff, reconcile spec/evidence/Linear and assess wiki/memory reuse; no filler memory. A finished issue requires a fresh coordinator session for the next issue. No merge, release, runtime trust activation, global authority change, settings mutation or aggregate closure is authorized here.
