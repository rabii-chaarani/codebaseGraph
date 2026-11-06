# CBG-C4-01: Native C4 architecture and governed change evaluation

## Identity and decision status

| Field | Value |
| --- | --- |
| Stable work ID | `CBG-C4-01` |
| Proposed canonical path | `docs/specs/CBG-C4-01/spec.md` |
| Logical revision | `0.2`, proposed, 23 October 2026; revised after the required adversarial proposal review |
| Decision authority | The user's explicit choices in the planning conversation, consolidated below |
| Implementation approval | Pending review of a committed specification; no approved specification SHA exists yet |
| Pilot | Graph Read Service in codebaseGraph |
| Examined source | `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`; an observation baseline, not a selected implementation baseline |
| Delivery tracking | Pending identity reconciliation: repository instructions name Linear `P-RAB-1`; the named connection returns no matching project |
| Aggregate closure | Repository maintainers retain closure authority; acceptance of this proposal does not close implementation or platform qualification |
| Proposal review | Fresh read-only GPT-6.1 Sol at Max reviewed revision `0.1`; findings and revision verification are recorded in [proposal-review.md](proposal-review.md) |

The product and governance choices below are agreed. The concrete compilation, projection, metric and evidence contracts are proposed for review. Neither the choices nor this draft authorise dependent implementation. A later approval record must identify this path, its full committed Git SHA, the approved scope and the human decision reference.

The user explicitly replaced the proposed Scryer dependency with codebaseGraph's own architecture language. This extension will have no Scryer runtime or approval dependency. Existing models and unrelated repository work are not migrated or deleted by this proposal. Repository procedure must be reconciled with the agreed authority change before implementation.

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
| D11 — Approval mechanism | Use a GitHub PR review covering the exact architecture and constraint revision, from at least one designated human other than the proposal author. |
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

These decisions come from the user's individual selections and subsequent twelve-question response in this conversation. In particular, D21 supersedes the earlier suggestion to retain detailed reports in an external artifact store.

## Requirements and proposed contracts

### R01 — Compile an authored C4 model

