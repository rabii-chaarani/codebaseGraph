# COD-16 decisions and documentation authorization

## Identity and scope

- Work: `CBG-C4-01/F01/S02` / [COD-16](https://linear.app/codebasegraph/issue/COD-16/cbg-c4-01f01s02-concrete-ontology-yaml-evidence-and-public-operation), P0, CodebaseGraph Extension.
- Date: 6 November 2026, Australia/Darwin.
- Human authority: explicit user answers and the final documentation implementation request in chat `01a1b055-af4c-7ee3-a44a-5e937379b1aa`.
- Reviewed planning input: temporary COD-16 proposal revision 0.1, SHA-256 `9ccebff3a98173ab83b1362332f16e445ab9d05d9aeb6d7da931bb2a1038cb7e`, plus the decisions/refinements below. The original temporary draft is preserved; finalized text is [cod16-contracts.md](cod16-contracts.md), supplement revision 0.2.
- Historical governing proposal: [revision 0.2 at 0a66cb31e397e523190618f211a613f7a095d9a0](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md). Preserve D01–D23, R01–R12, A01–A18 and APR-01–APR-05.
- C1/Q1 remain settled by [COD-17's decision record](cod17-decisions.md). This record adds no product implementation approval.

## Explicit user choices

| ID | User decision reference | Selected behavior and consequence |
| --- | --- | --- |
| C16-D1 | Async question “COD-16 D1 — Ownership evidence”, answer “Use conservative rule (Recommended)” | Unique independently qualified owner; competing owners excluded. Unsupported/shared-helper evidence can remain UNKNOWN. No human per-mapping approval queue. |
| C16-D2 | Async question “COD-16 D2 — Acceptance API”, answer “Expose guarded accept (Recommended)” | Public architecture-accept exists but promotion blocks outside the trusted integration path. Local PASS is not approval. |
| C16-D3 | Initial answer “do not ceilings”; clarifying question `execution_bounds_intent`, answer “Remove fixed caps (Recommended)” | Remove invented fixed numerical resource caps while retaining configurable execution safeguards, cancellation and UNKNOWN for incomplete mandatory checks. Existing graph_query bounds and semantic cardinality/grammar remain. |
| C16-D4 | Question `resource_policy`, answer “Shared repository policy (Recommended)” | Explicit repository settings shared across CLI/MCP/CI; caller overrides only tighten; missing required settings block evaluation. No invented numerical defaults. |
| C16-D5 | Question `contract_anchor_support`, answer “Export/path facts first (Recommended)” | v1 ownership qualification uses supported export/reach facts. Linked contract anchors remain contextual, with unsupported decisive proof UNKNOWN. No new parser, adapter or durable manifest. |
| C16-D6 | Question `scenario_contract_reference_match`, answer “Exact contract set (Recommended)” | Scenario contract comparison uses complete canonical-set equality of pinned ArtifactRef arrays. Reordering is equivalent; additions/removals/changed identity fail; duplicate refs diagnose; unavailable contents are UNKNOWN. |

Question identifiers refer to the chat prompts; they are decision locators, not GitHub architecture approvals. The user's answers, not preselected options or elapsed time, establish the choices.

## Accepted finalization plan

After the planning review and focused verification, the user explicitly requested **“PLEASE IMPLEMENT THIS PLAN: COD-16 concrete contract finalization”** and supplied the complete plan. The authorized deliverable is documentation/contracts and illustrative fixtures, preserving historical evidence and obtaining independent final review of the actual persisted documentation diff before PR handoff.

The requested refinements define domain/scope/coverage and qualified lineage; tagged scenario parameters and obligations with explicit paired reachability for bypass checks; exact contract-set comparison; immutable evaluated provenance and fixed-exclusion content equivalence; separate live authorization/post-transition observations; permit retention, cancellation and pre-dispatch-only replay. These are incorporated in the supplement and [fixture definitions](cod16-fixtures.md).

This instruction approves performing that documentation work. It **does not** approve dependent product implementation, numerical metric calibration, authority migration, merge, release or issue closure. It is not a COD-19 full-SHA product implementation approval record. Publication/review does not silently turn it into one.

## Preserved ownership and gates

- Root owns the COD-16 supplement, decision, fixture and review documents and serialized Git delivery for this one issue. No product file/manifests/lockfiles or other issue is assigned.
- Scryer ledger `chg-7jc6yv` records documentation work proportionally; no claims, links, directives or model folds are required. Existing pending intent/drift stays separate.
- COD-18 owns maintainer-controlled trust and supported protected integration; COD-19 owns final specification publication/pinning, the explicit repository procedure transition and exact-scope product implementation approval.
- COD-25/F04 must prove actual Q1 endpoint/coverage sufficiency before projection acceptance or calibration. COD-40 owns concrete pilot scenario functionality; D17 governs later measured/human-approved numerical budgets.
- Maintainers retain aggregate closure. [cod16-review.md](cod16-review.md) records the actual reviewed snapshot, checks and limits. This documentation delivery does not close COD-16 or advance another issue.
