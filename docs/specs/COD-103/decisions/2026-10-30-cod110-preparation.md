# COD-103 — COD-110 amendment preparation decision

## Decision and authority

- On 2026-10-30, the user replied **“approved”** after Engineer proposed preparing an amendment that appends COD-110 after COD-108 and expressly includes already-merged COD-54 in the final complete release contents.
- Engineer recorded the interpretation on [COD-103](https://linear.app/codebasegraph/issue/COD-103), comment ID `1f26d6dd-6044-4349-bb02-e44779e4c8cc`, and relayed the decision to Production. This record cites that decision; it is not a new implementation approval.
- The preparation ledger is existing Scryer change `chg-pxt0x2`. Engineer retains lifecycle/tracking ownership. Detailed execution briefs and file ownership remain in Scryer.

## Selected proposal scope

Prepare an amendment proposing the ordered queue **COD-105 → COD-106 → COD-56 → COD-107 → COD-108 → COD-110**. COD-110 is appended, not inserted before existing work. The approved five-item queue remains authoritative until the completed amendment is pinned and separately approved.

The selected final-release contents include already-merged **COD-54 / PR #163**, exact main commit [`c60a00c1d747b73789cd6edc7ee20601182a35ca`](https://github.com/rabii-chaarani/codebaseGraph/commit/c60a00c1d747b73789cd6edc7ee20601182a35ca). This selects that merged delta for the final complete-release proposal. It does not claim platform acceptance, change aggregate closure ownership, authorize immediate publication or approve unknown later changes.

## Preserved approval and pending approval

- Existing canonical `docs/specs/COD-103/spec.md`, revision **0.2**, remains approved at full SHA [`7e79f7e924536f29d3212e83899b01c38afd39de`](https://github.com/rabii-chaarani/codebaseGraph/blob/7e79f7e924536f29d3212e83899b01c38afd39de/docs/specs/COD-103/spec.md).
- Preserve the existing approval path `docs/specs/COD-103/approvals/2026-10-29-combined.md` and immutable approval-record commit [`383a481bd5a71c58dab7dd796ace92a365760da5`](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md). No approved link advances automatically.
- The new decision authorizes proposal preparation and selects the stated proposed contents. It does **not** approve an unwritten amendment, changed stack-base triggers, new requirements/contracts, implementation, claim sign-off, merge, release, CI rerun or settings/credential changes.
- After Requirements, Designer and Planner complete their assigned proposal, Production may freeze only the separately authorized snapshot. Dependent implementation waits for explicit approval identifying the completed canonical path, full Git SHA and exact scope.

## Preparation snapshot and unchanged gates

Production created isolated branch `docs/cod-103-cod110-amendment` at exact PR #168 head `e06d7404955abeb7a002b54ad740399dfe10c931`. The original dirty root and existing COD-105 worktree are not preparation workspaces. COD-105 remains the sole active implementation issue; this preparation grants no overlapping source ownership.

Retain the first-child staging exception, genuine non-main-base hosted proof, serial non-bypass strict-freshness main merges and full own-main CI, applicable independent verification/review, two current native targets (`macos-arm64`, `windows-x86_64`), and final complete accepted-release gates. COD-104/admin bypass configuration, COD-99 history, COD-56 original S05/R05/A09 and existing closure owners remain unchanged.

Independent Evidence Verifier's accepted [v4.0.4 publication audit](../../../testing/COD-103/release-baseline-v4.0.4/report.md) establishes the fully published baseline at exact source `7125900c740297be5c60821dbeb9811f32a5894a` for the two approved native targets. Its authored report, result and manifest are preserved unchanged. This resolves the baseline-publication prerequisite only, not later COD-54 qualification, platform acceptance, queue completion or aggregate closure. Reconcile the complete selected final delta before final release authorization.

The user clarified on 2026-10-30 that Windows 11 tests will run on a Windows machine after the PRs are pushed; Engineer recorded this on COD-103 comment `1ce18dd1-e446-468d-b491-3b5b7ecb0edf` and COD-56 comment `62b8347d-ebc5-41e7-b639-5fbe7dac415d`. The planned route is reviewed PR push first, then the user's actual Windows 11 x86_64 desktop PowerShell 5.1 and supported PowerShell 7 A09 results, followed by independent acceptance. Local Windows 11 provisioning is not a prerequisite to PR push. This records sequencing only: no result, pass, waiver, account/settings/endpoint change or early issue advancement is granted. Applicable acceptance and release stay pending until accepted results; authorized non-bypass strict-freshness merger capability remains a separate gate.
