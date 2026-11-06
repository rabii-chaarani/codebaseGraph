# CBG-C4-01: Adversarial proposal review and revision record

## Recommendation and scope

**Proceed with the bounded pilot with specified conditions.** The staged native C4 proposal is supportable, particularly its projection-feasibility gate. The acceptance contracts identified below must be settled before dependent implementation. This is not implementation approval, supported-platform qualification or a production-readiness verdict.

The reviewer assessed whether the proposed Graph Read Service pilot and its starting conditions are reasonable. They preserved the user's D01–D23 choices, including automatic ownership acceptance, no Scryer dependency, external-agent orchestration and YAML plus one durable evaluation-summary table as the product's persisted output. This review record is planning evidence, not a new product report format.

| Review identity | Recorded value |
| --- | --- |
| Date | 23 October 2026, Australia/Darwin |
| Reviewer | Fresh read-only agent, `agent_type: default`, `model: gpt-6.1-sol`, `reasoning_effort: max`, `fork_turns: none` |
| Review task | `/root/cbg_c4_adversarial_proposal_review`; no reviewer delegation |
| Required skill | [adversarial-proposal-review-engineer](/Users/rabii/.codex/skills/adversarial-proposal-review-engineer/SKILL.md), including proposal lenses, research protocol and quantitative checks |
| Reviewed input | [spec.md](spec.md), logical revision `0.1`, uncommitted, 256 lines |
| Input SHA-256 | `a50d13b998c31e8eb84cd62159c6090e219c021f1eaab5bd72c38a6a478bb6c4`, verified by the reviewer twice |
| Examined implementation | `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`; an observation baseline, not a selected supported pilot baseline |
| User request | Run the required adversarial proposal review with GPT-6.1 Sol at Max, then revise the draft to address its findings |
| Revision | `0.2` addresses the findings below; focused reviewer verification completed with no new material contradiction |
| Verified specification SHA-256 | `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e` |
| Review-record snapshot verified | `17256bc522f029adc362b352ff4f8b9ccfeaf82a3c663c53553b2e1a96fc8cf7`, before recording the verification result below |

The original input was frozen at `/private/tmp/cbg-c4-01-review-ub0zodd2/spec-r0.1.md` while review ran. That temporary copy is not a portable qualification artefact. The original proposal's content hash and section/line locators identify the reviewed snapshot; the draft and this record remain uncommitted. Line coordinates below refer to revision `0.1`, rather than the rewritten draft.

## Findings and remedies

| ID | Evidence class | Priority / dependency | Revision `0.2` response | Verification status |
| --- | --- | --- | --- | --- |
| APR-01 | Conditional risk | Material before automatic ownership/conformance acceptance | R03 requires independently qualifying correction/transfer evidence, previous-ownership comparison and classification-sensitive attribution; A04/A05 cover justified and self-serving corrections. | Resolved as a proposal contract; inference reliability remains to be qualified. |
| APR-02 | Decision-critical evidence gap for reuse | Material before trusted cache reuse/integration | R09/R11 separate input identity from trusted result integrity; require regeneration or verified producer-bound provenance and independently enumerated mandatory records. A12/A14/A15 cover tampering. | Resolved as a proposal contract; trusted-result behaviour remains to be executed. |
| APR-03 | Conditional concurrency failure | Material before promotion in P3/P4 | R02/R07 require an expected-state transition and revalidation on conflict; R06/R10 require qualification of composed integration content. A02/A09/A10/A17 cover the race and composition. | Resolved as a proposal contract; authoritative-path serialization remains unproved. |
| APR-04 | Decision-critical integration evidence gap | Material before claiming enforced integration | R10/P0/P4 define a trusted, non-skippable final decision, freshness/invalidation handling, actual integration inputs and an explicit hosted-enforcement proof/stop gate. New A18 exercises that path. | Resolved for the staged commitment; hosted enforcement remains unqualified until demonstrated. |
| APR-05 | Contract clarification gap | Improvement before scenario qualification | R08 requires common parameters, capability/contract references and structural postconditions for both transformations; A11 rejects weakened candidates. | Resolved as a proposal contract; scenario qualification remains to be executed. |

### APR-01 — Classification-sensitive improvements

**Input locations:** R03 lines 94–98, R04 line 102 and R05 lines 112–114.

The proposal validates mapping citations and records owner deltas, then removes source dependencies internal to each resulting owner. A source edge `s → t` originally projected as `A → B` disappears when `t` is reassigned to `A`, even though the source and its dependency did not change. Inside a populated projection this can remove a forbidden edge without triggering the existing zero-edge safeguard.

The existing independent evidence validation, pinned inference, uncertainty disclosure and visible deltas are meaningful counterevidence. They prevent silent rerolling or concealment, but do not by themselves adjudicate an improvement caused only by classification. General structural citations may support competing responsibility interpretations.

