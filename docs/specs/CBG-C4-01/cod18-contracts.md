# COD-18 trust and protected integration contract proposal

## Identity, authority and scope

- Work: **CBG-C4-01/F01/S04 / [COD-18](https://linear.app/codebasegraph/issue/COD-18)**, P0, CodebaseGraph Extension. Supplement revision **0.1**, 6 November 2026, Australia/Darwin. Status: **proposed documentation for review**.
- Inputs: historical [canonical revision 0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md), SHA-256 `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e`; [COD-16 contracts at final handoff](https://github.com/rabii-chaarani/codebaseGraph/blob/0c49902ba7e12e10a8a93ba8540c2be0527d7d47/docs/specs/CBG-C4-01/cod16-contracts.md), SHA-256 `c5c5f523deb9a214f9218c15b63c67e66b4cdc429d7a45cf156bdda687af6847`; [COD-17 C1/Q1 decisions](https://github.com/rabii-chaarani/codebaseGraph/blob/f67e1d95862484dee34c8e818c98b72959366c5b/docs/specs/CBG-C4-01/cod17-decisions.md).
- This supplement proposes an **explicit D11 amendment** selected by the user for the sole-maintainer repository. It does not claim all D01-D23 wording is unchanged. Historical files and their approvals/reviews remain preserved. [Decisions](cod18-decisions.md) identify the selections; [planned cases](cod18-fixtures.md) and [review evidence](cod18-review.md) distinguish definitions from execution.
- Documentation baseline: COD-16 head `0c49902ba7e12e10a8a93ba8540c2be0527d7d47`, whose nine documents are inherited unchanged. Product observation baseline: `90e2ab6fae2767bdb158e19f0072d43568e80dbc`. PRs [185](https://github.com/rabii-chaarani/codebaseGraph/pull/185) and [184](https://github.com/rabii-chaarani/codebaseGraph/pull/184) remain separate prerequisites; this work does not merge them.
- Root owns only `cod18-contracts.md`, `cod18-decisions.md`, `cod18-fixtures.md` and `cod18-review.md` under this specification directory, plus serialized documentation Git delivery. Direct documentation work; no implementers assigned. Scryer proportional ledger `chg-ymqbv8` has no proposed architecture claims, links or folds. Unrelated pending intent/drift is outside this scope.
- [COD-19](https://linear.app/codebasegraph/issue/COD-19) owns the final reviewed specification revision, exact full-SHA **HUMAN PRODUCT IMPLEMENTATION APPROVAL** and repository procedure transition. This proposal supplies none of those approvals. No product code, live workflow/protection, daemon, authority migration, merge, release or issue closure is included.

The product has no Scryer runtime or approval dependency. Scryer remains this repository's architectural authority until the explicit COD-19 transition. Maintainers retain aggregate closure. COD-25/F04 Q1 sufficiency, COD-40 concrete scenarios and D17 measured, human-approved numerical budgets remain with their owners.

## C18-01 - Human authority and proposed D11 amendment

Proposed replacement for D11:

> Require explicit human authorization covering the exact architecture, constraint and waiver revision. When maintainer-controlled trust configuration enables sole-maintainer mode, the designated maintainer may authorize a proposal they authored through the approved manual workflow action. Otherwise require a qualifying GitHub PR review from a designated human other than the proposal author.

D10 continues to require human approval for every accepted architecture/constraint change. D16 continues to require explicit GitHub authorization of scoped, expiring waivers. The selected sole-maintainer identity is `@rabii-chaarani`, immutable GitHub user ID `50892556`, in repository ID `1235031738` (`rabii-chaarani/codebaseGraph`). Usernames and repository names are display/routing values; resolve identities independently and reject an ID mismatch.

`authorization_mode` is `designated_review` or `sole_maintainer_action`, selected only by active trusted configuration. Sole-maintainer mode names exactly one designated human maintainer; the proposed initial identity is above. A mode/identity change is a separate trust update. Do not infer mode from PR authorship, repository ownership, collaborator counts or candidate YAML. A configuration that claims sole-maintainer mode while naming multiple active human maintainers is invalid, rather than a grant of self-approval to each.

The exception removes independent human approval in this mode. Required fresh independent engineering verification/review remains a separate repository gate; an AI review is not the human authorization. The user's policy selections are not an approval of any particular native model, verifier artifact, integration attempt or product implementation revision.

GitHub prevents authors from submitting an approving review on their own PRs. The selected manual action is therefore a separate authorization kind, never a fabricated `APPROVED` PR review. See [GitHub review behavior](https://docs.github.com/en/pull-requests/how-tos/review-pull-requests/reviewing-proposed-changes-in-a-pull-request).

## C18-02 - Bootstrap and trusted configuration lifecycle

Before first-model acceptance, establish an explicit maintainer-authorized trust anchor outside candidate control. It identifies the repository/model scope, authorization mode and human IDs, immutable verifier artifact/source and configuration revisions/digests, approved workflow/producer identities, effective shared execution-policy reference, supported final path and its qualification evidence. References use the C16 immutable identity/digest contracts. A branch name, latest release, check label or proposed source version cannot replace a pin.

Bootstrap records the exact trust configuration and authorizing maintainer decision against immutable content. Absence of an earlier trust revision is permitted only in that explicitly authorized bootstrap operation. A candidate cannot bootstrap itself. COD-19 product implementation approval and later trust activation are separate decisions; publication of this document activates neither. The current product baseline is not assumed to contain an architecture verifier. Concrete verifier/workflow/configuration pins must exist and be approved before activation; none is invented here.

After bootstrap, a proposed trust update is checked and authorized under the **previously active trusted revision**, with an expected prior trust identity. It cannot activate itself through its proposed verifier, identities or gate. Validate the replacement and conditionally activate it; concurrent changes conflict. A product/model candidate containing trust-related files cannot activate those bytes in its own acceptance attempt. After separate activation, dependent candidates must establish current identities and authorization again.

Resolve active trust and shared operational resource settings through the existing repository configuration owner/trusted execution context. Candidate-supplied references can acknowledge expected values but cannot select the active trust root, producer, verifier or gate. Do not introduce a parallel settings registry. Exact implementation wiring/files require the later approved Scryer brief. Required fields/pins that are unavailable block activation/evaluation as applicable; no permissive defaults.

Trust updates and integration attempts must participate in the same authoritative invalidation/serialization contract. A trusted configuration that changed during an attempt invalidates its authorization. Retain historical decisions; never roll an accepted state back merely because a later authorization becomes unavailable.

## C18-03 - Exact approval subject and evidence kinds

Compute the governing closure from the pinned root and its complete explicitly referenced architecture, constraint and waiver files using C16-01/03. Bind the exact path set and raw-byte digests, including the root; comments, formatting, changed imports and changed waiver bytes alter this closure. Hash equality is independently recomputed, not accepted from the submitter. Retain the reviewed/approved commit and applicable accepted baseline separately from the actual integration commit.

Replace the historical supplement's PR-review-only `observed_reviews` assumption in the **future reconciled contract** with tagged authorization evidence:

```text
ApprovalEvidence =
  { kind: github_pr_review, reviews: VerifiedReview[] }
  | { kind: sole_maintainer_action, action: VerifiedMaintainerAction }

AuthorizationObservation = {
  evaluation_id, actual_integration_commit, actual_integration_tree,
  expected_state_id, trust_config_ref, approval_evidence: ApprovalEvidence,
  observed_at, waiver_validity, decision
}
```

This is a proposed amendment to C16-01, not a claim that existing published bytes already use this shape. COD-19 must reconcile the final versioned schemas, linked operations and report records. Existing C1 Rust request variants/fields/executors remain protected; no legacy type or graph-query behavior changes here.

In designated-review mode, fetch complete current GitHub facts and verify review ID, immutable human identity, effective submitted approval state, reviewed commit, exact governing closure and baseline. At least one designated qualifying human must differ from the proposal author. Resolve effective opinion state without selecting an older approval around a later disqualifying state; comments/pending reviews do not themselves constitute approval. Dismissed, disqualified or invalidated relied-on approvals cannot qualify. If facts/ordering/eligibility cannot be established, return UNKNOWN. Preserve normal repository review requirements independently.

In sole-maintainer mode use C18-04. In both modes, fresh authorization is separate from deterministic evaluation identity. A valid old evaluation can remain reusable only under C16 input/result rules; it cannot supply a current authorization observation. Accepted intent remains historical accepted state until a valid new transition; revocation blocks pending/future transitions rather than silently undoing past transitions.

## C18-04 - Explicit manual sole-maintainer action

The approved pinned workflow exposes a deliberate approval action for one integration attempt. The maintainer must explicitly authorize that subject. Starting an advisory evaluation, assigning an agent, holding repository write/admin access, changing a label, seeing a green check or supplying `approved: true` in a candidate is insufficient. An agent must not originate the human approval from general task authority. Candidate execution receives no credentials capable of creating that approval as the maintainer.

The approved action uses a **new manual `workflow_dispatch`** with explicit `decision: approve` and the following strict subject. All identity/digest fields use C16 types; dispatch values are acknowledgements to compare with independently resolved trusted values, never authority overrides.

| Field | Required binding |
| --- | --- |
| `approval_version` | Exact supported `sole-maintainer/0.1` |
| `repository_id`, `pull_request_number`, `target_ref` | Trusted repository ID, one open proposal PR, `refs/heads/dev` |
| `head_commit`, `expected_base_commit` | Full immutable PR head and target-base revisions |
| `governing_closure_digest` | Complete root/architecture/constraint/waiver closure |
| `expected_state_id` | Authoritative accepted state, including comparison/waiver ratchet |
| `trust_config_ref` | Exact currently active trusted revision |
| `decision` | Explicit `approve`; no default approval value |

The trusted executor independently authenticates the action's platform provenance and verifies the designated human account, repository, event, executing workflow source, approved verifier/configuration and exact subject. Actor/run IDs copied into YAML or CLI inputs do not authenticate a caller. The authorization context reference is a locator resolved through this trusted execution path, not a bearer grant. A GitHub account is the designated human identity signal; this contract does not claim proof of physical human presence from an actor string alone. Credential separation and the explicit human action are required premises of qualification.

`VerifiedMaintainerAction` records the authenticated account ID, repository/PR, run ID/attempt, event, workflow ID/path/source revision, authenticated producer, verifier/configuration digests, exact approval subject, observed status and decision. These are verified observations, not candidate assertions. Retain compact provenance in the existing summary table; no separate event/evidence database or signing service is presumed.

Automatic triggers and workflow reruns cannot supply fresh sole-maintainer approval. Each retry needing authorization requires a new manual dispatch. Reruns retain the original actor's privileges, so checking that actor alone is insufficient; reject rerun attempts for this approval protocol. See [GitHub rerun semantics](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs).

Approval is confined to the exact subject and attempt. A changed head, base, governing closure, expected accepted state or trust revision requires re-evaluation and a new action. Report-only commits may reuse qualifying deterministic evidence under C16, but do not inherit this attempt's approval. A terminal failed/cancelled/completed run is not a reusable authorization for another transition. A successful transition consumes that attempt's authority; concurrent duplicate attempts cannot overwrite it.

Withdrawal/cancellation/disqualification before the authoritative transition must prevent that attempt's promotion. Establish its current validity at the final boundary; a cancellation request or an earlier observation alone is not proof that remote execution stopped. After a committed transition, discover/report the committed identity and receipt; do not claim cancellation undid it. If the platform cannot enforce this boundary, retain unsupported/UNKNOWN qualification and block acceptance under C18-08.

## C18-05 - Selected integration path and producer boundary

Selected path under qualification: `github-dev-manual/0.1`, repository ID `1235031738`, target `refs/heads/dev`, maintainer-controlled workflow provisionally named `.github/workflows/architecture-integration.yml`, launched from the trusted default-branch control revision. Its actual approved full source revision, workflow ID, verifier/configuration and producer identity must be pinned before use. A dispatch against another branch/tag cannot select a different control implementation. Pin reusable actions/workflows and executable dependencies as part of the approved control closure.

The candidate is data for the trusted verifier. Privileged control execution must not load candidate workflows, actions, verifier binaries, build scripts or dependencies as its trusted implementation. Execute necessary candidate tests in the qualified unprivileged boundary; verify the tested inputs, expected producer/run, complete result payload and linked durable execution evidence. Candidate-produced PASS cells remain untrusted and mandatory derived results are regenerated as C16 requires.

The final path must enforce that every acceptance-capable caller uses this authenticated, approved control implementation and current state. A matching GitHub Actions application ID plus check name is insufficient to distinguish workflows. Repository protections must reject paths that can bypass this final decision; ordinary merges, direct pushes, alternate workflows, API calls and local CLI/MCP previews are not implicitly qualified paths. Administrative powers are not a waiver of the product gate.

Manual dispatch is an authorization entry, not a claim that its job check satisfies a PR required-check rule. GitHub documents event restrictions for workflow-job checks; an arbitrary dispatch green cannot serve as the protected gate. The approved implementation must demonstrate the actual connection between authenticated final decision and the only permitted integration transition. No external GitHub App/controller is assumed by this proposal. If available platform mechanisms cannot provide that connection, this path remains blocked. See [required-check behavior](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

This proposal makes no change to existing release workflows or `dev` to `main` delivery. Merge-method execution details and exact configuration wiring must be settled in the implementation brief within the approved contract; no merge API is assumed to provide an expected-base or accepted-state transaction merely because it accepts a head SHA.

## C18-06 - Authoritative final transition

The required boundary covers **both** current authorization and the actual integration/state transition. A local mutex, concurrency group, two independent writes or a previously successful status does not demonstrate it. The implementation must establish one authoritative conditional protocol that prevents a stale authorization, base, content or expected accepted state from being promoted. Qualification must identify its linearization point and every writer covered by it. If no such supported protocol is available, refuse promotion rather than weakening freshness.

The trusted attempt must:

1. Resolve active trust, current target base and authoritative accepted-state ID; compare them with the approved subject.
2. Construct/identify the actual combined integration content for that base and proposal, including interactions with accepted source/ownership/policy/waiver state. A synthetic test-merge SHA or independent head success cannot substitute for actual governed content equivalence.
3. Resolve complete C16 input identity, shared effective execution-policy values/digest, required inventories and executed evidence. Regenerate required results and verify persisted table equivalence. Enumerate mandatory rows independently of supplied rows.
4. Require every applicable mandatory record complete and effectively PASS. Preserve simultaneous violation/unknown diagnostics. No skip/neutral/missing dependency result becomes an architecture PASS.
5. Freshly establish qualifying human authorization, approved producer/verifier/configuration, waiver authority/expiry and the latest comparable waiver ratchet at the final boundary. Equality with expiry is expired. No invented grace or freshness interval applies.
6. Conditionally transition only if the approved head/base/content, active trust and expected accepted state still match. Bind the resulting accepted intent, governed source/ownership/projection and waiver ratchet coherently to actual integration. A change yields conflict/invalidation and explicit re-evaluation; never restore an earlier ratchet or overwrite an accepted improvement.
7. Record actual integration commit/tree and resulting accepted identity in the post-transition receipt. Keep original evaluated provenance and EvaluationId; independently establish permitted content/build equivalence. The receipt is observational output, not an input to its own hash.

The final authorization must remain valid through the transition's qualified boundary. GitHub's head-SHA merge parameter, strict checks, manual invocation and workflow concurrency are each partial mechanisms to assess, not proof of the combined guarantee. In particular, asynchronous merge acceptance/queueing is not evidence of completed integration. See [GitHub merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request).

## C18-07 - Invalidation, failure and explicit recovery

| Trigger | Required result/recovery |
| --- | --- |
| Missing bootstrap, unapproved pins, unsupported final path | Block promotion; no first-model acceptance. Missing facts remain UNKNOWN; known unsupported path uses `unsupported_integration_path` |
| Candidate-selected mode/reviewer/verifier/gate or known wrong producer/identity | Reject authorization/path or integrity as applicable; never adopt candidate trust |
| Failed, skipped, cancelled, neutral, absent or unfinished mandatory work | No acceptance; preserve completed input-bound evidence and unfinished UNKNOWN records. A known independent violation remains visible |
| New/late qualifying approval | New final authorization attempt. Reuse deterministic records only after C16 identity/integrity checks; no stale-green shortcut |
| Edited/dismissed/disqualified relied-on review; withdrawn/cancelled/disqualified manual action | Invalidate relied-on authorization; establish current valid evidence again. Missing current facts are UNKNOWN |
| Changed PR/base, governing closure, actual integration content, effective policy or trust | Invalidate affected evidence/approval; `input_changed` or conflict as applicable. Sole-maintainer mode needs a new exact manual action |
| Waiver expiry after earlier green or unavailable current validity | Re-evaluate ordinary/raw overrun and current effective check; no waived PASS from expired/unknown authority. Retain any provable violation |
| Accepted metric 10 becomes 8 while candidate 9 still expects 10 | `state_conflict`; recompare with 8 and reject deterioration. Do not reuse the old ceiling or undo 8 |
| Individually acyclic proposals combine into a cycle | Evaluate the actual combined graph; block on the applicable violation, with qualified coverage |
| Facts unavailable, rate-limited, partially paged, contradictory or stale | UNKNOWN for dependent mandatory predicates; no empty successful inventory or default approval |
| Timeout/disconnect after possible dispatch | Keep real execution/leases accounted for until completion/cleanup. No automatic replay; inspect current accepted/integration identity before explicit recovery |
| Transition committed but receipt lost | Discover actual committed state; preserve it and recover receipt/provenance where possible. No blind rollback or repeated promotion |

Use C16 error/outcome/CLI semantics; reason diagnostics may distinguish these causes without inventing another success state. `state_conflict`/`input_changed` require explicit re-evaluation. Approval/path/integrity rejections block promotion. Cancellation/resource exhaustion preserve valid state and unfinished UNKNOWN records. Caller timeout is not native cancellation. Reuse of input facts never relabels original execution as a new run.

PR/base/review/trust events can request invalidation/re-evaluation, but asynchronous event delivery is not the final freshness guarantee. Every final attempt rechecks required live predicates through the authoritative boundary. Waiver expiry is checked there even if no event arrived. New grants cannot retroactively authorize a failed attempt. Historical accepted evidence remains distinguishable from a current authorization grant.

## C18-08 - Observed platform facts and qualification stop gate

Read-only observation on 6 November 2026; mutable settings require fresh verification at delivery:

| Observation | Evidence/limit |
| --- | --- |
| Public, personally owned repository; ID `1235031738`, default `main` | GitHub repository API read; not an independent trust activation |
| Active ruleset `26100127` covers default branch and `refs/heads/dev` | [Ruleset](https://github.com/rabii-chaarani/codebaseGraph/rules/26100127); no bypass actors returned; effective `dev` rules independently read with `gh api` |
| Zero required approving reviews; stale-review dismissal and last-push approval disabled | Active ruleset snapshot; no designated-human/native approval enforcement inferred |
| Strict required check `required`, integration/application ID `15368`; linear history, PR and code-quality rules | Producer application binding does not pin a workflow/verifier; current rules allow squash/rebase |
| Classic protection endpoint unavailable to connector (403); local authenticated GET returned 404 for `main`/`dev` | Record classic endpoint result separately from active ruleset protection; do not call these branches unprotected |
| `ci.yml` excludes `dev` PRs and has no architecture final path | [Exact workflow revision](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/.github/workflows/ci.yml); same CI blob on current dev base |
| COD-16 head has zero check runs and zero PR-triggered workflow runs | Exact head `0c49902ba7e12e10a8a93ba8540c2be0527d7d47`; absence is not green |
| GitHub merge queues are documented for organization-owned repositories | [GitHub merge queue availability](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue); no organization migration is proposed |

**Current qualification: no architecture acceptance path is proved.** The selected workflow is a concrete planning direction with mandatory proof obligations, not an installed or supported gate today. COD-37 implements/verifies the authorization contract; COD-46 integrates the final decision; COD-47/F09 must demonstrate real hosted enforcement, including the protected-path connection, alternate-producer/path rejection, revocation/expiry races, expected-state serialization and actual combined content. A configurable check is not passing evidence.

If the personal-repository platform/configuration cannot couple these predicates to the final transition without another authority component, report that capability gap and return it for an explicit scope/architecture decision. Do not introduce a hosted service, GitHub App, extra durable store, weaker freshness rule, organization migration or protection bypass implicitly. Keep acceptance blocked/UNKNOWN until an approved, executed qualification demonstrates the required path.

## C18-09 - Linked-contract impact and proposed acceptance

| Existing identity | Required reconciliation; ownership stays unchanged |
| --- | --- |
| D11 / R10 / A13 | New sole-maintainer/manual-action exception and tagged evidence; otherwise retain designated independent human review. Propose a new canonical revision through COD-19; preserve historical 0.2 |
| D10 / D16 / R07 / A10 | Human approval, explicit waiver references, expiry/raw overrun and zero-deterioration remain; references must recognize the new evidence kind without granting an expiry override |
| R02/R09 / A02/A09/A12/A17 / APR-03 | Exact expected-state, composed inputs, provenance/integrity and conflict recovery remain mandatory |
| A18 / APR-04 | Add automatic-trigger/rerun/replay/withdrawal controls and actual manual-workflow-to-protected-transition proof; retain all previous failure/freshness cases |
| C16-01/03/06/08/09 | Reconcile AuthorizationObservation, `approval_ref`, authorization-context resolution, guarded accept/report logical records and schemas. Preserve C1/Q1, existing errors/resources and one-table output |
| COD-37 | Current approval evidence, sole identity/bootstrap, exact subject and invalidation fixtures; no implementation assignment is made here |
| COD-33/36/41/42/43/44/45/46/47 | Respectively reconcile state/ratchet, evidence/result integrity, operation/transport/report records, final-path enforcement and hosted execution cases under their existing gates |
| COD-19 | Publish/pin the final revised specification, review changed contracts/linked-issue impact, record exact-scope human implementation approval and perform the explicit repository procedure transition |
| COD-17/C1/Q1; COD-25/F04; COD-40; D17 | No compatibility/resolution expansion, pilot sufficiency, scenario functionality or numerical calibration is granted |

Proposed acceptance refinements, preserving A13/A18 identities:

- **A13 / R10:** Given a maintainer-established trusted configuration, an exact qualifying designated review in normal mode or an authenticated explicit sole-maintainer manual action in sole mode can establish human authorization. Candidate-selected trust/mode, wrong identity, absent/invalidated action, changed governing files, stale subject or unavailable required facts cannot authorize acceptance. This establishes the authorization predicate only; all other mandatory acceptance checks still apply.
- **A18 / R09/R10:** Given the selected hosted path, demonstrate that failed/skipped/cancelled/missing work, automatic triggers/reruns, revoked/disqualified approval, expiry after green, wrong producer/control revision, changed combined content, stale expected state and alternate merge paths cannot cross the real final decision boundary. If a required condition cannot be enforced, retain the named unsupported/UNKNOWN gate.

[Planned cases](cod18-fixtures.md) trace these criteria and the inherited C16 cases. They are definitions, not executed extension tests. No new product claims/topology are invented to fill traceability rows. Detailed implementation ownership and evidence briefs belong in Scryer after the applicable approval/transition.

## C18-10 - Handoff and closure boundary

User selections settle the proposal's sole-maintainer policy, manual approval record, pinned `dev` workflow direction and old-trust update boundary. They do not approve a final committed specification or activate a runtime trust configuration. Concrete activation pins, the authenticated producer mechanism, operational wiring and the authoritative integration protocol remain implementation/qualification prerequisites, with unresolved capability leading to the explicit stop gate rather than guessed guarantees.

Preserve all nine inherited documents byte-for-byte. The COD-18 document diff requires fresh independent final review and applicable documentation checks before delivery. Report source/document/configuration snapshots, actual checks and limits. COD-18 remains open through its review/handoff gates; no next issue is advanced or aggregate closure claimed.

Reusable knowledge assessment: the selected policy and rationale belong in this specification/decision record. No additional wiki/memory claim is needed for an unimplemented contract, and no unrelated memory/model state is changed.
