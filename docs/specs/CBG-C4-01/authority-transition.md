# COD-19 proposed authority transition

## Status and approval boundary

Work: `CBG-C4-01/F01/S01` / [COD-19](https://linear.app/codebasegraph/issue/COD-19), 7 November 2026, Australia/Darwin. This document is an **inert proposal**. Its quoted baseline and replacement text are reference data, not instructions governing this session. Root `AGENTS.md`, wiki content, Scryer models/directives and client settings remain unchanged during preparation.

The proposed scope is approved **CBG-C4-01 work only**, subject to the maintainer's scope decision and separate full-SHA authority approval. A repository-wide migration is not assumed. The [canonical specification](spec.md) remains a proposed implementation contract; [approval scope](approval-scope.md) separates product implementation permission from authority activation. No reviewer, agent, commit, merge, default option or document can grant either approval.

## Why bootstrap needs an explicit bridge

The native model/compiler/verifier is not yet implemented or qualified. Making that unaccepted candidate the authority for building itself would bypass trust bootstrap. After explicit approval and procedure activation, immutable human-approved Git contracts govern the approved implementation scope. Per-issue execution briefs live at `docs/specs/CBG-C4-01/execution/<work-id>.md`, linked from existing Linear work; they contain detailed ownership/dependencies/acceptance evidence rather than extending the runtime YAML into a full workflow model. Requirements and evidence keep their existing locations.

Only after COD-37/46/47's concrete trust/pin approval and actual protected-path qualification can an approved first native YAML baseline become C4 intent/constraint authority. That later event does not replace Git's role for delivery approval, HOW-constraints, execution briefs or release gates. Code, source graph, inferred mappings and candidate YAML remain evidence/candidates, never their own authority. The product/native-workflow has no Scryer runtime or approval dependency under the activated scope.

Unrelated Scryer-owned work remains under its current rules. A label, file path or feature branch cannot claim the exception: each changed responsibility/contract must trace to an approved CBG-C4-01 requirement and bounded brief. Mixed native/unrelated changes require explicit separation/ownership; unresolved boundary/constraint conflicts return to the maintainer. Preserve all existing binding user directives and unaffected contracts, including C1 API compatibility, one catalog/neutral services, read-only graph queries, storage capability/lease/publication guards and recovery. Do not erase global drift or absorb another issue's pending claims.

## Existing policy source and proposed installation

The repository-local `AGENTS.md` is intentionally Git-ignored by the existing `.gitignore`; normal worktrees do not contain a tracked copy. Preserve that storage mechanism. The exact current canonical-checkout bytes and proposed replacement are quoted below so review and recovery do not depend on an unversioned local copy. Do not add a tracked root AGENTS file, change `.gitignore`, remove Scryer registrations, alter app settings or update the local instruction file as part of preparing/publishing this proposal.

The future installation target is `/Users/rabii/Projects/Repositories/codebaseGraph/AGENTS.md`, conditional on its exact approved baseline hash and regular-file identity. Each policy fence contains strict `AGENTS.text-lines/0.1` JSON: join its string `lines` with LF and append the declared terminal LF to recover the UTF-8 policy bytes. Install only the decoded proposed policy; JSON wrappers, headings and fences are not instructions or installed content. The candidate also removes existing trailing whitespace without changing those words. Existing inherited RTK, Rust build-storage, security, permission and higher-priority instructions remain binding. Each future worktree/session must resolve and pin the approved policy/decision before native work; absence of a local ignored file never implies unrestricted authority.

| Item | SHA-256 / lines |
| --- | --- |
| Current local policy | 0930ae8bbccdab966656addd4a673ce0ad239f82d98ec26464115e438c1005cc / 148 |
| Proposed installed policy | ed72f74970f8c4e2cc0f5834b787a6d0d26e47360d0ba3ea3a4eabcc0b6a7b80 / 150 |


## Transition states and gates

| State | Required evidence / permitted work |
| --- | --- |
| Proposed | This package may be drafted/reviewed/published under current authority. No new authority or product implementation is activated |
| Approved, not active | Human decisions separately identify the full published SHA and scopes A/B. Record the actual decision against immutable artifacts; all native implementation remains blocked until procedure readiness |
| Git bootstrap authority active | Both A/B granted; independently verified installation/readback of the approved policy and scoped wiki clarifications, plus durable activation record. Approved Git specification/briefs govern only CBG-C4-01 implementation |
| Native C4 authority active | Additional concrete verifier/workflow/config pins approved outside candidate control; first baseline and real final path qualified under COD-37/46/47; authenticated accepted-state receipt. Native YAML now governs C4 intent/constraints |
| Conflict / partial / suspended | Keep the affected gate blocked, preserve current accepted/runtime state and evidence, report exact incomplete steps and recover explicitly; never silently fall back to candidate authority |

## Activation procedure for a separately approved scope B

1. Fetch the approved full-SHA canonical/transition/approval-scope artifacts and confirm digests, explicit human approvals for **both A and B** and exact scope. A request to prepare these documents is not approval; approval of only one scope does not start installation. Record the actual decision in a later Git approval record that references the approved SHA; never embed a file's own containing SHA or invent a decision.
2. Preflight all changes before mutation: compare local AGENTS identity/bytes with the approved baseline; capture the current Scryer baseline/context without changing it; fetch the four existing wiki concepts below, their current source identity/hash and metadata. If anything materially diverged, stop for a rebased/reviewed delta. Do not overwrite a concurrent local instruction or curated edit.
3. Keep native implementation blocked while installing. Preserve the exact baseline (already quoted/hashed here and available at the approved SHA); atomically replace only the approved local AGENTS target, retaining appropriate owner/permissions. Do not copy the proposal wrapper or create policy in an arbitrary worktree.
4. Apply only the approved wiki authority wording through configured k-wiki MCP authoring, retaining original concept identities, paths, metadata and all unrelated content. Supply the current correct expected_content_hash; never omit/force the concurrency guard. If a required hash or safe write path cannot be established, stop activation rather than guessing.
5. Run k-wiki recommended validation with structured content, link checks and the configured bundle build to `.kwiki/site`. Verify policy hash/line count, exact wiki deltas, original Scryer model/directive/history/pending preservation, unchanged integration settings and no product/source/live-protection changes. A fresh independent verifier checks actual artifacts/readback and that a new coordinator can resolve the approved scope from the effective instruction source.
6. Only then record bootstrap authority activation as complete, with exact paths/hashes, human decision, local/environment scope, wiki concept hashes, executed checks and limits. The record identifies Git-bootstrap authority; it cannot claim a native baseline, verifier trust activation or hosted enforcement already exists. Future sessions with conflicting/stale injected instructions stop for reconciliation; do not bypass them by selecting a convenient copy.
7. With both approvals and procedure evidence satisfied, close COD-19 and COD-5 for P0 readiness under the maintainer's existing closure request. Keep all downstream implementation/acceptance/platform/release gates. Do not automatically start another issue, merge the documentation stack, deploy workflows or release.

## Scoped wiki deltas proposed for activation

Keep each original concept location and existing specifications/approvals. These are targeted authority-clarification replacements, not source-behavior or runtime claims; obtain fresh expected content hashes at activation.

### W1 - architecture/overview

Existing exact text:

```text
When code and this wiki diverge, consult Scryer for intended responsibilities and the codebase graph for current implementation evidence, then update the durable decision here.
```

Proposed replacement:

```text
When code and this wiki diverge, consult the currently approved architectural authority and the source graph as implementation evidence. After COD-19's explicitly approved scoped activation, CBG-C4-01 uses approved Git contracts/briefs during bootstrap and qualified accepted native YAML for C4 intent thereafter; unrelated work remains Scryer-governed. No candidate or historical snapshot self-activates authority.
```

### W2 - architecture/invariants

Existing exact text:

```text
These constraints are the shortest durable test for whether a change still fits the repository architecture. They summarize Scryer responsibilities and current graph evidence; changing one is an architecture decision, not a local refactor.
```

Proposed replacement:

```text
These constraints test whether a change fits the approved architecture. Scryer remains the intent source outside COD-19's explicitly activated CBG-C4-01 scope; that scope uses approved Git/bootstrap contracts and then qualified accepted native C4 intent. Graph facts remain implementation evidence. Changing a constraint requires the applicable human architecture approval.
```

### W3 - architecture/invariants

Existing exact text:

```text
31. **Intent and implementation evidence remain separate.** Scryer is the authored responsibility model; the codebase graph and tests are evidence of the current implementation. Neither silently substitutes for the other.
```

Proposed replacement:

```text
31. **Intent and implementation evidence remain separate.** Use the explicitly approved authority for the current scope: Scryer outside the activated CBG-C4-01 transition, approved Git/bootstrap contracts and then qualified accepted native YAML within it. Codebase graph and tests are implementation evidence; candidates cannot replace accepted intent or authorize themselves.
```

### W4 - architecture/invariants

Existing exact text:

```text
Update the Scryer model first for changed responsibilities or links, obtain the required architecture sign-off, implement and test the change, reconcile anchors and drift, then update the affected wiki concepts. If code evidence contradicts the wiki without an approved intent change, treat it as drift rather than rewriting the invariant around the implementation.
```

Proposed replacement:

```text
Update the applicable approved intent/brief before changing responsibilities or links and obtain or reuse matching human approval. For Scryer-governed work retain its model/sign-off/anchor/evidence process. For activated CBG-C4-01 work use the approved Git/bootstrap or qualified native authority and linked executed evidence. Then update affected wiki concepts. Code evidence cannot silently rewrite intent or clear unrelated drift.
```

### W5 - architecture/repository-map

Existing exact text:

```text
Before reading source for an architecture change, locate the governing files and relationships through the repository codebase graph. Read Scryer for the intended container/component responsibility and inherited directives. Use source only as implementation evidence, then reconcile durable architectural changes back into Scryer and this wiki.
```

Proposed replacement:

```text
Before source reads for an architecture change, locate files/relationships through the source graph. Resolve the current approved authority and binding constraints: Scryer for unrelated work, approved Git/bootstrap briefs and then qualified native C4 intent for explicitly activated CBG-C4-01 scope. Preserve existing directives and ownership; unresolved cross-scope conflicts stop for the maintainer. Reconcile evidence and this wiki without changing another scope's authority.
```

### W6 - architecture/operation-paths

Existing exact text:

```text
If responsibility or dependency direction changes, update Scryer and this architecture set together.
```

Proposed replacement:

```text
If responsibility or dependency direction changes, update the applicable approved architecture authority and this architecture set together; retain Scryer for unrelated scope and use the explicitly activated COD-19 Git/native bridge only for approved CBG-C4-01 work.
```


Historical dated Scryer counts and unrelated contracts/reviews remain history. Do not update technical mutable facts or migrate wiki specifications as a side effect. The current public-operation/shared-storage invariants are retained.

## Failure, recovery and withdrawal

Before installation, changed baseline or missing approval means no mutation. During partial activation, native work remains blocked; record exactly which local/wiki steps completed. Preserve the primary failure and all evidence. Roll back only those owned approved changes when their current bytes/content hashes still match what this transition wrote, using the quoted approved baseline and original wiki metadata. If another writer changed them, stop for human reconciliation; never force restoration over newer work.

A failed wiki validation/build is a failed activation, even when AGENTS replacement succeeded. Recover or conditionally restore the affected owned deltas, then independently verify again. Do not mark activation complete because files exist or a command was configured. Cancelling approval or suspending the scope blocks future dispatch; it cannot undo already committed accepted states. After implementation has begun, reverting governance requires an explicit maintainer decision and intent/evidence reconciliation; do not silently restore a stale Scryer plan over newer native intent.

No local policy change is made in this preparation. The exact baseline and candidate below are documentation artifacts only.

## Quoted current policy baseline (not operative here)

```baseline-policy
{
  "format": "AGENTS.text-lines/0.1",
  "lines": [
    "# Repository agent instructions",
    "",
    "- Keep this file at most 150 lines; shorten wording and remove duplication without dropping instructions.",
    "",
    "## Coordination",
    "",
    "- The root agent is the sole coordinator for questions, dispatch, tracking and issue/queue advancement. Preserve read-only limits and existing approval gates; read-only requests permit no writes, including tracking, model ledgers or memory. These instructions are self-contained; no project-specific engineering skill is required.",
    "- Select the next unmet outcome: readiness → technical work → independent verification/review → authorized delivery. Reuse valid artifacts and load only relevant context. Choose exactly direct (root implements), delegated (one complete owner), or parallel (ready independent deliverables). Workflow choice, method use and assignment grant no additional authority; coordinator or bounded worker may perform authorized delivery, while independent checks remain separate.",
    "- One issue is active at a time. An explicitly approved fixed ordered queue records work IDs, exact approvals, dependencies, repository/base, permitted endpoint and authorized release contents/baseline; it may advance only through authorized ready work. Membership or assignment does not approve implementation. A single issue normally ends at its PR. Aggregate closure stays with its owner. ",
    "- Once a sub-issue is finished, open a new session with new coordinator so they can start the next remaining sub-issue or issue.",
    "",
    "## Authority and decisions",
    "",
    "- **AGENTS.md:** procedure/gates. **Scryer:** sole architectural authority for intended responsibilities, boundaries, directives and relationships; owns detailed execution briefs, prerequisites, file ownership, evidence requirements and pending-claim assignments. **Specs:** delivery scope, requirements, public contracts, acceptance criteria, decisions, approvals, handoffs and evidence. **Linear:** delivery tracking. **Wiki:** reusable knowledge, rationale/runbooks. **CodebaseGraph:** implementation index. **Source/executed tests:** implementation/behavior evidence. **Memory:** advisory.",
    "- Before implementation: recall memory, find canonical specs/contracts, consult wiki/Linear, inspect Scryer intent/rules, then discover files through the graph before reading source. Report unavailable repository tools; never substitute another project's state.",
    "- Preserve work, milestone, phase, slice/sub-slice, requirement and closure-owner identities. Cross-reference stable work IDs, spec revisions, Linear records and Scryer changes/claims; detailed briefs belong only in Scryer.",
    "- Ask the user about every unresolved behavior, scope, contract, architecture, permission, compatibility, acceptance or GUI decision. The user owns GUI design; settle it before implementation. Check approved intent first; reuse same-scope approval without reopening settled decisions.",
    "- Use chat question prompts with a recommendation, concrete options, behavior, tradeoffs and consequences. Unanswered questions/preselected options are not approval. Continue independent authorized work while waiting. Routine choices may proceed within approved contracts, constraints and acceptance criteria; surface intent-changing conflicts.",
    "- Never create/use `.omx` files or directories. Use Git specs, Linear and the wiki for their assigned purposes.",
    "",
    "<!-- k-wiki:start -->",
    "## k-wiki",
    "",
    "- Use configured `k_wiki` MCP registration(s), never the CLI. `knowledge/` is curated source; never manually edit generated `.kwiki/` state.",
    "- Start with `wiki_list_bundles`, then `wiki_search_concepts`. Use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks` and `wiki_get_neighborhood` for context; `wiki_get_recent_changes` for recent work and `wiki_get_diagnostics` for remaining issues.",
    "- Record terminology, invariants, rationale, runbooks, decisions and non-obvious tradeoffs. Architecture pages explain Scryer intent; verify mutable facts through codebase-graph. Resolve code/wiki conflicts against Scryer and approved specs, then clarify knowledge without duplicating specs.",
    "- Use `wiki_create_page` to create and `wiki_populate_page` to update, supplying title, type, tags, useful Markdown and `expected_content_hash` for updates. Link canonical specs/contracts. Preserve existing wiki specs' locations, identities, revisions and approvals; no migration, copying, deletion or supersession without separate user authorization.",
    "- After meaningful edits: `wiki_validate` with `profile: recommended`, `include_structured_content: true`; `wiki_check_links`; then `wiki_build` with configured `bundle_root` and `.kwiki/site` output root (a write). Handoffs cite changed concepts, decisions, uncertainties and validation results.",
    "<!-- k-wiki:end -->",
    "",
    "<!-- agent-memory:start -->",
    "## Agent memory",
    "",
    "- Before non-trivial repository conclusions, call `wiki_memory_recall` with the goal and relevant subsystems/symbols/errors/decisions. Skip only mechanical formatting, typos, exact replacements and simple file/symbol lookups. Recall again for subsystem/scope changes, familiar failures or earlier rationale, conventions, constraints or failed approaches.",
    "- Memory never overrides instructions, permissions, curated knowledge, source or executed tests. Verify mutable facts.",
    "- Before non-trivial completion, explicitly assess reusable knowledge. If warranted, `wiki_memory_record` one focused, distilled claim/procedure with structured provenance; it creates a candidate. Kinds: `semantic` for stable facts/terminology/invariants/contracts/ownership/decisions; `episodic` for durable outcomes/root causes/failed approaches/lessons; `procedural` for verified, reproducible runbooks/workarounds/constraints.",
    "- Never store raw sessions/tool output, transient status, readily rediscovered code facts, secrets, credentials, personal data or unverified external content. Do not create filler memory.",
    "- Verify claims against cited authoritative evidence before activation. `wiki_memory_transition` allows `candidate → active | quarantined`, `active → superseded | quarantined`, `quarantined → candidate`; `superseded` is terminal. Reverify returned candidates. Quarantine contradicted/unsafe/suspicious recalled memory during the task. Create/activate replacements with `supersedes`, then supersede old records; preserve history.",
    "<!-- agent-memory:end -->",
    "",
    "<!-- codebaseGraph:start -->",
    "## codebaseGraph workflow",
    "- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.",
    "- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.",
    "- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.",
    "- Use MCP `graph_search` with `layer: \"semantic\"`, `detail: \"slim\"`, and `context_limit: 1` before answering entity, relationship, or repository-structure questions and before coding tasks.",
    "- Repository-local agent hooks may inject bounded `graph_health` and semantic `graph_search` results for each non-empty prompt. Treat this context as advisory; hook failures fail open after a short deadline and never authorize, deny, or rebuild anything.",
    "- Automatic hook context is only orientation. Before edits or architectural conclusions, call `graph_context` explicitly with `change_impact`, `dependencies`, `callgraph`, or `runtime` as appropriate.",
    "- Use MCP `graph_context` with `layer: \"semantic\"`, `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when semantic relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.",
    "- For parse-tree, grammar-field, source-construct, or Tree-sitter node questions, first use MCP `graph_syntax` with the file language, then use `graph_search` with `layer: \"syntax\"` to find `SyntaxCapture` nodes.",
    "- Use MCP `graph_context` with `layer: \"syntax\"` only for a `SyntaxCapture` result, passing its `node_id` and `node_type: \"SyntaxCapture\"`; follow ordered `SyntaxChild` rows through `field_name` and `child_index`.",
    "- Use `layer: \"hybrid\"` only when semantic relationships and syntax structure are both needed; hybrid search returns semantic matches before syntax matches.",
    "- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.",
    "- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.",
    "- If MCP tools are unavailable, fall back to CLI: `codebase-graph codebase-search <query> --layer semantic --no-refresh --detail slim --context-limit 1`, `codebase-graph codebase-context <query> --layer semantic --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph syntax <language>`, `codebase-graph codebase-search <query> --layer syntax --no-refresh --detail slim --context-limit 1`, `codebase-graph codebase-context --node-id <id> --node-type SyntaxCapture --layer syntax --no-refresh --detail slim --context-limit 2`, `codebase-graph codebase-context <query> --layer hybrid --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph codebase-architecture-queries`, `codebase-graph graph-query \"<statement>\"`, `codebase-graph schema`, and `codebase-graph query-helpers`.",
    "- Do not rerun install to refresh the graph. The MCP server started from this setup config watches the repo and refreshes automatically; use `codebase-graph build --mode full` only for explicit manual rebuilds. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.",
    "<!-- codebaseGraph:end -->",
    "",
    "## Scryer",
    "",
    "- For every coding task, use Scryer and fetch governing rules. Plan changed claims before coding and record user sign-off; reuse explicit same-scope approval. Link changes/claims to stable spec/Linear work IDs.",
    "- Docs, tests, chores, pure refactors and fixes restoring existing claims need no invented model entries; follow Scryer's proportional change-ledger and closure rules.",
    "",
    "## Specifications and verification",
    "",
    "- Track new canonical specs in Git at `docs/specs/<stable-work-id>/spec.md`. Reuse specs/contracts for small fixes; avoid unnecessary documents, IDs or claims. Read existing wiki specs through k-wiki at their original locations; never create parallel Markdown copies.",
    "- Define the problem, observable outcome, scope, exclusions, constraints, failure/recovery behavior and acceptance criteria before dependent implementation. Reuse settled material and decisions.",
    "- Commit reviewable specs before dependent implementation. Approval records identify the path, approved full Git SHA, exact scope and user decision reference. Verify reused approvals match committed content. Commits, merges and Linear status are not user approval.",
    "- Material changes to approved behavior/contracts/boundaries/acceptance require a new spec revision, linked-issue impact review, applicable Scryer updates and user approval before dependent work. Preserve approvals; never silently advance approved-spec links. Progress/evidence updates do not reopen unchanged requirements.",
    "- Trace acceptance criteria to requirements, applicable Scryer claims, implementation anchors, methods and executed evidence. Claims may cover multiple criteria; contract/verification work may need none. Never invent topology/claims to fill tables.",
    "- Verify observable behavior and failure paths. Ingest executed JUnit reports where Scryer requires them; supply anchors and attached tests when marking claims implemented; follow test-radius and drift reconciliation rules.",
    "- Before completion, reconcile approved specs, criteria, claims, contracts, anchors, executed evidence, Linear and handoffs. Keep durable evidence with canonical specs or linked Git-tracked reports; reconcile architectural evidence in Scryer. Distinguish approval, local implementation, supported-platform qualification and aggregate closure. Attached tests/configured CI are not passing execution; report limitations.",
    "",
    "### New-spec outline",
    "Link existing material instead of copying it; explain omitted sections. Keep detailed execution briefs in Scryer.",
    "",
    "1. **Identity/status:** stable work ID, canonical path/logical revision, Linear links, contracts/decisions, Scryer change/node/claim IDs, closure owner, approval scope/reference. Link the approval record containing path/full SHA; never embed a file's own containing commit SHA. Preserve wiki-spec revisions/approvals. Separate proposed, approved, implemented, qualified and closed states.",
    "2. **Problem/outcome:** users, current/desired behavior and rationale.",
    "3. **Scope/constraints:** inclusions, exclusions, compatibility, permissions, data ownership and unchanged release gates; reference Scryer boundaries/directives.",
    "4. **Requirements:** stable work-scoped IDs, triggers/results, contracts and claims, or why no architecture change is needed.",
    "5. **Failure/recovery:** invalid input, unavailable dependencies, cancellation, retries, conflicts, partial completion, persistence/reopen and recovery, as applicable.",
    "6. **Acceptance/verification:** criterion → requirement → Given/When/Then → applicable claim → method/evidence. Evidence identifies revision, environment, command/method, result and limitations; distinguish planned from executed checks.",
    "7. **Open decisions:** question, recommendation, alternatives/tradeoffs, owner and blocked work; link dated decisions when resolved.",
    "8. **Handoff/closure:** satisfied gates, Linear/PR/commit links, exact evidence, limitations and remaining qualification; preserve aggregate closure ownership.",
    "",
    "<!-- linear workflow:start -->",
    "## Linear delivery",
    "",
    "- Use connected Linear MCP tools and the existing CodebaseGraph workspace. Use milestones for major outcomes, issues for slices/steps and sub-issues for independently deliverable sub-slices. A small fix may need one issue only; do not create feature projects unless requested.",
    "- Search stable work IDs/canonical specs, including relevant completed/archived records, before creating records. Automatically create/update records for approved implementation; no new scope or changed approval/closure ownership.",
    "- Issues link the canonical spec/approved revision, requirement/acceptance IDs, applicable Scryer brief/change/claims, exact approval scope and decision reference. Git specs use immutable file permalinks at the approved full SHA; ensure remote availability and preserve approved revisions through history rewrites. Wiki specs keep original references/approvals. Never copy full specs into Linear or the wiki.",
    "- Summarize outcome, dependencies, progress and remaining gates. Encode Scryer prerequisites as blocking relationships; assignment/scheduling cannot override ownership or briefs. Use existing statuses/labels. Update at implementation start, blockage, review, verification and handoff.",
    "- Record PRs/commits, reviewed/tested revision/environment, executed checks/reports, review outcomes and limitations. Keep canonical evidence and Scryer reconciliation current. Close issues only after their acceptance/review/verification gates pass; track remaining platform/release qualification. Milestone closure requires aggregate gates and owner sign-off, not merged PRs, completed children or progress percentages.",
    "- Resolve spec/Scryer/Linear conflicts through user decisions before dependent implementation. If Linear is unavailable, continue independent approved work, report pending synchronization and reconcile without duplicates. Never claim unavailable updates succeeded or change integration settings to bypass gates.",
    "<!-- linear workflow:end -->",
    "",
    "<!-- multi-agent workflow:start -->",
    "# multi-agent workflow",
    "",
    "1. **Map:** recall memory; read the canonical spec/wiki; inspect Scryer, Linear and graph dependencies. Define behavior and acceptance before spawning implementers.",
    "2. **Settle:** agree signatures, types, errors and compatibility; assign shared types/config/manifests/lockfiles one owner. Resolve applicable decisions and obtain or reuse matching explicit user approval before implementation. Meet starting conditions and implement prerequisite contracts before dependent workers.",
    "3. **Assign:** keep detailed briefs/ownership in Scryer. Each bounded slice names exact writable files, deliverable, dependencies/contracts, Linear issue, approved spec revision, requirement/acceptance IDs, Scryer brief, tests and completion criteria. Tell workers they share the workspace, must preserve others' edits, and must report blockers or required contract/ownership changes promptly.",
    "4. **Run:** Select exactly direct (root implements), delegated (one complete technical owner), or parallel (start one; expand to two or three ready independent complete deliverables). Shared contracts must be implemented/checked and files disjoint before dependents. Root uses `coordinator`; actual assignments use the settings below, normally `code_implementation` for implementation. References never select settings or mandatory agents. Repartition overlap or use isolated worktrees without treating isolation as interface settlement. One writer per file; mode changes require old-owner/process/action quiescence and exact reassignment, preserving valid approval/evidence and retry counts.",
    "5. **Coordinate:** The coordinator owns dispatch/tracking, routes consequential unresolved decisions to the user/authority and broadcasts agreed interfaces. Coordinator or bounded worker may apply every method, including architecture/performance/delivery; the complete technical owner investigates, designs locally, implements and checks within approval. Preserve Scryer intent and user scope/contracts/GUI; no self-approval, independent self-verification/review or silent takeover. Inspect actual diffs and evidence by acceptance ID.",
    "6. **Integrate:** The coordinator serializes all explicitly authorized Git/integration/delivery actions, performing them directly or assigning a bounded worker; no concurrent Git mutations. Technical owners resolve behavior-affecting conflicts/warnings in their files. Run applicable full checks and independent verification; method adoption/authorship never grants endpoint permission.",
    "7. **Review final:** a fresh read-only Final Reviewer must use [adversarial-review-engineer](/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md) on the integrated implemented change before its PR handoff, against the baseline, acceptance criteria and test evidence. Fix every confirmed P0–P3 defect, rerun affected checks and require focused reviewer verification of fixes/affected paths against the final snapshot. Report conditional risks/open questions separately.",
    "8. **Complete:** require satisfied acceptance, passing checks and resolved warnings. Handoff includes reviewer settings, reviewed snapshot, spec revision, Linear links and limitations; complete Git/spec/Linear/wiki/memory gates.",
    "",
    "Use `final_review` from the assignment settings below through Codex's native `collaboration.spawn_agent` (`agent_type: \"default\"`, `fork_turns: \"none\"`) in a fresh independent bounded prompt-only session without conversation replay. Map the model selector's name (without provider prefix) to `model` and effort to `reasoning_effort`; omit each `inherit` override. Missing required review/settings/method leaves the completion gate pending; request the user’s decision, never substitute or edit settings to bypass a gate. `verification` independently executes evidence; `proposal_review` runs only for user-requested assessments. Verification/review/delivery are checks/methods, not workflows. No separate final-review release-candidate gate; retain all operational release/repository checks and independent publication verification.",
    "",
    "### Assignment settings",
    "",
    "Only the coordinator dispatches; workers return bounded results without nested dispatch. References never select settings or mandate additional agents.",
    "",
    "| Assignment | Model selector | Effort (`thinking`) |",
    "| --- | --- | --- |",
    "| `coordinator` | inherit | inherit |",
    "| `code_implementation` | `openai-codex/gpt-6.1-terra` | `medium` |",
    "| `analysis_high` | `openai-codex/gpt-6.1-sol` | `high` |",
    "| `analysis_xhigh` | `openai-codex/gpt-6.1-sol` | `xhigh` |",
    "| `verification` | `openai-codex/gpt-6.1-terra` | `medium` |",
    "| `final_review`, `review` | `openai-codex/gpt-6.1-sol` | `xhigh` |",
    "| `proposal_review` | `openai-codex/gpt-6.1-sol` | `high` |",
    "",
    "Optimisation work uses exact approved source/benchmark/test ownership; diagnosis does not authorize implementation. Coordinator or bounded assigned owner applies consequential design methods within Scryer intent and user approval; independent verifier checks performance/correctness. Preserve read-only limits and explicitly authorized serialized delivery; add no performance stage to unrelated work.",
    "<!-- multi-agent workflow:end -->",
    "",
    "## Production delivery and coherence",
    "- **PC-01 — Baseline:** Record the source baseline, approved scope, affected components/callers/contracts and pre-existing scoped drift before implementation; qualify graph/model gaps against source and executed evidence.",
    "- **PC-02 — Placement:** Put behavior in its Scryer-defined owner. The execution brief identifies the existing path to extend, changed component dependencies, affected consumers and verification radius; propose changed ownership before coding.",
    "- **PC-03 — Shared semantics:** Keep product semantics in transport-neutral services. CLI/MCP/library adapters translate and present; requests, defaults, validation, errors and operation metadata use their canonical owners.",
    "- **PC-04 — Dependencies:** Preserve approved dependency direction and component APIs. New cross-component cycles, internal bypasses or changed boundaries require an approved architecture change.",
    "- **PC-05 — Modularity:** Keep modules cohesive, visibility minimal and dependencies explicit. File size prompts inspection; split by responsibility. Do not widen visibility merely to bypass ownership.",
    "- **PC-06 — Contracts:** Settle shared types, errors, serialized formats, configuration and compatibility before dependent implementation; justify new dependencies, features, crates and abstractions with concrete requirements.",
    "- **PC-07 — Scope:** Keep each slice complete and reviewable. Limit refactoring to authorized prerequisites or the approved change; separate behavior-preserving moves from behavior changes where practical.",
    "- **PC-08 — Replacement:** Keep one authoritative implementation per behavior. Replacements account for every caller and retired path; temporary compatibility paths need an approved reason, owner, parity tests and removal condition.",
    "- **PC-09 — Failure behavior:** For changed stateful paths, verify applicable failure, cancellation, retry, concurrency and recovery contracts; preserve identity, confinement, publication and cleanup invariants without inventing new resource limits.",
    "- **PC-10 — Testing:** Test observable outcomes at the owning boundary and affected integrations, including relevant failure paths and transport parity. Use current approved platform/feature contracts and existing required checks.",
    "- **PC-11 — Coherence:** Compare against the baseline for new or worsened responsibility mixing, duplicated semantics, coupling, public exposure and drift. Pre-existing debt never waives an existing gate; acceptance conflicts return to the user.",
    "- **PC-12 — Evidence:** Bind implementation, tests and independent review to the final snapshot. Reuse evidence only while its relevant inputs remain valid; rerun affected checks and focused review after changes.",
    "- **PC-13 — Review:** The required independent final review explicitly checks placement, dependency changes, shared contracts, affected callers, replacement cleanup and failure behavior, as well as correctness; resolve confirmed defects before handoff.",
    "- **PC-14 — Handoff:** Identify changed boundaries/contracts, satisfied acceptance, exact evidence, remaining qualification and recovery limitations; reconcile the existing authorities and preserve all approval, merge and release gates.",
    "## Git delivery",
    "After requested code changes, ",
    "- review and commit relevant changes with a Conventional Commit message. ",
    "- Bugs fixes from main branch should be merged to main through a PR. ",
    "- New features or large refactors should be merged to dev branch through a PR to be tested before merged to the main branch.  ",
    "- push the branch and create a PR with a clear title/summary. Include Linear links, spec path/approved revision and acceptance evidence; record the PR on corresponding issues. Merge/release require all applicable repository/publication checks to pass; PR creation/merge does not replace verification or closure gates. Scryer change require PRs."
  ],
  "terminal_newline": true
}
```

## Quoted proposed policy (not operative here)

```proposed-policy
{
  "format": "AGENTS.text-lines/0.1",
  "lines": [
    "# Repository agent instructions",
    "",
    "- Keep this file at most 150 lines; shorten wording and remove duplication without dropping instructions.",
    "",
    "## Coordination",
    "",
    "- The root agent is the sole coordinator for questions, dispatch, tracking and issue/queue advancement. Preserve read-only limits and existing approval gates; read-only requests permit no writes, including tracking, model ledgers or memory. These instructions are self-contained; no project-specific engineering skill is required.",
    "- Select the next unmet outcome: readiness → technical work → independent verification/review → authorized delivery. Reuse valid artifacts and load only relevant context. Choose exactly direct (root implements), delegated (one complete owner), or parallel (ready independent deliverables). Workflow choice, method use and assignment grant no additional authority; coordinator or bounded worker may perform authorized delivery, while independent checks remain separate.",
    "- One issue is active at a time. An explicitly approved fixed ordered queue records work IDs, exact approvals, dependencies, repository/base, permitted endpoint and authorized release contents/baseline; it may advance only through authorized ready work. Membership or assignment does not approve implementation. A single issue normally ends at its PR. Aggregate closure stays with its owner.",
    "- Once a sub-issue is finished, open a new session with new coordinator so they can start the next remaining sub-issue or issue.",
    "",
    "## Authority and decisions",
    "",
    "- **AGENTS.md:** procedure/gates. **Architecture authority:** selected only under the Architecture authority section below; owns intended responsibilities/boundaries/constraints and applicable execution records. **Specs:** delivery scope, requirements, public contracts, acceptance criteria, decisions, approvals, handoffs and evidence. **Linear:** delivery tracking. **Wiki:** reusable knowledge, rationale/runbooks. **CodebaseGraph:** implementation index. **Source/executed tests:** implementation/behavior evidence. **Memory:** advisory.",
    "- Before implementation: recall memory, find canonical specs/contracts, consult wiki/Linear, inspect the applicable approved architectural intent/rules, then discover files through the graph before reading source. Report unavailable repository tools; never substitute another project's state.",
    "- Preserve work, milestone, phase, slice/sub-slice, requirement and closure-owner identities. Cross-reference stable work IDs, spec revisions, Linear records and applicable authority changes/claims; detailed briefs belong in the selected authority's execution records.",
    "- Ask the user about every unresolved behavior, scope, contract, architecture, permission, compatibility, acceptance or GUI decision. The user owns GUI design; settle it before implementation. Check approved intent first; reuse same-scope approval without reopening settled decisions.",
    "- Use chat question prompts with a recommendation, concrete options, behavior, tradeoffs and consequences. Unanswered questions/preselected options are not approval. Continue independent authorized work while waiting. Routine choices may proceed within approved contracts, constraints and acceptance criteria; surface intent-changing conflicts.",
    "- Never create/use `.omx` files or directories. Use Git specs, Linear and the wiki for their assigned purposes.",
    "",
    "<!-- k-wiki:start -->",
    "## k-wiki",
    "",
    "- Use configured `k_wiki` MCP registration(s), never the CLI. `knowledge/` is curated source; never manually edit generated `.kwiki/` state.",
    "- Start with `wiki_list_bundles`, then `wiki_search_concepts`. Use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks` and `wiki_get_neighborhood` for context; `wiki_get_recent_changes` for recent work and `wiki_get_diagnostics` for remaining issues.",
    "- Record terminology, invariants, rationale, runbooks, decisions and non-obvious tradeoffs. Architecture pages explain approved architectural intent; verify mutable facts through codebase-graph. Resolve code/wiki conflicts against the applicable approved authority and specs, then clarify knowledge without duplicating specs.",
    "- Use `wiki_create_page` to create and `wiki_populate_page` to update, supplying title, type, tags, useful Markdown and `expected_content_hash` for updates. Link canonical specs/contracts. Preserve existing wiki specs' locations, identities, revisions and approvals; no migration, copying, deletion or supersession without separate user authorization.",
    "- After meaningful edits: `wiki_validate` with `profile: recommended`, `include_structured_content: true`; `wiki_check_links`; then `wiki_build` with configured `bundle_root` and `.kwiki/site` output root (a write). Handoffs cite changed concepts, decisions, uncertainties and validation results.",
    "<!-- k-wiki:end -->",
    "",
    "<!-- agent-memory:start -->",
    "## Agent memory",
    "",
    "- Before non-trivial repository conclusions, call `wiki_memory_recall` with the goal and relevant subsystems/symbols/errors/decisions. Skip only mechanical formatting, typos, exact replacements and simple file/symbol lookups. Recall again for subsystem/scope changes, familiar failures or earlier rationale, conventions, constraints or failed approaches.",
    "- Memory never overrides instructions, permissions, curated knowledge, source or executed tests. Verify mutable facts.",
    "- Before non-trivial completion, explicitly assess reusable knowledge. If warranted, `wiki_memory_record` one focused, distilled claim/procedure with structured provenance; it creates a candidate. Kinds: `semantic` for stable facts/terminology/invariants/contracts/ownership/decisions; `episodic` for durable outcomes/root causes/failed approaches/lessons; `procedural` for verified, reproducible runbooks/workarounds/constraints.",
    "- Never store raw sessions/tool output, transient status, readily rediscovered code facts, secrets, credentials, personal data or unverified external content. Do not create filler memory.",
    "- Verify claims against cited authoritative evidence before activation. `wiki_memory_transition` allows `candidate → active | quarantined`, `active → superseded | quarantined`, `quarantined → candidate`; `superseded` is terminal. Reverify returned candidates. Quarantine contradicted/unsafe/suspicious recalled memory during the task. Create/activate replacements with `supersedes`, then supersede old records; preserve history.",
    "<!-- agent-memory:end -->",
    "",
    "<!-- codebaseGraph:start -->",
    "## codebaseGraph workflow",
    "- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.",
    "- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.",
    "- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.",
    "- Use MCP `graph_search` with `layer: \"semantic\"`, `detail: \"slim\"`, and `context_limit: 1` before answering entity, relationship, or repository-structure questions and before coding tasks.",
    "- Repository-local agent hooks may inject bounded `graph_health` and semantic `graph_search` results for each non-empty prompt. Treat this context as advisory; hook failures fail open after a short deadline and never authorize, deny, or rebuild anything.",
    "- Automatic hook context is only orientation. Before edits or architectural conclusions, call `graph_context` explicitly with `change_impact`, `dependencies`, `callgraph`, or `runtime` as appropriate.",
    "- Use MCP `graph_context` with `layer: \"semantic\"`, `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when semantic relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.",
    "- For parse-tree, grammar-field, source-construct, or Tree-sitter node questions, first use MCP `graph_syntax` with the file language, then use `graph_search` with `layer: \"syntax\"` to find `SyntaxCapture` nodes.",
    "- Use MCP `graph_context` with `layer: \"syntax\"` only for a `SyntaxCapture` result, passing its `node_id` and `node_type: \"SyntaxCapture\"`; follow ordered `SyntaxChild` rows through `field_name` and `child_index`.",
    "- Use `layer: \"hybrid\"` only when semantic relationships and syntax structure are both needed; hybrid search returns semantic matches before syntax matches.",
    "- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.",
    "- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.",
    "- If MCP tools are unavailable, fall back to CLI: `codebase-graph codebase-search <query> --layer semantic --no-refresh --detail slim --context-limit 1`, `codebase-graph codebase-context <query> --layer semantic --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph syntax <language>`, `codebase-graph codebase-search <query> --layer syntax --no-refresh --detail slim --context-limit 1`, `codebase-graph codebase-context --node-id <id> --node-type SyntaxCapture --layer syntax --no-refresh --detail slim --context-limit 2`, `codebase-graph codebase-context <query> --layer hybrid --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph codebase-architecture-queries`, `codebase-graph graph-query \"<statement>\"`, `codebase-graph schema`, and `codebase-graph query-helpers`.",
    "- Do not rerun install to refresh the graph. The MCP server started from this setup config watches the repo and refreshes automatically; use `codebase-graph build --mode full` only for explicit manual rebuilds. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.",
    "<!-- codebaseGraph:end -->",
    "",
    "## Architecture authority",
    "",
    "- Scryer remains the authority unless COD-19's exact-scope human approvals and verified activation record select the scoped transition in `docs/specs/CBG-C4-01/authority-transition.md`; existence of a proposal or candidate YAML grants nothing. Unrelated work remains Scryer-governed; preserve its models, directives, history, open changes and closure owners.",
    "- For activated CBG-C4-01 scope only, human-approved Git contracts and per-issue Git execution briefs govern bootstrap; qualified accepted native YAML then governs C4 intent/constraints. Specs/briefs retain delivery, HOW-constraints, prerequisites, file ownership and evidence outside the v1 DSL. Scryer is optional historical context, never a required native-work ledger, sign-off, fold or runtime dependency.",
    "- For Scryer-governed coding, fetch its governing rules, plan changed claims before code, record/reuse matching user sign-off and link stable spec/Linear IDs. Docs/tests/chores/refactors/restorative fixes need no invented claims; retain proportional ledgers and closure rules.",
    "- Native work requires a versioned issue brief citing approved scope/baseline, retained constraints, intended changes, prerequisites, one owner per file, acceptance/evidence and human decision references before code. Record genuine executed evidence and failures; reconcile under the approved Git/native authority without fake claims or Scryer verdicts. File paths/work IDs alone grant no exemption; conflicts or missing bindings stop for the owner.",
    "",
    "## Specifications and verification",
    "",
    "- Track new canonical specs in Git at `docs/specs/<stable-work-id>/spec.md`. Reuse specs/contracts for small fixes; avoid unnecessary documents, IDs or claims. Read existing wiki specs through k-wiki at their original locations; never create parallel Markdown copies.",
    "- Define the problem, observable outcome, scope, exclusions, constraints, failure/recovery behavior and acceptance criteria before dependent implementation. Reuse settled material and decisions.",
    "- Commit reviewable specs before dependent implementation. Approval records identify the path, approved full Git SHA, exact scope and user decision reference. Verify reused approvals match committed content. Commits, merges and Linear status are not user approval.",
    "- Material changes to approved behavior/contracts/boundaries/acceptance require a new spec revision, linked-issue impact review, applicable architecture-authority updates and user approval before dependent work. Preserve approvals; never silently advance approved-spec links. Progress/evidence updates do not reopen unchanged requirements.",
    "- Trace acceptance criteria to requirements, applicable architecture claims, implementation anchors, methods and executed evidence. Claims may cover multiple criteria; contract/verification work may need none. Never invent topology/claims to fill tables.",
    "- Verify observable behavior and failure paths. Ingest executed JUnit reports where the applicable authority requires them; supply anchors and attached tests when marking claims implemented; follow test-radius and drift reconciliation rules.",
    "- Before completion, reconcile approved specs, criteria, claims, contracts, anchors, executed evidence, Linear and handoffs. Keep durable evidence with canonical specs or linked Git-tracked reports; reconcile architectural evidence under the applicable approved authority. Distinguish approval, local implementation, supported-platform qualification and aggregate closure. Attached tests/configured CI are not passing execution; report limitations.",
    "",
    "### New-spec outline",
    "Link existing material instead of copying it; explain omitted sections. Keep detailed execution briefs in the applicable authority's execution records.",
    "",
    "1. **Identity/status:** stable work ID, canonical path/logical revision, Linear links, contracts/decisions, applicable authority change/node/claim IDs, closure owner, approval scope/reference. Link the approval record containing path/full SHA; never embed a file's own containing commit SHA. Preserve wiki-spec revisions/approvals. Separate proposed, approved, implemented, qualified and closed states.",
    "2. **Problem/outcome:** users, current/desired behavior and rationale.",
    "3. **Scope/constraints:** inclusions, exclusions, compatibility, permissions, data ownership and unchanged release gates; reference the approved authority's boundaries/directives.",
    "4. **Requirements:** stable work-scoped IDs, triggers/results, contracts and claims, or why no architecture change is needed.",
    "5. **Failure/recovery:** invalid input, unavailable dependencies, cancellation, retries, conflicts, partial completion, persistence/reopen and recovery, as applicable.",
    "6. **Acceptance/verification:** criterion → requirement → Given/When/Then → applicable claim → method/evidence. Evidence identifies revision, environment, command/method, result and limitations; distinguish planned from executed checks.",
    "7. **Open decisions:** question, recommendation, alternatives/tradeoffs, owner and blocked work; link dated decisions when resolved.",
    "8. **Handoff/closure:** satisfied gates, Linear/PR/commit links, exact evidence, limitations and remaining qualification; preserve aggregate closure ownership.",
    "",
    "<!-- linear workflow:start -->",
    "## Linear delivery",
    "",
    "- Use connected Linear MCP tools and the existing CodebaseGraph workspace. Use milestones for major outcomes, issues for slices/steps and sub-issues for independently deliverable sub-slices. A small fix may need one issue only; do not create feature projects unless requested.",
    "- Search stable work IDs/canonical specs, including relevant completed/archived records, before creating records. Automatically create/update records for approved implementation; no new scope or changed approval/closure ownership.",
    "- Issues link the canonical spec/approved revision, requirement/acceptance IDs, applicable authority brief/change/claims, exact approval scope and decision reference. Git specs use immutable file permalinks at the approved full SHA; ensure remote availability and preserve approved revisions through history rewrites. Wiki specs keep original references/approvals. Never copy full specs into Linear or the wiki.",
    "- Summarize outcome, dependencies, progress and remaining gates. Encode execution-brief prerequisites as blocking relationships; assignment/scheduling cannot override ownership or briefs. Use existing statuses/labels. Update at implementation start, blockage, review, verification and handoff.",
    "- Record PRs/commits, reviewed/tested revision/environment, executed checks/reports, review outcomes and limitations. Keep canonical evidence and the applicable authority reconciliation current. Close issues only after their acceptance/review/verification gates pass; track remaining platform/release qualification. Milestone closure requires aggregate gates and owner sign-off, not merged PRs, completed children or progress percentages.",
    "- Resolve spec/authority/Linear conflicts through user decisions before dependent implementation. If Linear is unavailable, continue independent approved work, report pending synchronization and reconcile without duplicates. Never claim unavailable updates succeeded or change integration settings to bypass gates.",
    "<!-- linear workflow:end -->",
    "",
    "<!-- multi-agent workflow:start -->",
    "# multi-agent workflow",
    "",
    "1. **Map:** recall memory; read the canonical spec/wiki; inspect approved authority, Linear and graph dependencies. Define behavior and acceptance before spawning implementers.",
    "2. **Settle:** agree signatures, types, errors and compatibility; assign shared types/config/manifests/lockfiles one owner. Resolve applicable decisions and obtain or reuse matching explicit user approval before implementation. Meet starting conditions and implement prerequisite contracts before dependent workers.",
    "3. **Assign:** keep detailed briefs/ownership in the approved authority's execution records. Each bounded slice names exact writable files, deliverable, dependencies/contracts, Linear issue, approved spec revision, requirement/acceptance IDs, applicable execution brief, tests and completion criteria. Tell workers they share the workspace, must preserve others' edits, and must report blockers or required contract/ownership changes promptly.",
    "4. **Run:** Select exactly direct (root implements), delegated (one complete technical owner), or parallel (start one; expand to two or three ready independent complete deliverables). Shared contracts must be implemented/checked and files disjoint before dependents. Root uses `coordinator`; actual assignments use the settings below, normally `code_implementation` for implementation. References never select settings or mandatory agents. Repartition overlap or use isolated worktrees without treating isolation as interface settlement. One writer per file; mode changes require old-owner/process/action quiescence and exact reassignment, preserving valid approval/evidence and retry counts.",
    "5. **Coordinate:** The coordinator owns dispatch/tracking, routes consequential unresolved decisions to the user/authority and broadcasts agreed interfaces. Coordinator or bounded worker may apply every method, including architecture/performance/delivery; the complete technical owner investigates, designs locally, implements and checks within approval. Preserve approved architectural intent and user scope/contracts/GUI; no self-approval, independent self-verification/review or silent takeover. Inspect actual diffs and evidence by acceptance ID.",
    "6. **Integrate:** The coordinator serializes all explicitly authorized Git/integration/delivery actions, performing them directly or assigning a bounded worker; no concurrent Git mutations. Technical owners resolve behavior-affecting conflicts/warnings in their files. Run applicable full checks and independent verification; method adoption/authorship never grants endpoint permission.",
    "7. **Review final:** a fresh read-only Final Reviewer must use [adversarial-review-engineer](/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md) on the integrated implemented change before its PR handoff, against the baseline, acceptance criteria and test evidence. Fix every confirmed P0–P3 defect, rerun affected checks and require focused reviewer verification of fixes/affected paths against the final snapshot. Report conditional risks/open questions separately.",
    "8. **Complete:** require satisfied acceptance, passing checks and resolved warnings. Handoff includes reviewer settings, reviewed snapshot, spec revision, Linear links and limitations; complete Git/spec/Linear/wiki/memory gates.",
    "",
    "Use `final_review` from the assignment settings below through Codex's native `collaboration.spawn_agent` (`agent_type: \"default\"`, `fork_turns: \"none\"`) in a fresh independent bounded prompt-only session without conversation replay. Map the model selector's name (without provider prefix) to `model` and effort to `reasoning_effort`; omit each `inherit` override. Missing required review/settings/method leaves the completion gate pending; request the user’s decision, never substitute or edit settings to bypass a gate. `verification` independently executes evidence; `proposal_review` runs only for user-requested assessments. Verification/review/delivery are checks/methods, not workflows. No separate final-review release-candidate gate; retain all operational release/repository checks and independent publication verification.",
    "",
    "### Assignment settings",
    "",
    "Only the coordinator dispatches; workers return bounded results without nested dispatch. References never select settings or mandate additional agents.",
    "",
    "| Assignment | Model selector | Effort (`thinking`) |",
    "| --- | --- | --- |",
    "| `coordinator` | inherit | inherit |",
    "| `code_implementation` | `openai-codex/gpt-6.1-terra` | `medium` |",
    "| `analysis_high` | `openai-codex/gpt-6.1-sol` | `high` |",
    "| `analysis_xhigh` | `openai-codex/gpt-6.1-sol` | `xhigh` |",
    "| `verification` | `openai-codex/gpt-6.1-terra` | `medium` |",
    "| `final_review`, `review` | `openai-codex/gpt-6.1-sol` | `xhigh` |",
    "| `proposal_review` | `openai-codex/gpt-6.1-sol` | `high` |",
    "",
    "Optimisation work uses exact approved source/benchmark/test ownership; diagnosis does not authorize implementation. Coordinator or bounded assigned owner applies consequential design methods within approved architectural intent and user approval; independent verifier checks performance/correctness. Preserve read-only limits and explicitly authorized serialized delivery; add no performance stage to unrelated work.",
    "<!-- multi-agent workflow:end -->",
    "",
    "## Production delivery and coherence",
    "- **PC-01 — Baseline:** Record the source baseline, approved scope, affected components/callers/contracts and pre-existing scoped drift before implementation; qualify graph/model gaps against source and executed evidence.",
    "- **PC-02 — Placement:** Put behavior in its approved architecture owner. The execution brief identifies the existing path to extend, changed component dependencies, affected consumers and verification radius; propose changed ownership before coding.",
    "- **PC-03 — Shared semantics:** Keep product semantics in transport-neutral services. CLI/MCP/library adapters translate and present; requests, defaults, validation, errors and operation metadata use their canonical owners.",
    "- **PC-04 — Dependencies:** Preserve approved dependency direction and component APIs. New cross-component cycles, internal bypasses or changed boundaries require an approved architecture change.",
    "- **PC-05 — Modularity:** Keep modules cohesive, visibility minimal and dependencies explicit. File size prompts inspection; split by responsibility. Do not widen visibility merely to bypass ownership.",
    "- **PC-06 — Contracts:** Settle shared types, errors, serialized formats, configuration and compatibility before dependent implementation; justify new dependencies, features, crates and abstractions with concrete requirements.",
    "- **PC-07 — Scope:** Keep each slice complete and reviewable. Limit refactoring to authorized prerequisites or the approved change; separate behavior-preserving moves from behavior changes where practical.",
    "- **PC-08 — Replacement:** Keep one authoritative implementation per behavior. Replacements account for every caller and retired path; temporary compatibility paths need an approved reason, owner, parity tests and removal condition.",
    "- **PC-09 — Failure behavior:** For changed stateful paths, verify applicable failure, cancellation, retry, concurrency and recovery contracts; preserve identity, confinement, publication and cleanup invariants without inventing new resource limits.",
    "- **PC-10 — Testing:** Test observable outcomes at the owning boundary and affected integrations, including relevant failure paths and transport parity. Use current approved platform/feature contracts and existing required checks.",
    "- **PC-11 — Coherence:** Compare against the baseline for new or worsened responsibility mixing, duplicated semantics, coupling, public exposure and drift. Pre-existing debt never waives an existing gate; acceptance conflicts return to the user.",
    "- **PC-12 — Evidence:** Bind implementation, tests and independent review to the final snapshot. Reuse evidence only while its relevant inputs remain valid; rerun affected checks and focused review after changes.",
    "- **PC-13 — Review:** The required independent final review explicitly checks placement, dependency changes, shared contracts, affected callers, replacement cleanup and failure behavior, as well as correctness; resolve confirmed defects before handoff.",
    "- **PC-14 — Handoff:** Identify changed boundaries/contracts, satisfied acceptance, exact evidence, remaining qualification and recovery limitations; reconcile the existing authorities and preserve all approval, merge and release gates.",
    "## Git delivery",
    "After requested code changes,",
    "- review and commit relevant changes with a Conventional Commit message.",
    "- Bugs fixes from main branch should be merged to main through a PR.",
    "- New features or large refactors should be merged to dev branch through a PR to be tested before merged to the main branch.",
    "- push the branch and create a PR with a clear title/summary. Include Linear links, spec path/approved revision and acceptance evidence; record the PR on corresponding issues. Merge/release require all applicable repository/publication checks to pass; PR creation/merge does not replace verification or closure gates. Architecture-authority changes require PRs."
  ],
  "terminal_newline": true
}
```

## Preservation and approval checklist

- Baseline and proposed local instruction bytes are pinned by hashes and the approved full-SHA transition artifact; maximum 150 installed lines is retained.
- Root AGENTS, `.gitignore`, Scryer models/directives/prior history, unrelated pending entries, curated wiki and client settings are preserved; only empty documentation-ledger history is appended. The baseline copy is inert, not a parallel model or runtime store.
- Native scope uses approved Git/bootstrap briefs until qualified native C4 intent exists; unrelated scope remains Scryer-governed. No file/work-ID-only exemption or self-approval by candidate is permitted.
- Actual model trust pins, first baseline and protected final transition remain COD-37/46/47 gates. Specification/policy approval cannot manufacture them.
- Scope A, scope B and later concrete runtime approvals are distinct. COD-19/COD-5 stay open until their actual approval and activation gates pass.
