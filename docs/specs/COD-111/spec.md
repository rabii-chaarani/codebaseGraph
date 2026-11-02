# COD-111 — Isolated Git worktree graphs and live refresh

## 1. Identity, status and authority

| Field | Value |
| --- | --- |
| Work | [COD-111 — Specify isolated Git worktree graph routing and live refresh](https://linear.app/codebasegraph/issue/COD-111) |
| Canonical artifact | `docs/specs/COD-111/spec.md`, logical revision **0.2**, 2026-11-02; qualification-method amendment only |
| Status | Revision 0.1 implementation approval retained; revision 0.2 method approved in scope, exact-SHA record pending; **no passing implementation evidence, qualification or closure claimed** |
| Proposal branch / base | `docs/cod-111-worktree-support-spec`; main base `6a98fb0ef4a6bbde02f1556683401eeded49574e` |
| Repository | <https://github.com/rabii-chaarani/codebaseGraph> |
| Architecture | Scryer project `/Users/rabii/Projects/Repositories/codebaseGraph`, historical proposal change `chg-dek36q`; later 19-entry sign-off is recorded in the original approval record |
| Authoring brief | Scryer Repository Runtime Resolver `node-143`, description section “COD-111 — unsigned specification-authoring brief (2026-11-02)” |
| Ownership | Engineer coordinates; Requirements owns behavioral meaning; Architect owns model intent; Designer owns later internals; Production owns Git delivery; Evidence Verifier owns executed acceptance evidence; user owns final approval and aggregate closure |

The original specification-drafting request and D01–D07 are preserved below. Revision 0.1 subsequently received exact implementation approval at full Git SHA `57d89df6f5bb8e10473b8813d61d754ca792ade1`; the [original approval record](approvals/2026-11-02-implementation.md), committed at `92893e1ecd7c943010c2db3ccd61fd4ca272ee4f`, preserves its scope and user decision `c8c59db5-2516-4902-8b0a-d922fa7a0a43`. That approval does not automatically advance to revision 0.2. Historical proposal/pending descriptions below describe the original drafting state; the original approval record supplies the later signed-plan/implementation authority, and the dated amendment below supplies only the new qualification method.

### Amendment history — 2026-11-02

Revision **0.2** changes only COD-111's qualification method. In session `01a19ac7-b463-7200-9829-08d043ecb2b1`, the user directed: “yes you should run the full Windows suite under x64 emulation in the VM. the emulated should be considered as native qualification” (COD-111 decision comment `579d8b2a-b3c8-4796-8e6d-e469ce2a580a`). Successful full applicable Windows x64 suite and applicable COD-111 acceptance execution on the existing local Parallels Windows 11 **ARM64 OS / x64-emulated processes** VM, UUID `7eb56d19-dbd8-4a09-b6e8-85bf4f08a5a0`, is accepted as COD-111 Windows qualification, including A12. It must not be labeled an actual native x86_64 OS execution. This expressly supersedes earlier native-only/pending-native-gate wording for COD-111 alone; no additional native x86_64 host gate remains for this issue after successful required execution by this method. Full applicable local macOS ARM64 execution remains required at the same final source snapshot; hosted runs do not substitute for either local run.

COD111-R01–R13, all observable A01–A12 outcomes, D01–D07, public/source contracts, exclusions, feature architecture/claims, existing work identities and closure owners are unchanged. Direct method impact is A12 (R02/R04/R06/R07/R13); the same Windows method applies to all other applicable COD-111 acceptance and full-suite evidence without changing their oracles. Linked-issue impact is COD-111 only: no other issue, CBG-PLAT-01 requirement/qualification method or aggregate closure is amended or satisfied. Production must freeze revision 0.2 and record its exact full approved SHA, path, narrow scope and this actual user decision before dependent Windows verification; no containing SHA or completed test result is asserted here.

### Settled decisions

Decision references are comments on COD-111. The full comment IDs are stable references; the issue remains the delivery index.

| Decision | Approved direction | User decision record |
| --- | --- | --- |
| D01 | One shared project MCP URL; immutable explicit selection on each worktree graph request; omission preserves canonical behavior. Invalid explicit selection fails, never falls back. No global/session selected-worktree switch. | `62082595-3b5d-4841-800a-8a32000bc743`: “yes for the specification to use the four recommended choices” |
| D02 | Project-owned, isolated per-tree state outside disposable source worktrees. | Same first decision record |
| D03 | Explicit opt-in registration/activation; queries, hooks and discovery do not activate trees. | Same first decision record |
| D04 | Initially serialize graph-producing builds across the project family, keeping published-generation reads available. | Same first decision record |
| D05 | Apply project scan/build/refresh settings to each selected root; no independent per-tree policy overrides initially. | `524e3193-ad01-468b-85de-4ada44fcd9d1`: “i approve the recommendation, update COD-111” |
| D06 | Durable registrations/active intent resume after daemon restart; detach stops monitoring/new work and retains state; unavailable or changed identity fails clearly and requires explicit revalidation/re-registration where changed. | Same second decision record |
| D07 | First-party transport-neutral CLI/API/MCP selection, plus existing trusted-root hook propagation for already registered trees. External callers supply selection. | Same second decision record |

### Canonical contracts retained

These original wiki concepts remain authoritative at their existing locations; this proposal does not migrate or copy them:

- [Graph Runtime Architecture](../../../knowledge/architecture/graph-runtime.md): transport-neutral operations, coordinator admission, sessions and no ambiguous replay.
- [Graph Freshness and Recovery](../../../knowledge/architecture/graph-freshness-recovery.md): pinned identity, dirty epochs, supervised retry, source selection and truthful freshness.
- [Graph Storage Lifecycle and Recovery](../../../knowledge/architecture/graph-storage-lifecycle.md): immutable generations, read leases, atomic publication, worker containment and recovery.
- [Automatic graph disk cleanup contract](../../../knowledge/architecture/graph-disk-cleanup-contract.md): owned-state confinement and conservative deletion authority; retain its original revisions and approvals.
- [Agent-Loop Hooks](../../../knowledge/architecture/agent-loop-hooks.md): trusted local hooks, advisory output, bounded deadlines, fail-open behavior and no graph ownership.

Where historical wiki prose describes resource ceilings removed by other work, this proposal does not restore those ceilings or alter current inner-build settings. Scryer remains the sole architectural authority. Detailed execution briefs, file ownership and prerequisites remain in Scryer, not this specification.

## 2. Problem and observable outcome

An agent can edit an isolated linked Git worktree while querying a project MCP endpoint that still serves the canonical checkout. The graph and watcher selected by that endpoint do not follow the agent's working directory. Canonical and linked trees can have different uncommitted content even at the same commit.

Desired outcome: an opted-in linked worktree has its own graph and live refresh, selected explicitly through the same project endpoint. Concurrent agents can interleave canonical, tree A and tree B requests without changing one another's selection. Every selected graph reflects only that tree's eligible source content. A mistaken or obsolete selector cannot silently return a different tree's results.

### Diagnosis evidence and limits

The source study inspected **`9ae63f3302bf4cae90ebd5a56173fff341ca815b`**, not this proposal's main-base source and not proof of installed behavior:

| Historical anchor at study SHA | Finding |
| --- | --- |
| `src/adapters/mcp/tools.rs:120–150`; `src/coordinator.rs:1003–1008` | MCP operations use startup selection; coordinator pins that selection. |
| `src/adapters/mcp/daemon.rs:189–235,309–345` | Daemon resolves and starts one configured source context. |
| `src/api/refresh.rs:2592–2609` | Refresh follows the selected runtime root. |
| `src/api/context.rs:837–845,821–834` | Linked-worktree `.git` files are accepted; ancestor configuration discovery can still select a parent installation. |

The study checkout was 29 in-flight commits ahead of proposal base main when prepared. Those commits are not imported into this proposal. Installed daemon metadata reported version 4.0.1 and may lag either source snapshot. No end-to-end worktree test, executed acceptance result or performance measurement exists for this feature. Source-supported separate-endpoint workarounds were not executed. Database/cache size observations motivate avoiding eager copying; they are not acceptance quotas or throughput targets.

## 3. Scope and constraints

### Included

- The installed project's canonical working tree and explicitly opted-in **linked Git worktrees of that same project**. A project family comprises those trees sharing the project's Git repository, with distinct working-tree/admin identities.
- Linked trees outside the canonical directory, nested linked trees and `.git` file layouts, detached HEADs and multiple trees at the same commit. Branch names, commit SHAs and Git common-directory identity alone cannot identify a tree.
- Explicit registration/activation, retaining detach, durable active intent, per-request routing, isolated graph/refresh/recovery state, shared effective settings and family build admission.
- First-party CLI, embedded/public API and MCP selection; existing supported local hook adapters carrying trusted worktree identity when available.
- The existing two-target scope from [CBG-PLAT-01 revision 0.11 at its approved SHA](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md): current stable macOS on Apple Silicon (ARM64), and Microsoft-supported Windows 11 x86_64 client releases, including their normal path and permission rules. Preserve that contract’s qualification/closure ownership and exclusion of Linux, Intel macOS and Windows ARM64 expansion; stale four-target prose is not support authority. The COD-111-only qualification-method exception in the amendment history accepts the specified Windows ARM64 VM running x64-emulated product/tests as Windows qualification when the full applicable suite and acceptance pass; it does not add a Windows ARM64 product target/support or alter CBG-PLAT-01/global qualification and closure. Qualification is separate and pending; no platform pass is claimed.

### Excluded / unchanged

- Arbitrary clones, unrelated roots, cross-project routing, remote worktree services or a GUI.
- Automatic registration, activation by query/hook/discovery, watching every `git worktree list` result, or automatic inference of an HTTP caller's cwd.
- A global/session “active worktree”, automatic integration with arbitrary external runners, new client trust bypasses or automatic request replay.
- Eager canonical database copies, cross-tree cache sharing, new graph-engine/format work, independent per-tree policy overrides or performance optimisation targets.
- Automatic deletion of detached/missing-tree state, new retention periods, disposal commands or stronger cleanup authority. Existing generation/run cleanup remains governed by its original contract.
- Migration or reinterpretation of custom Direct destinations and their path-derived journal/lock identities; schema-v1 compatibility and explicit reinstall requirements remain unchanged.
- New RSS/memory ceilings, inner-build setting changes, new queues/caps/timeouts, unrelated release/installer work, live installation/restart changes or separate wiki-connector worktree registration.

## 4. Requirements

IDs are stable within COD-111. “Canonical” means the existing project-default context when no worktree selector is supplied. “Active” means explicitly opted in for serving/monitoring, subject to readiness and configured refresh policy; it does not mean an initial graph is already built or fresh.

| ID | Requirement / observable result | Decision |
| --- | --- | --- |
| COD111-R01 | On each graph-bound first-party worktree operation, bind its explicit selection for the entire operation. Shared URL, session, caller interleaving or another request cannot retarget it. Omission keeps existing canonical semantics. Invalid explicit selection fails without canonical/other-tree fallback. | D01, D07 |
| COD111-R02 | Register only actual linked trees in the installed project's family. Validate the selected working-tree identity independently of branch/HEAD/common-dir alone. Detect obsolete, moved, removed or substituted identity before using it as another tree. No ancestor config or equivalent path spelling may silently change the source context. | D01, D03, D06 |
| COD111-R03 | Create/activate a worktree context only through explicit opt-in lifecycle action. Discovery and graph queries/hooks have no activation side effect. Distinguish registration/activation success from initial graph readiness. | D03 |
| COD111-R04 | Keep each opted-in tree's graph, manifest/generation, coordination, refresh and recovery ownership isolated in project-owned state outside disposable source roots. A build, failure, detach or deletion of source files in one tree cannot publish into another tree's state. | D02 |
| COD111-R05 | Apply the project's effective scan/build/refresh settings to the selected root, retaining existing source-selection, ignore-file, generated-state, reload and explicit-override semantics where applicable. Relative source selection is evaluated against that root. A local/ancestor installation cannot redirect it or introduce an independent per-tree policy. | D05 |
| COD111-R06 | Persist explicit registration and active intent across daemon restart. Resume active contexts under the configured refresh policy after identity validation; detached contexts remain inactive. Missing/unreadable/changed identities report their blocked reason without activation, fallback or deletion authority. Changed identity requires explicit re-registration/revalidation. | D03, D06 |
| COD111-R07 | Explicit detach stops monitoring and admission of new work for that context, preserves graph/recovery state, and does not affect other contexts. Apply existing cancellation/drain, containment, publication and lease safety to in-flight work; detach cannot report completion while its monitoring/build ownership remains live. Only explicit activation resumes it. | D02, D03, D06 |
| COD111-R08 | Serialize graph-producing builds targeting canonical or registered-tree project state across automatic refresh, one-shot/manual CLI and explicit API routes. Preserve ordinary single-destination ownership too. Retain existing explicit busy/no-replay and refresh pending/retry semantics; pending tree changes are not acknowledged merely because another tree builds. Do not add a waiting graph-operation queue. | D04 |
| COD111-R09 | Do not hold a build-wide exclusion that blocks reads of previously published generations. Reads keep their selected tree and generation lease during builds/publication. Existing graph execution admission, `graph_busy`, deadlines and native-read completion behavior still apply; this is not a promise that every simultaneous read is admitted. | D04 |
| COD111-R10 | For every active available tree, converge eligible create/edit/rename/delete changes through its own native/poll refresh and periodic reconciliation, preserving dirty work through failure. A failed/cancelled candidate leaves its previous validated generation readable; absent an initial generation, report not-ready rather than fabricated/other-tree data. | D02, D04, D05 |
| COD111-R11 | Expose the effective selected tree identity/root, graph-generation identity when present, readability/readiness and freshness/blocked reason through operation result/status presentation. Do not label a retained generation current without the existing successful-reconciliation evidence. Selection failure identifies the failed selection, not a successful canonical result. | D01, D06 |
| COD111-R12 | First-party CLI/API/MCP carry the same selection semantics. Existing local hooks propagate trusted event root only to an already registered active matching tree; preserve advisory deadlines/fail-open and trust policy. Missing/untrusted/mismatched/unregistered input cannot activate a tree or inject a different tree's context. External callers supply explicit selection themselves. | D03, D07 |
| COD111-R13 | Preserve canonical omission compatibility, existing public graph operations/output modes, Direct destination recovery identity, legacy storage policy, immutable publication, containment and conservative cleanup. Selection support must not create new data-deletion or permission authority or restore removed resource ceilings. | D01, D02, D04–D07 |

### Caller and settings semantics

Selection is an input to one operation, not a session preference. Canonical omission compatibility applies to ordinary explicit callers; it is not permission for a worktree-aware hook to knowingly inject canonical results for a mismatched caller root. Hooks warn or return no context under their existing fail-open contract when they cannot establish an eligible matching selection.

The project remains the settings authority. Worktree-local source/ignore content is still evaluated by the existing source-selection contract against that tree; this is not an independent per-tree policy override. A worktree need not acquire its own ordinary local installation to use the project registration. Configuration repair/reload cannot silently rebind an existing tree's storage/source identity. Source, configuration or permission failure remains visible under the existing retry/recovery contract.

## 5. Lifecycle, failure and recovery

Registration intent, source availability and graph readiness are separate facts. The states below describe observable outcomes, not a prescribed registry schema or new API names.

| Condition / transition | Required behavior |
| --- | --- |
| Unregistered tree is discovered or queried | Report unregistered/invalid selection as applicable; no watcher, graph build or implicit opt-in. Canonical omission remains canonical. |
| Explicit valid registration/activation | Persist opted-in active intent for that exact identity. Report readiness separately. Startup/reconciliation/build uses only that tree's source and project policy. |
| Active, available, no initial graph | Health/status reports no readable generation and pending/not-ready or failure reason. Graph reads fail not-ready; never borrow canonical/another tree's graph. |
| Active, available, prior graph; refresh pending/running/failed | Reads may use its last validated generation with truthful freshness. Dirty changes survive failed/deferred attempts and reconcile through existing retry rules without requiring another source edit. |
| Active tree becomes unavailable, is moved/removed or its identity is replaced | Explicit selection fails with that reason; retain registration, graph and recovery evidence, but do not serve the retained graph as though the current path still denotes the registered tree. Do not fall back or delete. Re-establish availability/identity before normal operation; changed identity requires explicit re-registration/revalidation. |
| Explicit detach, with or without initial graph | Stop monitoring/new work under existing safe teardown rules and retain state. Report detached/inactive on selection; queries do not reactivate it. Explicit activation is required to resume. |
| Daemon stops/restarts | Preserve durable registration/active intent; active validated contexts resume monitoring per project policy. Detached remains detached; unavailable remains visibly blocked. Restart does not prove freshness or initial readiness. |
| Permission/storage/configuration/Git metadata failure or interrupted lifecycle action | Report failure/blocked readiness without claiming completed activation/detach or a new generation. Preserve already committed intent and valid graph/recovery evidence under existing atomicity/containment rules; retry/revalidation observes authoritative current state. Never erase evidence or widen access to force progress. |
| Build cancellation/crash, family admission conflict or unknown containment | Preserve the prior generation and unsatisfied changes. Use existing busy, no-ambiguous-replay, containment and recovery semantics before another conflicting writer. Other trees retain their own identity/state. |

The inactive retaining-detach outcome is derived from the approved explicit-opt-in/stop-new-work direction; it is proposed behavior, not evidence of an implemented feature. Detached status must not claim freshness and does not add an offline archive/query capability. Detach does not authorize purge, nor does disappearance or a path reuse. No time-based retention or special orphan cleanup is introduced. Existing lease-aware retirement and ordinary owned-state cleanup continue only within their already approved authority. Registry persistence, activation ordering and safe shutdown mechanics belong to Designer; they must realize these outcomes without inventing automatic retries of ambiguous lifecycle operations.

## 6. Acceptance and planned verification

**All acceptance below is planned; evidence is pending.** No configured test, source anchor or attached test is a passing execution. Implementation test names/locations and exact commands will be established against the approved implementation baseline, not inferred from historical study line numbers.

Claim references below are genuine existing governing Scryer claims, not an assertion that worktree support is modeled or implemented. **Proposed worktree intent pending** means Architect-owned feature-claim mapping remains a prerequisite before implementation, as explained in section 7. Existing governing contracts/claims provide context only; contract-only rows need no invented claim.

| Criterion | Requirements | Given / When / Then | Claims / contract | Planned method and evidence |
| --- | --- | --- | --- | --- |
| COD111-A01 | R01, R04, R11 | Given canonical+A+B with distinct uncommitted symbols, including two trees at the same commit, when calls interleave on the **same URL and same initialized MCP session**, then each explicit call returns only its selected content/identity; omitted calls remain canonical and no call changes another's selection. | Proposed worktree intent pending; `resp-277`, `resp-278`, `resp-144` | Real MCP process scenario; record requests/session, selected roots, generations and independent source oracle. |
| COD111-A02 | R02, R03, R13 | Given external and nested linked `.git`-file trees, detached HEAD and same-commit trees, when explicit opt-in/queries run, then distinct identities work; unrelated clones/roots and unknown selectors fail without fallback or automatic registration/build. | Proposed worktree intent pending; `resp-144` | Disposable Git family fixtures, before/after owned-state observations and negative membership cases. |
| COD111-A03 | R03, R06, R10, R11 | Given a newly activated tree without a generation, when its first build is pending or fails, then readiness distinguishes activation from readable graph and queries never return another tree's data; after successful publication its own graph is readable. | Proposed worktree intent pending; `resp-269`, storage contract | Initial-build success/failure process fixture; status and publication/state comparison. |
| COD111-A04 | R05, R10 | Given missing local config and a conflicting ancestor/local installation plus tree-specific eligible/ignored files, when registration and project-setting/ignore changes reconcile, then only project policy applied to that root governs content, no source retarget/per-tree override occurs, and unchanged source-selection behavior remains. | Proposed worktree intent pending; `resp-266`, `resp-144` | Source-selection/configuration fixtures including nested roots, project reload, ignore changes and policy off/on; record effective settings/root and content oracle. |
| COD111-A05 | R04, R10, R11 | Given active A+B, when files are created/edited/renamed/deleted independently, then each graph converges to its own eligible source and the other tree remains unchanged. Native and poll paths, missed notifications/pathless rescan and periodic reconciliation preserve this outcome. | Proposed worktree intent pending; `resp-147`, `resp-420kd4`, `resp-269` | Real watcher/process scenarios for both backends; compare source/graph after successful configured reconciliation, not a new latency promise. |
| COD111-A06 | R02, R06, R11 | Given a registered identity with/without prior graph, when its root is removed, moved, substituted at the same path, or Git metadata becomes unavailable, then selection reports the reason without fallback/deletion or treating replacement content as the old tree; explicit identity revalidation/re-registration permits recovery. | Proposed worktree intent pending; identity/storage/cleanup contracts | Identity substitution and Git move/removal fixtures, retained-state snapshots, explicit recovery and path-alias checks. |
| COD111-A07 | R03, R06, R07 | Given active and detached registrations with/without a graph, when daemon restart occurs, then only valid active intent resumes per policy; detached does not resume. When detach meets active monitoring/build, completion proves safe stop and retained state; explicit activation can resume. | Proposed worktree intent pending; `resp-267`, containment/storage contracts | Restart/detach process scenarios, observable watcher/build liveness, retained graph/recovery evidence and other-tree checks. |
| COD111-A08 | R08, R09, R13 | Given canonical+A+B and concurrent automatic refresh/manual CLI/explicit API builds, when they contend, then graph-producing execution never overlaps across family contexts, ordinary ownership holds, explicit busy/no-replay and pending refresh semantics remain, and selected prior-generation reads are not held behind a build-wide lock. | Proposed worktree intent pending; `resp-2nrnv4`, `resp-nehrn9`, storage read leases | Controlled build barriers across entrypoints with actual start/finish/publication observations; admitted-read lease/content evidence and honest `graph_busy` outcomes. No timing-only overlap oracle. |
| COD111-A09 | R04, R08–R11 | Given a prior valid generation and dirty edits during a build, when candidate construction fails/cancels or owner/worker dies, then the old generation remains valid, unsatisfied edits remain pending, recovery respects containment and later publishes the correct tree without another edit. A fresh tree instead stays visibly not-ready. | Proposed worktree intent pending; `resp-147`, `resp-279`, recovery/storage contracts | Failure-injection and process-death scenarios; active-pointer/content, dirty epoch and teardown evidence before/after retry. |
| COD111-A10 | R01, R03, R12 | Given equivalent first-party CLI/API/MCP selections and trusted local hook events for registered trees, when they query, then identities/content agree. Missing/untrusted/unregistered/mismatched hook roots, daemon unavailability, busy execution and timeout fail open without activation or cross-tree context. | Proposed worktree intent pending; hook contract | First-party contract tests and existing supported local-hook adapter fixtures; bounded advisory output and no-side-effect observations. External caller supplies selector explicitly. |
| COD111-A11 | R11, R13 | Given existing canonical calls, output modes, Direct recovery destinations and legacy storage, when selection support is present, then omission semantics and existing recovery/legacy outcomes remain unchanged, direct path spelling/journal/lock identities are preserved, and no graph-state migration/deletion or new ceiling occurs. | `resp-144`; existing public/storage contracts | Compatibility regressions including interrupted Direct publication and canonical health/search/context/query; inspect real state and errors, not option-string presence alone. |
| COD111-A12 | R02, R04, R06, R07, R13 | Given supported macOS/Windows paths, equivalent aliases, permission denial, symlink/traversal escape attempts and uncertain ownership, when registration/lifecycle/read/recovery executes, then identity is unambiguous, owned state remains confined and inaccessible/foreign state is not silently used or deleted. | Proposed worktree intent pending; identity/cleanup/containment contracts | Local macOS ARM64 and Windows x64 fixtures, permissions/path controls and retained-state evidence at the same final source snapshot. For COD-111 Windows qualification, use the approved existing local Windows 11 ARM64 VM with x64 emulation under the amendment-history method; successful full applicable Windows suite plus applicable acceptance, including this row, satisfies the gate without an additional native x86_64 host run. Record actual OS/process architecture and unsupported/unavailable checks as pending, not passes. |

Every requirement is covered: R01→A01/A10; R02→A02/A06/A12; R03→A02/A03/A07/A10; R04→A01/A05/A09/A12; R05→A04; R06→A03/A06/A07/A12; R07→A07/A12; R08→A08/A09; R09→A08/A09; R10→A03/A04/A05/A09; R11→A01/A03/A05/A06/A09/A11; R12→A10; R13→A02/A08/A11/A12. No acceptance row authorizes unrelated implementation or expands platform support.

Evidence for each executed criterion must identify approved spec revision, actual tested source/base/full SHA and content/artifact identities, environment/toolchain/actual OS and process architecture (explicitly ARM64 OS / x64 emulation on Windows), exact command or method, fixture/oracle, observed result, report location, applicable claims and limitations. Execute the full applicable suite, including all applicable COD-111 acceptance, locally on both macOS ARM64 and the specified Windows VM at the same final source snapshot; hosted execution cannot substitute. Inventory actual tests, profiles, features, compiler warnings, failures, ignored/skipped/inapplicable cases with reasons, unavailable and unrun checks; no narrower selection or missing work is silently a pass. Preserve failures and distinguish method approval, local execution, supported-platform qualification and aggregate closure. Exact implementation anchors and genuine JUnit ingestion/claim reconciliation, where required by Scryer, follow implementation; none exist for COD-111 now.

## 7. Architecture and later internal-design prerequisites

Existing governing hosts: Repository Runtime Resolver `node-143`, Repository Refresh Service `node-144`, MCP Server Adapter `node-35`, Public API Contracts `node-96`, Graph Coordinator `node-209`, Graph Store `node-88`, Repository Lifecycle Service `node-136`, Agent Hook Adapter `node-7mkzrq`. Existing claims cited above retain their identities. The authoring brief belongs to unsigned `chg-dek36q`; it is neither product-model sign-off nor implementation evidence.

At this proposal handoff, `chg-dek36q` contains only the `node-143.description` documentation-authoring brief delta. No feature responsibility has been added or reworded, and no authored worktree feature-model/claim mapping exists. The prior existing-host architecture proposal provides context, not an authored feature model. Architect-owned minimal worktree intent and requirement/acceptance-to-claim reconciliation remain pending **before implementation**, alongside internal design. This does not prevent handoff of the complete behavioral specification proposal or create another approval/review stage. No symbol, topology or claim is invented to populate acceptance tables. Detailed implementation work is not assigned by this specification.

The following are **internal design prerequisites**, not reopened user decisions:

- Exact public selector/lifecycle request spelling, error types and result/status placement, preserving transport-neutral meaning and compatibility.
- Git/path identity representation and validation, durable registration/active-intent mechanics and safe activation/detach ordering.
- Isolated project-owned state allocation, configuration provenance and recovery across restarts without source-root retargeting.
- Family-wide admission across actual entrypoints, its relationship to per-context coordinator/store/worker ownership, and preserved read admission/leases. No new queue or automatic ambiguous replay.
- Existing hook trusted-root extraction/propagation, adapter-specific inability reporting and no mismatched context.
- Exact implementation baseline/approved dependencies, files/owners, meaningful test inventory, native qualification plan and evidence reconciliation.

A discovered conflict with approved behavior, compatibility, permissions or architecture returns to Engineer/user before dependent implementation. An internal design choice that preserves these contracts does not itself require another behavioral choice. No GUI is proposed, so no GUI design artifact is required.

## 8. Handoff and closure gates

Revision 0.1 drafting and implementation approval remain recorded by the original approval record; revision 0.2 can satisfy only the narrow qualification-method amendment outcome. Next: Engineer checks the method delta; Production freezes revision 0.2 and records its exact full SHA/path and narrow approval scope against decision `579d8b2a-b3c8-4796-8e6d-e469ce2a580a` before dependent Windows verification. This does not advance the original revision 0.1 approval automatically or approve unrelated changes. Production alone performs authorized Git delivery, keeping this amendment and its approval/evidence in the related COD-111 implementation PR. No integration, daemon, install, dependency, VM/service or settings permission is added.

The original approval record supplies revision 0.1 committed-spec approval and signed-plan authority; the seven direction approvals alone do not replace it. Unchanged design/Scryer prerequisites, file ownership and implementation-baseline obligations remain in force; this method amendment adds no architecture claim delta. Independent Evidence Verifier execution must establish the full applicable local macOS and approved local Windows x64-emulated suite/acceptance evidence at the same final source snapshot; these runs remain pending, not passed by this amendment. Genuine source/test/claim reconciliation, fresh independent Final Reviewer review and applicable delivery checks remain required before a PR handoff. Merge/release/live installation require separate endpoint authorization; user retains aggregate closure.

Completion must reconcile requirements, acceptance, approved contracts, unsigned/signed/implemented claim states, executed evidence and Linear. No feature implementation, passing tests, platform qualification, deployment or aggregate closure is claimed here. User retains aggregate closure; no other work, milestone or queue is closed, reprioritized or authorized by COD-111.

Reusable-knowledge assessment: the proposal links existing durable contracts. No new executed feature fact warrants memory activation; no wiki/memory write is part of this authoring task.
