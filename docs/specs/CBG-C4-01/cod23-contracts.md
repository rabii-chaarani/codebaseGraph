# COD-23 evidence-validation supplement — proposed revision 0.1

## Identity and decision boundary

Work: `CBG-C4-01/F03/S01` / [COD-23](https://linear.app/codebasegraph/issue/COD-23), parent COD-7, M1/P2; 7 November 2026, Australia/Darwin. Status: **concrete contract proposal; exact-subject approval pending**. This document refines R03/R09/R11, A04/A12/A14, D05–D08 and APR-01. It neither completes those aggregate criteria nor changes their downstream owners.

Reuse [specification 0.3](https://github.com/rabii-chaarani/codebaseGraph/blob/66dff2564e76ad06cdf172332e2da318d150fc73/docs/specs/CBG-C4-01/spec.md), its exact C16/C18/C1/Q1 contract set, [actual Scope A/B approval and scoped Git/bootstrap activation](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/approvals/cod19-approval.md), [approved C21 supplement](https://github.com/rabii-chaarani/codebaseGraph/blob/9b4384378c03e05a458f8f94291c8bba4d82c7c3/docs/specs/CBG-C4-01/cod21-contracts.md) and [approved C22 supplement](https://github.com/rabii-chaarani/codebaseGraph/blob/8635297ff7f25bfd745d959f0ec7917982afa802/docs/specs/CBG-C4-01/cod22-contracts.md). Preserve their immutable subjects and historical approvals. No Scryer runtime, sign-off or invented claim is introduced for this activated scope; unrelated Scryer authority remains intact.

The maintainer selected these directions in the COD-23 planning chat and then explicitly requested implementation of the resulting plan:

| Decision locator | Exact selected answer | Consequence |
| --- | --- | --- |
| `cod23_qualified_facts_boundary` | Keep staged validation (Recommended) | Source/node checks can complete; qualified export/binding/path evidence awaiting COD-25 remains UNKNOWN. COD-24 retains ownership selection. |
| `cod23_rationale_shape` | Structured explanations (Recommended) | Explicit responsibility and competing-component IDs with exact explanation text; text does not prove structure. |
| `cod23_inference_provenance` | Identify supplied inference (Recommended) | Require agent/model/input/rule/result attribution; do not require raw inference payloads or claim authenticated execution. |
| `cod23_source_fact_validation` | Re-extract pinned bytes (Recommended) | Verify immutable Git bytes, then reuse existing parser/materializer in memory; no live-node shortcut, graph rebuild or graph write. |

Those choices authorize preparing this contract and its review/publication. The accepted plan expressly retains **exact-SHA human approval before dependent implementation**. No public input shape below is implementation-approved merely because it is now written or reviewed. A later actual decision record identifies the published full SHA, exact path/scope and human answer; this file never embeds its own containing commit SHA.

## C23-01 — Bounded service and staged result

Add a transport-neutral `architecture::evidence` service. Its consumers supply an opaque successful `CompiledModel`, declared evaluated `SourceProvenance`, mapping submissions, independently configured local `RepositoryBinding`s and cooperative execution context. Candidate strings cannot select a local directory, parser configuration, broader resource policy or graph generation.

COD-23 validates the submitted facts; it does not enumerate the whole governed source inventory, assign an owner, prove a correction/transfer, lift dependencies, evaluate architecture constraints, persist an evaluation table or accept a model. Missing qualification remains visible. An empty collection is a valid empty mapping-facts input, never proof that mandatory mappings or coverage are complete.

The Rust entrypoints are:

```rust
pub fn validate_evidence(
    model: &CompiledModel,
    source: &SourceProvenance,
    mappings: &[MappingSubmission],
    repositories: &[RepositoryBinding],
    context: &ExecutionContext,
) -> Result<EvidenceValidation, EvidenceError>;

pub fn mapping_facts_digest(
    source: &SourceProvenance,
    mappings: &[MappingSubmission],
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<MappingFactsDigest, EvidenceError>;

pub fn source_extractor_identity(
    context: &ExecutionContext,
) -> Result<SourceExtractorIdentity, EvidenceError>;
```

Validation reuses the compiled model's complete effective execution policy, without broadening it or restarting an overall caller deadline. Digest-only service calls resolve their explicit policy/overrides once. The extractor helper reports the implementation's built-in descriptor; it grants neither source/build qualification nor execution authority. Output types have private fields, read-only getters and no unchecked success constructor or deserializer. Input records are typed untrusted claims, not capabilities. No new dependency, numerical default or adapter operation is added.

## C23-02 — Exact input vocabulary

All records deny unknown fields and duplicate object keys. Use existing C16/C20 `Id`, `Text`, `Digest`, `SourceRef`, `SourceProvenance`, `ArtifactRef` and `ByteSpan` primitives. Required fields must be present, including explicit `null` and empty collections where specified; there are no implicit input defaults. Ordered arrays remain ordered unless explicitly declared sets in C23-03. No float, confidence or submitted qualification/completeness boolean is accepted.

```text
MappingSubmission = {
  source_ref: SourceRef,
  proposed_component: Id,
  responsibility_ids: Id[],
  structural_citations: StructuralCitation[],
  rationale: Rationale,
  inference_provenance: InferenceProvenance,
  previous_mapping: PreviousMapping | null,
  change_kind: "initial" | "retained" | "correction" | "transfer",
  discriminator: Text
}
Rationale = {
  responsibilities: [{ responsibility_id: Id, explanation: Text }],
  competing_components: [{ component: Id, explanation: Text }]
}
InferenceProvenance = {
  agent_id: Text, model_id: Text, input_digest: Digest,
  rule_version: Text, result_digest: Digest
}
StructuralCitation = { id: Id, source_ref: SourceRef, assertion: Assertion }
Assertion = { kind: "node_exists" }
          | { kind: "scope_contains", member: SourceRef }
          | { kind: "exported_item", crate: Text, namespace: Text, item_path: Text }
          | { kind: "reaches_exported_item", crate: Text, namespace: Text,
              item_path: Text, max_hops: positive u64 | null, path: SourceRef[] }
          | { kind: "linked_contract_anchor", reference: ArtifactRef, anchor: Text }
PreviousMapping = {
  source_ref: SourceRef, component: Id, mapping_facts_digest: Digest,
  evidence_ref: ArtifactRef, lineage: LineageRecord | null
}
LineageRecord = {
  previous: SourceRef, current: SourceRef, rule_version: Text,
  basis: "unchanged_qualified_identity" | "unique_structural_move",
  evidence_refs: ArtifactRef[]
}
```

`node_exists` and `scope_contains` describe contextual source-graph facts only. They add no ownership predicate to YAML or to `responsibility-evidence/0.1`. `scope_contains` asserts a directly reconstructed `Contains` relation from a `Scope` node to the named member; it is not a transitive, export or dependency claim. The export/reach forms name the existing C16 predicate vocabulary. The reach `path` is an ordered claimed witness, not a trusted binding chain. An empty path is allowed as absent supporting proof and remains UNKNOWN. `linked_contract_anchor` remains unsupported decisive evidence; merely qualifying a file's bytes would not validate the anchor's semantics.

For a current mapping, every current SourceRef in the source field, citation source, scope member and reach path must use `source.repository` and `source.evaluated_commit`. Previous-mapping and lineage-previous references may name a different pinned revision; this exception does not permit current citations to mix revisions. Lineage endpoints must equal their referenced previous/current SourceRefs exactly. History descriptors and their linked evidence are retained and hashed, never qualified by their presence or digest alone. COD-27 retains history/lineage/correction qualification.

`discriminator` supports exactly `responsibility-evidence/0.1`. Other nonempty versions produce unsupported-evidence/UNKNOWN rather than silently selecting another rule. The inference `rule_version` must equal the submission discriminator; disagreement is an explicit invalid-evidence diagnostic. Attribution fields identify supplied information. Their digests are not claims that COD-23 captured those payloads, authenticated the named model or reproduced its execution.

Against the compiled model, require a declared proposed component and a nonempty set of its declared responsibility IDs. Rationale responsibility IDs must equal that set. The validator independently enumerates all declared components; rationale competitors must name each other declared component exactly once and cannot include the proposed owner. This conservative explanation obligation includes competitors whose rules are missing or unqualified; explanations never refute those rules. Missing explanations, undeclared IDs and inconsistent references are invalid evidence. An empty structural-citation set remains explicit insufficient-evidence/UNKNOWN, not qualifying support.

`initial` requires no previous mapping. The other change kinds require a previous descriptor; missing or inconsistent history is reported, and history-dependent qualification remains UNKNOWN. Source movement cannot reuse a positional ID as proof of continuity. No recursive previous-submission structure or new historical ledger is introduced.

## C23-03 — Mapping-facts identity

Hash SHA-256 over ASCII `architecture-mapping-facts/0.1`, one LF, and the compact C22 canonical JSON payload, without a trailing newline:

```text
{ evidence_version: "architecture-evidence/0.1",
  source: SourceProvenance,
  mappings: MappingSubmission[] }
```

Return the C16 `sha256:` Digest in an opaque `MappingFactsDigest`. The digest includes the complete input records above, not validation results, timestamps, model caches, diagnostic ordering, local clone paths or itself. Including declared source provenance does not verify it. The compiled model identity and effective policy are separate validation-context constituents; a mapping digest alone cannot authorize validation reuse, establish a complete InputIdentity or authenticate persisted results.

Canonical sets and their exact sort keys are:

| Field | Sort key |
| --- | --- |
| `mappings` | `(source_ref.repository, source_ref.commit, source_ref.node_kind, source_ref.node_id)` |
| `responsibility_ids` and rationale responsibilities | Exact responsibility ID |
| Rationale competing components | Exact component ID |
| `structural_citations` | Citation `id`, unique within its mapping |
| Lineage `evidence_refs` | `(repository, commit, path, content_digest)` |

Reject duplicate set keys; never deduplicate them silently. Two submissions with the same mapping sort key reject, including conflicting owners or changed location data, rather than selecting one or publishing a partial digest. Distinct keys with conflicting claimed identity are checked independently during validation. Reach witness `path` is ordered; every other array in this vocabulary is covered by the table. Exact decoded explanation/identifier text is retained without Unicode, whitespace, case or temporal normalization.

Changing any source/provenance field, owner, responsibility, citation ID/content, explanation, inference attribution, previous mapping, lineage or discriminator changes the payload and digest. Reordering a declared set preserves it; changing a witness's order does not. Independent tests must bind the actual canonical bytes and domain separator, not merely compare the implementation to itself.

## C23-04 — Independent pinned source reconstruction

Resolve repository identities only through supplied configured bindings. Reuse the existing verified commit/tree/path/blob capture machinery; preserve its SHA-1/SHA-256 support, literal path behavior, confinement, integrity checks and unavailable-versus-corrupt distinction. Verify the evaluated commit's tree even for an empty mapping collection. Verify each current file's raw digest, regular-file membership and byte-span bounds against immutable Git bytes. Never fetch, checkout, follow symlinks/submodules, run filters or consult working-tree bytes. Read duplicate files once per repository/commit/path/digest within the invocation; inconsistent claimed digests do not select a winner.

Use the existing built-in Rust profile, parser, normalization and source-graph materializer. Re-extract the complete cited file in memory, without installing a graph or using heuristically resolved dependency edges as qualified facts. No new language parser or resolver is introduced. Unsupported language, malformed UTF-8, parse errors, unsupported extractor identity or unavailable reconstruction produce explicit unknown reconstruction; they do not establish node absence. Continue independent checks that can still identify invalid Git/revision/span input.

The complete `SourceExtractorIdentity` descriptor is `{ version: "native-source-evidence/0.1", language: "rust", profile: <complete actual built-in Rust LanguageProfile>, grammar_node_types_digest: <raw SHA-256 of the linked grammar's NODE_TYPES>, node_identity_version: "native-source-graph/1" }`; `digest` hashes that descriptor with domain `architecture-source-extractor/0.1`. Its public logical output is `{ descriptor, digest }`. `native-source-graph/1` denotes the existing graph ID/label/scope/materialization semantics at the pinned prerequisite; any change to those semantics requires a new descriptor version and regression evidence. This is deterministic implementation identification, not an authenticated binary measurement. Candidate profiles, descriptor overrides and a candidate's asserted extractor qualification are excluded.

Compare the declared `extractor_digest` with the implementation's actual descriptor digest before using reconstructed nodes as proof. An unsupported/mismatched extractor yields UNKNOWN for reconstruction, with the actual and declared identities retained. The declaration of a `build_context_digest` is retained in the mapping-facts identity but cannot qualify cfg, imports, exports or bindings; build-context qualification remains explicitly UNKNOWN pending Q1.

Match SourceRef `node_kind` to the reconstructed row's exact graph table, `node_id` to its exact ID, `symbol` to its exact `qualified_name`, `path` to its repository-relative path, and span to its exact nonempty `[byte_start, byte_end)` interval. Do not substitute a display label, line number, substring, approximate span or name placeholder. Only source-located rows with all these fields can establish a reference. Reconstruct with repository-relative `path`, actual built-in grammar version, configured repository identity as `repository_label`, empty physical `source_root`, and the materializer's existing default relation allowlist. Physical clone location must not change cited source-node identity. Support-only rows without source spans cannot qualify a SourceRef.

A missing/mismatched node under complete compatible reconstruction is explicit invalid evidence. A valid `node_exists` citation proves only this exact row. A valid scope assertion also requires its direct reconstructed Contains edge to the exact member. If a graph export/path claim names valid endpoints but lacks qualified binding evidence, retain valid endpoint checks and UNKNOWN predicate support. Neither a heuristic graph edge, a public-looking token nor a valid scope citation can establish/refute a C16 ownership predicate. Unavailable facts never hide proven inconsistent citations.

## C23-05 — Logical result and failures

`EvidenceValidation` logically serializes as:

```text
{ evidence_version: "architecture-evidence/0.1", mapping_facts_digest: Digest,
  source: SourceProvenance, model_id: Id, model_compile_input_digest: Digest,
  extractor_identity: SourceExtractorIdentity,
  effective_policy: ArchitectureExecutionPolicy,
  records: [{ submission: MappingSubmission, competing_components: Id[],
              source_check: EvidenceCheck, responsibility_check: EvidenceCheck,
              rationale_check: EvidenceCheck, inference_check: EvidenceCheck,
              history_check: EvidenceCheck,
              citations: [{ id: Id, source_checks: EvidenceCheck[],
                            assertion_check: EvidenceCheck }],
              ownership_qualification: "unknown", reason_codes: Text[] }],
  diagnostics: [{ code: Text, mapping_index: u64 | null,
                 citation_id: Id | null, subject: Text | null, message: Text }],
  has_invalid_evidence: bool, has_unknown: bool }
EvidenceCheck = { status: "valid" | "invalid" | "unknown", reason_codes: Text[] }
```

Records, submissions, citations and independently enumerated competitors follow canonical set order. `mapping_index` is the zero-based canonical record index. Citation `source_checks` order is citation source, then scope member or ordered reach path when present. `reason_codes` are sorted unique output codes. Diagnostics order is mapping index (global first), citation ID (mapping-wide first), code, subject, message. Canonical output carries every required submitted record and every resulting diagnostic, without truncating adverse evidence.

EvidenceCheck status describes a bounded check, not C16 predicate truth or architecture conformance. Inference `valid` means complete consistent attribution only. Initial history with no prior descriptor is valid as an absence statement; retained/correction/transfer qualification is UNKNOWN even if descriptor links are consistent. Every record's ownership qualification remains UNKNOWN in this slice. There is no accepted-owner list, eligibility flag, mapping-approval queue, full-input/evaluation/accepted-state ID or overall mapping PASS. Independently enumerated competitor IDs do not assert that all their rules were qualified.

Set `has_invalid_evidence` if any check or diagnostic identifies invalid evidence. Set `has_unknown` whenever any check or ownership qualification is unknown, including the missing Q1/build qualification; it is also true for an empty collection because required-scope completeness has not been established. Proven invalid evidence and unknown evidence coexist. Invalid evidence blocks dependent qualification; it is not automatically a mandatory architecture-conformance VIOLATION. Public error/code constants document these meanings without relying on message prose.

Malformed primitive/input/set shape and unsupported wire shape reject before a successful result, using `architecture_invalid_input` or `invalid_reference` as appropriate. Semantic evidence inconsistencies, missing repository/object/file inputs and compatible-file reconstruction failures are retained as per-record invalid/unknown checks where possible; missing inputs use `architecture_input_unavailable`, corruption uses explicit invalid-evidence diagnostics. Global evaluated-tree corruption/mismatch is an invalid global diagnostic, and its dependent source checks cannot be valid. Missing global revision/binding is global UNKNOWN. Failed capture must not manufacture a compatible tree from candidate claims.

Configuration, allocation, cancellation, deadline, execution-limit or internal failures return `EvidenceError` with stable code/diagnostics and no complete successful result. An over-limit result is not reduced to look complete. No automatic retry, accepted/source mutation or persistence occurs. Explicit retry supplies current inputs/context. No error return erases already known invalid/unknown distinctions in its diagnostic payload, but it carries no partial successful validation capability.

## C23-06 — Execution resources and placement

Apply the existing complete positive shared policy. Count submissions against `max_mapping_records`, each mapping's citations against `max_citations_per_mapping`, and the UTF-8 byte sum of all responsibility/competitor explanations against `max_rationale_bytes` per mapping. Count the canonical mapping-facts payload plus captured distinct file bytes against `max_evidence_bytes`; byte spans/references/paths and history/lineage descriptors are included in the payload. Count each file against `max_file_bytes`. No raw inference payload is allocated. Policy values and changes remain validation inputs, even though they are separate from MappingFactsDigest.

Count source-tree depth against `max_parse_depth`; source syntax/materialization visits and evidence traversal work against the invocation-wide `max_traversal_steps`. Count reconstructed source nodes/edges against `max_projected_vertices`/`max_projected_edges` for this evidence reconstruction, without claiming an architectural projection. These applications of existing configurable fields are explicit new interface semantics in this proposal; no hardcoded source-depth/node/time cap is invented. Enforce complete canonical output size against `max_result_bytes`, including submissions, context and all diagnostics. The extractor-descriptor helper is fixed implementation data, has only cooperative cancellation/deadline checks, and does not itself perform input evidence evaluation.

Check counters and byte budgets before copying/allocation where possible; use fallible owned scratch allocation and checkpoints within source normalization, capture marking, materialization, canonicalization, hashing and result construction. Reuse the existing parser/materializer through narrow guarded entrypoints; preserve legacy caller signatures, node IDs and emitted logical rows. Native parsing must observe cooperative cancellation/deadline through its supported progress mechanism. New guarded traversal and teardown must not rely on unbounded Rust recursion, including deep-source failure paths and dropping partial owned syntax trees. Guard instrumentation is an authorized prerequisite, not permission to redesign extraction or change its ordinary semantics. No global rebuild or persistent cache is required.

Native Architecture owns mapping validation, attribution and mapping-facts identity. Language Syntax Graph retains parsing, normalization and source-node/edge construction. A narrow internal bridge reuses compiler Git/canonical/policy helpers; no duplicated Git reader or canonical encoder. Neutral extraction control callbacks must not import Native Architecture types into parser/materializer. Existing compiler output/identities and public C1 dispatch remain unchanged. No writer/store/lease/publication/configuration/workflow or Scryer change belongs to this slice.

## C23-07 — Acceptance and linked-issue impact

These checks are planned, not executed product evidence:

| Criterion | Required outcome / verification | Trace |
| --- | --- | --- |
| C23-A01 | Temporary immutable Git fixtures establish exact current node/scope facts; revision/tree/path/digest/ID/kind/name/span mutation, moved source and dirty working trees cannot substitute facts. Both object formats and missing/corrupt objects retain their distinctions. | R03; bounded A04 |
| C23-A02 | Undeclared/missing responsibility or competitor explanations, conflicting submissions, discriminator mismatch and stale/fabricated citations report invalid evidence; missing Q1/unsupported assertions remain UNKNOWN; invalid and unknown evidence coexist. | R03; A04/APR-01 |
| C23-A03 | Independent canonical-byte/domain/hash oracles and per-field mutations cover every input, history/lineage and provenance field; only declared set reorder is equivalent. | R09; bounded A12 |
| C23-A04 | Attribution survives output intact without raw-payload/execution authentication; exact submitted facts and regenerated checks remain distinguishable after canonical serialization. No submitted verdict or confidence can qualify evidence. | R09/R11; bounded A12/A14 |
| C23-A05 | Exact/minus-one bounds, cancellation/deadline at capture/parser/normalization/materialization/hash/output, deep-source traversal/teardown and failure propagation produce no partial successful result or writes. | R09/R12; resource/failure contract |
| C23-A06 | Existing architecture model/compiler/identity/schema and affected parser/materializer behavior remain compatible; fmt/strict Clippy, independent executed verification, final adversarial review and publication checks bind the final snapshot. | R03/R09/R11/R12; PC-01–14 |

COD-24 consumes validated facts/explicit unknowns and retains automatic unique/mixed ownership selection; no dependency is removed. COD-25/F04 owns the qualified binding/export/path/build-context provider, independent inventories, actual Q1 pilot sufficiency and resolver-expansion decision gates. This staged interface breaks no gate by returning an invented qualification. COD-27 retains history/correction/transfer qualification. COD-32 constructs complete input/evaluation identity; COD-45 owns durable table regeneration/round-trip. COD-37/46/47 retain native trust/accepted-state/hosted-path gates. Maintainers retain feature COD-7/milestone aggregate closure.

The execution brief supplies exact file ownership, pre-code GO conditions, source baseline, methods and eventual executed evidence. Human approval of this supplement selects these concrete contracts only; it does not pass their tests, approve numerical deployment settings, accept a model or grant merge/release authority.