The revised contract preserves automatic acceptance while requiring evidence that distinguishes a justified correction or transfer. It separates source changes from ownership changes, uses a previous-ownership counterfactual where lineage permits, and leaves unresolved reassignment-dependent results `UNKNOWN`. A better metric is not evidence that its new classification is correct.

**Resolving evidence:** execute both a valid automatic correction and an attempted classification-only removal of a forbidden edge. Merely printing a delta and citing existing nodes must not qualify the latter. Settle this contract in P0 before dependent P2 acceptance behaviour. No human mapping-approval gate is added.

### APR-02 — Cached-result integrity

**Input locations:** R09 lines 148–150 and R11 lines 176–178.

The report exclusion and independent mapping hashes correctly detect changed inputs. They do not authenticate derived results. A complete-looking table can retain identical input identities while its outcome changes from `VIOLATION` to `PASS`, its numeric value changes, or an execution assertion is substituted. Input equality cannot distinguish those records.

Trusted evaluator selection, provenance fields and rejection of incomplete records are useful safeguards already present. A producer that regenerates and verifies results would defeat the concern; the initial draft left that requirement implicit.

The revision treats persisted result cells as untrusted until regenerated or verified through producer-bound provenance for the complete payload. The trusted verifier enumerates required records independently of candidate rows. A candidate-supplied output hash or producer label is insufficient. Recomputing mandatory results is an allowed first-version solution; no signing service or additional durable store is required. Persisted logical results must match the trusted evaluation.

**Resolving evidence:** test altered verdicts and values, missing mandatory rows and substituted execution references despite unchanged input identities. Reject or regenerate them, while allowing legitimate report-only commits to preserve independently trusted evidence. This remains inside the agreed YAML/table output contract.

### APR-03 — Concurrent acceptance and composed changes

**Input locations:** R07 line 136, R09 line 146 and R10 lines 156–164.

Two candidates with values `8` and `9` can both satisfy zero deterioration against accepted value `10`. Once `8` is accepted, the old result for `9` must not authorise a regression of `1`. Similarly, independent proposals adding `A → B` and `B → A` are each acyclic but form a cycle when combined.

The latest-state comparison, pinned baseline and integration-input equivalence requirements express the right objective. The missing contract was serialization between checking that state and promoting the candidate. GitHub's distinction between strict and loose required checks also matters: a passing old proposal head does not qualify arbitrary later base content. [GitHub protected-branch documentation](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

The revision binds a transition to its expected accepted-state identity, including the applicable source, ownership, policy and waiver comparison state. Conflicts preserve current accepted state and force revalidation. It also requires the actual combined integration content to be evaluated through the supported protected path.

**Resolving evidence:** exercise concurrent promotions and interacting changes through the actual acceptance/integration path, including preservation of an already accepted improvement. Fixing the document does not prove that the implementation serialises a remote integration path correctly.

### APR-04 — Final hosted enforcement

**Input locations:** R09 line 152, R10 lines 156–164 and P4 line 215.

