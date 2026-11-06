# COD-18 decisions and planning authorization

## Identity and authorization scope

- Work: `CBG-C4-01/F01/S04` / [COD-18](https://linear.app/codebasegraph/issue/COD-18), P0, CodebaseGraph Extension.
- Date: 6 November 2026, Australia/Darwin. Human authority: the user's explicit answers in this COD-18 chat, following the COD-16 handoff from chat `01a1b055-af4c-7ee3-a44a-5e937379b1aa`.
- Authorized objective: reconcile the handoff and prepare a concrete trust/integration contract proposal for review. The user subsequently asked why work was not continuing; this authorizes completing the proposal within that planning scope, not product implementation.
- Proposed supplement: [cod18-contracts.md](cod18-contracts.md), revision 0.1. Historical [canonical revision 0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md) and all inherited COD-16/17 documents remain unchanged.
- This is an explicit proposed amendment to D11 and related authorization contracts. It is not represented as unchanged adherence to the historical independent-human wording.

## Explicit selections

| ID | Decision locator and exact selected answer | Consequence |
| --- | --- | --- |
| C18-D1 | `request_user_input_async` call `call_739ebd9d4f49439eb78a5e1d87921cae`, item 0: "i'm the sole maintainer of the codebase" | Establishes the sole-maintainer planning context. By itself this did not repeal D11 or approve self-authorization |
| C18-D2 | Call `call_6a1c420a00cc44eda9c8b291bcc8fb0c`, item 0: "Plan the pinned dev workflow; keep acceptance blocked until proved (Recommended)" | Select one pinned maintainer-controlled workflow into dev; actual freshness, composition, producer and serialization guarantees still require COD-47 proof |
| C18-D3 | Same call, item 1: "Separate trust update under the old trust revision (Recommended)" | Separate trust updates, authorized under the active prior revision; reject same-candidate self-activation |
| C18-D4 | Call `call_2a1af327399b427b8c09e8de8ceb462d`, item 0: "Propose an explicit sole-maintainer self-approval revision" | Propose a scoped D11/R10/A13 amendment; preserve the historical independent-review contract and require linked-contract impact/review through COD-19 |
| C18-D5 | Call `call_771f45ce135245748c19be608f26e7cb`, item 0: "Manual approval in the pinned workflow (Recommended)" | Use an explicit manual action bound to head, full governing closure, expected state, trusted configuration and attempt. Automatic runs and reruns cannot count as fresh approval |

These are human selections, not defaults inferred from preselected options or silence. Locators identify chat decisions; they are not GitHub reviews, native authorization observations or product implementation approval records.

## Resulting amendment and tradeoff

Sole-maintainer mode is explicitly enabled by trusted configuration outside candidate control and identifies `@rabii-chaarani`, GitHub user ID `50892556`. The maintainer may authorize their own proposal through the approved manual action. Normal designated-review mode retains the different-human requirement. Candidate files cannot select either mode or the active identity.

This substitutes explicit sole-maintainer accountability for independent human approval in the scoped exception. Required independent engineering review remains a separate procedure. GitHub's prohibition on authors approving their own PRs is respected by using a distinct manual-action record; no approving PR review is fabricated or delegated to an apparent second identity.

The proposed protocol requires a new explicit manual dispatch per authorization attempt. It independently binds platform identity, control/verifier revisions and exact subject, rejects automatic/rerun/replayed authority, and retains all final decision checks. These are proposed contract details reviewed with the complete document; no live workflow was created or run to approve a candidate.

## Ownership and retained gates

- Direct documentation work. Root is sole writer for the four new COD-18 contract/decision/fixture/review documents and serialized documentation Git actions. No product/manifests/lockfiles/workflow/protection ownership is assigned. The canonical source checkout and both earlier evidence worktrees are preserved.
- Scryer proportional documentation ledger `chg-ymqbv8`; no model entries/claims/links/folds are needed for this documentation proposal. Existing pending intent/drift is not adopted.
- COD-19 must reconcile a newly reviewed canonical specification revision, pin its full SHA and exact scope, record HUMAN PRODUCT IMPLEMENTATION APPROVAL and perform the repository procedure transition. Neither these selections nor documentation publication satisfy that gate.
- COD-37/46/47 own subsequent authorization/final-path implementation and hosted qualification under their own gates. No demonstrated protected integration path is claimed by this planning record.
- COD-25/F04 Q1 sufficiency, COD-40 concrete scenarios, D17 calibration and maintainer aggregate closure remain separate. No next issue advancement, merge, release or issue closure is authorized here.

The contract, [planned cases](cod18-fixtures.md) and [review record](cod18-review.md) carry the concrete proposal and its evidence limits. Reusable knowledge is retained here rather than duplicated into advisory memory.
