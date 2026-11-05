# PR158 focused main-integration proposal review — 2026-10-29

## Verdict

**Proceed with specified conditions. No decision-blocking finding.** The proposed normal merge of pinned main into the repair branch is a proportionate way to remove the demonstrated CI merge conflict. This is branch integration, not authorization to merge PR158 into main, publish, release, or close COD-53.

Reviewed `merge-conflict-plan.md` and `postpush-merge-conflicts.txt`; repair parent `359d946e91c6cb3ee7e170b73c6e9bab577b0b04`, main parent `9f69a0bd4b468eaa8652d933d518fafb8c596b6a`. Applied the adversarial-proposal-review-engineer and codebasegraph-engineer skills. Review remains read-only: no merge, product edit, build, tests, install, or Git write performed. Fresh initial review session at the requested Sol / High settings is reused for this focused material revision.

## Evidence and option choice

I read the supplied conflict artifact and independently inspected both committed manifests and generated conflict-tree metadata through read-only Git commands. The sole recorded content conflict is the adjacent `codebase-graph`/`pulldown-cmark` dependency block in `crates/k-wiki/Cargo.toml`. Main uses workspace version 4.0.2 and parser lower bound 0.13; the repair uses workspace version 4.0.0 and parser lower bound 0.13.5. The correct resolution retains **codebase-graph 4.0.2 + pulldown-cmark 0.13.5**, with unchanged feature declarations. The automatically merged lock metadata retains the parser repair while moving the three workspace package versions to 4.0.2. No new dependency/ABI decision follows from synchronizing already merged workspace metadata.

A normal merge preserves history and both parents. Rebase/force-push is unnecessary. Choosing either conflict side wholesale would discard the parser floor or main's version alignment. The proposal correctly avoids both. The generated conflicted tree `49d14691a82567d05d52dd8707db786ee59778ea` is diagnostic evidence only, never a deliverable tree.

The main-side history includes source, tests, workflows, docs and release metadata, not just the four archived helpers. These are already upstream changes, not newly implemented work under this repair. Their interactions still require final combined-tree checks. The prior combined-tree logs show release-gate success, 44 xtask tests passed, and a completed package dry-run at 4.0.2; they support feasibility but do not by themselves establish that the not-yet-created two-parent merge has passed. The parent-reported GitHub dirty/zero-check state is not independently queried here.

## Conditions before delivery

1. Merge exactly the pinned main SHA with the proposed normal, non-rewriting integration. Expect only the stated manifest conflict. Stop for any additional conflict or unexpected resolution; do not silently broaden manual edits.
2. Resolve only the two dependency lines as above. Preserve all main-side automatic changes and original repair semantics, Windows default bundling, native asset/ABI identities, historical archives, original 65-record index bytes, reports and hashes. Keep immutable approval/spec references unchanged.
3. Inspect the final tree against both parents and the expected resolved merge, not only the first-parent diff. The merge must retain both main ancestry and repair ancestry and contain no conflict markers. Keep additive integration/evidence notes distinct from historical evidence.
4. Execute the planned final combined-tree checks and the required focused Sol / Max final review before completion. Previous package/test evidence may be reused only with exact relevant input equivalence recorded; record resulting new head/merge and rerun affected checks. Do not treat expected test counts as results. Strict checks, audit, release policy, configuration and archive/index audits remain required.
5. Push non-force to the existing PR, verify the actual new head and mergeability, then collect its own CI outcome. Zero checks or only old-head runs are not a passing result. Unexpected remote changes require reconciliation. This review does not authorize integration settings changes, bypasses, merge/release, or platform qualification claims.

No established defect remains in the proposed conflict resolution. The material conditional risk is ordinary combined-tree integration: upstream source/workflow changes can interact with the repair despite a one-block textual conflict. The final checks and focused review already proposed are the proportionate gate; no speculative claims, new product tests, or unrelated implementation are needed.

## Limits and knowledge assessment

Consulted task-local evidence and refreshed wiki memory. No new external factual premise required additional research; the earlier review established the parser advisory/MSRV facts. The current user authorization is supplied by the coordinator, not independently reconstructed from user chat. Final merged source/runtime behavior was not audited or tested here. No durable memory entry is warranted from this task-local integration review.
