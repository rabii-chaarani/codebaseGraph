# CBG-ENG-SKILL-01 — codebaseGraph Engineer Agent

## Identity and approval

- Canonical path: `docs/specs/CBG-ENG-SKILL-01/spec.md`; revision **0.2**, 29 October 2026.
- Scope: adapt the existing Engineer Agent instructions for approved queues, protected merges, approved-content releases and configurable role models. No live delivery batch is authorized.
- User decision: **“PLEASE IMPLEMENT THIS PLAN”**, followed by Terra/xhigh authorship, concise-writing and model-config instructions. The user chose host inheritance for the coordinator and evidence verifier. This does not claim the user reviewed a later-created Git SHA.
- Tracking: [COD-102](https://linear.app/codebasegraph/issue/COD-102/cbg-eng-skill-01-r02-extend-engineer-agent-through-approved-queues). Scryer ledger: `chg-7dqyj1`, documentation only; no product claims change.
- Approval mapping: [revision 0.2](approvals/rev-0.2.md). Exact instruction bytes/hashes: [manifest](adaptation-0.2/manifest.json). Freeze the commit and scope record before installation; a commit alone is not approval.
- Preserve [revision 0.1 approval](approvals/rev-0.1.md) and its [immutable specification](https://github.com/rabii-chaarani/codebaseGraph/blob/01416f1ea888d349d1358bd5d7eddfcb3c0ae4ad/docs/specs/CBG-ENG-SKILL-01/spec.md). Its creation-only no-Linear/no-PR exceptions do not apply here.
- Aggregate closure remains with the user. Installation, behavior checks and live-host qualification are separate outcomes.

## Problem and outcome

The installed skill already coordinates planning, workers, integration and evidence. Its unconditional one-issue stopping rule prevents the approved queue workflow. It lacks a merge/release procedure and role-model configuration.

The codebaseGraph Engineer Agent remains the sole coordinator. It follows the authorized endpoint, handles one issue at a time, delegates independent slices, merges after required gates and releases only a completed approved queue. Read-only and default single-issue requests retain their limits.

## Scope and constraints

Update the shared skill, its issue-selection, agent-focus and verification references, one new merge/release reference, a single `models.toml` role map, and the original repository's `AGENTS.md`. Preserve the skill identifier, original `.agents` symlink and unchanged lessons reference. Keep `AGENTS.md` at most 150 lines.

Retire the former tracked `.prime/agent/skills/codebasegraph-engineer` copy on this branch. Preserve original bytes, approvals and history as inert evidence. The shared directory remains the sole active source; evidence uses `.txt` suffixes and cannot become another discoverable skill.

No product code/API, architecture topology, scheduler, task database, credentials, integration settings, hooks, live issue trial, merge, publication or aggregate closure changes. The host supplies execution/recovery; instructions do not enforce policy or switch the running coordinator's model.

Preserve authority: repository instructions govern procedure; Scryer governs architecture and briefs; specs govern contracts/acceptance/approval; Linear tracks delivery; wiki supplies knowledge; graph indexes implementation; source/tests establish behavior. Memory is advisory. User-owned decisions remain with the user.

## Requirements

R04, R05, R07 and R10 retain their revision 0.1 meaning. The following requirements revise or extend that contract.

| ID | Required behavior |
| --- | --- |
| R01 | Keep `codebasegraph-engineer` discoverable at its shared source. Name the Engineer Agent as sole coordinator. Read-only requests cause no writes. A single issue defaults to PR handoff unless a further endpoint is explicit. |
| R02 | Treat a named issue or fixed authorized queue as selected. Recommend only when selection is needed. Verify approvals/dependencies; advance only inside the authorized queue, one active issue at a time. |
| R03 | Reuse matching exact-scope approvals. Queue membership, adoption, status and review do not confer implementation or publication permission. Ask unresolved behavior, scope, contract, architecture, permission, compatibility, acceptance and GUI decisions. |
| R06 | Coordinator owns dispatch, file ownership, integration, Git and authorized merge/release. Delegate substantial independent slices; settle prerequisites/shared files first. Keep detailed briefs in Scryer. |
| R08 | Resume from actual approvals, ownership, evidence and publication state. Stop previous writers before replacement. Reconcile uncertain writes before retrying. Missing gates remain pending; preserve unrelated work. |
| R09 | Verify behavior and executed evidence with required independent reviews. Bind merge to the reviewed/tested head and integration candidate plus actor-applicable server freshness. Verify main CI before advancing. Release only a complete queue and approved contents; release-PR merge commits to publication. Promote the release merge's exact CI artifacts, verify publication and stop at the authorized endpoint. Preserve aggregate closure ownership. |
| R11 | Terra/xhigh writes the skill changes. Use concise plain wording and established names. Bind installed files to reviewed bytes/hashes; reject stale bases. Preserve the symlink and lessons bytes. |
| R12 | Reuse the existing publisher. Missing host, policy, reviewer or publication capabilities block dependent live actions. Synthetic checks and loader observations do not qualify production delivery. |
| R13 | One `models.toml` assigns model/effort to coordinator, proposal reviewer, backend/frontend workers, evidence verifier, final reviewer and skill writer. Preserve agreed defaults; coordinator/verifier inherit host settings. Read it before dispatch, record actual settings and block unsupported assignments without silent fallback or editing the map to pass a gate. Explicit coordinator settings must match actual host execution. |

## Failure and recovery

Blocked items remain in the queue. Independent approved work may bypass them under the user's approved policy; unfinished items prevent release. Unknown release contents require a user decision.

Before release/resume, inspect existing release PRs, pending publication, jobs, tags and registry versions. Activity is not authorization. Recover only the same authorized target after successful dry-run validation. Do not substitute CI, automatically rebuild missing artifacts, move conflicting tags, clear labels or change permissions to pass a gate.

Cancellation stops new work and reconciles in-flight actions. Publication may complete after release-PR merge. Report actual outcomes and remaining gates.

## Acceptance and evidence

| Criteria | Requirements | Method |
| --- | --- | --- |
| A01 | R01, R11, R13 | Structural validator; links, TOML roles, line count, file hashes, unchanged lessons/symlink. |
| A02 | R01, R12 | Fresh native Prime Agent inventory, explicit invocation and one observed automatic routing run. Record actual settings/actions; leave unavailable checks pending. |
| A03–A07, A11, A13–A16 | R01–R10 | Blind fictitious scenarios for read-only/single-issue limits, approval/dependency/ownership conflicts, unavailable reviewers, stale evidence, resume and authorized stopping. A16 now permits explicitly authorized continuation. |
| A17 | R02, R03, R06 | Two-issue queue advances after successful merge/main CI without repeated selection, extra backlog work or partial release. |
| A18 | R09, R12 | Changed base/head blocks merge until affected evidence and server checks are current. Expected head alone is insufficient. |
| A19 | R09, R12 | Extra contents or changed release PR blocks commitment. Verify exact release CI/four-target artifacts, assets, crate and finalization. |
| A20 | R08, R09 | Lost response, partial publication, missing artifacts, conflicting tags and cancellation trigger scoped readback/recovery rather than blind writes. |
| A21 | R11, R13 | Review exact instruction/config diffs. Check base hashes before installation and installed hashes afterward. Preserve legacy history as inert evidence; dispatch uses the configured role. |

Evidence belongs in [adaptation-0.2](adaptation-0.2/). Keep expectations separate from participants. Checks establish bounded instruction behavior, not hard enforcement or supported-platform qualification. Product compilation/JUnit is unnecessary for this Markdown/TOML adaptation.

## Reviews, lessons and handoff

Proposal reviewer: fresh GPT-6.1 Sol/High. Author: GPT-6.1 Terra/xhigh. Final reviewer: fresh GPT-6.1 Sol/Max, with focused fix verification. Models/effort live in the role map; independent review procedure stays in repository rules.

Applied lessons: wiki `architecture/release-verification` and memory `exact-commit-release-publication-and-recovery-2026-09-24` require exact source/run promotion and scoped recovery. The original checkout's older release docs are historical, not proof of the deployed publisher. Verify live revisions/rules before actual publication; this task does not change them.

Handoff: reviewed hashes, approval/scope mapping, installed results, actual checks, COD-102, commits/PR and remaining qualification. Preserve revision 0.1 reports. Synthetic tests alone warrant no new reusable memory.
