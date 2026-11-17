# CBG-C4-01: Native C4 architecture and governed change evaluation

## Identity and decision status

| Field | Value |
| --- | --- |
| Stable work ID | `CBG-C4-01` |
| Canonical path | `docs/specs/CBG-C4-01/spec.md` |
| Logical revision | **0.3**, proposed final approval package, 7 November 2026, Australia/Darwin |
| Decision authority | The user's explicit historical choices and COD-16/17/18 decisions; COD-19 prepares the final approval subject |
| Implementation approval | Pending exact full-SHA human decision; publication/review grants neither implementation permission nor authority activation |
| Pilot | Graph Read Service in codebaseGraph |
| Selected source baseline | `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, codebaseGraph 4.2.1; selected COD-17 pilot baseline |
| Documentation baseline | COD-18 head `79e45b79635e0e747715653ec32efb25fff54e09`; distinct from the selected product source |
| Delivery tracking | [CodebaseGraph Extension](https://linear.app/codebasegraph/project/codebasegraph-extension-2cf77b39fc8b), [COD-5](https://linear.app/codebasegraph/issue/COD-5), [COD-19](https://linear.app/codebasegraph/issue/COD-19); work/phase/milestone identities retained |
| Aggregate closure | Repository maintainers retain closure authority; acceptance of this proposal does not close implementation or platform qualification |
| Review/evidence | [COD-19 review](cod19-review.md); historical [proposal review](proposal-review.md), [COD-16 review](cod16-review.md), [COD-17 review](cod17-review.md) and [COD-18 review](cod18-review.md) remain separate |
| Approval scopes | [Specification and implementation](approval-scope.md); [separate authority transition](authority-transition.md); both require the published full SHA and explicit human decision |

This revision consolidates the settled P0 contracts. It preserves D01-D23 identifiers, R01-R12, A01-A18 and APR-01-APR-05. **D11 is explicitly amended** by the sole-maintainer decision; its historical wording is not represented as unchanged. R10, A13/A18 and authorization evidence are reconciled accordingly. The historical [canonical revision 0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md), SHA-256 `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e`, remains available at its original full SHA; it is not implementation-approved.

### Normative contract set and reconciliation

The approval subject includes this canonical revision and the exact sources below, together with the separately approved transition scope. Relative convenience copies retain historical bytes; immutable references/digests identify the incorporated versions.

| Contract source | Immutable revision / SHA-256 | Governing detail |
| --- | --- | --- |
| [COD-16 supplement 0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/0c49902ba7e12e10a8a93ba8540c2be0527d7d47/docs/specs/CBG-C4-01/cod16-contracts.md) | `0c49902ba7e12e10a8a93ba8540c2be0527d7d47`; `c5c5f523deb9a214f9218c15b63c67e66b4cdc429d7a45cf156bdda687af6847` | C16-01-09: identity, ontology, strict YAML, ownership/coverage/lineage, constraints/metrics/scenarios, C1 API, operations, resources/errors/recovery and one-table reporting |
| [COD-18 supplement 0.1](https://github.com/rabii-chaarani/codebaseGraph/blob/79e45b79635e0e747715653ec32efb25fff54e09/docs/specs/CBG-C4-01/cod18-contracts.md) | `79e45b79635e0e747715653ec32efb25fff54e09`; `bf57465a7062880f1075201261f20551adf58c502a7c16b358d9f56e77c03823` | C18-01-10: explicit D11 amendment, bootstrap/update isolation, tagged authorization, manual action, producer/path/freshness/expected-state and qualification stop gate |
| [COD-17 C1/Q1 decisions](https://github.com/rabii-chaarani/codebaseGraph/blob/f67e1d95862484dee34c8e818c98b72959366c5b/docs/specs/CBG-C4-01/cod17-decisions.md) | `f67e1d95862484dee34c8e818c98b72959366c5b`; `ff0f03f2e05de1a41872ea593e8e33cab4e02e2ab05ce170bbaf7147897cad70` | Preserve old Rust callers through one shared private dispatch; qualify scoped imports/direct references before pilot acceptance/calibration |

This canonical revision governs scope, requirements, criteria and explicit reconciliation. C16 supplies concrete details except where C18 and the canonical amendments change authorization. In particular, C16's historical PR-review-only `observed_reviews` assumption is replaced by R10's tagged `approval_evidence`. C18 does not waive C16 result integrity, resources, transport parity, C1/Q1 or durable output. Any remaining material contradiction stops dependent work for a human decision; a submitter cannot select a convenient version or broaden exclusions.

Procedural references in incorporated C16/C18 to Scryer placement, plans, briefs or reconciliation retain their current meaning before activation. Only after actual Scope A/B approval and verified scoped activation, those references resolve through the authority-transition contract: approved Git contracts and versioned Git execution briefs during bootstrap, followed by qualified accepted native C4 intent as specified there. This clarification preserves all product contracts, placement/dependency obligations, binding directives, evidence requirements and unrelated Scryer authority; it grants no native-model acceptance or runtime trust activation.

The [COD-16 decisions](cod16-decisions.md) and [COD-18 decisions](cod18-decisions.md) preserve original human decision locators. The maintainer's 7 November closure clarification, question `call_dd90a6278c834ecf88f1c25091fc838a` item 0, closes COD-18's **P0 contract definition only**. Concrete verifier/workflow/configuration pin approval, trust activation and hosted proof remain mandatory COD-37/46/47 gates before first-model acceptance. No trust anchor or product execution is fabricated by P0 closure.

The extension has no Scryer runtime or approval dependency. Current repository instructions remain in force during preparation. The separately proposed authority transition is inert until explicit approval and verified activation; this specification cannot self-install an instruction change, migrate existing models or accept a first native baseline.

## Problem and observable outcome

The current source graph describes extracted implementation structure. It does not independently define the architecture that implementation should satisfy. An agent therefore needs an authored model, explicit constraints and an evaluation mechanism before it can assess a proposed change against architectural intent.

The extension will compile a versioned YAML C4 model into the shared ontology, infer how observed code implements that model, and evaluate isolated proposals. It will then compare the implemented result with the accepted proposal and current constraints. A successful design evaluation describes a proposed model under stated assumptions; implementation acceptance additionally requires current observations and executed evidence.

The first milestone will demonstrate this complete loop on the Graph Read Service. It will preserve the existing read-only graph-query contract while establishing native architecture governance through MCP, CLI and CI.

## Agreed scope and decisions

| Decision | Agreed behaviour |
| --- | --- |
| D01 — Initial workflow | Extend an existing repository first, using a bounded codebaseGraph pilot. |
| D02 — Language | Use a native YAML DSL for architecture, constraints and code mappings. Requirements and executed test evidence remain linked artefacts. The complete development model comes later. |
| D03 — Ontology | Extend the shared ontology, keeping intended architecture distinct from observed code. |
| D04 — C4 detail | Author systems, containers and components. Obtain code-level detail from extracted modules and symbols. |
| D05 — Ownership | Infer mappings automatically using structural analysis and external-agent AI reasoning. Each symbol has one primary component owner; a module may span components. |
| D06 — Mapping evidence | Require current structural corroboration and an AI rationale tied to declared responsibilities. Validate cited evidence independently; AI confidence alone is insufficient. |
| D07 — Mapping approval | Accept sufficiently evidenced mappings and reassignments automatically. No separate human mapping-approval gate is required. |
| D08 — Uncertainty | Return `UNKNOWN` for unresolved mappings and block proposal acceptance when mandatory checks depend on them. |
| D09 — Dependencies | Require explicit permission within governed scope. Declare required relationships separately. |
| D10 — Architecture authority | Require human approval for every change to the accepted architecture or constraints. |
| D11 — Approval mechanism | Require explicit human authorization of the exact architecture/constraint/waiver revision. In maintainer-controlled sole-maintainer mode, the designated maintainer may authorize their own proposal through the approved manual workflow action; otherwise require a qualifying designated human PR reviewer other than the proposal author. |
| D12 — Enforcement | Enforce model acceptance and CI/integration gates. Local candidate editing remains possible; a filesystem write gateway is outside the first version. |
| D13 — Agent integration | Existing agents orchestrate through MCP/CLI. The product does not host an agent runtime in the first version. |
| D14 — Metrics | Enforce budgets for dependency cycles, direct fan-out and transitive dependency reach, separately scoped to containers and components. |
| D15 — Budget comparison | Enforce both absolute limits and permitted deterioration from a pinned baseline. Both apply. |
| D16 — Existing overruns | Permit explicit, scoped, expiring waivers approved through GitHub. Keep the underlying overrun visible and allow no further deterioration. |
| D17 — Calibration | Measure the pilot, propose justified numerical limits and obtain human approval before enforcing those limits. |
| D18 — Simulation | Include two explicit structural scenarios: a component extension and a dependency or boundary-contract change. Behavioural and performance simulation follow later. |
| D19 — Evaluation freshness | Evaluate every proposal and CI revision. Reuse results only when all declared inputs match. |
| D20 — Failure | An unfinished mandatory evaluation returns `UNKNOWN` with diagnostics and supports explicit retry. Preserve valid completed evidence; stale success cannot replace an unfinished check. |
| D21 — Durable output | Retain the resulting architecture YAML and a durable evaluation-summary table in Git. Do not require a separate durable raw-report or evidence store. |
| D22 — File organisation | Use one root manifest with explicit references to architecture and constraint YAML files. |
| D23 — Surfaces | Deliver MCP, CLI and machine-readable CI reports. A visual editor is separate future work. |

The historical decisions retain their original locators. D21 still supersedes the earlier external raw-report-store suggestion. D11's explicit amendment is selected in C18-D4/D5; every other D01-D23 row is preserved verbatim from historical revision 0.2. Publication/review is not human implementation approval.

## Requirements and proposed contracts

### R01 — Compile an authored C4 model

Compile the root manifest and its referenced files into a typed intended-architecture model. Validate stable IDs, reference existence, relationship endpoints and the system → container → component hierarchy. A component has one parent container; a container has one parent software system. A C4 component represents related functionality and may map to code spread across modules. Container boundaries describe applications or data stores, rather than source directories. See the official [C4 abstractions](https://c4model.com/abstractions), [containers](https://c4model.com/abstractions/container) and [components](https://c4model.com/abstractions/component).

Use C16-01-03's exact extension versions, primitive identities/hashing, distinct `ArchitectureSystem`, `ArchitectureContainer`, `ArchitectureComponent` and related types, legal endpoints/cardinalities and snapshot separation. Do not repurpose source `Component` or invent source locations. The pinned supplement supplies concrete type/schema contracts; the example below is illustrative organization, not a competing grammar or a complete approved pilot topology.

Use C16-03's strict JSON-compatible YAML 1.2 subset and complete explicit root-relative file closure. Reject duplicate keys/IDs/paths, unknown fields, unsupported scalars/tags/aliases/merge keys, escaping/symlink/remote references and trailing documents. No recursive/glob discovery or executable interpolation. Apply the explicit shared execution policy in R12; resource exhaustion cannot produce truncated mandatory PASS. These are implementation contracts, not implemented features.

Illustrative organisation, not an implemented grammar:

```yaml
schema_version: "0.1"
model_id: codebasegraph
architecture:
  - architecture/runtime.yaml
