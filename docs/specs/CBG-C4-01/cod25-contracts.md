# COD-25 source-evidence supplement — proposed revision 0.1

## Identity, authority and actual decisions

Work: **CBG-C4-01/F04/S01 / COD-25**, parent COD-8, M1/P2, R04/R12 and A06. Date: 12 November 2026, Australia/Darwin. Status: **concrete contract proposal awaiting exact-full-SHA human approval**. The maintainer's request to implement the plan authorizes preparing/reviewing/publishing this supplement and the [execution brief](execution/COD-25.md). It retains a separate approval gate before dependent product code; neither that request, a commit, a review nor issue status supplies the future approval.

Governing inputs are [specification 0.3](https://github.com/rabii-chaarani/codebaseGraph/blob/66dff2564e76ad06cdf172332e2da318d150fc73/docs/specs/CBG-C4-01/spec.md), [C16-01–09](https://github.com/rabii-chaarani/codebaseGraph/blob/0c49902ba7e12e10a8a93ba8540c2be0527d7d47/docs/specs/CBG-C4-01/cod16-contracts.md), the [actual C1/Q1 selection](https://github.com/rabii-chaarani/codebaseGraph/blob/f67e1d95862484dee34c8e818c98b72959366c5b/docs/specs/CBG-C4-01/cod17-decisions.md), [actual Scope A/B approval](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/approvals/cod19-approval.md) and [verified scoped activation](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/activation/cod19-activation.md). Approved Git contracts/versioned briefs govern this activated bootstrap scope. Unrelated Scryer authority/state remains intact; no Scryer claim or native acceptance is fabricated.

Prerequisite product source is **`7c81720f3cb32c392e34833033b0ccec6766e2ca`**, tree `5e71d82b0c0fcf55d7447876d19e6acbceba791c`, [draft PR192](https://github.com/rabii-chaarani/codebaseGraph/pull/192). It includes the separate unmerged COD-20/21/22 prerequisites. Its inherited delivery manifests/version 4.2.0 are distinct from original pilot **`90e2ab6fae2767bdb158e19f0072d43568e80dbc`**, version 4.2.1. [Approved C23 supplement](https://github.com/rabii-chaarani/codebaseGraph/blob/e766d14ee80908b21edc76315dbbb28c98a55f69/docs/specs/CBG-C4-01/cod23-contracts.md), [actual decision](https://github.com/rabii-chaarani/codebaseGraph/blob/41e6eeb956a6da68c6334b41592b4119c526ad8c/docs/specs/CBG-C4-01/approvals/cod23-approval.md) and [final brief](https://github.com/rabii-chaarani/codebaseGraph/blob/c36569ed6445e6eede2d698af802903a3a726bca/docs/specs/CBG-C4-01/execution/COD-23.md) remain unchanged.

The [actual sequencing decision](https://github.com/rabii-chaarani/codebaseGraph/blob/fcc293b4f8f09854e1cae905ebb3016d68c5d27c/docs/specs/CBG-C4-01/decisions/cod24-cod25-order.md) establishes COD-23 → COD-25 → COD-24 without reducing acceptance. This supplement records the following actual choices in the current coordinator chat, not unanswered defaults:

| Decision reference | Actual answer / resulting direction |
| --- | --- |
| `call_cbd50171cf3f48a58617c45fa845c910` item 0 | Actual build-context capture, rather than static-manifest assumptions. |
| Same call, item 1 | macOS arm64 first, Rust 1.98.1, production library/CLI and Wiki graph-context caller contexts; other platforms remain unqualified. |
| `call_165af782cc0c47f3a8e58664a3e729af` item 0 | Visibility-aware module exports, including restricted audiences; private items do not become exports. |
| Subsequent synchronous planning reply, key `context_preparation` | Explicit preparation; qualification performs no build. |
| Same reply, key `workspace_bindings` | Captured same-repository workspace crates are eligible; uncaptured/third-party definitions remain UNKNOWN. |
| Same reply, key `neutral_boundary_scope` | Keep approved v1 wire shapes; isolate Rust interpretation behind the neutral evidence boundary. |
| User's instruction to continue planning, followed by the exact plan implementation request | Preserve a reusable architecture core; prepare the concrete subject before seeking exact-SHA approval. |

The synchronous response is identified by stable question keys and quoted selections because the tool result did not expose a call identifier. This is not an invented locator. The actual product approval record, when obtained, must name the full published SHA and exact paths; no file embeds its own containing commit SHA.

## C25-01 — Responsibilities and compatibility

The new private `source_evidence` module belongs to Language Syntax Graph. It owns language-neutral source anchors, inventories, fact queries/results and a sealed internal provider interface. Its Rust implementation owns module/item identity, scopes, import chains, visibility, direct bindings and source-level reach evidence. Only that actual implementation can produce its successful in-memory provider results. There is one built-in Rust-Q1 provider; no public registration, dynamic plugin loader, second language implementation, compiler semantic-analysis integration or generic foreign verdict import is introduced.

Runtime execution owns operation-private snapshot/target/capture workspaces, Cargo/rustc/helper execution, containment, process ownership and cleanup. Language code consumes verified source bytes and captured context through neutral types/control callbacks. Neither the neutral layer, Rust provider, parser/materializer nor runtime capture module imports `crate::architecture`, mapping/rationale types, architecture model IDs or CLI/MCP request types.

The Native Architecture bridge reuses its compiler's verified Git reader/canonical writer/policy helpers, translates legacy predicates into provider queries and binds returned anchors to exact existing SourceRefs, model/rule inputs and mapping-facts identity. It does not implement a second resolver. Existing primitives stay at their current public paths; do not move them into the lower layer or duplicate their validation. Lower anchors deliberately describe captured source rows rather than claiming Git membership. The bridge independently establishes that membership.

Current C16 YAML and C23 submission/result shapes and canonical digests are unchanged. Parsing and canonical hashing of `crate`/`namespace`/`item_path` remain with their current owners; their Rust target semantics live in the bridge/provider. Future languages may add separately approved, versioned predicate adapters while reusing generic facts and architecture algorithms. This does not claim the v1 authored language is already a complete multi-language DSL.

Preserve old heuristic extraction rows, IDs, relation allowlists, public operation variants, request literals, executors and CLI/MCP defaults. New qualified facts are separate immutable evidence, not replacement graph rows. COD-23's existing `validate_evidence` and `source_extractor_identity` behavior remains staged; the new result does not rewrite its ownership-UNKNOWN records. There is no graph writer, schema installation, persistent evidence service, accepted-state transition or new public CLI/MCP operation in this slice.

## C25-02 — Public library interface and input types

Add these functions at the named facades; all referenced existing primitive/compiler/evidence types retain their current meanings:

```rust
// architecture::evidence::rust
pub fn prepare_context(
    revision: &GitRevision,
    selection: &RustBuildSelection,
    repositories: &[RepositoryBinding],
    tools: &RustToolchainBinding,
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<PreparedSourceEvidence, EvidenceError>;

// architecture::evidence
pub fn qualify_source_evidence(
    model: &CompiledModel,
    evidence: &EvidenceValidation,
    prepared: &PreparedSourceEvidence,
    context: &ExecutionContext,
) -> Result<QualifiedSourceEvidence, EvidenceError>;
```

The comments identify each function's module. Export `PreparedSourceEvidence` and `QualifiedSourceEvidence` from `architecture::evidence`; export the two Rust input types from its `rust` submodule. Keep lower provider interfaces crate-private.

`RustBuildSelection` is typed untrusted selection data, with exact logical shape:

```text
{ package: Text, target_kind: "lib" | "bin", target_name: Text,
  target_triple: Text, profile: Text,
  default_features: bool, features: Text[] }
```

All fields are required; there are no implicit defaults. `features` is a sorted unique set of nonempty names. `package` identifies exactly one package in the captured Cargo metadata; display-name ambiguity is invalid input, not first-match selection. Resolve physical package IDs to pinned logical source/manifest identities. The chosen target must exist with the requested kind/name. Unknown fields/duplicate input keys fail in any provided JSON constructor. Reject unsupported target kinds rather than treating tests/build scripts as production roots. The actual resolved feature set is captured independently; the requested list is not that proof.

`RustToolchainBinding` is an opaque trusted-host configuration object, not deserializable candidate data. Its fallible constructor `new(cargo: PathBuf, rustc: PathBuf, capture_helper: PathBuf, scratch_root: PathBuf)` requires absolute paths; resolve and verify executable identities/protocol support before use. Retain read-only getters. `capture_helper` is the reviewed codebaseGraph executable's private runtime capture/wrapper mode, not a new public operation or candidate script. The binding names the actual compiler/Cargo executables; it does not install tools, alter aliases or select a tool from candidate text. Source-selected wrappers or conflicting ambient/configured wrappers are unsupported in v1 and cause an unavailable preparation, rather than silent replacement. Candidate configuration can affect the selected build only through captured, disclosed inputs.

`PreparedSourceEvidence` owns the admitted immutable source corpus and complete captured-context facts after actual child completion/cleanup. It has no unchecked constructor, mutation API, Deserialize implementation or accepted-state meaning. Required getters are `source()` (a constructed SourceProvenance suitable for C23), `domain()`, `effective_policy()` and `canonical_metadata_json()`. It does not expose a local scratch path as a source identity or require a live scratch directory during later qualification.

Qualification requires exact agreement among prepared provenance, C23 source/extractor/mapping identities, the supplied model's compilation-input identity and effective policy. Changed effective settings, including an increase, invalidate reuse. Match each binding/citation to the exact pinned bytes, node table/ID/qualified_name/path/nonempty span. A different model can be qualified only by a new invocation/query set with its own binding; a caller cannot relabel an old result. Existing invalid C23 evidence remains visible and blocks its dependent use; independently valid source facts can still be reported.

Each public call is an invocation. Internal composition into a later evaluation must use one shared in-run policy/deadline/counter bridge across preparation and qualification; nested public calls must not reset that operation's controls. Separate explicit top-level calls are not represented as one uninterrupted execution.

## C25-03 — Neutral provider vocabulary and opacity

Use existing source-graph row identities without teaching the lower layer about architecture SourceRef types. Neutral `FileKey` identifies repository-relative path plus actual raw SHA-256; `SourceAnchor` adds exact row table, node ID, qualified name and nonempty byte span. `UnitKey` identifies a captured logical source/build unit. These are source facts with no component owner or responsibility field. String/digest encodings at the public bridge follow C16; lower raw SHA-256 values can use `[u8; 32]`.

A neutral `ProviderRef` carries `id`, `version`, `language` and `descriptor_digest`. This implementation is `id: "rust-q1", version: "0.1", language: "rust"`. Its fixed descriptor includes supported relation forms, actual built-in grammar/profile identity, binding/visibility/cfg rule revisions and capture protocol version. Changing these semantics changes the descriptor digest/version as applicable. This is implementation identification, not remote attestation or proof of model execution.

The sealed prepared-provider interface supplies descriptor/inventory access and `qualify(queries, control)`. Generic query kinds are export identity and directed reach from an anchor to a provider-scoped capability key; reach carries an optional positive semantic hop bound. Capability keys are opaque, canonical, provider-scoped selectors created by that provider's bridge. For Rust they encode the exact legacy crate/namespace/item-path claim, interpreted against the actual unit/extern context. They are not an arbitrary executable expression, unchecked success flag or globally comparable name.

Generic facts carry query key, subject anchor, `established | refuted | unknown`, binding/export witnesses, coverage reference and sorted reason codes. The source-level graph retains qualified self-edges when present; removal of intra-component/container edges belongs to later architectural projection. A positive reach witness has at least one edge.

The bridge derives a complete ordered query inventory from every applicable compiled ownership-rule clause for every submitted source subject, plus submitted export/reach assertions. Bind keys to model compilation inputs, mapping facts, source/provider/context identities, rule/clause location and all query arguments. Do not let submitters select only favorable rules/competitors. This is fact production, not all_of/none_of reduction or unique-owner selection. Missing rules, missing source subjects or empty mappings do not prove a governed scope complete.

Opaque output construction prevents caller-supplied verdicts or serialized flags from becoming capabilities. It does not purport to isolate arbitrary native code already running with the host account's privileges. No sandbox/OS-OOM/opaque-native-interruption guarantee is added. All semantic binding proof still comes from independently verified pinned source, not from trusting a compiler-success label.

## C25-04 — Actual capture protocol and immutable inputs

Preparation resolves the configured repository binding and verifies the full immutable commit/tree. Extend the existing verified Git object reader with bounded enumeration; never add a parallel reader, fetch, checkout/filter/hook execution or working-tree fallback. Traverse tree entries and referenced object bytes coherently under their verified hashes. Enumerate source/configuration inputs independently of submitted mappings or graph query output.

Compute C16's SourceContentKey over every governing tree path/mode/content and submodule reference, with only the fixed `architecture/evaluation-summary.md` exclusion. No candidate-controlled whitelist, ignored-source shortcut or extra report exclusion is allowed. Required governed dependency/submodule contents must be independently available; a commit ID or artifact filename alone does not establish completeness. Unavailable contents remain named UNKNOWN obligations. Retain raw Git provenance separately from content identity.

Create a fresh operation-private source snapshot and target/capture areas under the configured scratch root, with exclusive ownership and confined paths. Populate source/configuration bytes from the verified objects, not the user's checkout. Reject required symlink/path escapes rather than following them. Preserve effective repository configuration, including existing Cargo configuration semantics; do not modify user settings, aliases, HOME or shared caches to make capture succeed.

Run the configured Cargo with `--locked --offline`, explicit manifest/package/target/profile/features/target triple and a fresh target. Invoke its supported JSON message stream and the tool-owned rustc wrapper. The wrapper checks the parent-owned job binding and expected compiler identity, records actual compiler arguments/context and forwards the real invocation. Retain compiler exit status and output/input identities. Do not parse human verbose command text as a binding oracle. Any helper launch/record not bound to the operation or inconsistent with Cargo units fails capture; PID or candidate-written output alone is not an execution receipt.

Capture host versus target units separately. Required context includes actual Cargo/rustc identity, edition, crate name/type/root, requested and resolved features, effective cfg, relevant rustflags, `--extern` mappings, Cargo manifests/lock/configuration identities, relevant build-script cfg/env/output identities, generated-input identities and dependency artifact/source identities. Record full semantic settings internally; public metadata retains their canonical identities and useful safe summaries rather than dumping raw environment or process output. Unknown Cargo fields may be ignored only where irrelevant to this pinned supported protocol; unknown semantic values affecting context are unsupported, not assumed defaults.

Cargo's `fresh` and `build-script-executed` messages are observations: the latter may contain cached output. This invocation uses a new target and requires actual helper receipts for each relevant compiler unit and matching actual build-script execution/context. No cached record, label or successful Cargo exit alone establishes capture. Required unsupported capture forms, compilation failure, missing receipts, unexpected unit/extern mappings, source drift or integrity mismatch produce no complete PreparedSourceEvidence. Keep diagnostics and actual completed observations in the error payload.

The wrapper checks source snapshot identities around compiler dispatch, and the parent rechecks the snapshot before admitting it. Binding analysis uses the independently captured Git byte corpus only. Compiler metadata/build success does not certify Rust endpoints, expansion or type resolution. Generated source, proc-macro effects or unverified external inputs do not become Q1-qualified because compilation succeeded; they remain visible unsupported evidence when relevant.

Runtime execution retains real containment/ownership through the Cargo/helper/build-script child closure. Cancellation, deadline, owner loss or output failure must stop and reap/observe actual children before releasing ownership or deleting their workspace. If termination/cleanup is unconfirmed, return the actual incomplete error and retain ownership/recovery information; do not report cancellation as completed cleanup. No blind replay, durable task service, installation, daemon change or existing storage-protocol migration is introduced. Transient capture material is operation-owned, not a second durable evidence archive; executed verification evidence retains the repository's separate retention requirements.

## C25-05 — Rust Q1 identity, scopes and visibility

Build crate/module/item identities from the captured unit graph and exact pinned syntax. Never use lowercased graph indexes, last-label matches or non-placeholder endpoints as proof. The same file can participate in distinct crate/build units; keep those identities distinct. Resolve module paths against the Git path inventory rather than the host filesystem's case folding. Detect missing/ambiguous module files and unsupported path/attribute effects.

Support explicit `crate`/`self`/`super` paths, explicit/grouped `use` trees, `self` group members, aliases and acyclic explicit re-export chains when every step is established. Bind namespaces independently. Rust item scopes, block items and local pattern/parameter scopes must be modeled before selecting a target. A local variable/parameter shadowing a function prevents falling through to that function; invocation through that value remains type-dependent UNKNOWN. Respect declaration-order differences between items and local bindings. Visibility/accessibility and the actual crate/extern binding are required at every crossing.

Case sensitivity does not permit misinterpreting Rust identifier spelling rules. Handle raw identifiers consistently with the selected edition. Retain exact source spelling/digests. Where identifier normalization, syntax, namespaces or edition behavior cannot be independently established with the built-in facilities, return a named UNKNOWN rather than inventing ASCII-only completeness or adding a new parser/dependency silently.

Evaluate supported cfg predicates only from the complete captured unit cfg set. An absent option is false only when that set's completeness is established. Account for applicable cfg_attr/path/visibility effects; unhandled effects remain UNKNOWN. Distinguish independently proven inactive source from an unsupported site or candidate exclusion. SourceContentKey still includes inactive files.

An exported capability is a visible item or explicit re-export through a module boundary. `pub`, `pub(crate)`, `pub(super)` and `pub(in path)` qualify only with proven declaring scope, path, audience and effective visibility along the export chain. Restricted exports do not become externally public. Ordinary private items do not qualify as exports. The legacy capability path identifies an export route to a concrete source definition, including aliases; exporting the same definition under another route requires its own proof.

Same-repository workspace targets may qualify across crates only when both pinned source units and the actual `--extern`/crate-alias mapping are captured in the same compatible context. Third-party/uncaptured definitions remain UNKNOWN, even when artifacts or names are available. Glob imports, unresolved cfg, macro-introduced bindings, re-export cycles, trait/type-dependent dispatch and unsupported syntax remain visible. A site is qualified only when relevant unsupported effects can be independently excluded; successful compilation or a submitter's assurance is insufficient.

## C25-06 — Inventories, positive and negative proof

Independently inventory the selected Cargo roots/module trees and potential import/direct-reference sites from complete pinned syntax, not only existing profile captures or submitted citations. Syntax known not to be a source dependency may be classified by the versioned provider rules; unsupported/unrecognized dependency-capable constructs receive explicit sites. Missing/malformed source cannot establish absence.

Maintain `source_unit_inventory` and `site_results` with stable unit/file/site keys, actual digests and reasons. Site status is exactly C16's `qualified | unsupported | unresolved | unscanned`. A qualified site references its immutable binding; every other status names a reason. Distinguish raw/tree inventory, context-active units, required query closure and actually scanned units. Counts alone are never equality proof.

For each query, derive its required source-unit/site closure and compare it with actual scans. Follow newly reached dependencies outside initial pilot files/components. A known target outside initial origins is retained, not dropped at the boundary. An unknown outgoing site that can reach the queried capability keeps the negative proof unknown; an unknown frontier cannot be treated as a leaf.

An export positive requires a proven export route/audience to the exact subject. A refutation requires sufficient relevant export/alias coverage to exclude that route. A reach positive requires an ordered chain of qualified bindings ending at the exact capability. A negative requires closure of every relevant frontier up to the semantic hop bound, or to a fixed point if unbounded. For a bounded query, outgoing sites beyond the last permitted hop need not be scanned merely to prove the bounded result; record that semantic frontier rather than a resource truncation. A resource limit never acts as a hop bound. Preserve cycle/self-edge semantics and at least-one-edge reach.

Irrelevance/exclusion must be independently established for the particular query under approved policy. An unsupported/unscanned site is not an approved exclusion, and an exclusion cannot narrow a broader mandatory guarantee. Positive witnesses can survive unrelated unknowns; negative facts require their own sufficient coverage.

This slice supplies C16-compatible DomainRef data, binding/site inventories and source-level `PredicateCoverage` records (`query_key, required_inventory_digest, scanned_inventory_digest, complete|incomplete, reasons, witnesses/frontier`). It does not fabricate component/container Scope completeness before ownership exists. COD-26/28 combine these source facts with actual ownership and architectural scope to construct final projection/check coverage. COD-25's source-level proofs required by COD-24 are not architectural required-edge or metric verdicts.

## C25-07 — Logical outputs and identity

The prepared metadata is `{ preparation_version: "source-evidence/0.1", source, provider_ref, domain_ref, build_context_summary, effective_policy, capture_input_digest, preparation_digest }`. Its opaque body owns admitted bytes/indices/receipts needed by qualification and no live build. The public source includes the actual C23 extractor digest and captured build-context digest, not candidate-declared substitutes. Domain data retains supported relation forms, source inventory digest and explicit exclusions per C16.

The qualified logical output is:

```text
{ qualification_version: "source-evidence/0.1",
  source: SourceProvenance, provider_ref, domain_ref,
  model_id: Id, model_compile_input_digest: Digest,
  mapping_facts_digest: Digest, preparation_digest: Digest,
  effective_policy: ArchitectureExecutionPolicy,
  bindings: QualifiedBinding[],
  capability_facts: [{ query_key: Digest, subject: SourceRef,
      kind: "export_identity" | "reaches_export", capability_key: Digest,
      truth: "established" | "refuted" | "unknown",
      binding_witnesses: Digest[], export_witnesses: Digest[],
      coverage_ref: Digest, reason_codes: Text[] }],
  source_unit_inventory, site_results, predicate_coverage,
  diagnostics, has_invalid_evidence: bool, has_unknown: bool,
  qualification_digest: Digest }
```

`QualifiedBinding` retains every C16-05 field: caller, reference_span, namespace, lexical_scope, import_chain, target, build_context, rule_version and evidence_digest. The caller is the exact source-located owning declaration/module, and the reference span is within its file/scope. Targets must be independently reconstructed source-located definitions/modules, never name placeholders. Import steps and lexical-scope/audience witnesses retain source anchors, order, context identity and their actual proven relation; no imported string is a target.

The generic public envelope uses SourceRefs and provider-scoped identities. Rust-only capture/selector/audience detail is available as provider evidence identified by digest, not Rust types in generic consumers' signatures. Diagnostics use stable code, optional query/site/unit subject and message, retaining invalid and unknown distinctions. Their canonical order is unit/file/site/query, then code/subject/message; reason sets and unordered inventories are sorted/deduplicated by stable keys, while import/path witnesses remain ordered.

Use the existing domain-separated C16/C22 canonical writer for all new upper identities: `source-evidence-provider/0.1`, `rust-build-selection/0.1`, `rust-build-context/0.1`, `source-evidence-inventory/0.1`, `source-evidence-query/0.1`, `source-evidence-preparation/0.1` and `source-evidence-qualification/0.1`. Each digest excludes its own result field. SourceContentKey follows C16's full-content rule with domain `architecture-source-content/0.1`. Hash exact semantic settings/inputs; no timestamps, debug formatting, result claims or arbitrary session IDs enter deterministic inputs. Retain raw invocation provenance separately; session-private physical paths may be represented logically only when their exact source/artifact mapping is independently verified. Other semantically meaningful path changes invalidate identity.

`has_unknown` refers to this source-fact/query result, not ownership acceptance. Preserve the original C23 report unchanged; its staged ownership/build/predicate unknowns are not overwritten or mislabeled as old execution of the new result. A compatible Q1 query can now have an established fact while that original report remains staged. Unknown source inputs, incomplete query coverage and an empty mapping/query set keep appropriate qualification uncertainty visible. `has_invalid_evidence` preserves invalid supporting claims. Neither flag set, a digest nor an opaque result is an overall conformance/owner PASS or trust receipt.

This is a constituent source-evidence identity, not COD-32's complete InputIdentity/EvaluationId/AcceptedStateId. New provider/context/extractor/policy/source/model/mapping/query arguments invalidate their dependent identities. No historical lineage, correction or transfer is qualified here.

## C25-08 — Resources, failures and recovery

Use every existing positive policy field; add no fixed numerical limit, default, disabled-infinite setting, source exclusion, architecture metric budget or global build setting. Missing/invalid policy/configured tools fail before dispatch. The qualifier reuses the model's effective policy and requires an equal prepared-policy identity; callers only tighten at the existing policy boundary.

Apply `max_closure_files` to distinct captured source/config/dependency files admitted for this service, `max_file_bytes` per captured/read blob or receipt, `max_evidence_bytes` to complete request/capture metadata plus actual distinct verified input/source/context bytes and retained provider evidence, and `max_result_bytes` to each complete logical public result. Whole-tree source-key work is charged too; do not create a free scan path. Verify/charge a distinct blob before comparing a submitted digest, cache its actual digest and avoid repeated reads for bad-to-good claims. Stream process output under the same admitted byte accounting; oversized/truncated output is an error, not a reduced successful context.

Use `max_parse_depth` for syntax nesting and `max_traversal_steps` for enumeration, parsing/normalization, scope/import/cfg resolution, binding/frontier work, comparisons, canonicalization and hashing. Charge source/provider nodes and edges against `max_projected_vertices`/`max_projected_edges`, as bounded source evidence rather than an architectural projection; do not reset counts per file or phase. Existing model/mapping fields keep their C21/C23 meanings; this service adds no alternative meaning for unused scenario/rationale limits. The total configured duration and caller deadline apply through actual capture/cleanup and qualification suboperations.

Owned Cargo artifacts/workspaces remain real runtime resources. The policy bounds admitted evidence and cooperative service work; it is not a whole-Cargo/compiler disk/RSS/OS-OOM limit. No new build-artifact size ceiling is invented. Preserve actual runtime failures and required workspace cleanup without claiming opaque native interruption.

Malformed selection/reference/query shapes use existing `architecture_invalid_input`/`invalid_reference`; configuration uses `architecture_configuration_required`. Missing/unavailable tools, objects, capture or context use `architecture_input_unavailable` with specific diagnostics. Changed/mixed identities use `input_changed`; integrity failures use `result_integrity_failed` or the existing invalid-evidence distinction where a well-defined result can retain it. Unknown/unsupported semantic facts are normal per-site/query UNKNOWN records. Existing C23 invalid evidence remains explicit and is not relabeled as a conformance violation.

Allocation, work/byte/result limits, cancellation, deadlines and unexpected internal errors use existing stable error codes through EvidenceError, with known-invalid/known-unknown diagnostics preserved and no partial successful capability. Keep unsupported/unavailable evidence distinct from fatal resource failures. Qualification never starts a build, mutates source/accepted state, installs a graph, persists a cache or automatically retries. Explicit retry supplies current identities and context; do not replay after ambiguous dispatch. Runtime owns unconfirmed child/cleanup recovery until it is actually resolved.

## C25-09 — Acceptance and linked-issue impact

All checks below are **planned product checks**, not executed acceptance evidence:

| ID | Trace | Required outcome / method |
| --- | --- | --- |
| C25-A01 | R04/R12, A06; PC-02–06 | Static/API dependency checks: no Native Architecture imports in lower provider/runtime capture; generic consumer signatures need no Cargo/Rust types. Neutral synthetic contract fixtures demonstrate only data consumption, never a second qualified provider or pilot success. Existing v1 compile/submission/digest fixtures remain identical. |
| C25-A02 | R04/R09/R12, A06 | Real immutable Git/build fixtures: per-unit host/target/features/cfg/config/tool/dependency changes invalidate context; conflicting wrappers, cached/missing/truncated/helper-unbound capture, source drift and failed builds cannot mint preparation. Preserve actual process/cleanup outcomes. |
| C25-A03 | R04, A06 | Independent endpoint oracles for duplicate names, case collisions, namespaces, explicit/grouped imports, aliases, raw identifiers, nested module/block/local scopes, shadowing and captured workspace extern aliases. Establish expected target paths before resolver execution; use simple independently compiled/instrumented fixtures as a separate oracle. |
| C25-A04 | R04, A06 | Wrong-concrete/placeholder controls, inaccessible/restricted export routes, re-export cycles, globs, cfg/macros/type-dependent calls and unsupported identifiers never manufacture positive or negative proof. Every dependent UNKNOWN names the exact site/reason. |
| C25-A05 | R04/R12, A06 | Independently compared raw/active/required/scanned inventories; omitted/newly reached units, missing source/relations, hop boundaries, cycles/self-edges and truncated frontiers. Known witnesses survive unrelated unknowns; absence requires its own complete coverage. |
| C25-A06 | R09/R12, bounded A12/A16 | Independent canonical-hash oracles and per-field mutations; exact/minus-one byte/work/depth/result limits; cancellation/deadline/allocation/process failure injection across real paths. Existing architecture model/compiler/identity/evidence and affected parser/materializer/API/caller suites, fmt and unsuppressed strict Clippy pass on final source. |
| C25-A07 | R04/R12, A06 | Actual original Graph Read Service pilot endpoint/domain sufficiency, including API Core, Coordinator, Agent Hook and Wiki caller paths and newly reached production dependencies, then relevant checks at delivery source. Contexts remain separate. A concrete endpoint alone is not closure sufficiency. |
| C25-A08 | AGENTS/PC-01–14 | Exact final source/test/environment/binary receipts, independent executed verification, fresh adversarial final review, essential-only PR and independent complete remote publication readback. |

The first capture qualification set is `aarch64-apple-darwin`, Rust/Cargo **1.98.1**, production `cfg(test)=false` using explicit **release** profile selections: root `codebase-graph` lib `codebase_graph` with default features; root bin `codebase-graph` with default features; and `k-wiki` lib `k_wiki` with `graph-context` and its declared defaults. Actual resolved dependency features are captured per invocation, not assumed equal. This concrete initial profile is part of the approval subject; it does not qualify all profiles/targets. Local unit-test profiles are separate evidence.

Run C25-A07 as soon as capture/resolution are operational, before treating synthetic fixture greens as pilot acceptance. Inspect production dependency closure, not a file/component whitelist. The original observed API Core import/calls to `execute_graph_search` must bind to the independently established Graph Read Service definition; likewise audit affected callers. Macro/method/external gaps can make full Q1 sufficiency fail. Report actual sites, affected mandatory obligations and the smallest Q2/Q3 scope decision. Keep COD-25 open; do not broaden resolution, weaken guarantees or calibrate an empty graph.

COD-24 retains actual unique/mixed ownership and complete competitor rule evaluation; COD-26 retains owner-based projection; COD-28 retains architectural absence/conformance coverage. COD-27 retains lineage/correction/transfer, COD-32 complete evaluation identity, COD-45 durable table regeneration, COD-37/46/47 trust/native acceptance/hosted enforcement, and maintainers aggregate/platform/release closure. Producer truth/coverage must support those consumers without taking over their decisions or making them prerequisites of this producer.

No product acceptance, platform qualification, hosted green, approved native model, merge or release follows from approving this documentation proposal. Exact source/file ownership, prerequisite checks, evidence and staged delivery remain in the linked execution brief.
