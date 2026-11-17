# COD-16 concrete contract proposal

## Identity, authority and review boundary

- Work: **CBG-C4-01/F01/S02 / COD-16**, P0, CodebaseGraph Extension. Supplement revision **0.2**, 6 November 2026, Australia/Darwin. Status: **finalized documentation proposal**, not product implementation approval.
- Supplement path: `docs/specs/CBG-C4-01/cod16-contracts.md`. This implements the user's documentation-finalization plan; it does not supersede the historical canonical specification. [Decision and authorization record](cod16-decisions.md), [illustrative contract fixtures](cod16-fixtures.md) and [review/evidence record](cod16-review.md) accompany it.
- Historical specification: [revision 0.2 at 0a66cb31e397e523190618f211a613f7a095d9a0](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md), SHA-256 `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e`. Its D01–D23, R01–R12, A01–A18 and historical APR-01–APR-05 remain the governing proposal inputs; this supplement concretises them without changing the historical file.
- Settled choices: [COD-17 decision record at f67e1d95862484dee34c8e818c98b72959366c5b](https://github.com/rabii-chaarani/codebaseGraph/blob/f67e1d95862484dee34c8e818c98b72959366c5b/docs/specs/CBG-C4-01/cod17-decisions.md), SHA-256 `ff0f03f2e05de1a41872ea593e8e33cab4e02e2ab05ce170bbaf7147897cad70`: **C1** compatible additive request family/shared internal dispatch and **Q1** qualified scoped imports/direct references with sufficiency proved before calibration.
- Source observation baseline: clean `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, version 4.2.1. Documentation evidence: [PR 184](https://github.com/rabii-chaarani/codebaseGraph/pull/184), head `f67e1d95862484dee34c8e818c98b72959366c5b`, targeting dev. Prior test results qualify existing behaviour only; no extension tests have executed.
- [COD-18](https://linear.app/codebasegraph/issue/COD-18) owns reviewer/verifier trust and the supported integration path. [COD-19](https://linear.app/codebasegraph/issue/COD-19) owns publication of the final revised specification, explicit repository authority/procedure transition and exact-scope implementation approval. [COD-25](https://linear.app/codebasegraph/issue/COD-25)/F04 owns Q1 qualification. This document selects no human reviewer, workflow, protected path or calibrated budget.
- Root is the sole documentation/Git writer for the four COD-16 files named above. No ownership of product files, manifests, lockfiles, shared types, existing evidence or other issues is assigned. Direct documentation work; no implementers dispatched. Scryer proportional documentation ledger `chg-7jc6yv`; no model claims/links are proposed or folded. Existing unrelated pending intent/drift is preserved.

The proposed product has no Scryer runtime or approval dependency. Scryer remains this repository's architectural authority until the explicit COD-19 procedural transition. Proposed Native Architecture responsibilities below are design text, not accepted model intent. Aggregate closure stays with maintainers.

## C16-01 — Version, identity and primitive types

Use distinct version identifiers: `architecture_schema = "0.1"`, `architecture_wire = "0.1"`, `projection = "rust-q1/0.1"`, `ownership_rule = "responsibility-evidence/0.1"`, `metrics = "structural/0.1"`, `table = "architecture-table/0.1"`. Versions are exact matches in v1; unsupported values fail explicitly. These are extension versions, not a change to the product's existing version or source schema numbering.

`Id` is case-sensitive ASCII `[a-z][a-z0-9]*(?:[-_.][a-z0-9]+)*`, model-local and globally unique across authored entity/rule/scenario IDs. Renaming an ID is remove/add; continuity needs explicit lineage evidence. Display names and responsibility text are nonempty UTF-8. IDs contain no filesystem meaning. String sizes are subject to configured execution resources, not invented fixed field-length caps.

`Digest` is `sha256:` followed by 64 lowercase hex digits. `GitRevision` is a resolved full commit object ID plus repository identity; accept both supported Git object formats, never treat a branch/tag abbreviation as immutable identity. `ArtifactRef = { repository, commit, path, content_digest }`; `SourceRef` additionally carries `node_kind`, revision-specific `node_id`, `symbol`, byte span and file digest. Validate spans and file bytes independently. Git paths use repository-relative `/` syntax. Line numbers are presentation, not identity.

Hash structured values as domain-separated, versioned canonical JSON: UTF-8; object keys sorted by Unicode scalar order; no floats; integers decimal; ordered arrays preserved; schema-declared sets sorted by stable key before encoding; reject duplicate keys. Domain prefix is `<type>/<version>\n`. Strings are exact decoded text without Unicode normalization; no timestamp or derived result enters its own input hash. Hash file closure as sorted `(relative_path, raw_byte_digest)` entries including the root. Thus comments/format edits change the reviewed YAML closure even when typed intent is equal.

`SourceProvenance = { repository, evaluated_commit, evaluated_tree, extractor_digest, build_context_digest }` is immutable. `SourceContentKey` hashes all entries of the selected immutable Git tree, including paths, modes, file bytes and referenced submodule commit identities, with **only** the fixed tool-owned generated table path excluded. Effective runtime resource settings, governed dependency content and resolved build configuration are additional hashed inputs. Missing governed submodule/dependency contents cannot be treated as complete because a commit ID is present. No candidate-controlled whitelist/exclusion is allowed.

`MappingFactsDigest` separately hashes source references, owners, responsibility references, structural citations, rationale, inference provenance and lineage. Excluding the table never excludes mapping facts within it. `InputIdentity` includes original evaluated provenance, SourceContentKey, complete YAML closure, expected accepted-state ID, ontology/extractor/domain/ownership/projection/metric/evaluator versions and digests, mapping facts, scenario definitions, linked requirements/contracts, test definitions/configuration/execution evidence, and effective resource policy values/digest. Optional inputs state typed inapplicability; missing required inputs are never empty successful inputs.

`AcceptedStateId` hashes accepted intent closure, governed content key/build context, mapping facts and ownership result, projection, comparison baseline, waiver ratchet and policy versions. Actual Git commit provenance is recorded separately; post-transition receipts do not enter their own input hashes. A candidate's content-derived ID is not an accepted-state ID. `EvaluationId` hashes phase and complete deterministic inputs, including original evaluated provenance. Reusing an evaluation retains its evaluated commit/ID; it does not relabel old execution as execution of a newer commit.

`AuthorizationObservation = { evaluation_id, actual_integration_commit, actual_integration_tree, expected_state_id, trust_config_ref, observed_reviews, observed_at, waiver_validity, decision }` is freshly verified through COD-18's trusted path, separate from deterministic evaluation identity. On a report-only commit, reuse requires independently established SourceContentKey/build equivalence and unchanged mapping facts plus valid evidence and regenerated results. Changed mapping/source/policy/config inputs invalidate dependent reuse. Trusted acceptance also requires current authorization, regardless of deterministic equivalence.

## C16-02 — Additive ontology and state separation

New logical node types: `ArchitectureSystem`, `ArchitectureContainer`, `ArchitectureComponent`, `ArchitectureRelationship`, `ArchitectureConstraint`, `ArchitectureScenario`, `ArchitectureSnapshot`, `ArchitectureMapping`, `ArchitectureEvaluation`. Existing source `Component`, `Symbol`, `Module`, `Evidence`, node IDs and relationships retain their meaning. New types have no fabricated source location.

Every new graph row carries `model_id`, `snapshot_id`, `state_kind` (`accepted_intent`, `candidate_intent`, `observed_projection`, `predicted_projection`, `evaluation`) and `schema_version`. All row keys are namespaced by snapshot and logical ID; candidate writes cannot update accepted/source keys. Source refs are typed references to pinned source generations, never architecture/source ID aliases.

| Relationship | Legal endpoints | Cardinality and meaning |
| --- | --- | --- |
| `ArchitectureContains` | System→Container, Container→Component | Exactly one parent per container/component; no cycles; systems have no parent |
| `ArchitectureRelates` | Container→Container or Component→Component | Directed declared intent, with `relationship_id` referencing its ArchitectureRelationship record; not an observed edge or permission rule |
| `ArchitectureOwns` | ArchitectureComponent→revision-specific Symbol | One qualified primary owner per governed symbol; unresolved assignments have no accepted ownership edge |
| `ArchitectureDependsOn` | Component→Component or Container→Container | Directed qualified projection with source witnesses, domain and coverage; no self-edge at the selected level |
| `ArchitectureChecks` | ArchitectureEvaluation→constraint/scenario/mapping | Typed record reference with input identity, phase, outcome and provenance |

Cross-system edges are legal only through explicitly modeled same-level endpoints; no implicit ancestor permission. Validate the whole candidate and all endpoint combinations before publishing any projection. Do not use source-extraction filtering that silently drops invalid relationships. Persisted graph rows are disposable indices of YAML/table, never a second authority store. Physical schema installation/storage layout is a later bounded implementation design requiring Scryer placement and compatibility evidence.

## C16-03 — Strict modular YAML

One root, proposed default `architecture/model.yaml`, explicitly lists architecture and constraint documents. No recursive includes, globs, directories or implicit file discovery. All paths resolve relative to the root directory, not the including file. Reject absolute paths, `..`, empty segments, backslashes, remote URLs, symlinks and duplicate canonical paths; read a confined, coherent byte snapshot and recheck identities before use. Path replacement/races must not escape confinement or mix revisions.

YAML is the JSON-compatible YAML 1.2 subset: one document/file, string map keys, booleans, null, integers and strings; no custom tags, aliases/anchors, merge keys, floats, timestamps or implicit dates. Duplicate keys, unknown fields, wrong scalar types and trailing documents fail with file/span diagnostics. Input order does not grant override precedence. No executable interpolation or remote fetch. No graph→k-wiki dependency to borrow parsing.

```yaml
# architecture/model.yaml
schema_version: "0.1"
model_id: codebasegraph
architecture: [runtime.yaml]
constraints: [policy.yaml]
```

Each architecture document is `{ schema_version, systems, containers, components, relationships }`; all four lists are required, including empty lists. System entry: `{ id, name, responsibilities }`. Container: those fields plus `system`, `kind: application|data_store`. Component: those fields plus `container`. Responsibilities are nonempty arrays of `{ id, statement }` with globally unique IDs. Relationship: `{ id, level: container|component, from, to, description, contract_refs }`; contract refs are `ArtifactRef[]`. No handwritten source selector is required or accepted in these entity records.

```yaml
schema_version: "0.1"
systems:
  - id: codebasegraph
    name: codebaseGraph
    responsibilities:
      - { id: repository-knowledge, statement: "Provide repository knowledge operations." }
containers:
  - id: graph-runtime
    system: codebasegraph
    kind: application
    name: Graph Runtime
    responsibilities:
      - { id: runtime-operations, statement: "Execute graph operations." }
components:
  - id: graph-read
    container: graph-runtime
    name: Graph Read Service
    responsibilities:
      - { id: bounded-reads, statement: "Execute bounded read-only graph operations." }
relationships: []
```

This minimal syntax example is not the complete pilot topology and cannot qualify pilot coverage or policy acceptance.

Each constraint document is `{ schema_version, ownership_rules, allow, require, budgets, waivers, scenarios }`, all lists required. Entries:

| Entry | Required fields and interpretation |
| --- | --- |
| Ownership rule | `{ id, component, responsibility_ids, all_of, none_of }`; versioned structural predicate clauses defined in C16-04; no arbitrary executable expressions |
| Allow | `{ id, level, from, to, projection }`; endpoints are explicit ID arrays (nonempty), evaluated as a Cartesian set; no glob or wildcard permissions |
| Require | `{ id, level, from, to, projection }`; each named directed pair must also be allowed; contradiction fails compilation |
| Budget | `{ id, level, origins, projection, metric, absolute_max, max_increase, baseline }`; nonnegative integer limits; baseline is an immutable evaluation/input reference |
| Waiver | `{ id, budget_id, baseline, original_limit, ceiling, expires_at, approval_ref }`; UTC RFC3339 expiry, approved identity/reference verified through COD-18 trust; no waiver for unknown evidence |
| Scenario | `{ id, parameters, preconditions, obligations, baseline_transform, candidate_transform }`; tagged definitions below; shared preconditions/obligations are part of the approved constraint closure |

`level` is component/container; projection is an exact version/domain reference. Governed origins and observation domains are pinned in evaluation inputs, not inferred from a provided result table. All governed projected edges require permission at each evaluated level. Required rules assert existence independently; permission alone asserts none. No deny language in v1. No defaults that create permission, required edges, budgets or waivers.

## C16-04 — Ownership and correction evidence

`MappingSubmission = { source_ref, proposed_component, responsibility_ids, structural_citations, rationale, inference_provenance, previous_mapping, change_kind, discriminator }`. Provenance includes external agent/model identification, inference input digest, rule version and supplied-result digest; no hosted model invocation. `change_kind = initial|retained|correction|transfer`. Rationale names the responsibility and explains why each plausible competing owner is less supported; confidence scores cannot satisfy a predicate.

An independent deterministic validator resolves every citation against pinned source and checks ownership cardinality, declared responsibility membership, current bytes/build context, unique binding qualification and the rule's discriminator. Well-formed citations that do not establish the required predicate yield `UNKNOWN`, not an automatic mapping. Fabricated/stale/malformed citations are explicit invalid-evidence diagnostics and block dependent checks. Each result records competing owners, rejected predicates and source lineage.

The selected conservative `responsibility-evidence/0.1` discriminator uses named structural capabilities in the approved rule closure. Evidence must establish qualified export identity or a qualified path from the symbol to the capability and distinguish competing components. Each rule has a nonempty `all_of` list and possibly empty `none_of`. Supported clauses are `{ kind: exported_item, crate, namespace, item_path }` and `{ kind: reaches_exported_item, crate, namespace, item_path, max_hops? }`. Export clauses test qualified case-sensitive export identity; reach clauses test directed qualified dependency paths with at least one edge. Optional `max_hops` is a positive semantic query parameter, not a hardcoded maximum; omission means full qualified reach subject to the effective execution budget. Linked contracts remain contextual ArtifactRefs; `linked_contract_anchor` is unsupported as decisive v1 ownership proof and produces `unsupported_evidence`/UNKNOWN. No adapter or additional durable manifest is introduced.

Predicate truth is `established`, `refuted` or `unknown`; do not confuse predicate falsity with a mandatory conformance violation. `all_of` requires every clause established; `none_of` requires every clause refuted with sufficient relevant coverage. A rule is nonmatching when at least one all_of clause is refuted or one none_of clause established. Otherwise missing proof makes it unknown. A matching rule requires complete evidence; one rule's failure is not proof that another matches.

Qualify an owner only when exactly one component's rule matches and all other component rules are provably nonmatching for that symbol. A missing competitor rule or incomplete negative evidence yields UNKNOWN. The validator never consumes the submitter's list of competitors as complete: it enumerates declared components/rules itself. Shared helpers are not assigned wholesale by filename, lexical proximity, majority owner or a better metric. A rule with no discriminating structural predicate cannot qualify semantic ownership; the result remains `UNKNOWN` until an approved evidence rule can distinguish it. These predicates qualify a policy interpretation; they do not prove semantic correctness. They may limit pilot sufficiency and must be tested, not presumed.

Retain pinned mapping when owner-relevant inputs match. A correction additionally supplies the prior mapping/evidence and independently checkable falsification of its decisive predicate plus qualifying new-owner evidence. A transfer additionally supplies changed responsibility/source inputs, qualified lineage and new-owner evidence; changed ownership alone is not proof of transfer. Missing/ambiguous lineage or equally supported owners yields `UNKNOWN`. No human per-mapping approval is introduced.

For every reassignment, record source delta, responsibility delta, ownership delta and (where qualified lineage exists) candidate dependencies under old versus proposed ownership. Record classification-only internalisation as classification, not removed code coupling. If the old-ownership counterfactual is unavailable, identify dependent results as `UNKNOWN`; do not accept an unsubstantiated improvement. Rule changes require the normal approved architecture/constraint revision, not an ad hoc mapping override.

**Settled C16-D1/D5:** use the conservative discriminator and export/path facts first. Unsupported contract anchors stay contextual/unresolved. AI confidence alone and a human mapping queue remain excluded. Actual sufficiency is a later qualification gate, not a consequence of this choice.

## C16-05 — Q1 dependency evidence and coverage

`QualifiedBinding = { caller: SourceRef, reference_span, namespace, lexical_scope, import_chain, target: SourceRef, build_context, rule_version, evidence_digest }`. Matching names alone never qualifies a binding. Preserve existing heuristic edges as advisory; qualified evidence is separately typed/versioned so old graph consumers retain their behaviour.

Q1 supports case-sensitive item/module identity, explicit/grouped `use`, aliases and statically named direct references only when all binding steps are independently established in the selected Rust build context. Disclose glob imports, unresolved cfg, macro-introduced bindings, re-export cycles, trait/type-dependent calls, external definitions and unsupported constructs. If any can affect the binding and cannot be excluded, return `UNKNOWN`. Do not invent resolution from a raw-source string match or trust a concrete `ResolvesTo` node.

The shared domain/scope/coverage types are:

| Type | Required fields and semantics |
| --- | --- |
| `DomainRef` | `{ id, version, repository_id, source_content_key, build_context_digest, extractor_digest, supported_relation_forms, source_unit_inventory_digest, exclusions }`; forms are `qualified_import` and/or `direct_reference`. Each exclusion carries `{ id, unit_or_site, reason, approved_policy_ref }` |
| `Scope` | `{ id, level: component\|container, origin_ids, domain_ref, boundary_mode: retain_known_targets }`; origins are a nonempty sorted set, references are validated at their level |
| `CoverageRecord` | `{ domain_ref, scope_ref, scanned_inventory_digest, site_results, check_coverage }` |
| Site result | `{ source_ref, reference_span, status: qualified\|unsupported\|unresolved\|unscanned, binding_ref?, reason? }`; qualified requires a valid binding; every other status requires a reason |
| Check coverage | `{ check_id, required_inventory_digest, status: complete\|incomplete, reasons }`; the validator derives the inventory and completeness rather than trusting submitted flags |

Independently enumerate pinned source/build inventories and the required source-unit closure for each check, including newly reached dependencies outside initial named components. Compare actual scanned inventories and bind the site-level results to them; counts alone are insufficient. Completeness requires matching inventories and all required bindings qualified, or an approved exclusion independently proved irrelevant to that particular check. Unsupported/unscanned sites are not approved exclusions. Exclusions cannot narrow a broader mandatory guarantee: if excluded facts are required for it, return UNKNOWN. Unavailable relation tables and truncated scans/traversals are explicit unknowns.

`LineageRecord = { previous: SourceRef, current: SourceRef, rule_version, basis, evidence_refs }`, where basis is `unchanged_qualified_identity` or `unique_structural_move`. Unchanged identity requires a unique same-namespace, case-sensitive qualified item path and kind/signature in each revision, plus unchanged owner-relevant source/build/responsibility predicates; changed semantics do not retain an unchanged-input mapping. Structural moves require exact normalized syntax/signature fingerprints (positions/location excluded; semantic token text preserved), unique one-to-one matches across independently enumerated required prior/current inventories and qualified surrounding binding/parent evidence excluding competing matches. Name, fingerprint or positional ID alone cannot establish continuity. Unsupported rewrites, splits/merges and ambiguous matches remain UNKNOWN. Lineage supports comparison; it never substitutes for separate correction/transfer ownership evidence.

Lift verified source→target symbol edges through each symbol's unique owner. Deduplicate directed architectural pairs, retaining witnesses. Remove same-component edges at component level and same-container edges at container level separately. Container lifting follows authored parents. Mixed-owner modules cannot create module-wide ownership shortcuts.

Proven unpermitted dependencies are violations even if other coverage is unknown. Required-edge absence is a violation only with complete evidence for that absence; otherwise unknown. A known witness can establish existence without proving the entire graph complete. Metric passes require coverage of their full relevant closure; a known lower bound exceeding a limit may establish violation despite incompleteness, but cannot produce an exact value or pass. Report exact/lower_bound/unavailable value kinds.

COD-25/F04 must prove the actual Graph Read Service pilot is sufficient, including its production dependency closure and affected API Core, Coordinator, Agent Hook and Wiki callers. The `execute_graph_search` placeholder gap at the selected baseline is an unresolved observation, not a pilot pass. Collision, case and shadowing negative controls need independently established endpoints. If Q1 is insufficient, return the measured blocker to the user; do not broaden resolution or narrow mandatory obligations.

## C16-06 — Checks, metrics, scenarios and transitions

`Phase = design|implementation|integration`. `CheckOutcome = PASS|VIOLATION|UNKNOWN`. Every `CheckRecord` has `{ record_id, kind, subject, phase, mandatory, rule_ref, scope, input_id, outcome, raw_value, effective_limit, evidence_refs, reason_codes, limitations }`. `raw_value` is a tagged exact/lower-bound/unavailable metric value or null for a non-metric check. Evaluation reports independent `has_violation`, `has_unknown`, `mandatory_complete`, `eligible`; eligibility requires all enumerated mandatory records complete and PASS. Never collapse coexistence into one verdict that hides either state.

Design results label assumptions and predicted dependencies, not actual ownership or executed tests. Implementation requires current qualified observations and linked passing execution evidence. Integration additionally requires actual combined integration contents, current trust/review/expiry observations and expected-state comparison. Design eligibility is not implementation/integration permission.

Metrics: `cyclic_nodes` counts selected vertices in SCCs of size >1 in the full relevant projection; fanout/reach maxima use selected origins while retaining out-of-origin targets. Reach excludes the origin; include reachable closure and intermediate coverage. Empty origin sets are invalid. A real single-container projection can yield exact zero only with qualified coverage; synthetic graphs qualify algorithms, not the pilot. Retain per-origin changes and SCC witnesses; aggregate maxima allow redistribution and are not per-origin guarantees.

Both absolute ceiling and `candidate - baseline <= max_increase` apply to comparable exact values; compute signed deltas without unsigned wrap. Baseline refs pin projection, scope, metric definition and source/mapping identity. Incompatible comparisons yield UNKNOWN. Recompute affected reach origins or verify complete closure identity. No numeric pilot limits are proposed here.

Waivers retain original limit/raw overrun; effective PASS requires a qualifying current waiver, candidate <= ceiling and zero deterioration against latest accepted comparable ratchet state. Never raise the ratchet on improvement. Expiry equality is expired. Invalid or unavailable waiver authority cannot authorise PASS; retain any provable ordinary violation separately. Current expiry/review rechecked at the final decision.

Typed isolated proposal operations: `add_element`, `replace_element`, `remove_element`, `add_relationship`, `replace_relationship`, `remove_relationship`, `add_constraint`, `replace_constraint`, `remove_constraint`; each carries its complete typed record or exact ID and expected record digest for replace/remove. Apply a bounded ordered batch transactionally, validate the final candidate, reject dangling endpoints, no implicit cascading delete. No raw Cypher/SQL, source mutation or arbitrary field patch. Complete candidate YAML is an alternative, not combined ambiguously with operations.

Scenario parameter values are tagged `ElementId { level, id }`, `ContractRef { reference: ArtifactRef }`, `Integer { value }` (nonnegative) or `String { value }`. Parameter substitution uses a whole typed `{ parameter_ref }`, never string interpolation or coercion. All parameter references must exist and match the field's type. `ElementRef` carries level and ID. The obligation schemas are:

| Kind | Required fields besides kind/id | Meaning |
| --- | --- | --- |
| `element_exists` | `element: ElementRef` | Element exists at the specified level |
| `relationship_exists` | `from, to, level, projection` | Directed relationship exists in that projection; endpoint levels must match |
| `contract_ref_equals` | `relationship_id, expected: ArtifactRef[]` | Complete canonical-set equality of the relationship's contract refs, comparing repository/commit/path/content_digest |
| `reachable` | `from, to, level, projection` | A qualifying directed path with at least one edge exists |
| `no_bypass` | `from, to, via, level, projection` | No qualifying from→to path remains after removing via; endpoints/intermediary must be distinct and at the selected level |

For contract equality, reordered identical refs are equivalent; added/removed/changed refs violate. Duplicate refs and malformed input are compilation diagnostics; unavailable required contents are UNKNOWN. An empty expected set explicitly asserts no contract refs. Equality is structural, not behavioural compatibility.

Every no_bypass obligation requires an explicit paired reachable obligation with the same endpoints/projection at scenario compilation; the runner must not silently inject or weaken authored obligations. A found bypass is VIOLATION; complete absence is PASS for that obligation; insufficient coverage is UNKNOWN. The paired reach prevents disconnected candidates from succeeding vacuously. Absence of a required path/relationship is a violation only with sufficient coverage.

Scenario `preconditions` are required alongside parameters/obligations and use the same tagged positive predicate forms; no arbitrary code or negation language. An unproved or inapplicable precondition makes a mandatory scenario UNKNOWN, never skip-PASS. Common parameters, preconditions, capability/contract refs and obligations are immutable in the approved constraint closure. Both transformations use typed operations against their own snapshots. Only concrete transformations may differ. Check each transformed snapshot's obligations before comparing metrics; proven omitted obligations violate, unresolved postconditions are UNKNOWN. No behavioural/performance simulation is implied.

Pilot fixtures must define both a Graph Read Service extension and a dependency/boundary-contract change plus a bypass control. Their concrete target IDs and added capability/contract are still a pilot scenario design deliverable; do not fabricate approved functionality here. Until those definitions are settled/approved, mandatory scenarios remain unavailable, not empty successful tests.

`TransitionRequest = { expected_state_id, candidate_input_id, evaluation_id, integration_content_id, authorization_context_ref }`. Final trusted execution regenerates deterministic mandatory checks, verifies table equivalence, fetches current authority/reviews/expiry through the COD-18 path, then conditionally promotes against the expected state. A mismatch returns `state_conflict` with current identity; never rolls it back. Revalidate on conflict. A local preview/dry run cannot publish an accepted-state change. COD-18 must supply the actual serialization boundary covering both freshness checks and integration; until qualified, transition is blocked as `unsupported_integration_path`.

## C16-07 — C1 public Rust and private dispatch contract

Keep existing `OperationRequest` variants, public struct fields, `OperationDescriptor` fields, `OperationExecutor::execute`, `CodebaseGraphApi<C>::execute_operation` and public paths unchanged. No retroactive `non_exhaustive`, new required trait methods or extra public literal fields on those types. Existing error/response shapes and old operation defaults remain intact.

Add exported `ArchitectureRequest` (non-exhaustive from introduction), `ArchitectureResponse` and `ArchitectureExecutor` with:

```rust
pub trait ArchitectureExecutor {
    fn execute_architecture(
        &self,
        request: &ArchitectureRequest,
        context: &ArchitectureExecutionContext,
    ) -> Result<ArchitectureResponse, ApiError>;
}
// Additive inherent facade method only when C: ArchitectureExecutor:
// CodebaseGraphApi<C>::execute_architecture_operation(
//     &self, &ArchitectureRequest, &ArchitectureExecutionContext)
//     -> Result<ArchitectureResponse, ApiError>
```

Old custom executors need implement only their old trait. ApiCore implements both. `ArchitectureExecutionContext` is a new opaque context constructed from validated shared resource policy with a cooperative cancel handle; it carries no serialized authority and invents no numerical defaults. New types start with private fields plus validated constructors/builders and accessors; structured deserialization invokes the same validation. Reuse RepoSelector and OutputFormat without changing them. The existing generic `execute_invocation(operation_id, OperationInvocation)` remains usable and returns the existing `OperationResponse`; its architecture payload is the same logical ArchitectureResponse record set, with execution context resolved through shared repository policy. Missing required policy blocks architecture evaluation. An additive context-aware invocation method enables transport cancellation without altering the old method. Typed/block presentation never changes logical outcomes.

One private registry owns all operation IDs, descriptors, normalization, schema references, handlers and output metadata. Replace private old-family-only handler coupling with a private tagged dispatch request (`Legacy`/`Architecture`) and typed handler association; legacy requests are wrapped internally. One normalization/validation entry routes by catalog family; transport adapters do not implement defaults or semantics. Family mismatches reject explicitly. The public descriptor shape stays unchanged; a private family field belongs on private registration metadata, not the public struct. Existing legacy catalog and resolve methods obtain descriptors from this same catalog; additive architecture descriptors use the existing shape. No parallel authoritative catalog, validation registry or adapter-specific evaluator.

Public Graph Read Service retains graph reads. Language Syntax Graph owns qualified binding evidence under proposed Q1 extension. Proposed Native Architecture domain consumes those facts and owns inference validation, projection, constraints, metrics/scenarios and evaluation. Core composes services; adapters translate. These future responsibilities require a separately approved Scryer plan before coding. Begin with cohesive modules, no speculative crate split. Preserve private native-verification capability construction, retained writer/read borrows and publication validation; this change does not expose forgeable tokens or raw storage mutation.

## C16-08 — Operation and wire contracts

Every new operation accepts a strict object with `wire_version: "0.1"`, `repo: RepoSelector`, `output_format` (default Typed) and a typed body below. Unknown fields/types reject before execution. Each input reference includes its digest; there are no ambient “latest” mutable inputs. `ModelInput` is exactly one of a confined pinned root closure or complete inline file map; inline files have the same paths/parser/bounds. `CandidateInput` is exactly one of complete ModelInput or a typed batch against a pinned base. Evidence submissions are inline typed records or explicit confined ArtifactRefs, never arbitrary remote requests selected by YAML.

`ArchitectureRequest` is internally tagged on the wire as `{ operation, request }`; proposed variants are `Compile`, `ValidateEvidence`, `Evaluate`, `Scenarios`, `Verify`, `Report`, `VerifyApproval`, `Accept`, mapped exactly to the operation IDs below rather than deriving names from Rust spelling. The nested request contains the common fields and body fields specified here. An invocation's operation ID must match its typed variant; mismatches are invalid input. Requests use the same integer/enumeration/path validation in Rust, CLI argument translation and MCP JSON. No permissive string-to-number coercion.

| Operation ID / MCP tool / CLI subcommand under `architecture` | Required body | Result and side effects |
| --- | --- | --- |
| `architecture-compile` / `architecture_compile` / `compile` | `model` | Typed model, closure/input identities, compile diagnostics; no acceptance |
| `architecture-validate-evidence` / `architecture_validate_evidence` / `validate-evidence` | `model`, `source`, `mappings`, `domain` | Qualified/unresolved mappings, corrections and coverage; automatic validation, no human mapping gate |
| `architecture-evaluate` / `architecture_evaluate` / `evaluate` | `candidate`, `baseline`, `phase` (design only), `evidence` | Predicted design checks and assumptions; no promotion |
| `architecture-scenarios` / `architecture_scenarios` / `scenarios` | `candidate`, `baseline`, `scenario_ids`, `evidence` | Both transformed snapshots, common obligations and metric comparisons; no active mutation |
| `architecture-verify` / `architecture_verify` / `verify` | `candidate`, `baseline`, `phase: implementation|integration`, `evidence`, `integration_content` (integration only) | Actual conformance, test/provenance checks; a successful preview does not promote |
| `architecture-report` / `architecture_report` / `report` | `evaluation_inputs`, `claimed_table` (optional) | Regenerated table bytes and record equivalence; no file writes by default |
| `architecture-verify-approval` / `architecture_verify_approval` / `verify-approval` | `closure`, `review_context_ref`, `expected_state_id` | Current authorization observation through trusted configuration; read-only, not acceptance |
| `architecture-accept` / `architecture_accept` / `accept` | `transition` | Conditional promotion only on the supported trusted final path; local/unsupported paths return blocked result |

`evidence` is `{ mappings, coverage, requirements, contracts, test_executions, inference_inputs }`, with typed refs and explicit missing/inapplicable entries. `test_execution` identifies definition/config/input revisions, producer/run identity, result, durable report ref/digest and tested content fingerprint; a candidate-supplied PASS is untrusted until independently verified/regenerated. No operation accepts arbitrary trust-policy/workflow/reviewer overrides. COD-18 resolves authorization context refs through maintained trust.

ArchitectureResponse is `{ wire_version, operation, input_id, evaluation_id, records, summary, diagnostics, continuation }`. IDs not applicable to compile/validation are null with typed applicability, not fabricated evaluation evidence. Diagnostics contain stable code, severity, file/span or record subject, message and related refs. `continuation` identifies completed input-bound records and unfinished work for explicit retry; it is not authority or a durable job database. No silent mutation or automatic promotion on retry. The architecture response is complete logical data; block output can compact presentation but cannot drop mandatory unknown/violation evidence.

**Settled C16-D2:** expose all eight operations, including explicitly guarded `accept`, with previews separated from promotion. Unsupported local/integration paths cannot promote; COD-18 still supplies the trusted integration contract. Local PASS is not approval.

## C16-09 — Errors, cancellation, bounds and reporting

Malformed invocation/model errors return existing-shaped ApiError with stable new codes: `architecture_invalid_input`, `unsupported_version`, `duplicate_key`, `duplicate_id`, `unknown_field`, `invalid_reference`, `invalid_hierarchy`, `path_not_confined`, `contradictory_policy`. `architecture_configuration_required` reports missing/invalid operational resource policy. `details` includes bounded structured diagnostics. Completed evaluation with violation/unknown is a normal response, not an invocation error. Runtime unavailable input produces an UNKNOWN mandatory record when a well-defined evaluation exists; failure to establish any input identity returns `architecture_input_unavailable` instead. No empty PASS response.

`state_conflict` and `input_changed` are retryable only by explicit re-evaluation with new identities. `result_integrity_failed`, `approval_unqualified`, `unsupported_integration_path` block promotion. Invalid input is not retryable unchanged. `resource_limit`, `cancelled`, `deadline_exceeded` leave unfinished mandatory records UNKNOWN and preserve current accepted/readable state; valid completed records may be returned bound to their original inputs. Unexpected internal failures use `architecture_internal_error`; never mask them as PASS.

CLI codes for this new command family: 0 successful compile/validation or all mandatory checks PASS; 2 malformed invocation/model/configuration or pre-evaluation resource-limit rejection; 3 evaluation includes mandatory VIOLATION; 4 mandatory UNKNOWN (including resource/cancellation) or required input unavailable before an evaluable identity; 5 expected-state conflict/input-changed conflict; 6 integrity/authorization/path rejection; 1 unexpected infrastructure failure before an evaluable identity. If violation and unknown coexist, return 3 and retain both flags/records in output. Machine CI JSON retains the same records and reasons. MCP reports invocation errors through existing error machinery, while evaluable check failures remain typed results. Existing commands retain their exit/error semantics.

Architecture tasks use cooperative cancellation/deadline context below public request semantics; adapters translate interrupt/MCP cancellation into it. Check before expensive stages and within bounded loops; stop at a safe point. Preserve the coordinator's real execution permit until actual execution completes or cleans up. Caller timeout/disconnect is not proof of native cancellation and cannot release that permit. Never replay any operation after ambiguous post-dispatch failure; only proved pre-dispatch failures may retry. If transition may have begun, explicit recovery queries current accepted identity; no blind rollback/replay. Cancellation after atomic transition reports the actual committed identity when discoverable, never claims it was undone. Temporary candidate resources release leases on real completion/cleanup. Existing native verification-token construction and writer/read borrows remain intact.

**Settled C16-D3/D4:** no invented fixed numerical payload, depth, cardinality, hop or execution-time caps. Use explicit shared repository `ArchitectureExecutionPolicy` values across CLI/MCP/CI; callers may only tighten. Policy is operational repository configuration outside candidate YAML, not an architecture constraint or candidate-selected authority. It is resolved once per invocation through the common request path and pinned by digest plus full relevant values in evaluation provenance. Configuration location wiring follows the existing repository configuration owner; no separate settings registry or product source changes are made here.

Required policy fields are positive integers for `max_closure_files`, `max_file_bytes`, `max_yaml_bytes`, `max_parse_depth`, `max_authored_entities`, `max_expanded_rule_pairs`, `max_mapping_records`, `max_citations_per_mapping`, `max_rationale_bytes`, `max_proposal_operations`, `max_scenarios`, `max_projected_vertices`, `max_projected_edges`, `max_traversal_steps`, `max_evidence_bytes`, `max_result_bytes`, and `max_evaluation_duration_ms`. No numerical defaults are supplied. Missing/invalid required settings return `architecture_configuration_required` before evaluation; explicit per-invocation overrides may only lower values. Effective deadline is the earlier caller deadline or configured duration from invocation start. A configuration change invalidates dependent reuse even when it only increases a budget. Trusted integration selects its repository policy independently of candidate claims.

Check input bytes before allocation where possible and counters during parsing/expansion/traversal; use fallible allocation and safe cancellation points. Exceeding a configured limit rejects before evaluable identity or leaves unfinished mandatory checks UNKNOWN afterwards. No silent clamping, truncated mandatory PASS or infinite disabled limits. Semantic cardinality (one owner/parent), C4 grammar, version/digest formats, existing graph_query bounds and separately approved numerical metric budgets remain; they are not arbitrary execution caps. Resource values are deployment inputs, not a performance guarantee or D17 calibration.

Durable output remains YAML plus **one** Markdown table, proposed fixed path `architecture/evaluation-summary.md`, all evaluations/record kinds in that table. Cells contain canonical JSON values (Markdown-escaped reversibly), not free-form lossy summaries. Columns: `evaluation/phase`, `kind/subject/id`, `inputs/baseline`, `result/raw/effective`, `evidence/rationale`, `authorization/waiver/limitations`. Include evaluation identity/versions, each accepted symbol mapping and compact discriminator evidence, coverage, source/classification deltas, constraints, metrics/origin changes, scenarios, test refs, transition and summary. Required row sets come from trusted scope/policy, not supplied rows. Mapping inputs are hashed separately despite excluding this fixed table path from source fingerprint. No other exclusions, report-selected filenames or candidate-controlled excludes. Rendering/round-trip must preserve logical records.

The report operation returns bytes; an explicit adapter output action may atomically replace only that fixed file after successful generation, never partially truncate it. It cannot commit, approve or promote. Full logical output over the configured bound is UNKNOWN/limit diagnostic, never a reduced table presented as complete. No separate raw-report archive, signing service, external evidence database or durable task queue. Existing linked executed evidence retains its original location. Persisted derived cells are regenerated through the trusted evaluator in v1; candidate hashes or producer labels do not authenticate them. Later signing/provenance mechanisms would require a new contract.

Table equivalence compares independently enumerated deterministic logical mapping/evidence/result records against regenerated trusted results. Fresh authorization/expiry observations are verified separately and do not enter their evaluation's input hash. A post-transition receipt is observational output, not a pre-required input; an unavailable receipt cannot undo an atomic accepted transition. Actual integration commit/tree identity is checked against the evaluated content key while retaining original evaluated provenance. Report-only commits therefore do not require a fabricated new execution or a self-referential accepted-state hash.

## Traceability and planned qualification

| Requirement / acceptance / review | Contract | Downstream work and required evidence |
| --- | --- | --- |
| R01 / A01 | C16-01–03 | COD-20/21/22: additive schema, strict YAML/confinement/closure identity fixtures |
| R02 / A02–03 | C16-02/06/08/09 | COD-29/31/33: isolation, phase separation, cancellation and stale-state races |
| R03 / A04–05 / APR-01 | C16-04/05/09 | COD-23/24/27: qualifying/competing owners, valid correction/transfer, classification-only negative control |
| R04 / A06 | C16-05 | COD-25/26/28: actual Q1 binding/coverage, wrong-concrete-target controls, mixed modules; sufficient pilot before calibration |
| R05 / A07 | C16-03/05/06 | COD-30/31: explicit allow/require, contradiction and unknown absence |
| R06 / A08–09 | C16-05/06 | COD-34/35/38: hand-computable SCC/fanout/reach, nonlocal invalidation, real measured pilot and later human budgets |
| R07 / A10 / APR-03 | C16-03/06 | COD-36/33: expiry/raw overrun and 10→8 then stale-9 transition race |
| R08 / A11 / APR-05 | C16-06 | COD-39/40: both pilot scenario definitions, common obligations, weakened-candidate/bypass controls |
| R09 / A12 / APR-02 | C16-01/08/09 | COD-32/41/42: complete identities, tampering/omission, explicit retry and trusted recomputation |
| R10 / A13/A18 / APR-03/APR-04 | C16-06/08/09 | COD-18/37/46/47: trusted bootstrap, current reviews/expiry/producer, actual protected integration serialization and skip/failure controls |
| R11 / A14 | C16-09 | COD-45: table round-trip, graph retirement, mapping provenance, forged result rejection |
| R12 / A15–16 | C16-07–09 | COD-43/44: shared catalog, MCP/CLI/CI parity, unchanged exhaustive matches/literals/custom executors and read-only graph_query suites |
| R02–12 / A17 | All | COD-48/49/50: full actual pilot loop, supported-platform qualification and fresh final adversarial review |

Each downstream owner receives these input/output/error contracts, not product-file ownership. Exact implementation files and shared-file owner are set in the applicable Scryer brief after COD-19 approval; manifests/lockfiles have one writer. No topology or ownership assignment is invented by a traceability row.

## Reconciliation and handoff gates

All material choices raised in this contract finalization are recorded in [cod16-decisions.md](cod16-decisions.md), including the user's instruction to implement the documentation plan. Illustrative fixtures are [cod16-fixtures.md](cod16-fixtures.md); actual static validation and independent review evidence belong in [cod16-review.md](cod16-review.md). These are specification/review artifacts, not extra product runtime outputs.

Historical revision 0.2 and all COD-17 files remain byte-identical. The branch starts at COD-17 head `f67e1d95862484dee34c8e818c98b72959366c5b`; PR #184 remains a separate prerequisite targeting dev. Source baseline `90e2ab6fae2767bdb158e19f0072d43568e80dbc` and the handed-off COD-17 worktree are preserved. Linear's old uncommitted-spec wording is historical/stale; publication is distinct from approval. COD-16 tracking records this documentation work only, without closing the issue or advancing another issue.

Reconciliation used AGENTS/RTK, memory/wiki, the CodebaseGraph Linear workspace, Scryer rules/orientation and explicit semantic graph search/context. Unrelated Scryer pending intent, stale anchors and drift were not adopted, reconciled or declared fixed. No product implementation, fresh product suite, hosted-protection qualification, Q1 sufficiency experiment or extension execution is claimed. Existing baseline tests/CI remain historical evidence for existing behaviour.

The finalized supplement, decisions and illustrative fixture definitions require independent final review of their actual persisted diff before PR handoff. COD-18 supplies trust and the supported integration path; COD-19 must publish/pin the final specification and exact-scope product implementation approval and perform the explicit repository procedure transition. COD-25/F04 must prove Q1 coverage before projection acceptance/calibration; COD-40 owns concrete pilot scenario instances; numerical limits remain subject to D17. Material downstream changes require the existing revision/impact/approval process. No product implementation, merge, release, authority migration or issue closure is authorized here.

Reusable knowledge assessment: issue-specific specification decisions are already retained in canonical documents; no additional durable memory claim is warranted. No wiki/memory writes are needed for this documentation supplement. Aggregate closure remains with maintainers.