constraints:
  - constraints/dependencies.yaml
  - constraints/budgets.yaml
  - constraints/waivers.yaml
```

The architecture files describe IDs, names, responsibilities and parentage. They do not require handwritten code selectors. Linked requirements and test evidence retain their original identities, locations and pinned revisions.

### R02 — Preserve distinct model states

Keep accepted intent, observed implementation, candidate predictions and evaluation evidence distinguishable even though they share an ontology. The accepted model comes from an approved immutable artefact revision. Observed facts come from the pinned source/extractor combination. Candidate changes remain isolated from both.

Use phase-specific mandatory evidence. Design evaluation can assess declared architecture and predicted structural changes before new source exists. It must label predictions and assumptions. Implementation and integration checks additionally require actual ownership mappings, observed dependencies and executed acceptance evidence. A design pass cannot stand in for implementation conformance.

Proposals use validated typed graph operations or complete candidate YAML revisions. Reject unknown types, invalid references and forbidden endpoint combinations explicitly. Do not reuse extraction paths that silently discard invalid relationships as proposal-acceptance semantics. Preserve the active source graph on rejection, cancellation or partial evaluation.

Model acceptance and integration are conditional transitions against one authoritative accepted-state identity, not independent assignments of a passing status. That identity includes the accepted intent, governed source and ownership projection, applicable comparison/waiver state and their versions. A transition names the state it evaluated and its actual integration inputs. Serialise or conditionally update that state; if it has advanced, return a conflict, preserve the current accepted state and re-evaluate before promotion. Never restore an older snapshot over a concurrent accepted change.

### R03 — Infer and qualify ownership

Accept ownership only when the mapping names the evaluated source identity, one declared component owner, current structural citations, an AI rationale and inference provenance. An independent validator checks the cited facts, revisions, ownership cardinality and explicit evidence rules. A rationale is evidence of an inference, not proof of semantic correctness.

Record ownership changes between baseline and candidate. A change in classification must not disappear inside an improved metric. Reuse the pinned inference result when its declared inputs match; do not silently replace a baseline by calling a stochastic model again. A changed inference result creates a new evaluation input and must be checked again, without introducing a separate human mapping gate.

Use a versioned, trusted evidence rule to distinguish a correction of prior inference from a code/responsibility transfer. It must validate the evidence distinguishing the proposed owner from competing owners, including the previous ownership evidence where applicable. Current citations and a better metric alone do not establish that distinction. Unchanged owner-relevant inputs retain their pinned mapping unless an independently substantiated correction qualifies. Equally supported owners, unresolved correction evidence or uncertain lineage leave affected mandatory checks `UNKNOWN`. Justified corrections and transfers can still qualify automatically; there is no human mapping-approval gate.

Attribute classification-sensitive results explicitly. Where lineage supports it, compare the same candidate source dependencies under previous ownership and proposed ownership, and separate that effect from source dependency changes. An edge internalised by reassignment must not be reported as an implemented dependency removal. Acceptance cannot use a policy/metric improvement as evidence that the reassignment is correct. Record the comparison and qualifying evidence in the existing table; unresolved reassignment-dependent improvements cannot authorise a mandatory pass. These are policy-qualified inferences, not proof of semantic ownership.

Source identities are revision-specific. Moving code can change extracted node IDs. Preserve continuity only when supported by evidence; otherwise record the new identity and unresolved lineage rather than claiming that a positional ID is durable.

### R04 — Build an explicit dependency projection

Define and version the relation-to-dependency projection. Use source containment and qualified symbol evidence to lift verified dependencies to component owners, then to their authored parent containers. Preserve direction and deduplicate identical architectural edges. Each projection removes its own intra-vertex edges: same-component edges are internal at component level, and same-container edges are internal at container level even when they connect different components. Module ownership cannot be inferred wholesale when its symbols have different owners.

The existing graph needs qualification here: an import inventory does not establish its target component, and a `ResolvesTo` edge can lead to a `name_placeholder`. Ambiguous targets, unresolved import destinations, truncated traversals and unsupported relationships remain `UNKNOWN` where a mandatory check depends on them.

A non-placeholder endpoint is not sufficient either. The examined resolver normalises labels and can select the last matching registered ID without establishing the caller's scope. Qualify expected concrete endpoints independently using source scope/binding evidence, including duplicate names, case collisions and shadowing. If the proposed observation domain cannot distinguish the actual target, dependent checks remain `UNKNOWN` rather than accepting the concrete node as sound resolution.

Declare the observation domain of every rule and metric. A statement about supported static dependencies must not be presented as a guarantee about unobserved runtime or dynamic relationships. Relevant exclusions and extraction coverage must be visible. A missing required relationship is a violation only when its absence is established within the applicable observation domain; otherwise it is `UNKNOWN`.

**Selected Q1 domain:** C16-05 qualified case-sensitive module/item identity, explicit/grouped imports, aliases and statically named direct references under pinned build context. Unsupported cfg/macros/globs/re-export cycles/type-dependent dispatch/external definitions remain UNKNOWN when relevant. Independently derive source/site/required-closure inventories and qualified lineage; a concrete heuristic endpoint does not supply qualification.

**Feasibility gate:** COD-25/F04 must establish actual Graph Read Service endpoint/coverage sufficiency, including affected API Core, Coordinator, Agent Hook and Wiki callers, before projection acceptance or calibration. If Q1 is insufficient, return the measured blocker for a human decision; do not expand resolution or narrow obligations silently. Zero observed edges cannot manufacture a pilot pass.

### R05 — Evaluate constraints and outcomes

Within governed scope, observed or predicted inter-element dependencies require an explicit allow rule. A required relationship also needs permission; contradictory rules are compilation diagnostics. Permission alone does not require a relationship to exist.

Each check returns `PASS`, `VIOLATION` or `UNKNOWN`, with its phase, scope, rule, inputs and explanation. These are effective results under the applicable approved policy, including any valid waiver. An effective violation and an unknown mandatory check both block acceptance, while remaining distinct results. The summary must retain both when they coexist. A lower metric cannot compensate for either.

### R06 — Define and enforce numerical budgets

The following are proposed executable definitions for the selected metric families. Approve these definitions before calibration; no numerical limits have been measured or approved.

| Metric | Proposed unit and calculation |
| --- | --- |
| `cyclic_nodes` | Count scoped architectural vertices participating in strongly connected groups containing more than one vertex. Report the groups and witness relationships. Intra-owner source edges are removed before this calculation. |
| `max_direct_fanout` | Maximum number of distinct outgoing architectural neighbours over the selected origins. Parallel source relationships count once. |
| `max_transitive_reach` | Maximum number of distinct reachable architectural vertices over the selected origins, excluding the origin itself. This measures potential dependency reach, not expected editing effort. |

Compute container and component projections separately. Every budget identifies its level, stable scope, relation projection, treatment of boundaries, absolute maximum and maximum permitted increase. Do not omit known targets merely because they lie outside the selected origin scope. Explicitly excluded external relationships remain recorded as exclusions rather than silently disappearing.

The proposed maxima are aggregate guarantees, not per-origin guarantees. For example, fan-outs `A=10, B=0` becoming `A=8, B=10` retain maximum `10`. A waiver on that aggregate can therefore preserve its metric while coupling moves to a different origin. Show relevant origin-level changes in the summary and make this limitation explicit when approving scopes and calibration. Do not invent stricter per-origin gates or describe aggregate compliance as proof that no individual origin worsened.

Reach evaluation must account for non-local effects. Adding `B → C` can increase reach from `A` through an unchanged `A → B` edge. Recompute affected origins or validate the complete relevant dependency-closure fingerprint; unchanged direct edges at an origin do not justify reusing its old reach result. Unavailable intermediate coverage remains `UNKNOWN`.

For comparable baseline and candidate inputs, require `candidate <= absolute_max` and `candidate - baseline <= max_increase`. Do not invent limits or silently advance the baseline. Changes to scope, ontology, projection or metric definitions invalidate incompatible comparisons; recompute or return `UNKNOWN` until a comparable baseline is established.

### R07 — Apply narrow, expiring waivers

A waiver identifies its metric, stable scope, original limit, accommodated overrun, approved baseline, expiry and qualifying human authorization under R10/C18. It cannot waive unknown ownership, insufficient extraction, stale tests or missing approval. Other checks remain mandatory.

Retain the raw metric value and its overrun against the original limit separately from the effective mandatory-check result. A valid waiver can make that effective check `PASS` only within its accommodated ceiling and zero-deterioration rule. This does not erase the raw overrun or introduce a fourth check outcome. An invalid waiver cannot authorise acceptance; if required waiver validity cannot be established, the effective check is `UNKNOWN`.

Within a waived scope, the permitted deterioration is zero. Propose comparison against the latest accepted, comparable state so an improvement cannot later be surrendered by returning to an older, worse waiver ceiling. Recheck expiry at acceptance and integration; an earlier cached pass cannot survive expiry. Renewal or scope changes require a new approved constraint revision.

Include that comparison/ratchet state in the expected-state transition from R02. If candidates `8` and `9` both evaluate against accepted value `10`, acceptance of `8` makes the old evaluation of `9` stale: `9` must be compared with `8` before promotion. A local calculation or lock that does not cover the authoritative integration path cannot establish this guarantee.

### R08 — Simulate two structural changes

Each scenario declares stable targets, applicability preconditions, typed graph transformations and expected observations. Apply equivalent scenario intent to baseline and candidate; different designs need not use identical edits. Validate references and constraints after transformation, then compare the same metrics.

Define that shared intent through common parameters, required capability/contract references and structural postconditions in the approved constraint closure. Validate both transformed graphs against those obligations before comparing metrics. A transformation that omits a required capability is not a favourable alternative; a proven omission fails the scenario, while unresolved applicability or postconditions remain `UNKNOWN`. Candidate-local edits cannot weaken the shared obligation. This formalises structural equivalence without adding a behavioural simulator.

The pilot scenarios are: extending the Graph Read Service without changing its intended boundary; and changing a dependency or a referenced boundary contract. A bypass of the Graph Read Service provides a negative control. An inapplicable or incomplete mandatory scenario is `UNKNOWN`. A contract-reference change models structural consequences; behavioural compatibility still depends on linked tests or later modelling capability.

### R09 — Pin inputs and invalidate stale results

An evaluation identifies the source commit and content fingerprint, complete YAML closure, accepted baseline, ontology/extractor/projection/evaluator versions, inference inputs and mapping result, scenario definitions, and test definitions/configuration/execution evidence. Test definitions alone never count as passing execution.

Exclude only the generated evaluation-summary output from its own source-input fingerprint, using a fixed tool-owned exclusion. This avoids a report-commit loop while preventing proposals from broadening exclusions. At integration, verify equivalence of the actual evaluated inputs and the proposed revision; a recorded earlier commit is insufficient by itself.

The report-file exclusion does not exclude its logical mapping records from evaluation inputs. Independently canonicalise and hash the source identity, owner, structural citations, rationale and inference provenance used by each mapping. Editing those fields invalidates dependent results even if the containing table is excluded from the source-content fingerprint. Derived evaluation IDs and result cells do not become their own inputs.

Input identity and result integrity are separate checks. Treat persisted result cells, measured values and execution assertions as untrusted claims until the trusted evaluator regenerates them or verifies producer-bound provenance for the complete result payload. Equal input hashes, a self-declared producer name or a candidate-computed output hash do not authenticate results. Enumerate required records from trusted policy and scope, not from the rows the candidate supplies; reject or regenerate omitted records, substituted execution references and complete-looking altered verdicts.

The first version can recompute mandatory results through the trusted evaluator instead of introducing a signing service or additional evidence store. It may reuse validated input facts and pinned inference records without trusting derived cells. The persisted table's logical derived records must match the trusted results before it serves as accepted evidence; fresh evaluation must not leave a forged table presented as accurate. Stable evaluation records and fresh final-authorisation observations are distinct, so report-only commits and live guard rechecks need not create a source-fingerprint loop.

Approval freshness and waiver expiry are rechecked at acceptance time even when deterministic evaluation records can be reused. Unavailable required inputs, an unfinished check, a resource limit or cancellation return `UNKNOWN`; retain completed records only while their input identities remain valid. Retry is explicit.

### R10 — Verify human authorization at acceptance and integration

Bind approval to the root and complete architecture/constraint/waiver closure, exact raw-byte/path-set identity, source head, applicable accepted baseline and current trusted configuration. Candidate YAML cannot choose its authorization mode, human identity, verifier, producer or gate. D10 still requires human approval for every accepted architecture/constraint change; no human per-mapping queue is added.

Use the C18 modes. `designated_review` requires a currently qualifying designated human other than the proposal author, with independently verified effective GitHub review state, reviewed commit and closure. `sole_maintainer_action` permits the designated sole maintainer to authorize their own proposal through a new explicit manual action in the approved pinned workflow. Proposed initial repository/account IDs are `1235031738` / `50892556` (`rabii-chaarani/codebaseGraph` / `@rabii-chaarani`); they become active only through independently established maintainer trust. A mode cannot be inferred from authorship or collaborator counts.

A sole-maintainer action binds exact repository/PR/`refs/heads/dev`, full head and expected-base commits, complete governing-closure digest, expected accepted-state ID, active trust revision, explicit decision and authenticated run/attempt/control identity. Automatic events, actor strings, copied real run IDs, inherited rerun privileges, labels and earlier greens are not fresh human approval. A changed subject needs new approval; report-only commits may reuse qualifying deterministic evidence but do not inherit an attempt's manual authorization. Candidate execution receives no approver/integration credentials. An account identity alone does not prove an explicit human decision or credential separation.

The reconciled C16/C18 shape is:

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

The historical `observed_reviews` field is replaced, never overloaded with fake PR approvals. Public operations resolve authorization-context locators through maintained trust; a serialized reference is not a bearer grant or override. Preserve tagged logical evidence across CLI/MCP/CI and the existing summary table. Original evaluated provenance and fresh authorization observations remain separate.

P0 establishes the trust contract and proposed human identity/mode/path. Concrete immutable verifier/workflow/configuration pins and explicit maintainer approval of them remain COD-37/46/47 gates before first-model acceptance or runtime activation, as clarified when the maintainer closed COD-18. No architecture verifier or protected path is assumed present at baseline. Bootstrap is established outside candidate control; separate later trust updates use the previously active revision and conflict on changed expected trust. A product/model candidate cannot activate its own trust changes.

The selected path under qualification is `github-dev-manual/0.1`: one maintainer-controlled pinned workflow into dev, provisionally `.github/workflows/architecture-integration.yml`. C18-04-08 defines action/producer/control/dependency pins, unprivileged candidate execution, result regeneration/integrity, failure handling and the protected-path connection. Its actual source/configuration and authoritative protocol must be approved and executed before use. A manual-dispatch green, check name or application ID alone does not establish that connection. No hosted controller/App, signing service, extra store, organization migration or protection bypass is assumed.

The trusted final boundary requires every mandatory record complete and effectively PASS, actual combined integration content, fresh qualifying human authorization and waiver validity, and a conditional transition against current accepted-state/trust/base identities. It covers both freshness and actual promotion/integration; a local mutex, workflow concurrency, two independent writes, head-SHA API parameter or queued merge receipt alone is insufficient. Preserve the latest comparable waiver ratchet and reject stale 10-to-9 after 10-to-8. Evaluate interacting proposals on their actual composition.

Invalidate on proposal/base/content/policy/trust changes, review dismissal/disqualification or action withdrawal/cancellation, and waiver expiry after earlier success. Failure/skip/neutral/cancellation/missing mandatory work and UNKNOWN cannot authorize acceptance. Events may request re-evaluation; they do not replace the final live check. Missing required facts are UNKNOWN; a known unsupported path blocks promotion. After ambiguous dispatch inspect actual state before explicit recovery; retain real execution permits and never claim cancellation undid a committed transition.

**Hosted-enforcement stop gate:** COD-47/F09 must demonstrate the real authenticated producer-to-protected-path connection, expected-state/composition/waiver/review/action freshness and rejection of alternate acceptance-capable paths. If the platform cannot enforce a required predicate at the authoritative transition, retain `unsupported_integration_path`/UNKNOWN and blocked acceptance, then return the capability gap for a human scope decision. Specification approval permits bounded staged implementation; it neither proves nor activates this path.

### R11 — Retain YAML and one durable summary table

The durable evaluation output consists of the resulting architecture YAML file set and a Git-tracked evaluation-summary table. Keep compact evidence in that table rather than creating a separate durable raw-report store. This specification is planning material, not an additional runtime report format.

Proposed table columns:

| Evaluation ID / phase | Record kind / subject | Pinned inputs / baseline | Result / value / limit | Evidence / rationale | Approval / waiver / limitation |
| --- | --- | --- | --- | --- | --- |
| Format example only | `mapping`, `constraint`, `metric`, `scenario`, `test` or `summary` | Source and artefact revisions or digests | Typed result and comparison | Immutable source references, validated graph facts and compact reasoning | Applicable review, expiry or unknown reason |

Include a compact mapping row for every accepted ownership assignment in governed scope. Retain the symbol's revision-specific identity, owner, structural citations, rationale and inference provenance in its cells. Include sufficient evaluation-level provenance to interpret all rows after disposable graph generations are gone. Link existing test evidence at its original durable location; never replace executed evidence with a summary assertion.

The same table records classification-only effects, accepted-state identity, trusted-result verification and final-authorisation provenance as applicable. Producer attribution must be verified through the trusted path or results regenerated as R09 requires. A durable complete-looking row is inspectable history, not self-authenticating authority to pass a new gate.

Machine-readable CI output serialises the same logical records. It does not require a third committed report or an external evidence store. Raw transcripts, model conversation histories and transient graph databases are not required durable outputs. The final implementation must reject incomplete summary records where mandatory evidence would otherwise be lost.

### R12 — Preserve existing interfaces and bounded execution

Use C16-07-09's settled **C1 compatible family**. Preserve old exhaustive `OperationRequest` variants, public request/descriptor/response/error literal fields, `OperationExecutor`, facade methods/paths and old defaults. No retroactive `non_exhaustive`, new required trait methods or changed old literals. Add an architecture request enum non-exhaustive from introduction, its response/executor contracts and opaque validated execution context through **one shared private dispatch/catalog**, not a second authoritative registry.

The eight operation IDs are `architecture-compile`, `architecture-validate-evidence`, `architecture-evaluate`, `architecture-scenarios`, `architecture-verify`, `architecture-report`, `architecture-verify-approval` and guarded `architecture-accept`, with exact request/response/schema/MCP/CLI mappings in C16-08 and the R10 authorization amendment. Core composes transport-neutral services; adapters translate/present. Graph Read Service retains bounded reads; Language Syntax Graph supplies qualified bindings; the proposed Native Architecture domain owns evidence validation/projection/constraints/metrics/scenarios/evaluation. Exact file ownership and necessary dependency arrangement are settled in each approved execution brief.

Use strict inputs and C16 validation/error/CLI codes; retain simultaneous unknown/violation records. Local PASS, approval preview and report generation cannot promote accepted intent. `graph_query` remains single-statement, parameterized, bounded and read-only, with no source/storage mutation escape. Native verification capability construction and read/write/publication lifetimes remain intact.

Architecture tasks use explicit shared repository `ArchitectureExecutionPolicy` values across CLI/MCP/CI: every C16-09 required positive field, no invented numerical defaults, tighten-only caller overrides, and full effective values/digest in provenance. Missing/invalid required settings block evaluation. Configuration changes invalidate dependent reuse. Apply fallible allocation, configured counters/deadlines and cooperative cancellation; unfinished mandatory work is UNKNOWN, never truncated PASS. Keep real execution permits/leases until actual completion/cleanup and prohibit replay after ambiguous dispatch.

Operational resource settings remain separate from D17 human-calibrated architecture budgets. This approval selects neither numerical resource values nor numerical metric limits. Existing graph-query bounds and semantic cardinalities are unchanged. No hosted model/agent runtime, raw evidence archive, extra durable state/task service or graph-to-k-wiki dependency is included.

## Repository evidence and extension boundaries

The selected clean product baseline is `90e2ab6fae2767bdb158e19f0072d43568e80dbc` (4.2.1). The [COD-17 baseline](https://github.com/rabii-chaarani/codebaseGraph/blob/f67e1d95862484dee34c8e818c98b72959366c5b/docs/specs/CBG-C4-01/cod17-baseline.md) records source/environment/tool identities, topology, the actual placeholder observation and existing test/CI evidence with limits. Historical 6aa5ba8 source observations remain in revision 0.2 and its review; they are not relabeled as current qualification.

Graph Runtime is the application/library container; Knowledge Wiki and Release Verifier retain their separate boundaries. The principal pilot origin is Graph Read Service within Graph Runtime, following its production dependency closure and affected Core/Coordinator/Agent Hook/Wiki callers. A single-container pilot can legitimately have no inter-container edges; synthetic graphs do not establish coverage or calibrated budgets.

The `execute_graph_search` placeholder gap, collision/shadowing risk and unqualified Q1 sufficiency remain explicit. No extension compiler/resolver/evaluator/approval verifier/hosted gate is claimed implemented by P0 documentary closure.

Preserve existing source ontology/storage format, native capability guards, confinement, cancellation, generation publication/leases/recovery and shared operation semantics. Additive intended/observed/predicted/evaluation states cannot alias source identities. No graph-to-k-wiki parser dependency is approved. Existing wiki specifications stay at their original locations; unrelated specifications/models are not migrated or superseded.

Detailed implementation/file/dependency ownership remains per approved issue brief. Shared files/manifests/lockfiles have one writer. The [authority transition](authority-transition.md) separately defines bootstrap governance without treating an unqualified native model or candidate as its own authority.

## Proposed delivery sequence

| Step | Deliverable and starting conditions |
| --- | --- |
| P0 - Final readiness | COD-16/17/18 documentary outcomes are Done. COD-19 publishes this reviewed revision and inert transition/approval scopes; exact full-SHA human implementation approval and verified procedure activation remain pending. Concrete runtime trust pins/hosted proof remain pre-acceptance gates, not fabricated P0 execution |
| P1 - Compile intent | After approval/procedure readiness, implement additive ontology, strict bounded YAML and isolated candidates; qualify diagnostics while preserving source/query behavior |
| P2 - Qualify mappings | Implement evidence/ownership/lineage/Q1 projection and prove actual pilot sufficiency before projection acceptance/calibration. Stop for material resolver or obligation changes |
| P3 - Evaluate changes | Implement constraints, metrics, comparable baselines, ratchets and shared-obligation scenarios. COD-40 settles concrete instances; measure and obtain D17 approval before numerical enforcement qualifies |
| P4 - Integrate governance | Implement shared operations, integrity, authorization, YAML/table/CI records and selected final path. Approve actual pins and demonstrate protected-path enforcement; unsupported capability blocks acceptance |
| P5 - Demonstrate and qualify | Execute the actual pilot, failure/bypass/interaction controls, affected suites and independent integrated review. Preserve separate platform/release and maintainer aggregate closure |

Future stages retain existing Linear prerequisites and bounded per-issue readiness. Plan-level implementation approval does not dispatch every issue, authorize live GitHub settings changes, accept a native baseline, waive tests/qualification, grant merge/release authority or expand Q1.

Historical fresh Sol/Max proposal review and revision 0.2 verification remain in [proposal-review.md](proposal-review.md). COD-16/18 reviews remain valid only within unchanged inputs. This materially reconciled revision receives its own fresh final documentation review; future integrated implementation reviews are not replaced by document review.

## Acceptance and verification plan

All checks below are planned. No compiler, evaluator, inference engine, metric gate, numerical pilot baseline or extension test suite has been implemented or executed in this task.

| Criterion | Requirement | Given / When / Then and verification method |
| --- | --- | --- |
| A01 | R01 | Given valid referenced YAML, compilation produces distinct typed C4 entities; duplicate IDs/keys, invalid parents, escaping includes and unresolved references produce precise diagnostics. Compiler fixtures. |
| A02 | R02 | Given an accepted snapshot, candidate rejection, cancellation or partial evaluation preserves valid accepted state and the observed source graph. A stale expected-state promotion returns a conflict without overwriting a concurrent accepted change. Isolation, concurrent-promotion and failure-injection tests. |
| A03 | R02 | Given planned new source, design evaluation labels predictions; implementation acceptance remains blocked until required actual mappings and executed evidence exist. Phase-transition tests. |
| A04 | R03 | Given current structural evidence and a qualifying rationale, ownership and a justified correction are accepted automatically. Multiple primary owners, unresolved competing interpretations, stale citations or a changed owner with no independently qualifying correction/transfer evidence remain unresolved. Evidence-validator fixtures. |
| A05 | R03/R04 | Given symbols with different owners in one module, dependency lifting uses each symbol's owner. A classification-only attempt to internalise a forbidden edge cannot qualify merely by citing facts and printing a delta; show the previous-ownership counterfactual where lineage supports it. Mixed-module, valid correction and self-serving reassignment controls. |
| A06 | R04/R05 | Given placeholders, unresolved imports, missing relation tables, truncated observations or ambiguous/wrong concrete targets affecting a mandatory check, evaluation returns `UNKNOWN`; a non-placeholder or empty result cannot alone pass that check. Rust fixtures include case/name collisions, shadowing and independently established expected endpoints. These are qualification obligations, not claimed executed reproductions. |
| A07 | R05 | Given a permitted dependency, no required-edge obligation is invented; a proven unpermitted dependency or missing required relationship is a violation. Projection/constraint fixtures. |
| A08 | R04/R06 | Given hand-verifiable graphs, cycles, distinct fan-out and transitive reach match their definitions independently at component and container levels. Same-container component edges do not inflate container metrics. Algorithm fixtures, including SCCs and duplicate edges. |
| A09 | R02/R06/R09 | Given an absolute or deterioration overrun, acceptance is blocked; incompatible metric/projection inputs cannot reuse an earlier comparison. An external edge change that alters an unchanged origin's reach invalidates its old result. Interacting proposals must be checked on their actual combined graph, including individually acyclic changes that form a cycle together. Budget, invalidation and integration tests. |
| A10 | R02/R05/R07 | Given an approved waiver, its effective check can pass while its named raw overrun stays visible, without further metric deterioration. Concurrent candidates `8` and `9` evaluated against `10` cannot accept `9` after `8` without revalidation against `8`. Exercise expiry, improvement rollback, authoritative-state conflicts and unrelated failures through the real transition path. |
| A11 | R08 | Given both pilot scenarios, validate common parameters, required capabilities and structural postconditions on both transformations before comparing metrics. A candidate omitting a shared obligation cannot appear as a favourable comparison. Unresolved applicability or mandatory evidence returns `UNKNOWN`. Scenario and bypass-control fixtures. |
| A12 | R09 | Given changed declared inputs, dependent results are invalidated; report-only commits can reuse evidence only after fixed-exclusion input equivalence and trusted-result integrity are verified. Changed mappings, complete-looking altered verdicts/values, omitted mandatory rows and substituted execution references are rejected or regenerated. Freshness, tampering and self-reference tests. |
| A13 | R10 | Given maintainer-established trusted configuration, an exact qualifying designated review in normal mode or authenticated explicit sole-maintainer action in sole mode establishes only the human-authorization predicate. Candidate-selected trust/mode, wrong identity, missing/invalidated action, changed governing closure, stale subject or unavailable facts cannot authorize acceptance; all other mandatory checks still apply. API and controlled hosted cases, C18-F01-F24/F33/F38-F40. |
| A14 | R09/R11 | Given disposable graph retirement, the committed YAML/table still identifies each accepted mapping, compact evidence, results and input revisions. A table matching input hashes but containing altered result payloads cannot authorise acceptance; regenerate or verify trusted provenance and require the persisted logical records to match. Round-trip, tampering and provenance tests. |
| A15 | R09/R11/R12 | Given the same trusted evaluation, CLI, MCP and CI serialisation preserve identical logical records and effective outcomes, including integrity failures, missing mandatory records and `UNKNOWN`. Cross-surface contract tests. |
| A16 | R12 | Given existing query requests and write-like statements, retain parameterisation, bounds and rejection behaviour. Execute the existing affected query/registry/transport suites on the reviewed revision. |
| A17 | R02–R12 | Given the accepted pilot design, implement a bounded conforming change, re-extract and run its tests. Demonstrate that deviation, stale evidence, changed combined integration content and lost expected-state comparison block the real integration path. End-to-end pilot evidence. |
| A18 | R09/R10 | Given the selected hosted path, failed/skipped/cancelled/missing work, automatic events/reruns, revoked/disqualified approval, expiry after green, wrong producer/control revision, changed combined content, stale expected state and alternate acceptance paths cannot cross the real final boundary. If a predicate cannot be enforced, retain the named unsupported/UNKNOWN gate. Controlled COD-47 execution is required; no check name or dispatch green alone qualifies. |

Record executed commands, source/environment identities, results and limitations in the durable table or the existing linked canonical evidence. Native platform qualification remains separate from local execution. A configured gate or attached test is not a passing observation. The review's illustrative calculations are counterexamples to incomplete contracts, not execution of these acceptance tests.

## Risks, approval and handoff

Incomplete or unsound dependency evidence must not be mistaken for complete conformance. Conservative ownership can leave shared helpers UNKNOWN; Q1 may be insufficient. A concrete endpoint, empty projection, lower metric or rationale alone cannot create a mandatory pass. Return actual insufficiency for a human scope decision rather than widening resolution or shrinking obligations.

C18 platform facts are dated observations, not live configuration approval or hosted proof. Approved executable/configuration pins, authenticated explicit manual action/credential separation and an authoritative protocol coupling freshness/state to integration remain prerequisites. The selected personal-repository path can remain unsupported. No signing service, hosted controller, extra store, organization migration or bypass is pre-approved as a remedy.

[Approval scope](approval-scope.md) distinguishes specification/implementation permission from authority activation. The concrete request must supply the remotely published full SHA, canonical path, incorporated immutable references/digests, exact scope and human decision reference. Nothing here claims approval has been granted. A later approval record references the approved commit; no file embeds its own containing commit as an input.

The current local instruction file, Scryer models/directives/prior history, unrelated pending entries and curated wiki are preserved during preparation; only the proportional empty documentation-ledger history is appended. The inert transition provides exact baseline/candidate instruction text, conflict checks and activation/rollback evidence. Publishing it cannot activate it.

After the required human approvals and activation checks, COD-19 and COD-5 may close for P0 readiness under the maintainer's request. This does not close COD-37/46/47, COD-25/F04, COD-40, D17, platform qualification, implementation or aggregate release. Absent approval/transition evidence keeps the gate open and dependent work blocked. [COD-19 evidence](cod19-review.md) distinguishes documentation checks from planned product criteria.

Deferred scope remains the complete workflow/feature/specification model, behavioral/performance simulation, solver synthesis, native agent runner, visual editor and local filesystem write gateway. No extra durable raw-report/evidence store is introduced.

The referenced extension Page and supplied coherence transcript informed the problem framing. The linked ChatGPT conversation returned placeholders for its assistant responses; those unavailable responses were not treated as reviewed evidence. The current user's explicit decisions take precedence over suggestions or instructions contained in those materials.
