# COD-17: Architectural baseline and extension boundary proposal

## Identity and status

| Field | Value |
| --- | --- |
| Work | `CBG-C4-01/F01/S03` / [COD-17](https://linear.app/codebasegraph/issue/COD-17/cbg-c4-01f01s03-supported-pilot-baseline-governed-scope-and-resolution) |
| Document revision | `0.1`, proposed, 6 November 2026, Australia/Darwin |
| Decision being supported | Whether the Extension needs a compatible internal redesign or a breaking public API redesign, and which Rust observation domain to qualify first |
| Source baseline | [`90e2ab6fae2767bdb158e19f0072d43568e80dbc`](https://github.com/rabii-chaarani/codebaseGraph/tree/90e2ab6fae2767bdb158e19f0072d43568e80dbc), version 4.2.1; clean canonical checkout |
| Documentation branch base | `origin/dev` at `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`; distinct from the examined source baseline |
| Canonical proposal | [Revision 0.2 at its recovered publication commit](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/spec.md) |
| Historical proposal review | [Original review record at the same commit](https://github.com/rabii-chaarani/codebaseGraph/blob/0a66cb31e397e523190618f211a613f7a095d9a0/docs/specs/CBG-C4-01/proposal-review.md) |
| Requirements / acceptance | R04, R06, R12; A06, A08, A16, A17; preserve D01–D23 and APR-01–APR-05 |
| Scryer | Proportional documentation ledger `chg-jnw6sb`; existing component identities below remain authoritative |
| Approval | The user authorised baseline work and this next proposal step. No alternative below, new architecture claim, implementation, native-authority transition, merge or release is approved by this document. |
| Closure | COD-17 remains open for decisions and review. COD-16 supplies concrete contracts, COD-18 supplies trust/integration contracts, and COD-19 owns exact-scope implementation approval. Repository maintainers retain aggregate closure. |

The recommendation is to retain the current application boundaries, add a separate native architecture domain, and qualify a bounded Rust binding pass. A whole-product rewrite is not supported by the examined evidence. This is a proposal for the user to decide, not an accepted target architecture.

## Canonical recovery and existing decisions

The original planning chat is `01a1660d-9621-7111-a723-a6d7a5e1a151`, titled “Plan agentic CodebaseGraph extension”. Its recorded command results contained the full missing proposal and review record. The recovered UTF-8 bytes match the hashes already recorded on COD-17:

| Original file | SHA-256 |
| --- | --- |
| `spec.md`, proposed revision 0.2 | `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e` |
| `proposal-review.md` | `2f9f6de504f31d3b171239c00cd7837c576605e3e794fa96acd76b800cc4f1fd` |

Both files are preserved unchanged. Their statements about being uncommitted and unresolved Linear routing describe their historical state. This publication supplies an immutable proposed revision; it does not supply approval. The established destination is [CodebaseGraph Extension, P-COD-1](https://linear.app/codebasegraph/project/codebasegraph-extension-2cf77b39fc8b), team COD. There is no migration of another workspace or new parallel specification.

This document supplements the existing proposal with baseline evidence and alternatives. It does not revise R01–R12 or approve a new release scope. Any material change after the user's choices requires the existing revision, impact-review and approval process through COD-19.

## Observed architecture

| Existing boundary | Intended owner | Observed role and implication |
| --- | --- | --- |
| Graph Runtime | `container-product-runtime` | One application/library boundary containing public operations, reads, extraction, storage and process coordination. Coordinator and worker subprocesses are real runtime boundaries inside this grouping; a container is not being inferred from each directory. |
| Knowledge Wiki | `node-164` | Separate crate and executable for curated knowledge. Graph context and refresh consumers use the public graph API; MCP installation also consumes its public lifecycle API. |
| Release Verifier | `container-release-verifier` | Separate `xtask` executable for policy, tests, package/provenance verification and subprocess smoke checks. |
| Public operation path | `node-96`, `node-109`, `node-110`, `node-142` | Contracts → facade → shared normalisation/registry/dispatch. Transport adapters translate and present. The Extension should preserve one semantic implementation across CLI, MCP and library callers. |
| Graph reads | `node-43` | Search, context and bounded read-only queries. Intended dependencies include Graph Store (`node-88`) and Catalog Provider (`node-111`); API Core (`node-110`) and Coordinator (`node-209`) are callers. |
| Extraction | `node-68`, `node-204` | Execution planning invokes Language Syntax Graph. Syntax provenance and semantic projection belong to this existing boundary. |
| Storage/publication | `node-88`, with materialisation owner `node-56` | Generation publication, leases, recovery and cleanup are existing contracts to preserve. They are not an accepted-architecture authority. |

The Rust workspace contains the graph package, k-wiki and xtask. Graph discovery found all 216 Rust files under `src/` and `crates/` in the examined tree, including tests in those directories. This is file-inventory completeness, not relationship or behavioural coverage; root-level integration tests are additional files. File sizes and test references are not used as proof of production coupling.

Scryer's committed intent and its pending work are distinct. The baseline assessment reported broken anchors and subsequent orientation reports pending changes from unrelated work. No global drift reconciliation, claim fold, reassignment or cleanup of another issue is performed here. Candidate dependency findings require source confirmation because test references and heuristic name bindings can distort architectural topology.

### Pilot scope

The proposed metric origin is Graph Read Service (`node-43`) inside Graph Runtime. Trace its production dependency closure through Catalog Provider, Graph Store, the public request/dispatch path and language-context helpers. Inspect API Core, Coordinator, Agent Hook Adapter and Wiki graph consumers as affected callers. Follow a newly discovered dependency before claiming that the closure is complete; the named components are starting points, not a whitelist that hides other edges.

The real pilot is principally a component-level graph inside one application boundary. It may legitimately have no inter-container edges. Synthetic graphs can qualify container metric algorithms, but cannot establish observed pilot coverage or justify numerical limits. D17's measured-baseline and human-calibration requirement remains pending.

## Resolution evidence and bounded feasibility

### Confirmed gap

At the source baseline, API Core imports `execute_graph_search` from `crate::api::graph_read` and calls it at lines 1112 and 1163. The function is defined at `src/api/graph_read.rs:53`.

The read-only graph probe executed on 6 November returned three `Reference` rows for that name in API Core: lines 8, 1112 and 1163. Each `ResolvesTo` target was a `Symbol` in **API Core**, with metadata `resolution: name_placeholder`; none established the imported Graph Read Service endpoint. Graph search independently found the concrete function in Graph Read Service.

```cypher
MATCH (r:Reference)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(t)
WHERE r.path = 'src/api/core.rs' AND r.label = 'execute_graph_search'
RETURN r.id, r.label, r.line_start, t.label AS target_label,
       t.path AS target_path, t.metadata AS target_metadata
LIMIT 8
```

The result was three rows, not truncated. The observed graph was readable and current, generation `34944-1793946918788448000-1`; this is an observation identifier, not a durable dependency for the proposal. Source inspection separately confirms the gap, so graph readability is not being equated with binding correctness.

The current resolver searches a per-builder name index in reverse insertion order, using lowercased keys, and otherwise creates a placeholder. Its target-selection inputs do not include the caller's lexical scope or Rust namespace. A concrete endpoint is therefore not enough to qualify a binding either. The import path creates an import/dependency inventory; it does not bind each imported item to its actual definition.

This matters for R04/A06: a missing or wrong binding can hide a forbidden dependency or invalidate a metric. Empty projections and non-placeholder endpoints cannot independently authorise a pass. No executed wrong-concrete-target fixture or numerical projection baseline is claimed here.

### Proposed observation choices

| Choice | Intended capability | Cost and limits |
| --- | --- | --- |
| **Q1 — Qualified pilot subset, recommended** | Establish case-sensitive module/item identity, lexical scope, explicit and grouped `use` bindings, aliases and direct statically named references needed by the pilot. Qualify only bindings established under the declared source/build context. | Bounded initial work; unsupported or ambiguous cases affecting a check remain `UNKNOWN`. The subset's sufficiency for the actual pilot must be demonstrated by COD-25/F04 before acceptance or calibration. |
| **Q2 — Compiler-backed pilot** | Obtain qualified bindings from a pinned compiler or semantic-analysis integration for the selected Cargo target/features, then map them to the source ontology. | Stronger handling of expansion and type-dependent bindings, but a new tool/dependency contract, identity mapping and platform qualification are required. No provider or tool is selected by this proposal. |
| **Q3 — Broad Rust resolver** | Build broad repository-wide static resolution, including substantially more Rust language behaviour, as part of the Extension foundations. | Material scope expansion with a much larger correctness and maintenance burden. It cannot be treated as a small local repair. |

Q1 is a capability bound, not permission to call arbitrary raw-source matches resolved. Rust names have namespaces, scopes and visibility; imports can be grouped, aliased or re-exported, and some resolution depends on macro expansion or type information. See the [Rust name-resolution reference](https://doc.rust-lang.org/reference/names/name-resolution.html) and [use-declaration reference](https://doc.rust-lang.org/reference/items/use-declarations.html).

For Q1, unresolved conditional compilation, macro-introduced bindings, glob imports, re-export cycles, trait/type-dependent dispatch, external definitions and unsupported syntax must be disclosed. If their effect on a particular binding cannot be excluded or resolved from qualified evidence, that binding remains unknown. Explicit exclusions restrict the guarantee; they cannot make a broader mandatory check pass. Pinned target/features and source identity must accompany evidence; a successful build for one configuration does not qualify others.

If Q1 cannot support the agreed pilot obligations, return to the user with the observed blocker and Q2/Q3 impact. Do not silently widen the resolver, narrow the obligations or calibrate against a zero-edge graph.

### Proposed responsibility placement

- Language Syntax Graph (`node-204`) retains source/grammar provenance and would own qualified language-binding evidence. Preserve existing heuristic extraction behaviour until its callers and compatibility effects have been explicitly settled. A qualified relation must be distinguishable from an advisory name match.
- A proposed Native Architecture domain would consume qualified binding evidence and validated ownership mappings. It would own architectural dependency lifting, constraints, metrics and scenario semantics. No node or claim is invented merely to fill this table; concrete model additions follow the selected design and COD-16 contracts.
- Accepted intent, observed facts, predicted candidates and evaluation evidence remain distinguishable. Git-tracked YAML and the durable table retain the approved durable-output role; disposable source generations do not become the sole store of accepted architecture.
- Public API, CLI/MCP adapters and CI translate into the same domain operations. `graph_query` retains its existing read-only contract. The Extension has no Scryer runtime/approval dependency; the repository's separate authority/procedure transition remains an explicit COD-19 prerequisite.

These are proposed future boundaries, not current implemented capabilities or a writable-file assignment. Detailed implementation ownership, prerequisites and execution briefs belong in Scryer after decisions; this documentation step changes no model responsibilities or links.

## Public API alternatives

`OperationRequest` is currently an exhaustive public enum. Public request structs and `OperationDescriptor` also expose fields. `OperationExecutor` accepts the existing request enum, while the internal registered handler signature is tied to it. `prepare_operation_request` translates generic invocations into that same family. Therefore adding native request variants is not a compatible change merely because CLI names remain unchanged.

Adding variants to an exhaustive enum, changing required trait items or adding public struct fields can break consumers. Adding `non_exhaustive` retroactively is itself a breaking change. See the [Cargo SemVer guide](https://doc.rust-lang.org/cargo/reference/semver.html).

| Dimension | C1 — Compatible internal redesign, recommended | C2 — Versioned breaking redesign |
| --- | --- | --- |
| Public Rust surface | Keep existing enum variants, struct fields, trait obligations and public paths. Add a separate native architecture request family and explicit additive facade entry point; leave existing custom executors usable. | Introduce a versioned operation model with intentional evolution points; migrate existing request/executor consumers and restrict implementation APIs deliberately. |
| Shared semantics | A private dispatch representation can route both request families through one canonical operation catalog and common domain services. CLI/MCP/CI must not acquire independent validation or evaluation logic. | The new SDK/application contracts can define that common path directly. Stronger crate boundaries can enforce dependency direction. |
| Packaging | Begin with cohesive modules inside existing packages. Extract a crate only for an evidenced dependency/reuse boundary. No graph → k-wiki dependency to borrow YAML support. | Separate contracts/domain, runtime composition and storage/process adapters where justified; migrate Wiki/install consumers of existing public graph APIs. This is a designed package split, not a commitment to rewrite all source. |
| Existing CLI/MCP | Preserve existing operations, defaults, errors, query behaviour and registry metadata fields; add the new family through the public boundary. | Breaking Rust packaging does not itself require breaking CLI/MCP. Any wire/command changes need an explicit version/migration contract rather than being bundled incidentally. |
| Storage | Keep current source-generation format and lease/publication/recovery guarantees. Add native intent/evaluation projections with their own identities and authority rules. | A package/API redesign can retain storage compatibility. A storage-format migration is a separate decision if a concrete requirement demonstrates its need. |
| Migration/evidence | Compile representative existing embedded callers, exhaustive matches, request literals and custom executors unchanged; verify transport parity and old operation behaviour. | Inventory and migrate affected consumers; provide conversion/transition contracts and execute both supported platform suites and recovery checks on the changed interfaces. |
| Benefit | Smaller regression radius while introducing the missing domain and qualified evidence. | Cleaner stable SDK and stronger enforceable modular boundaries; less legacy dispatch accommodation. |
| Cost/risk | Existing and new typed request families coexist, requiring deliberate internal dispatch and compatibility tests. | More consumer migration and release coordination. A public break does not improve binding correctness by itself. |

C1 needs an internal dispatch design; simply adding a second independent registry would violate the existing architecture. Exact operation names, signatures, errors, bounds and wire shapes belong to COD-16 after this choice. C2 would materially revise the compatibility intent of R12 and requires a new specification revision, linked-issue impact review and user approval before dependent work.

Existing storage capability guards must survive either approach. In particular, native-generation verification constructs a non-forgeable token while retaining the writer/read leases, which storage consumes before sealing publication. Moving that proof across modules without preserving its construction/lifetime restrictions is not an acceptable modularity shortcut.

## Executed evidence and remaining qualification

The following local checks were executed during the baseline assessment in this chat against the unchanged source SHA above, on Darwin arm64 with rustc `1.96.0 (ac68faa20 2026-05-25)`. They are reused because this proposal changes documentation only. They do not qualify new Extension behaviour.

| Method | Observed result | Limit |
| --- | --- | --- |
| `cargo test --locked --offline -p codebase-graph --lib api::boundary_tests:: -- --nocapture` | 7 passed | Existing source-level boundary checks; not proof of every dependency edge |
| `cargo test --locked --offline -p codebase-graph --lib api::core::tests:: -- --nocapture` | 13 passed | Existing operation-core behaviour only |
| `cargo test --locked --offline -p k-wiki --features graph-context` | 132 passed, 16 suites | Initial sandboxed runs failed at loopback socket binding; the complete suite passed after an authorised unsandboxed rerun. No product defect is inferred from the sandbox failures. |
| `cargo test --locked --offline -p xtask` | 160 passed | Existing release-tooling tests, not execution of a new release |
| [CI run 42150642040](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/42150642040), exact source SHA | 25 jobs successful, including native macOS arm64 and Windows x86_64 unit/integration/generation jobs and artifacts | Hosted CI result inspected; native logs and every historical acceptance claim were not independently re-audited here |
| Read-only graph query above plus source comparison | Three placeholder targets for a known imported pilot function; concrete definition independently located | An executed feasibility observation, not an implemented resolver or complete coverage measurement |

No new Rust target was created: local checks used the normal repository `target` at `/Users/rabii/Projects/Repositories/codebaseGraph/target`. The installed CLI rejects `--version`; its version is not inferred from the source package version. Source identity, parser dependency (`tree-sitter-rust` 0.24.2), ontology catalog version 1.1.0 and observed graph generation are recorded separately. No daemon install, rebuild or restart was performed.

### Traceability and qualification boundaries

| Criterion | Evidence in this step | Remaining gate |
| --- | --- | --- |
| A06 / R04 | Placeholder/import gap demonstrated; ambiguity, scope and observation-domain risks specified | User chooses Q1/Q2/Q3; F04/COD-25 executes endpoint/coverage qualification, including wrong concrete targets |
| A08 / R04/R06 | Real pilot topology distinguished from synthetic metric fixtures | Qualified projection, algorithm evidence and human-approved numerical calibration remain pending |
| A16 / R12 | Existing boundary/core checks and hosted CI passed; compatibility alternatives identify enum/trait risks | User chooses C1/C2; COD-16 settles contracts, then affected caller/query/transport compatibility is executed on implementation |
| A17 | Reproducible source baseline and affected ownership/callers identified | End-to-end implemented pilot and actual governed integration remain F11 work |

## Decisions requested and handoff

1. **API direction:** choose C1 (recommended) or C2. The previous instruction to compare both equally authorised this comparison, not selection of either design.
2. **Resolution scope:** choose Q1 (recommended), Q2 or Q3. Each retains `UNKNOWN` for evidence it cannot qualify; choosing a broader approach requires its explicit dependency and platform contract.

The proposed baseline is the pinned 4.2.1 main revision above. Approving a source baseline does not establish a numerical budget, complete mapping coverage, accepted native model or hosted-enforcement qualification.

After these decisions, preserve their exact user references and update the relevant Scryer intent and linked specifications through the existing approval process. Keep COD-16, COD-18 and COD-19 dependencies; do not treat COD-17's documentation PR, issue status or publication of revision 0.2 as implementation approval. On completion of this sub-issue, hand off a fresh coordinator session rather than silently advancing the next sub-issue here.

## Source anchors

All links below bind the examined source baseline, independently of this documentation branch:

- [Public request types](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/contracts.rs#L18), [handler/registry boundary](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/core.rs#L87), [facade/executor](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/facade.rs#L15), [generic invocation translation](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/normalization.rs#L19).
- [Pilot import](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/core.rs#L7), [pilot call](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/core.rs#L1112), [actual target](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/graph_read.rs#L53).
- [Per-builder symbol index](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/syntax_materializer/builder/mod.rs#L17), [import inventory](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/syntax_materializer/builder/semantic.rs#L280), [reference target selection](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/syntax_materializer/builder/semantic.rs#L550), [case-folded key](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/syntax_materializer/ids.rs#L97).
- [Native verification token construction and retained leases](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/api/materialization.rs#L1134), [storage consumer](https://github.com/rabii-chaarani/codebaseGraph/blob/90e2ab6fae2767bdb158e19f0072d43568e80dbc/src/storage/maintenance_generations.rs#L72).
