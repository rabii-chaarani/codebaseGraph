# COD-22 identity and linked-artifact supplement — proposed revision 0.1

## Identity and decision boundary

Work: `CBG-C4-01/F02/S03` / [COD-22](https://linear.app/codebasegraph/issue/COD-22), parent COD-6, M1/P1; 7 November 2026, Australia/Darwin. Status: **proposed public contracts, awaiting exact-subject human approval**. No product implementation or native acceptance is claimed. Maintainers retain aggregate closure.

Reuse [specification 0.3](https://github.com/rabii-chaarani/codebaseGraph/blob/66dff2564e76ad06cdf172332e2da318d150fc73/docs/specs/CBG-C4-01/spec.md), its incorporated C16/C18/C1/Q1 contracts, [actual Scope A/B approval](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/approvals/cod19-approval.md), verified scoped Git/bootstrap activation at that same revision, and the [approved C21 supplement](https://github.com/rabii-chaarani/codebaseGraph/blob/9b4384378c03e05a458f8f94291c8bba4d82c7c3/docs/specs/CBG-C4-01/cod21-contracts.md). Those approval subjects remain unchanged. COD-21's [reviewed implementation](https://github.com/rabii-chaarani/codebaseGraph/tree/e9bcaf0cb21e4865ffe54387833954940b7c7d58) supplies immutable compiled models.

C16-01 specifies canonical hashing primitives but does not name the compile-level hash domains, exact payloads, document-reorganization equivalence or linked-content supply protocol. The following choices settle those observable contracts before coding. This file never embeds its own containing commit SHA; a later actual approval record must identify its published full SHA, exact scope and human answer.

- **C22-D01 — recommended:** adopt the separate intent, raw closure, effective policy and compilation-input identities below, including the explicit set/order rules. Moving declarations between files preserves typed intent; changing a governing path or byte invalidates the raw closure and compilation-input identity.
- **C22-D02 — recommended:** inventory all typed linked ArtifactRefs without resolving them during identification; provide a separate explicit, all-or-nothing local immutable-Git qualification service. Unavailable linked content blocks qualification. Inline model compilation remains supported; supplying loose inline bytes as proof of an ArtifactRef's commit membership is outside this protocol.

An alternative to D01 is a different explicitly specified canonical payload/equivalence rule before implementation. An alternative to D02 is a separately specified caller-supplied-content protocol with explicit provenance limits. Neither alternative may be silently selected by an adapter. Numerical deployment settings, runtime trust, accepted-state transitions, new YAML fields and public operation adapters are outside this supplement.

## C22-01 — Canonical byte encoding and domains

For every structured hash, compute SHA-256 over ASCII `<type>/0.1`, one LF byte (`0a`), then the compact canonical JSON payload. There is no trailing newline. Render the result as C16 `Digest`: `sha256:` plus 64 lowercase hexadecimal digits. These exact type prefixes are:

| Value | Type prefix |
| --- | --- |
| Typed intent | `architecture-intent` |
| Raw file closure | `architecture-closure` |
| Effective execution policy | `architecture-policy` |
| Compilation inputs | `architecture-compile-input` |
| Linked artifact locator | `architecture-artifact-reference` |

Canonical JSON uses UTF-8, object keys sorted by Unicode scalar order, exact decoded strings without Unicode normalization, decimal integers and no floats. No insignificant whitespace is emitted. Escape quotes and backslashes; use `\b`, `\t`, `\n`, `\f`, `\r` for those controls and lowercase `\u00xx` for other U+0000–U+001F values. Other scalars, including non-ASCII text, are emitted as UTF-8; `/` is unescaped. Reject duplicate keys rather than selecting a winner. Ordered arrays retain order; only the explicitly enumerated sets below are sorted. Do not hash Rust Debug output or object allocation/order details.

File and ArtifactRef `content_digest` values remain SHA-256 of exact raw file bytes, without a structured-value prefix. Git object IDs retain Git's separate SHA-1/SHA-256 object framing; neither is a substitute for a raw-byte content digest.

## C22-02 — Authored identity and typed intent

`model_id` and every authored record/responsibility ID remain C16's case-sensitive `Id`, separate from source node IDs and file positions. Renaming an authored ID is remove/add; hashes cannot establish lineage. Identification consumes only an opaque successful `CompiledModel`, never unchecked/deserialized success records.

The intent payload is exactly:

```text
{ schema_version: "0.1", model_id: Id,
  systems: System[], containers: Container[], components: Component[],
  relationships: Relationship[], ownership_rules: OwnershipRule[],
  allow: EdgeRule[], require: EdgeRule[], budgets: Budget[],
  waivers: Waiver[], scenarios: Scenario[] }
```

Gather each top-level list from all corresponding compiled documents. The root's filename, document grouping, document-reference ordering, raw YAML syntax, source spans, Git provenance, execution policy and derived caches are excluded. Every typed authored field is retained, including scenario parameter bindings, resolved typed transformations, expected-record digests, waiver references and expiry text. This identifies typed intent; it does not prove conformance, trusted evidence or approval.

Canonical sets are exactly:

| Field | Sort key |
| --- | --- |
| The ten top-level record lists above | Record `id` |
| System/container/component `responsibilities` | Responsibility `id` |
| Ownership `responsibility_ids`, edge-rule `from`/`to`, budget `origins` | Exact Id text |
| Relationship `contract_refs`, contract-equality predicate `expected` | Tuple `(repository, commit, path, content_digest)` |
| Each scenario's `preconditions` and `obligations` | Predicate `id` within that list |

These rules apply equally to typed records embedded in operations, including nested scenarios. Existing compile-time uniqueness/set checks remain; normalization never silently discards a malformed declaration. Distinct declarations do not become duplicates merely because their other fields are equal. Parameter mappings follow sorted object-key encoding. `baseline_transform` and `candidate_transform` are ordered; `all_of` and `none_of` clause arrays, and every array not listed as a set, retain authored order. The hasher must not sort arbitrary arrays or drop unused parameter bindings.

Comments, quoting changes, mapping-key order, declared-set order and file regrouping leave the intent digest equal when all typed values remain equal. Different exact text, IDs, fields, ordered operations or non-set array order changes it. An expiry spelled with `Z` versus `+00:00` is different typed text even when it denotes the same instant; no undocumented temporal or Unicode normalization occurs.

## C22-03 — Closure, policy and compilation-input identities

The closure payload is a JSON array of two-item arrays `[[relative_path, raw_byte_digest], ...]`, including the root and every explicitly referenced architecture/constraint file, sorted by exact repository/inline-map-relative path. Paths and raw bytes come from COD-21's captured immutable closure. No unreferenced files, symlink resolution, discovery, candidate exclusions, Git blob ID, timestamp or filesystem location enter this payload. Changing a governing path, comment, encoding byte or referenced content changes the closure digest. Identical inline and Git-backed path/byte closures have equal closure digests.

The policy payload is the complete effective C21 `ArchitectureExecutionPolicy` object with all 17 positive integer fields, after validated tighten-only overrides. It excludes wall-clock start/deadline, cancellation state and local repository directory. Any relevant policy-value change, including an increase, changes this digest. Resource values are supplied operational configuration; no numerical defaults are introduced.

The compilation-input payload is exactly:

```text
{ identity_version: "architecture-identity/0.1", schema_version: "0.1",
  model_id: Id, intent_digest: Digest, closure_digest: Digest,
  policy_digest: Digest, root_path: RelativePath,
  provenance: { kind: "inline" }
           | { kind: "git", repository: Text, commit: GitObjectId,
               tree: GitObjectId } }
```

Git provenance comes from the verified pinned compile capture. Inline inputs explicitly state their lack of Git provenance. Equal bytes in a different repository/commit/tree, or switching between inline and Git inputs, preserve equal intent/closure where applicable but change this compilation-input digest. Local clone directory movement does not change any identity. The root path is explicit even if two compilations happen to contain the same path set.

`CompiledIdentity` logically contains `{ identity_version, model_id, intent_digest, closure_digest, policy_digest, compile_input_digest, linked_artifacts }`, with `identity_version` as above. Each `linked_artifacts` entry is `{ reference: ArtifactRef, reference_digest: Digest }`; its digest uses the artifact-reference domain over the exact four-field ArtifactRef. Inventory entries are a canonical set sorted by the ArtifactRef tuple. Repeated use of the same locator across records yields one inventory entry; all original record uses remain in the intent payload. No success constructor/deserializer can bypass validation.

This compilation-input digest is **not** C16's complete `InputIdentity`, `SourceContentKey`, `EvaluationId` or `AcceptedStateId`. It lacks source/build/domain/ownership/metric/evaluator/linked-execution inputs that their downstream owners must establish. It cannot authenticate persisted results, prove linked-content availability, or authorize evaluation reuse/acceptance. COD-32 still constructs complete evaluation identity; this service exposes a correctly named constituent rather than fabricating absent inputs. COD-21's complete-output canonical byte cache retains its admission/serialization role and is not hashed wholesale as intent.

## C22-04 — Linked-reference inventory and immutable qualification

Inventory every typed ArtifactRef in compiled relationships, waiver authorization-context references, scenario ContractRef parameters, contract-equality predicates and all nested transformation records containing those types. Traverse the typed vocabulary, including nested scenarios; do not infer references from arbitrary String parameters or from objects that merely resemble an ArtifactRef. Digest-only projection/domain/baseline references remain their existing typed locators for downstream resolution, not invented file references.

Existing requirements/contracts/test-definition/execution files may also be supplied explicitly to the qualification service as ArtifactRefs by their owning workflow. Return their exact repository, full commit, path and content digest without parsing, copying, renaming or superseding the documents or their internal IDs. This adds no requirement/test-reference field to the YAML grammar. File qualification does not assert that an internal requirement ID exists or that a test execution passed; those validations retain their downstream owners.

Qualification accepts an explicit collection of ArtifactRefs, independently configured local `RepositoryBinding`s, full shared execution policy, optional tighten-only overrides and cooperative execution context. Candidate strings cannot select a local directory or remote fetch. Match repository Text identity exactly; reject ambiguous duplicate repository bindings. Never infer identity from the current checkout, remote aliases or a missing binding.

For every distinct reference, resolve its full commit in its matching binding; verify the commit/tree/path/blob chain and object bytes using COD-21's existing confined Git capture machinery. Reject symlink components/leaves, submodule traversal, nonregular leaves and invalid paths. Read only immutable object contents: no working-tree fallback, checkout, filters, replacement objects, lazy fetch, network access or execution of linked material. Recompute SHA-256 of raw bytes and require exact equality with `content_digest`.

Return an opaque complete `QualifiedArtifacts` collection. Its exact logical output is `{ artifacts: [{ reference: ArtifactRef, tree: GitObjectId, object_id: GitObjectId, bytes: [u8, ...] }, ...], effective_policy: ArchitectureExecutionPolicy }`, with entries sorted by ArtifactRef tuple. Each entry retains the original reference, verified tree/blob object IDs and captured raw bytes. Its meaning is only "these bytes belong at this pinned repository/commit/path and match this digest." It is not a trust receipt, verified human action, approved constraint, passing test or complete evaluation input. A reference's digest alone never proves its content is available, authentic or current for acceptance.

Repeated identical references are deduplicated across uses. Different repository/commit/path/digest fields retain distinct identities. Conflicting digests for the same repository/commit/path reject as `invalid_reference`; no last-write-wins or silent substitution. Missing bindings, objects or files return `architecture_input_unavailable`; malformed refs, invalid object/path integrity or digest mismatches return the existing appropriate `invalid_reference`/`path_not_confined` diagnostic. Do not return partial qualified success. No linked qualification is implicit in successful identification; callers must qualify every required linked input before using it as complete evaluation evidence. Later evaluation maps unavailable required inputs to UNKNOWN, never an empty successful set.

## C22-05 — Resources, failures and service boundary

Keep identification and linked qualification transport-neutral under the existing Native Architecture compiler owner. Provide separate callable services with validated private-field success types; settle concrete Rust signatures in the versioned issue brief before code. Preserve the existing compile signature/output bytes, primitive public literal compatibility and C1 dispatch contracts. No storage writer, graph schema installation, adapter defaults, runtime trust path or new dependency is introduced.

Identification reuses the compiled model's effective policy and takes the existing cooperative context. Qualification resolves its supplied policy/overrides once. Use the earlier caller deadline/configured duration for each invocation; future operation composition must retain an overall caller deadline. Check during traversal, hashing, sorting boundaries, byte capture and serialization. Traverse nested scenarios explicitly without recursive stack exhaustion. Preserve owned child cleanup through actual completion.

Admission counts exact canonical output bytes against `max_result_bytes`; identity hash payloads are streamed, with fallible scratch allocation and policy-bounded work. Qualification bounds each file by `max_file_bytes` and counts distinct canonical reference descriptors plus all captured bytes against `max_evidence_bytes`, checking before allocation/read where possible. Descriptors count once per distinct reference; shared bytes for different refs still count for each returned entry. Do not reinterpret the YAML-specific closure-file count as a new unrelated artifact limit or invent numerical caps. Complete output includes captured bytes as JSON byte arrays, plus tree/blob/reference fields; its exact serialized length is also bounded by `max_result_bytes`.

Configuration, resource, cancellation and deadline failures retain existing C16/C21 codes and deterministic diagnostics; unavailable pre-identity input is `architecture_input_unavailable`. There is no evaluable PASS or partial success on failure, no automatic retry and no accepted/source mutation. Explicit retry starts with independently supplied current inputs/context. Identity outputs contain no timestamps or themselves as inputs.

## C22-06 — Acceptance and verification

The following are **planned checks**, not executed product evidence. They refine COD-22's bounded contributions to R01/R09/R10/R11 and A01/A12–A14; full evaluation, trust, durable-table and platform acceptance remain separate.

| ID | Given / when / then | Requirement and method |
| --- | --- | --- |
| C22-A01 | Equal compiled typed values with reordered keys/sets, relocated declarations or equivalent scalar quoting produce equal intent digests; changed typed text/ID or ordered operations change them. | R01/R09; independently computed canonical-byte/digest fixtures, Unicode/control strings, nested scenarios, order controls. |
| C22-A02 | Changing any root/imported governing byte or path changes closure/input digests; format-only changes can preserve typed intent. Inline/Git equal closures agree; provenance/policy/root selection changes are distinguished as specified. | R09/R10; root/import/rename/comment/provenance/policy fixtures and independent SHA-256 oracles. |
| C22-A03 | Original authored IDs and exact ArtifactRef fields survive source movement; all typed nested uses appear in the deduplicated inventory and arbitrary strings do not. | R01/R11; inventory/rename/reference-tamper and traversal fixtures. |
| C22-A04 | Available pinned artifact bytes qualify only through the correct repository/commit/path/digest; missing content, wrong binding/digest, conflicting refs, symlinks/submodules and corrupt objects cannot yield qualified success. | R09/R10/R11; temporary local Git repositories, tampering/missing-input/failure and working-tree-independence controls; no claim of trusted execution. |
| C22-A05 | Cancellation/deadline, allocation/resource rejection and boundary-sized outputs leave no partial success or mutation; deep nested records remain stack-safe. | R09/R12; exact/minus-one size, evidence-byte, cancellation/deadline/depth and unchanged-state checks. |
| C22-A06 | Existing compile/model/schema/public compatibility checks pass on the integrated snapshot; independent verification and fresh required adversarial review clear confirmed defects. | R01/R12 and PC-01–14; executed Rust 1.98.1 checks, final-snapshot review/publication readback. |

Evidence records actual revision, toolchain/environment, command/method, result and limitations outside the implementation PR. Preserve immutable approval subjects and predecessor evidence; no passing claim follows from fixture definitions, attached tests or configured CI. No user approval, qualification, release or aggregate closure is manufactured by this proposal.
