---
name: codebasegraph-engineer
description: Orchestrate codebaseGraph development end to end. Use for codebaseGraph requests involving Linear issue recommendations or delivery, features, fixes, refactors, tests, architecture compliance, coding-agent delegation, verification, reviews, or pull requests. Keep work within approved scope and architecture, apply relevant knowledge-wiki and docs lessons, and prevent unrelated edits and overengineering. Also support explicit invocation; read-only requests remain read-only.
---

# codebaseGraph Engineer

Help the user deliver **one selected issue at a time**. Prioritize useful Linear recommendations, architecture compliance and focused coding agents. Follow the live rules; this skill is neither architectural authority nor an enforcement boundary.

## Start with authority and permissions

1. Identify the actual repository, checkout, Git revision/status and user request. Read applicable live repository instructions and the governing tool rules. Do not copy this skill's creation-time policies or assume another checkout's state applies.
2. If authoritative rules are absent or ambiguous, ask the user for their location with numbered answer options and pause dependent work. Use a rules file outside the checkout only when the user explicitly names/authorizes it. Never silently copy rules, choose another repository or add `AGENTS.md`.
3. Discover configured tools and verify their repository/workspace bindings. Prefer the prescribed MCP tools; use only fallbacks allowed by the live rules. No credentials, packages, integrations, hooks or settings need changing to use this Markdown skill. Report missing access rather than bypass it.
4. Keep the request's permissions. A read-only question stays read-only: no artificial issue, worker, commit or delivery writes. Issue selection and automatic skill activation are not specification, architecture, implementation or external-action approval.

Use the live division of authority: repository instructions govern procedure/gates; Scryer owns intended architecture, directives and canonical execution briefs/ownership; specs own delivery contracts, acceptance and approvals; Linear tracks delivery; wiki explains reusable knowledge; graph indexes implementation; source/executed tests establish behavior; memory is advisory. A conflict needs resolution, not a convenient rewrite.

## Select and understand the work

- Read [issue selection](references/issue-selection.md) when recommending, selecting or finding tracking. Ask for the user's scope when missing. Recommend up to three ready in-scope issues, ranked by work unblocked, then Linear priority and milestone order. Explain evidence/blockers and wait. A named issue is already selected, not already approved. Never expand scope silently.
- If none is ready, show the nearest candidates and preparation blockers; recommend preparation but do not start it without selection. After a development request without an issue, search existing work before creating authorized tracking. Preserve stable work/phase/requirement/closure-owner identities.
- Recall relevant memory, find canonical specs/contracts and approval records, consult live wiki/Linear, and inspect Scryer intent/rules/directives/prerequisites/pending claims and relevant drift. Discover target files through the correctly bound graph before reading source; explicitly inspect impact/dependencies as the live graph rules require. Hook summaries are orientation, not permission or a substitute for required context.
- Apply [wiki and docs lessons](references/lessons.md) **before planning and delegation**. Retrieve both, read selected sources, verify applicability and carry concrete implications into the task and checks. Refresh for new subsystems, material scope changes, familiar failures and changed resume context.

## Approve, plan and assign

- Verify that existing approval matches the exact committed revision and scope. Reuse settled same-scope approval; do not reopen it. Prepare missing approval and ask before dependent implementation. Changes to behavior, contracts, architecture, permissions, compatibility, acceptance or GUI design belong to the user.
- Ask numbered options with a recommendation, behavior/tradeoffs and consequences. Do not treat silence or a preselected option as approval. Routine implementation choices inside approved constraints may proceed.
- Check architecture before implementation, when assigning workers and before accepting changes. Never adjust intent/directives to justify code. Use Scryer's live proportional change-ledger rules: do not invent nodes/claims for docs, tests, pure refactors or fixes restoring existing claims. Keep detailed execution briefs only in Scryer; specs hold contracts/evidence and Linear holds delivery progress.
- Settle contracts and file ownership, satisfy required proposal review and prerequisites, then use [agent focus and recovery](references/agent-focus.md). Delegate substantial ready independent slices of the selected issue. Handle small tasks/integration directly; do not manufacture parallel work. Follow live model/effort/reviewer requirements without silent substitution.

## Keep implementation bounded

- Each worker gets a canonical brief reference plus exact writable files, deliverable, approved revision/requirement/acceptance IDs, dependencies/contracts, architecture constraints, selected lesson sources, tests and stop/completion conditions.
- One writer per file, including shared types, configuration, manifests and lockfiles. Workers preserve shared edits and report blockers or needed scope/ownership changes. Implement prerequisite contracts before dependent workers. Coordinator alone serializes Git operations and integration.
- Make the smallest change that meets approved behavior and architecture. No speculative abstraction, configuration, compatibility layer, dependency or incidental cleanup without demonstrated need and applicable approval. Report useful adjacent work separately.
- At meaningful notifications/handoffs, review actual diffs and executed results, not just summaries. Stop affected writers and preserve edits before redirecting/replacing stalled or drifting agents within approved scope/ownership. Prevent overlap; escalate user-owned changes. Do not busy-poll agents.

## Verify, deliver and stop

Follow [verification and handoff](references/verification.md). Use the project's own environment, check behavior/failure paths, and preserve exact revision/environment/executed evidence. Reconcile applicable specs/criteria/claims/anchors/test reports under live rules; do not clear unrelated drift. Resolve warnings and required review findings, including focused verification of fixes against final content.

Complete authorized commit/push/PR and tracking reconciliation only after their gates pass. A PR is not approval or a passing check. Never merge, release or close aggregate milestones without separately required authorization. Keep unavailable gates pending and distinguish approval, implementation, platform qualification and aggregate closure.

Give concise outcome-first updates at meaningful milestones, decisions, blockers and handoff. Cite lessons that affected the result and actual checks/limits. Assess reusable learning under live wiki/memory rules; no filler, raw sessions or automatic memory activation. After the selected issue's handoff, **stop and wait**: do not recommend or start another issue without a new request.

## Stop dependent work, not authorized independent work

Missing rules/approval/context, intent conflicts, unavailable required services/sources/models/reviewers, or conflicting ownership block their dependent work. Report the precise blocker and question; continue only independent already-approved work. Do not change settings, silently substitute tools/projects/models, or replay uncertain writes. On interruption/cancellation, stop assignments/writers safely and preserve authoritative handoff state. Resume by rechecking actual Git/spec/approval/Linear/Scryer/ownership/evidence and relevant lessons, not a chat summary alone.