GitHub review records supply reviewer, state and reviewed `commit_id`, which are appropriate approval inputs. They still need eligibility and freshness checks. [GitHub review API](https://docs.github.com/en/rest/pulls/reviews).

GitHub required checks can accept skipped or neutral results; a conditionally skipped job can report success. Head and test-merge checks are also distinct. An earlier green check therefore cannot alone establish current waiver validity or approval eligibility at a later decision. [GitHub required-check troubleshooting](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

R10 already requires trusted configuration, protection inspection and actual enforcement. This finding is an integration evidence gap, not an observed bypass of the live repository. The revision specifies a non-skippable final decision requiring complete mandatory passes, verified result integrity, current review/expiry predicates, the expected accepted state and actual integration content. It requires invalidation/retrigger handling and verification of the approved producer/workflow, rather than a job name alone.

**Resolving evidence:** P0 must identify the concrete protected integration path and its guarantees. P4 must demonstrate upstream failure/skip, late approval, invalidated approval after success, expiry after success, an incorrect producer and changed integration content. If that path cannot enforce the required freshness/transition semantics, hosted enforcement remains unqualified. Merely adding another previously successful check does not close the finding's capability requirement.

### APR-05 — Comparable scenario obligations

**Input location:** R08 lines 140–142.

Permitting different edits for different designs is reasonable. However, both transformed graphs must satisfy the same obligation before metrics are compared. A candidate that omits one of two required capabilities cannot be treated as a better extension simply because it has lower dependency reach.

The existing stable targets, preconditions, expected observations and bypass negative control are useful safeguards. R08 now makes common parameters, capability/contract references and structural postconditions explicit within the approved constraint closure. It preserves different concrete transformations and defers behavioural simulation.

**Resolving evidence:** A11 must reject a weakened candidate transformation rather than reporting it as a favourable comparison. Unresolved applicability or mandatory postconditions remain `UNKNOWN`.

## Conditional qualifications retained separately

**Projection soundness.** Concrete endpoints can also be wrong. The examined source [registers multiple IDs under normalised labels](../../../src/syntax_materializer/builder/nodes.rs#L236), [case-folds labels](../../../src/syntax_materializer/ids.rs#L97), and [selects a last matching target without caller-scope validation](../../../src/syntax_materializer/builder/semantic.rs#L550). R04/P2/A06 now require concrete-target, collision and shadowing controls with independently established expected endpoints. This is source reasoning, not an executed Rust reproduction or proof of a current pilot false pass.

**Aggregate budgets.** The proposed maxima can redistribute coupling: fan-outs `A=10, B=0` and `A=8, B=10` both have maximum `10`. This follows the proposed definition and is not an arithmetic defect. R06 now states the limitation and requires visible origin changes and explicit scope/calibration approval. No stricter per-origin policy is silently added.

**Non-local reach.** Adding `B → C` increases reach from `A` where `A → B` exists, despite A's direct edges being unchanged. R06/A09 now require affected-origin or complete-closure invalidation and qualification of intermediate observation coverage.

**Pilot topology and usefulness.** Select a supported source baseline and honest C4 topology before measurement. A single-container pilot can legitimately have no inter-container edges; P3 distinguishes that fact from synthetic algorithm fixtures. The minimum-change baseline—curated guidance plus targeted conformance tests—has lower machinery cost for the single read-only invariant, but does not deliver the agreed mapping, projection, scenario and native acceptance capabilities. The pilot must demonstrate those additional capabilities without claiming rollout readiness.

## Supported strengths and actual evidence

The official C4 definitions support the draft's distinction between [runtime containers](https://c4model.com/abstractions/container) and [functional components](https://c4model.com/abstractions/component). Intended, observed and predicted states remain separate. Permitted and required relationships remain separate. Design evaluation does not replace implementation evidence.

The earlier four safeguards survived review: raw/effective waiver outcomes, removal of internal edges at each projection level, mapping-input hashes despite report exclusion, and first-baseline trust outside candidate control. The known Rust placeholder/import gap is explicitly qualified before calibration rather than hidden as a pass.

The reviewer inspected the [read-only query boundary](../../../src/api/graph_read.rs#L424) and [existing query-test definitions](../../../src/adapters/cli/tests/graph.rs#L739). Existing tests were not executed.

Actual review work comprised source hash/HEAD/status checks; k-wiki recall, search and architecture reads; repository-local graph discovery before targeted source reads; official C4/GitHub research; and small calculations over illustrative graphs. The calculations tested classification-only edge removal, aggregate maxima, concurrent waiver regression, composed cycles, non-local reach and unchanged input identities with altered result payloads. They are counterexamples and reasoning checks, not execution of the proposed product.

The CodebaseGraph Linear search returned no `CBG-C4-01` match, and its `get_agent_context` endpoint was unavailable. The reviewer did not verify live repository protections, a supported pilot baseline, extraction coverage, numerical limits, implementation cost, inference quality or supported-platform behaviour. No builds, benchmarks, load tests, product tests, Scryer calls, file edits, Git mutations or external-record writes were performed by the reviewer.

## Focused revision verification

The same GPT-6.1 Sol at Max reviewer verified the actual revision `0.2` specification, the original frozen input and the review-record snapshot identified above. All five findings are adequately addressed at the proposal-contract level, with APR-04 specifically resolved for proceeding with a staged commitment rather than as proof of hosted enforcement. No new material contradiction was found.

The reviewer and coordinator independently confirmed that all 23 D01–D23 decision rows are byte-for-byte unchanged. Automatic ownership acceptance, YAML/table product output, raw/effective waiver semantics, bootstrap trust and phase-specific evidence remain intact. The coordinator also checked local references, formatting, required IDs and the illustrative counterexample arithmetic. These checks are not product execution.

The reviewed specification hash remains unchanged while this result is appended to the review record. The recommendation remains **proceed with the bounded pilot subject to named conditions**, not implementation approval. Outstanding gates are:

- **P0:** commit and obtain exact-scope human specification approval; finalise concrete contracts; reconcile the native authority/procedure and Linear identity; select a supported clean baseline; establish reviewer/verifier trust and the supported integration-path contract.
- **P2:** execute ownership/projection qualification, including concrete-target ambiguity, coverage and classification-sensitive controls. Obtain approval for material resolver expansion.
- **P3:** qualify metrics, shared-obligation scenarios and state transitions; measure the pilot and obtain approval for numerical limits and aggregate scope semantics.
- **P4:** demonstrate trusted-result integrity and final protected-path enforcement against failed/skipped work, changed reviews, expiry, combined content and expected-state conflicts. If unsupported, keep the gate unqualified.
- **P5:** execute the full pilot and affected suites, resolve warnings, obtain the required fresh implementation review and preserve separate platform/aggregate closure decisions.

Focused verification used document comparison, identity checks and repository memory. It did not execute builds, product tests or implementation qualification, change external state or mutate Git. Reusable knowledge was assessed; no new repository-memory record was warranted for the unimplemented proposal contracts.
