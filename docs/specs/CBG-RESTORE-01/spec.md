# CBG-RESTORE-01 — Restore pre-collector storage and preserve subsequent work

## 1. Identity, status and authority

- Work: CBG-RESTORE-01 / [COD-136](https://linear.app/codebasegraph/issue/COD-136), Maintenance, CodebaseGraph. Revision 0.1, 17 November 2026 (Australia/Darwin).
- Canonical path: docs/specs/CBG-RESTORE-01/spec.md. Closure owner: repository maintainer. [Execution brief](execution.md), [change ledger](preservation-ledger.md), [authority proposal](authority-plan.json) and [exact proposed file ownership](file-ownership.json) are part of this contract package.
- State: preservation executed and independently verified within its recorded scope; restoration contract proposed; product reconstruction, native qualification and delivery remain pending.
- Actual current-chat instruction: “PLEASE IMPLEMENT THIS PLAN”, naming the agreed restoration/preservation plan. This supersedes that plan's earlier planning-only sentence. It authorizes preservation and this concrete approval package, while retaining its explicit exact-committed-spec approval gate before dependent product changes.
- No approval record exists yet for these new bytes. The later actual decision must identify this path, the approved full Git SHA, exact scope and question reference. A commit, PR, review result or earlier conversational plan is not that approval.
- Unrelated work remains Scryer-governed. Preparation ledger: chg-3a1ewe. Existing committed responsibilities and separately approved amendments govern restoration; other issues' pending intent stays untouched. Native C4 retains its approved Git-bootstrap authority and exact historical approvals.

The requested endpoint is verified PRs. No merge, release, runtime installation, daemon/client reconfiguration, live-store enrollment, migration or deletion is authorized here.

## 2. Problem and observable outcome

Automatic graph disk cleanup entered main in PR153 at 75a99c840aa53e75a3f14df8dab41979c9efa19c. It also bundled unrelated COPY, resource-policy and platform changes. Later releases added further unrelated fixes. A wholesale revert does not apply cleanly and would discard retained behavior.

Use v3.0.4 as the pre-collector implementation reference, retain unrelated improvements, remove the added collector and its public controls, and keep every identified source/evidence snapshot recoverable. Native C4 remains separate feature work; unfinished work keeps its actual status.

This is a reconstruction from a historical code baseline, not a claim that the live store can safely be downgraded or that v3.0.4 has been tested against current data.

## 3. Baselines, selected decisions and boundaries

| Item | Exact subject / selected behavior |
| --- | --- |
| Pre-collector reference | v3.0.4, ca115690b226384164cf364458a0e3255b8908d3 |
| Frozen current main | 90e2ab6fae2767bdb158e19f0072d43568e80dbc; independently read from local Git and GitHub |
| Frozen dev / PR target | 2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9; target branch dev |
| Delivery topology | New commits on codex/restore-v304 descended from current main; construct the desired tree from the reference plus retained changes. A branch based only on the old tag is insufficient to express removal on merge. |
| Native C4 decision | Actual answer “Keep C4 separate (Recommended)”: companion branches preserve contracts, PR references, approvals and unfinished status. |
| Endpoint decision | Actual answer “Verified PRs only (Recommended)”: installed runtime, daemon, client configuration and current graph data stay unchanged. |
| Interface decision | Actual answer “Remove cleanup interfaces (Recommended)”: remove cleanup-only CLI/MCP/library surfaces; no disabled compatibility stubs. |
| Platform/backend | Retain current macOS ARM64 and Windows x64 support, approved current-macOS policy and Ladybug 0.19 inputs. No backend upgrade or restored Linux/Intel qualification. |
| Compiler policy | Retain the actual root Rust minimum 1.98 decision, with explicit cargo +1.98.1 builds; do not rewrite other members' minima or global rustup settings. |
| Version/history | Preserve current version metadata, changelog history, original refs/tags and approvals. Do not publish or relabel the reconstructed tree as release 3.0.4. |

Retain full/incremental builds, watching, existing graph read semantics, Direct destinations, hooks, parsing/search behavior and other noncollector interfaces. COD-125/126's proposed explicit-build/two-slot/managed-only transition is preserved but not implemented by this issue. COD-135's current-store recovery remains separate.

Source graph MCP tools are not exposed; CLI semantic search/context fail generation_gate_binding_changed. Retained repo-local manifest discovery with verified source hashes supports bounded orientation. Product implementation still requires exact source/caller coverage; use an issue-owned disposable graph for missing coverage, never another project's graph or a live-store guard bypass.

## 4. Requirements

| ID | Contract |
| --- | --- |
| REST-R01 | Preserve all original refs, stashes, commit objects, detached worktree heads and identified execution-referenced source; capture staged/unstaged bytes, relevant untracked/ignored source, instructions, architecture metadata and evidence before product edits. Preserve original paths and files. A verified bundle and independent reconstruction must establish recoverability. |
| REST-R02 | Restore the pre-collector storage/coordination implementation from the exact v3.0.4 reference, integrating retained changes explicitly. Remove the added collector, scheduling, cleanup enrollment, reference/retirement engines and persistent cleanup admission guards. Do not merely default a still-active collector to off. |
| REST-R03 | Retain all unrelated behavior already delivered on frozen main and unrelated subsets bundled inside PR153, including fail-closed COPY, sequential chunking, large valid records, removal of worker/Rust resource ceilings, platform/client fixes, security patches and release safeguards. |
| REST-R04 | Remove cleanup-only CLI flags, MCP inputs/catalog/schema entries and Rust request/response fields, including legacy-quiescence confirmation and collector-specific reporting. Preserve all other public operation signatures/defaults/error conventions. CLI removed flags fail through the normal unsupported-argument path. Document the breaking interface delta without a silent compatibility shim. |
| REST-R05 | Preserve complete atomic Managed/Direct publication, last-reader protection, owned abandoned-run recovery, containment/ownership, cancellation, retry and truthful failure results. Current-operation cleanup and pre-existing reader-triggered retirement are retained; no blanket promise that no files are deleted. |
| REST-R06 | Keep native C4 on separate companion branches in dependency order: contracts/activation, COD-20 types, COD-21 compiler, COD-22 identities, COD-23 evidence, then recovered COD-25 WIP. Preserve COD-25 before COD-24. Do not implement missing provider/Q1/accepted-state work or claim aggregate completion. |
| REST-R07 | Preserve every identified working change in its original snapshot and owning branch/records. Port unrelated ready changes separately; retain identity/COD-113 collector-related work as recoverable WIP instead of applying it wholesale. Unknown classification or missing required source blocks the affected restoration step. |
| REST-R08 | Preserve original canonical specs, approved SHA links, wiki concept identities, Scryer models/directives/history, native activation and closure owners. Update only owned restoration intent/evidence. Do not restore whole historical governance files, fold other pending work or migrate historical specs. |
| REST-R09 | Use new owned commits/refs with a complete original-to-destination ledger. Every post-reference commit, preserved ref and working change is classified. Mixed production files receive behavior-level decomposition and retained-acceptance mappings before their edits are kept. |
| REST-R10 | Leave installed executables, services, client settings, current graph root/config/bindings and other repositories unchanged. All execution/recovery checks use isolated issue-owned fixtures. No old executable is run against the current store. |
| REST-R11 | Use one complete technical owner and one writer per exact file, serialized Git and independent verification/review. Preserve existing required checks and current platform qualification. No COD-113 qualification exception, warning suppression, weakened assertion or inherited green result substitutes for this final snapshot. |
| REST-R12 | Deliver reviewed Conventional Commits and an independently checked PR to dev. Native companion work follows the restoration handoff in fresh coordinators, one active issue at a time; original branches/PR subjects remain immutable. Merge, release and live activation require their separate gates. |
| REST-R13 | Match changed code placement to the existing Graph Store, application service, coordinator, loader, adapter and Release Gate owners. Keep shared semantics transport-neutral, preserve dependency direction and APIs outside REST-R04, and remove all callers of retired paths without duplicating implementations. |

### Retained contracts

The [ledger](preservation-ledger.md) identifies original sources and mixed subsets. Scope A [CBG-COPY-01 at c5b62ba77d2f94b71714b5319f3b3fa215983f9f](https://github.com/rabii-chaarani/codebaseGraph/blob/c5b62ba77d2f94b71714b5319f3b3fa215983f9f/docs/specs/CBG-COPY-01/spec.md) is retained together with the later [resource amendment R1 at c8dd4ea9ca2a6f992756a2a41828396a27eabb6d](https://github.com/rabii-chaarani/codebaseGraph/blob/c8dd4ea9ca2a6f992756a2a41828396a27eabb6d/docs/specs/CBG-COPY-01/resource-ceilings-amendment.md). R1 supersedes Scope A's earlier resource/pool-ceiling assumptions; do not restore those ceilings.

Retain [CBG-PLAT-01 revision 0.11](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md), applicable exact-slice approvals, and the delivered COD-103 safeguards/approval history. The [actual Rust minimum decision](https://github.com/rabii-chaarani/codebaseGraph/blob/ca5c213a8ce4e0f874d1e2cf0251b3fc82c1ddae/docs/specs/CBG-C4-01/approvals/cod21-rust-minimum.md) is an already selected compatibility constraint.

## 5. Failure and recovery

- Ref/source/index movement during capture invalidates that capture; preserve the failed attempt and obtain a new stable checkpoint. Never overwrite newer source or force-reset another owner.
- Reconstruct working state in a separate checkout and verify bytes, logical stage, modes, links and recorded deletions. A missing old worktree cannot be reconstructed from Git alone; use hash-bound retained snapshots. Do not invent unrecorded historical changes.
- Required source without a verifiable copy, unexpected archive traversal/special-file behavior, broken object coverage or an ambiguous mixed change stops affected work. FIFO fixtures are captured/restored as type/mode only without opening them.
- Compilation, tests, native-platform failures or confirmed P0–P3 findings remain real failures. Preserve their exact revision/environment and rerun affected checks after correction.
- Product failure/cancellation retains the ordinary publication/containment guarantees. No new age/PID-based deletion authority, cleanup quota or unbounded replay is introduced.
- Delivery failure leaves the owned branch and durable checkpoint recoverable. Reconcile remote state before retry; no force-push of historical approved subjects.
- Rollback of this development work means abandoning only its owned candidate branch. No live-store rollback, historical bulk deletion or automatic downgrade is performed.

## 6. Acceptance and verification

| ID | Requirement / applicable intent | Given / When / Then and method |
| --- | --- | --- |
| REST-A01 | R01/R07/R09 | Given the frozen refs/worktrees/evidence, reconstruct from the bundle and archives in independent locations; then all required entries, stages, modes, links, deletions and commit/ref coverage match. Independently execute checks, including recovered COD-25 inputs. |
| REST-A02 | R02/R09/R13; Graph Store resp-108/109/232/233/234, coordinator resp-277/278/279 | Given the final candidate, compare against both baselines and inspect every old collector caller; then the added collector/enrollment/guards are absent and the PR contains explicit removals. No unfinished replacement or duplicated implementation remains. |
| REST-A03 | R05/R10; resp-108/109/232/233/238/231 | Given isolated Managed/Direct graphs, exercise publication/crash points, concurrent readers/writers, cancellation, owner/guardian loss and failed replacement; then prior graph/pointer/search remain correct and uncertain containment is preserved. Verify ordinary cleanup and reader protection, not just successful builds. |
| REST-A04 | R03/R05; CBG-COPY-01 and R1 | Given native COPY success followed by acknowledgement failure, jumbo valid records and chunked/control input, then fail closed without candidate replay, preserve prior bundles and allow a fresh invocation; compare PK/connector/search and reopened results. No removed workload ceiling returns. |
| REST-A05 | R02/R04 | Given startup, idle, health and build-completion fixtures containing historical state, then no added collector is launched/enrolled. Help, MCP schemas/catalog and public types omit cleanup-only controls; removed CLI options fail normally and unrelated operation parity remains. |
| REST-A06 | R03/R11/R13; CBG-PLAT-01 and COD-103 | Execute retained Windows profile/shim/default-extension, macOS cursor/runtime packaging, installer rejection and release/preflight positive/negative checks on affected final inputs. Native tests/CI establish only their recorded platforms and environments. |
| REST-A07 | R06/R08 | Before the restoration PR handoff, verify original C4 refs/contracts/activation, recovered WIP and the explicit companion order are preserved. After that handoff, each separately owned companion must compare compiler output, IDs/digests, pinned facts, cancellation/resource/error behavior and callers against its reference, reexecuting affected checks under Rust 1.98.1. Those later per-issue gates do not create a circular prerequisite for the restoration PR. COD-25 preservation is not Q1/provider acceptance. |
| REST-A08 | R01/R07 | Given each captured working change and retained source snapshot, then the ledger names its original identity and destination, with original bytes retained and no dropped/unclassified entry. Unknown or missing required source blocks its step. |
| REST-A09 | R08/R10 | Compare original refs, worktree/index state, local governance and live-install metadata before/after; then only declared owned additions/deltas exist. Other pending architectural work and closure owners remain unchanged. |
| REST-A10 | R03/R05/R11 | On the final integrated source run formatting, strict applicable workspace/all-target/all-feature Clippy, workspace/native suites and required Mac/Windows CI. No new warning suppression, ignored coverage or stale executable provenance. Retain execution binaries/dependencies before target reuse. |
| REST-A11 | R08/R09/R13 | Independently reconcile specs, approved amendments, affected Scryer claims, anchors, required callers and executed evidence; then every acceptance result is truthful, review is bound to the final snapshot and no unrelated pending entry is folded. |
| REST-A12 | R11/R12 | Fresh Terra/medium independent execution and fresh prompt-only Sol/xhigh adversarial final review cover the final candidate. Fix/recheck all confirmed P0–P3; verify published branch/tree/parent/PR bytes and immutable approved-spec links. No merge/release/live activation follows. |

Preparation evidence: the owner reconstructed 11 worktrees and four external roots; fresh Terra/medium verification independently checked 13,325 manifest entries, 354 original refs, 13,861 objects, 1,022 protected commits and the 14-input COD-25 snapshot. The bundle SHA-256 is 440e5ccfbac9f023c5e2e2a660d356eb5e62e44cb659a212e7d187af3c7eab99. These are preservation checks, not REST-A02–A12 product passing results.

Limits: 75 worktrees were already missing/prunable. Their unrecorded former working bytes cannot be recovered by this claim. Build caches, live graph payload/state, generated wiki state and prohibited .omx data remain outside archival replay; originals are untouched. Two failed capture/check attempts and their bounded corrections are retained with the checkpoint.

## 7. Open decisions and approval gate

The C4 destination, PR-only endpoint, cleanup-interface removal, support/backend/compiler constraints and restoration topology are settled above. No new GUI is proposed.

The outstanding human decision is approval of the exact committed revision of this package, including its requirement/acceptance mappings, retained-contract precedence and preservation limitations. Product edits, dependent implementer dispatch and claim implementation remain blocked until that decision is recorded. A newly discovered material scope/contract/authority conflict returns to the maintainer rather than being resolved by silent omission.

## 8. Handoff and closure

Canonical backup/evidence remains outside the PR in this chat's durable artifact directory under restore-v304-preservation. Full machine catalogs and raw reports stay there; this package carries the reviewable contracts and disposition rules.

At handoff record the exact approved spec SHA, implemented/reviewed/tested/published snapshot, Linear/PR links, actual commands/platforms/results, independent settings and remaining gates. Update curated wiki through k-wiki MCP with expected hashes and required validation/link/build checks when its relevant facts change. Do not duplicate or migrate historical specs.

Restoration is complete only when its acceptance and PR handoff gates pass. C4 companion work retains separate issue acceptance and fresh coordinators. Preservation, PR creation or merged children do not close COD-125/135, native C4, platform or release aggregates.