Compile the root manifest and its referenced files into a typed intended-architecture model. Validate stable IDs, reference existence, relationship endpoints and the system → container → component hierarchy. A component has one parent container; a container has one parent software system. A C4 component represents related functionality and may map to code spread across modules. Container boundaries describe applications or data stores, rather than source directories. See the official [C4 abstractions](https://c4model.com/abstractions), [containers](https://c4model.com/abstractions/container) and [components](https://c4model.com/abstractions/component).

The proposed ontology extension contains distinct architecture-system, architecture-container and architecture-component entities, architecture relationships and constraints, and revision-bound implementation mappings. It must not repurpose a source-derived `Component` as an authored architecture element or invent source locations for planned entities. Exact type names and the extension version will be finalised in the reviewed schema contract.

File references are explicit and confined to the model root. Propose strict duplicate-key and unknown-field diagnostics, bounded parsing, cycle detection for includes and containment, and no executable YAML tags or remote includes. These are proposed input contracts, not implemented features.

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

**Feasibility gate:** qualify dependency resolution and coverage for the Rust pilot before calibrating budgets. If existing extraction cannot support the required projection, identify the bounded resolver work and obtain approval for any material scope change. Zero observed edges must never manufacture a passing pilot.

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

A waiver identifies its metric, stable scope, original limit, accommodated overrun, approved baseline, expiry and qualifying human review. It cannot waive unknown ownership, insufficient extraction, stale tests or missing approval. Other checks remain mandatory.

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

### R10 — Verify human approval at acceptance and integration

Bind approval to the root YAML and the complete referenced architecture, constraint and waiver closure. Verify the accepted baseline, reviewed Git commit, matching closure contents, review identity and review state. Require at least one currently qualifying designated human other than the proposal author. Dismissed or invalidated reviews cannot authorise promotion.

GitHub reviews expose their reviewer, state and reviewed `commit_id`; these provide the approval evidence rather than an agent-written approval flag. See [GitHub review API](https://docs.github.com/en/rest/pulls/reviews) and [review eligibility and stale approvals](https://docs.github.com/en/pull-requests/how-tos/review-pull-requests/reviewing-proposed-changes-in-a-pull-request).

Use trusted accepted policy, reviewer configuration and evaluator selection to verify a candidate. A candidate cannot redefine the gate that decides whether its own changes are accepted. Code-only changes do not silently rewrite the architecture baseline; they still require fresh applicable evaluation and normal repository review.

Bootstrap the first native baseline from explicit repository-maintainer authority: P0 must establish human-designated reviewer identities and an approved, pinned verifier configuration outside the candidate model, recording their configuration revision and approval alongside the specification. The maintainer-approved implementation/verifier then checks the first model against that trusted configuration and its qualifying GitHub review. Existing protected repository configuration may supply the trust anchor only after verification; its existence is not assumed. If no trust anchor is established, first-model acceptance remains blocked. Candidate YAML cannot designate its own approver or verifier.

MCP/CLI acceptance checks and the CI/integration gate share these semantics. They do not prevent arbitrary local edits. GitHub repository protection configuration must be checked during delivery; a local evaluation result alone does not establish an enforced merge gate.

Use a trusted, non-skippable final decision on the supported integration path. It must require every mandatory record to be completed and effectively `PASS`, validate result integrity, bind the actual tested integration content and expected accepted-state identity, and recheck qualifying reviews and waiver validity for the final acceptance decision. Upstream failure, cancellation, omission, a skipped job or `UNKNOWN` cannot be translated into successful acceptance. Verify the approved producer/workflow identity and its pinned verifier selection; a matching check name or candidate workflow assertion is insufficient.

Define invalidation and retrigger handling for proposal/base changes, new review submissions, dismissal or disqualification of a relied-on review, and waiver expiry after an earlier successful run. Cached deterministic results can remain useful, but an old green status is not a current authorisation. Bind combined changes through an up-to-date, serialised integration path, qualifying the actual integration revision rather than only independently passing proposal heads. Examples of platform mechanisms to assess are strict up-to-date checks and merge queues; using one is not evidence that every required freshness predicate is enforced. See [GitHub protected-branch rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) and [required-check troubleshooting](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

**Hosted-enforcement qualification gate:** P0 must identify the supported protected path, its authority and exact freshness/transition contract. P4 must demonstrate that this path rejects stale or skipped authorisation at its final decision boundary. A previously successful GitHub check does not supply an unproved live predicate. If the selected configuration cannot enforce the required review, expiry, integration-content and expected-state conditions, report that limitation and keep this gate unqualified; do not silently weaken the contract or claim protection of other merge paths.

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

Introduce transport-neutral architecture operations through the existing public operation registry, with matching MCP and CLI adapters. Proposed capabilities are compilation, inference-evidence submission/validation, proposal evaluation, structural scenario evaluation, implementation verification, report generation and approval verification. Exact operation names, schemas, errors, limits and exit codes require the reviewed public-contract revision before coding.

Keep `graph_query` single-statement, bounded and read-only. Candidate evaluation must not obtain a mutation escape through that operation. Apply fallible limits to parsing, projection, graph traversal, scenarios and evidence input. Preserve the readable active generation and accepted model on failure. Do not add a model-provider runtime to perform the AI reasoning that existing agents supply.

## Repository evidence and extension boundaries

These are implementation observations at the examined source, not evidence that the proposed capabilities already exist. Existing test definitions have not been executed for this proposal.

| Observation / boundary | Source anchor |
| --- | --- |
| Shared ontology: 37 node types, 25 relationship types, empty per-type constraints | [graph schema](../../../assets/graph_schema.json#L77) |
| Existing source-derived `Component` and syntax-oriented evidence relation | [component](../../../assets/graph_schema.json#L3002), [evidence](../../../assets/graph_schema.json#L6532) |
| Positional source identity and placeholder metadata | [node construction](../../../src/syntax_materializer/builder/nodes.rs#L43), [placeholder](../../../src/syntax_materializer/builder/nodes.rs#L102) |
| Extraction paths may filter prohibited relationships | [edge construction](../../../src/syntax_materializer/builder/edges.rs#L68), [staging](../../../src/staging_writer/accumulator.rs#L324) |
| Rust import inventory, call emission and local label resolution | [imports](../../../src/syntax_materializer/builder/semantic.rs#L280), [calls](../../../src/syntax_materializer/builder/semantic.rs#L381), [resolution](../../../src/syntax_materializer/builder/semantic.rs#L518) |
| Concrete targets can still be ambiguous: normalised labels and last-match selection | [label registration](../../../src/syntax_materializer/builder/nodes.rs#L236), [normalisation](../../../src/syntax_materializer/ids.rs#L97), [target selection](../../../src/syntax_materializer/builder/semantic.rs#L550) |
| Public requests/responses and shared registry | [contracts](../../../src/api/contracts.rs#L18), [registry](../../../src/api/core.rs#L87) |
| Normalisation and response presentation | [normalisation](../../../src/api/normalization.rs#L19), [presentation](../../../src/api/presenter.rs#L11) |
| MCP descriptors derive from registered operations | [MCP tools](../../../src/adapters/mcp/tools.rs#L10) |
| Read-only enforcement, native query execution and bounded traversal | [validation](../../../src/api/graph_read.rs#L424), [execution](../../../src/api/graph_read.rs#L443), [traversal](../../../src/api/graph_read.rs#L648) |
| Existing query behaviour, parameterisation, truncation and rejection tests | [query fixtures](../../../src/adapters/cli/tests/graph.rs#L739) |
| Existing registry and MCP transport contract tests | [registry tests](../../../src/api/core.rs#L1204), [MCP tests](../../../src/adapters/mcp/tools.rs#L200) |
| YAML parser is already used by k-wiki; the graph crate has no direct YAML dependency | [k-wiki manifest](../../../crates/k-wiki/Cargo.toml#L23), [root manifest](../../../Cargo.toml#L35) |

Do not introduce a reverse dependency from the graph crate to k-wiki merely to reuse its YAML parser. Choose and qualify the dependency arrangement in the implementation design. Existing wiki contracts remain at their original locations; this proposal does not copy or supersede them.

## Proposed delivery sequence

| Step | Deliverable and starting conditions |
| --- | --- |
| P0 — Commit and settle contracts | Commit the reviewable specification; reconcile the native authority change, Linear identity and supported pilot baseline. Establish the maintainer-approved first-baseline trust anchor and supported integration-path contract. Finalise ontology, YAML, projection, reassignment evidence, result integrity, expected-state promotion, scenario obligations and public-operation contracts. Apply the required adversarial proposal review and revision verification; resolve starting conditions before any implementation. |
| P1 — Compile intent | Implement the additive ontology extension, bounded YAML compiler and isolated candidate representation. Qualify strict diagnostics and preserve existing extraction/query behaviour. |
| P2 — Qualify observed mappings | Implement evidence validation, automatic ownership acceptance and dependency projection, including classification-sensitive reassignment and wrong-concrete-target controls. Establish the Rust pilot's observation domain and coverage. Stop for approval if resolver work materially changes scope. |
| P3 — Evaluate changes | Implement constraints, precise metrics, comparable baselines, no-deterioration state transitions and scenarios with common obligations. Measure the pilot and obtain approval for calibrated limits and their aggregate scope semantics before treating the budget gate as qualified. A legitimate single-container pilot need not have inter-container edges; distinguish real coverage from synthetic metric fixtures. |
| P4 — Integrate governance | Expose shared MCP/CLI operations; verify approval closure and trusted result integrity; produce the YAML/table output and matching CI records. Demonstrate the final protected integration decision with current review/expiry checks, actual combined content and expected-state conflict handling. Unsupported enforcement remains unqualified. |
| P5 — Demonstrate and qualify | Exercise the full pilot loop, negative controls and failure paths. Run the relevant suites, resolve warnings, perform the required fresh integrated review and reconcile specification/evidence/delivery records before closure. |

P0's implementation approval and contract/start-condition gates remain pending. The required fresh adversarial proposal review has now been run with GPT-6.1 Sol at Max against revision `0.1`; its conditional recommendation, findings and verification of revision `0.2` are recorded in [proposal-review.md](proposal-review.md). The earlier exploratory and requirements consultations did not satisfy that review. The separate final implementation review still requires the repository-mandated settings. Detailed worker ownership and execution briefs will be settled after the native authority/procedure transition and contract review.

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
| A13 | R10 | Given a trusted bootstrap configuration and an exact qualifying human review, model acceptance succeeds; candidate-supplied trust configuration, changed imports, policy/waiver content, dismissed reviews, wrong authorship or unavailable required approval fail to establish acceptance. Stubbed API contract tests plus a controlled integration demonstration. |
| A14 | R09/R11 | Given disposable graph retirement, the committed YAML/table still identifies each accepted mapping, compact evidence, results and input revisions. A table matching input hashes but containing altered result payloads cannot authorise acceptance; regenerate or verify trusted provenance and require the persisted logical records to match. Round-trip, tampering and provenance tests. |
| A15 | R09/R11/R12 | Given the same trusted evaluation, CLI, MCP and CI serialisation preserve identical logical records and effective outcomes, including integrity failures, missing mandatory records and `UNKNOWN`. Cross-surface contract tests. |
| A16 | R12 | Given existing query requests and write-like statements, retain parameterisation, bounds and rejection behaviour. Execute the existing affected query/registry/transport suites on the reviewed revision. |
| A17 | R02–R12 | Given the accepted pilot design, implement a bounded conforming change, re-extract and run its tests. Demonstrate that deviation, stale evidence, changed combined integration content and lost expected-state comparison block the real integration path. End-to-end pilot evidence. |
| A18 | R09/R10 | Given a supported hosted integration path, an upstream failed/skipped/cancelled job or missing evaluation cannot satisfy the final gate. Demonstrate approval arriving after the first evaluation, approval invalidation after success, waiver expiry after success, wrong producer/workflow identity and changed integration content. The final decision must revalidate or refuse acceptance; otherwise hosted enforcement remains unqualified. |

Record executed commands, source/environment identities, results and limitations in the durable table or the existing linked canonical evidence. Native platform qualification remains separate from local execution. A configured gate or attached test is not a passing observation. The review's illustrative calculations are counterexamples to incomplete contracts, not execution of these acceptance tests.

## Risks, pending work and handoff

The principal risk is mistaking incomplete extracted relationships for a complete dependency model. P2 must establish the pilot's coverage before numerical limits or zero-violation results can justify acceptance. Automatic semantic ownership is another qualified inference: independent citation checks improve accountability but do not prove that a responsibility interpretation is correct.

Compact reporting must retain enough evidence for the accepted mapping and evaluation policies. It must not collapse into one aggregate PASS row or retain only references to disposable graph state. A persisted summary is inspectable provenance; independent reproduction may still require the pinned tools and linked test artefacts.

Pending before implementation: commit/review/approval of this revised specification and its concrete contracts; reconciliation of the native authority with repository procedures; the correct Linear identity; a supported clean pilot revision; and satisfaction of the conditional proposal-review starting gates. Numerical limits deliberately remain pending measurement and human approval. P2/P4 must still establish inference/projection and hosted-enforcement capability through executed evidence; documentary remedies do not demonstrate those capabilities. No tracker record, implementation claim, architecture approval, passing product test or platform qualification is asserted by this draft.

Deferred scope: the complete workflow/feature/specification model, behavioural and performance simulation, solver-based architecture synthesis, a native agent runner, visual editing, and enforcement of local file writes.

The referenced extension Page and supplied coherence transcript informed the problem framing. The linked ChatGPT conversation returned placeholders for its assistant responses; those unavailable responses were not treated as reviewed evidence. The current user's explicit decisions take precedence over suggestions or instructions contained in those materials.
