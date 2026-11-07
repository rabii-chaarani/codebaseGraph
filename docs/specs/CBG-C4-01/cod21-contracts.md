# COD-21 compiler contract supplement — proposed revision 0.1

## Identity and decision boundary

Work: CBG-C4-01/F02/S02 / [COD-21](https://linear.app/codebasegraph/issue/COD-21), parent COD-6; 7 November 2026, Australia/Darwin. This is a **proposed clarification of serialized compiler inputs**, not an implementation, approval record or native model. Maintainers retain aggregate closure. The approved [specification 0.3](https://github.com/rabii-chaarani/codebaseGraph/blob/66dff2564e76ad06cdf172332e2da318d150fc73/docs/specs/CBG-C4-01/spec.md), its incorporated C16/C18/C1/Q1 contracts, [actual approval](approvals/cod19-approval.md) and [scoped Git-bootstrap activation](activation/cod19-activation.md) remain unchanged.

C16 specifies record fields and semantics but leaves several reference, parameter and transformation encodings unstated. This supplement proposes those encodings before dependent implementation. It also makes the source of a coherent file closure explicit. Approval of the original specification is reused for settled behavior; it is not presented as approval of these new choices. A later actual decision record must identify this path, its full published Git SHA, exact scope and human answer. This file never embeds its own containing SHA.

Two decisions require the maintainer:

- **C21-D01, recommended:** adopt the exact strict input shapes and scalar rules below. An alternative is to supply different shapes before compilation work; implementing arbitrary shapes now would create a public contract without a decision.
- **C21-D02, recommended:** COD-21 compiles immutable Git-backed closures and complete inline file maps. Live working-tree compilation requires a separately settled snapshot protocol; it is not silently approximated by sequential reads. If live-tree input is required in this slice, settle that protocol and update this supplement/brief before code.

No deployment resource values, trust pins, numerical architectural budgets, pilot scenarios, live GitHub settings, merge or release are selected here. No accepted/source state is written by compilation.

## C21-01 — Existing grammar and proposed scalar precision

C16-01/03 architecture/root/constraint fields remain exact; all required lists must be present even when empty. Unknown fields, duplicate decoded mapping keys, aliases/anchors, tags, merge keys, extra YAML documents, executable interpolation and remote loading reject with a file/span diagnostic. No input order gives override precedence. UTF-8 strings retain decoded text without Unicode normalization.

The proposed JSON-compatible scalar profile is explicit: quoted and block scalars are strings; plain `true`, `false` and `null` are their JSON values; plain decimal integers match `-?(0|[1-9][0-9]*)`; other ordinary plain text is a string. YAML-1.1-only boolean words such as `yes`, `no`, `on` and `off` remain strings. Reject plain floating-point/nonfinite, date/timestamp, alternate-base integer and leading-plus/leading-zero numeric spellings rather than coercing them. A quoted date or numeric string remains a string and must match its field's type. Mapping keys must be strings, with no merge-key exception. Empty YAML values and the YAML null shorthand `~` are rejected rather than supplied as implicit null. Empty documents reject. An explicit document-start marker is permitted for the one document.

Integer fields in this v1 grammar are exact unsigned 64-bit values unless explicitly stated otherwise; negative or unrepresentable values diagnose at the field, never coerce to floats. Resource values are additionally positive; architectural limits and scenario Integer values may be zero. This is the proposed serialized type range, not a deployment cap. Generic String scenario values may be empty; display names/responsibility statements retain C16's nonempty Text constraint.

Diagnostic positions use original UTF-8 file bytes with a nonempty half-open byte span, plus one-based line/column for presentation. EOF errors may use an empty span at EOF. Semantic errors name the authored record/field and retain its file/span. Codes reuse C16's stable error families; more specific reason fields may explain unsupported syntax without changing legacy errors. Return deterministic ordering by closure path, byte position, code and subject.

## C21-02 — Reference encodings

All objects below are strict and require all displayed fields. Existing Id, Digest, ArtifactRef and full Git object validation remain C16-01/COD-20 types.

| Type | Exact proposed shape |
| --- | --- |
| ProjectionRef | `{ version: "rust-q1/0.1", domain_ref: Digest }` |
| BaselineRef | `{ kind: "evaluation", id: Digest }` or `{ kind: "input", id: Digest }` |
| AuthorizationContextRef | `{ reference: ArtifactRef, context_id: Id }` |
| ElementRef | `{ level: "component"\|"container", id: Id }` |

Projection `domain_ref` identifies the complete separately supplied immutable DomainRef from C16-05. It is not a candidate-selected exclusion policy. The compiler validates reference shape and supported version; COD-22/25 and downstream evaluation owners resolve content and qualify identities/coverage. A matching digest is no proof that content is available or trusted. Baseline kind is significant and cannot be erased when comparing references.

Waiver `approval_ref` uses AuthorizationContextRef. Its ArtifactRef locates a pinned context record, while `context_id` names the record within it. This is a locator for COD-18's tagged review/manual-action protocol, never a bearer approval, verified action or choice of trust mode. The compiler checks shape only; COD-36/37/46/47 independently verify current authority, subject, expiry and expected state. No candidate assertion becomes VerifiedMaintainerAction or VerifiedReview.

Constraint fields retain C16-03: ownership rules have Id/component/responsibility_ids/all_of/none_of; allow/require use Id/level/nonempty from and to Id arrays/ProjectionRef; budgets use Id/level/nonempty origins/ProjectionRef/metric/nonnegative absolute_max and max_increase/BaselineRef; waivers use Id/budget_id/BaselineRef/nonnegative original_limit and ceiling/quoted UTC RFC3339 expires_at/AuthorizationContextRef. Expiry permits `Z` or `+00:00` UTC offsets and valid optional fractional seconds. Other offsets, invalid calendar dates and non-string expiry reject. Compiler validation does not decide whether a waiver is currently effective.

Ownership clauses are strict `{ kind: exported_item, crate: Text, namespace: Text, item_path: Text }` or `{ kind: reaches_exported_item, crate: Text, namespace: Text, item_path: Text, max_hops?: positive u64 }`, per C16-04. The three Text fields are nonempty UTF-8; the compiler preserves exact case/text and does not invent a Rust binding or treat textual syntax as qualified evidence. Namespace/item/export/path qualification remains downstream. Metric values are exactly `cyclic_nodes`, `max_direct_fanout`, `max_transitive_reach`. Rule endpoint arrays and budget origins are sets: reject duplicates and normalize by Id for deterministic typed output. Relationship contract refs are canonical sets: reject identical duplicate ArtifactRefs and normalize by repository/commit/path/content_digest; changed fields identify a different ref.

## C21-03 — Scenario parameters and substitution

`parameters` is a mapping from valid Id keys to one tagged value:

```text
{ kind: "element_id", level: "component"|"container", id: Id }
{ kind: "contract_ref", reference: ArtifactRef }
{ kind: "integer", value: u64 }
{ kind: "string", value: String }
```

Parameter names are local to a scenario and unique within that map; they are bindings, not new architectural element/rule identities. All other authored declaration IDs, including responsibilities, rules, scenarios and predicate records, share the model-wide uniqueness namespace. Operation targets/replacements and reference uses are not extra declarations of their existing IDs. Added records are declarations in the respective transformed snapshot and undergo its whole-model uniqueness checks.

The only substitution syntax is the whole object `{ parameter_ref: Id }`. Resolve it before validating the destination field. ElementId substitutes an ElementRef; ContractRef substitutes an ArtifactRef; Integer substitutes an integer; String substitutes a string, then the destination's Id/Text/version/digest validation still applies where relevant. Never interpolate a substring, convert an element to a bare Id, coerce a string to an integer, or allow replacement of arbitrary objects. Array elements may be substituted at their expected type. A missing binding, recursive binding or wrong destination type is a compilation diagnostic. Parameter values themselves are literal tagged values; parameter-to-parameter references are unsupported.

Scenario `preconditions` and `obligations` are required arrays of strict C16-06 tagged records, using `kind` and `id`. The exact remaining fields are:

| Kind | Fields |
| --- | --- |
| `element_exists` | `element: ElementRef` |
| `relationship_exists` | `from: ElementRef, to: ElementRef, level, projection: ProjectionRef` |
| `contract_ref_equals` | `relationship_id: Id, expected: ArtifactRef[]` |
| `reachable` | `from: ElementRef, to: ElementRef, level, projection: ProjectionRef` |
| `no_bypass` | `from: ElementRef, to: ElementRef, via: ElementRef, level, projection: ProjectionRef` |

Each from/to/via ElementRef must have the same level as its predicate's explicit sibling `level`. ElementId parameters may substitute these whole endpoint objects; a String cannot substitute an ElementRef. This catches wrong-level parameter substitutions without erasing the supplied level. Literal bare IDs are not an alternate endpoint encoding. String parameters can still fill actual string fields subject to those fields' Id/Text validation.

Within each of preconditions and obligations, every no_bypass must have an explicit reachable record with the same resolved from/to/level/projection. Endpoints/via are distinct at the selected level. Do not inject a missing predicate or treat disconnected/inapplicable inputs as PASS. Scenario compilation validates declared syntax, binding existence/type, endpoint level consistency and shared predicate pairing. Scenario element/relationship/operation targets may be introduced or changed by a transform: do not require their existence in the initial authored model. The later runner resolves applicability and checks actual existence, target types/digests and all final references against each correct transformed snapshot; unknown applicability is not compilation success evidence or scenario PASS.

## C21-04 — Typed transformation encoding

`baseline_transform` and `candidate_transform` are ordered arrays. Each operation has `kind` plus exactly these fields:

| Kind | Fields |
| --- | --- |
| `add_element` | `element: ElementRecord` |
| `replace_element` | `element: ElementRecord, expected_record_digest: Digest` |
| `remove_element` | `element_kind: system\|container\|component, id: Id, expected_record_digest: Digest` |
| `add_relationship` | `relationship: Relationship` |
| `replace_relationship` | `relationship: Relationship, expected_record_digest: Digest` |
| `remove_relationship` | `id: Id, expected_record_digest: Digest` |
| `add_constraint` | `constraint: ConstraintRecord` |
| `replace_constraint` | `constraint: ConstraintRecord, expected_record_digest: Digest` |
| `remove_constraint` | `constraint_kind: ownership_rule\|allow\|require\|budget\|waiver\|scenario, id: Id, expected_record_digest: Digest` |

ElementRecord is `{ kind: system|container|component, record: <complete corresponding C16-03 record> }`. ConstraintRecord is `{ kind: ownership_rule|allow|require|budget|waiver|scenario, record: <complete corresponding strict record> }`. Relationship is COD-20's complete relationship record. `element_kind` distinguishes removal of a system without extending C16's component/container Level enum. Complete embedded scenario records obey the same parse-depth and entity/operation accounting; nesting creates no unbounded exception.

Replace retains the embedded record's ID as its target; renaming is explicit remove/add. Replace/remove require the exact expected-record digest; no default digest, arbitrary field patch or cascading delete. Compiler output retains order. At scenario execution, each batch applies transactionally to its own pinned snapshot, checks expected digests and validates the final whole model including dangling refs. A syntactically valid transform is not evidence that its runtime target/digest exists or that either scenario passes. COD-29/39 retain execution ownership and must use this same grammar, not a second evaluator or schema.

## C21-05 — Immutable closure input and coherence

The proposed compile source alternatives are:

```text
PinnedRoot { repository: GitRevision, root_path: RelativePath }
InlineFiles { root_path: RelativePath, files: ordered-independent map<RelativePath, UTF8 bytes> }
```

These are compiler service types, not additions to the old OperationRequest or public transport configuration. The later C1 operation family wraps the same alternatives. Repository identity is a configured local repository binding resolved independently of candidate strings; a full revision must resolve to the exact commit object in that repository. No branch/tag abbreviation or ambient latest state. All paths name files relative to the repository/inline-map root; referenced document paths are resolved relative to the **root manifest's containing directory**, per C16-03, not each referring file.

PinnedRoot reads only immutable commit/tree/blob objects, never working-tree file bytes. Verify the commit/tree/blob relationship, inspect each tree component, reject symlink entries, submodule entries, nonregular leaf modes and traversal outside the selected model directory, and retain the object identities with captured bytes. Missing objects return an explicit unavailable-input error; do not fetch, run filters or read network URLs. A moving branch, modified worktree or changed filesystem directory entry cannot mix the selected revision's YAML files. Repository/object corruption or failed identity checks reject the closure.

The root lists the complete architecture and constraint closure. No recursion, directories, globs or implicit discovery; no closure document may list further files through unknown fields. Reject absolute/empty/dot/parent/backslash/remote paths and duplicate resolved paths across both lists, including a repeated root. Root-relative IDs are byte-exact and case-sensitive, following pinned Git paths; no filesystem case-fold/Unicode alias resolution. Inline file keys additionally obey the same rules. Require exactly the root and its listed files in an inline map: missing and unreferenced extra entries diagnose. Input map order grants no precedence.

Copy/freeze inline bytes into the owned compile input before parsing; callers cannot mutate borrowed buffers during compilation. Return an owned immutable raw-byte closure sorted by normalized path along with the complete typed model. This supplies COD-22's closure hashing inputs without fabricating InputIdentity, EvaluationId or AcceptedStateId. Actual versioned hashing and linked-artifact qualification remain COD-22. Source fetching is a narrow internal boundary beneath the transport-neutral compiler; adapters do not implement closure or validation semantics.

Live working-tree edits can be compiled by a caller first supplying a complete inline byte snapshot. That mode describes the supplied bytes; it does not attest that sequential caller reads represented one atomic filesystem instant. No live-tree coherence claim, accepted identity or trust is inferred from it. Adding direct mutable-filesystem input later requires an explicit protocol/contract rather than check-then-open or metadata-only race claims.

## C21-06 — Resources, cancellation and failure

Use the complete explicit ArchitectureExecutionPolicy from C16-09: all seventeen required positive fields, no defaults, tighten-only overrides. Resolve effective duration at invocation start, with the earlier caller/configured deadline. This slice supplies validated service policy/context types; repository configuration and CLI/MCP/catalog wiring remain COD-43/44. Missing/invalid policy blocks compilation, even if an unrelated required setting would not be consumed by this particular operation.

Before allocation/read, check file count and known object/file length against max_closure_files/max_file_bytes/max_yaml_bytes; include the root. Bound dynamic reads and verify actual length/content. Enforce max_parse_depth during event parsing, max_authored_entities across declarations including embedded records, max_expanded_rule_pairs before checked Cartesian expansion, max_scenarios across scenario declarations, and max_proposal_operations across both transformation arrays including nested scenarios. Enforce max_result_bytes for the complete logical compile output, including retained raw closure, before returning success; no truncated successful result. Unused policy fields remain recorded by the context, not replaced by defaults. Check cancellation/deadline between source and parse events and within semantic/expansion loops. Use checked arithmetic and fallible reservations for owned structures.

Recommended parser arrangement is a direct dependency on the already locked libyaml-rs event parser, without a graph-to-k-wiki dependency. Anchor/tag/style/mark events permit rejecting forbidden YAML before alias expansion or typed deserialization loses syntax. Its parser may buffer one scalar within the configured file bound before the next cooperative cancellation point; this is not a hard preemptive interrupt. Generic process OOM in dependency/system allocators remains a platform limit, not a new promised recoverable error. Validate the actual dependency/API behavior during implementation before relying on these safeguards; do not weaken explicit bounds to accommodate a parser.

Compilation returns either a complete typed result/raw closure or diagnostics. No partial model is presented as successful. It opens no graph writer, changes no accepted snapshot, installs no schema and publishes no file. Resource/cancel/deadline/source/parse/reference errors preserve current accepted and observed state by construction. Errors do not trigger automatic retry; input_changed requires explicit recapture/re-evaluation. Later evaluation UNKNOWN/permit/transition semantics stay with their existing owners.

## C21-07 — Validation and planned acceptance

Global authored-model validation covers required versions/fields, duplicate keys/IDs/paths/ArtifactRefs, nonempty responsibilities, architecture parents/endpoints/types, non-scenario constraint targets/responsibility ownership, and cross-document contradictions. Require pairs must be covered by the union of explicit allow pairs at exactly the same level and ProjectionRef; no ancestor permission or default edge. Budget/waiver links must exist and agree in referenced baseline/metric scope; current waiver authority/effectiveness is downstream. Scenario declarations and embedded complete record shapes, parameter binding types/levels and required paired predicates compile before execution. Scenario runtime target existence/expected-digest checks and complete final transformed-snapshot reference validation belong to COD-29/39, including IDs introduced by operations; they are not base-model existence checks. A hierarchy cycle or cross-level parent in an authored model cannot be made legal through input ordering.

| Criterion | Requirements / contracts | Planned observable evidence |
| --- | --- | --- |
| C21-A01 | R01/A01; C16-01/03; C21-01/02 | Valid multi-file/inline/pinned-root fixtures compile to the same deterministic typed intent; preserve raw closure bytes/order for downstream hashing |
| C21-A02 | R01/A01; C16-03 | Malformed/duplicate/unknown/wrong-scalar/tag/anchor/alias/merge/trailing-document fixtures produce exact file/span/reason; no partial success |
| C21-A03 | R01/A01; C16-02/03/06 | Cross-file IDs/parents/endpoints, non-scenario constraint references, require-without-allow, duplicate refs, malformed typed parameters/operations and missing paired reach/no-bypass reject; a well-typed scenario target introduced by a transform compiles without a fabricated runtime pass |
| C21-A04 | R01/R02, A01/A02 | Escaping/duplicate/symlink/submodule/missing/corrupt input and changed working-tree controls cannot escape or mix an immutable pinned closure; exact inline closure required |
| C21-A05 | R12/A15/A16; C16-09 | Explicit policy invalid/missing/tighten-only, boundary+one file/bytes/depth/entity/pairs/operations/scenarios, cancellation/deadline and checked arithmetic controls; accepted/observed fixtures unchanged |
| C21-A06 | R01/R12; PC-03/04/08 | COD-20 type/schema tests, existing relevant source-schema/API compatibility checks, formatting and strict Clippy; no runtime schema/adapter/store/config change or reverse wiki dependency |

These are **planned** product checks. Documentation review/static example validation is not executed compiler evidence. The [execution brief](execution/COD-21.md) names the exact ownership/start gate. Independent Terra/medium verification and fresh Sol/xhigh adversarial review of the final implementation remain mandatory before the authorized PR handoff.

## Linked-issue impact and remaining gates

COD-22 consumes the raw closure/typed refs and owns hashes/artifact qualification. COD-23/25 retain ownership/domain evidence, including externally resolved DomainRef availability. COD-29/30/36/39 must reuse the agreed operation/rule/waiver/scenario grammar; COD-37/46/47 retain tagged authority/trust/hosted verification; COD-43/44 retain shared request/context and transport/config integration. No dependency edge, closure owner or acceptance obligation is removed. COD-40 still owns concrete pilot scenarios; this document specifies syntax only. Existing spec0.3 and all approved SHA links remain immutable.

The only current writable deliverable is this proposal and its brief/evidence. Dependent product implementation awaits the actual human decision for C21-D01/D02 at a published full SHA, then the coordinator's start signal. COD-21 stays open. No next issue, native runtime activation, aggregate closure, merge or release follows from publication or review.
