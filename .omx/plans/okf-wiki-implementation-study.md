# OKF Wiki Implementation Study

## Summary

This repository already exposes a stable, transport-neutral public API for graph
health, search, context, read-only query, materialization, and lifecycle
operations through `CodebaseGraphApi::execute_operation`
(`src/api/mod.rs:1-26`, `src/api/facade.rs:16-39`, `src/api/core.rs:57-205`).
That API is a strong substrate for repository and code-graph retrieval, but it
is not yet an OKF consumer: current Markdown ingestion only produces
`DocumentationSource` and `DocumentationChunk` nodes from whole documents and
heading sections, with no YAML frontmatter model, no OKF reserved-file
semantics, and no markdown-link or citation extraction
(`src/profiles.rs:69-82`, `src/parser/markdown.rs:8-173`,
`src/syntax_materializer/relations.rs:103-115`).

The recommended direction is to build an adjacent OKF-specific compiler and wiki
service that uses the existing `codebaseGraph` public API for repository/code
graph features, while keeping OKF parsing, conformance, bundle navigation, and
wiki page projection in a separate bounded module or package. Pushing OKF
semantics directly into the existing documentation-node model would create
avoidable coupling with a code-search-oriented graph schema and search surface.

## Confirmed Current State

### 1. Public API boundary and exposure

Confirmed facts:

- The repository defines a stable API entry point intended for embedded
  consumers, and the same facade is used by CLI and MCP adapters
  (`src/api/mod.rs:1-26`).
- The public facade is `CodebaseGraphApi`, which executes an
  `OperationRequest` via `execute_operation`
  (`src/api/facade.rs:16-39`).
- Public request contracts currently cover `Health`, `Search`, `Context`,
  `Query`, `Materialize`, `Plan`, `Catalog`, `Setup`, `Reinstall`,
  `Uninstall`, and `Refresh`; there is no OKF- or wiki-specific request variant
  today (`src/api/contracts.rs:18-157`, `src/api/contracts.rs:159-180`).
- The authoritative operation registry exposes only twelve operations, and only
  `health`, `search`, `context`, `query`, `schema`, `query-helpers`, and
  `architecture-queries` are MCP-exposed (`src/api/core.rs:57-163`).
- Request normalization, validation, runtime resolution, dispatch, and response
  presentation are centralized in `ApiCore::execute`
  (`src/api/core.rs:172-205`).
- Boundary tests enforce that `src/api/**` does not import transport adapters
  and that CLI/MCP modules do not bypass the public facade
  (`src/api/boundary_tests.rs:1-88`).

Implication:

- There is already a viable public API boundary to reuse, but any OKF wiki
  capability would need either new public contracts or a separate service that
  composes this API rather than trying to tunnel through existing graph-search
  requests.

### 2. Current Markdown ingestion model

Confirmed facts:

- Markdown and MDX are recognized as one language profile with only two capture
  mappings: `doc.source -> DocumentationSource` and
  `doc.chunk -> DocumentationChunk` (`src/profiles.rs:69-82`).
- The markdown parser emits one `DocumentationSource` node for the whole file
  and additional `DocumentationChunk` nodes for heading-delimited sections
  (`src/parser/markdown.rs:8-59`).
- The parser discovers sections only by markdown headings. It does not parse
  YAML frontmatter, markdown links, citations, reserved filenames, or any OKF
  structure (`src/parser/markdown.rs:119-173`).
- Node labels default to named fields when present, otherwise to the raw text
  content of the node (`src/syntax_materializer/capture.rs:16-49`).
- For documentation nodes, the materializer stores the full captured text as the
  `summary` field (`src/syntax_materializer/builder/nodes.rs:43-99`).
- Documentation nodes emit only `Documents` and `EvidencedBy` relations to
  owners and parser evidence (`src/syntax_materializer/builder/semantic.rs:499-515`).
- The default relation allowlist permits `Documents` from documentation nodes to
  `Repository`, `File`, `Module`, or declaration nodes, not to OKF concept/link
  targets (`src/syntax_materializer/relations.rs:55-117`).
- The graph schema defines `DocumentationSource` and `DocumentationChunk` with
  generic graph fields such as `label`, `path`, `summary`, and `metadata`; it
  does not define OKF-specific fields such as `type`, `resource`, `tags`,
  `timestamp`, `bundle_id`, or `concept_id`
  (`assets/graph_schema.json:3580-3698`).
- The docs search index is `idx_docs` over `label`, `path`, and `summary` for
  those two node types (`assets/graph_schema.json:6551-6620`).

Implication:

- The current graph can retrieve documentation text, but only as generic
  document and section nodes. It does not preserve the semantic units that OKF
  requires.

### 3. Search, context, and query behavior

Confirmed facts:

- `execute_graph_search` runs across all configured search indexes and returns
  ranked node payloads with optional context (`src/api/graph_read.rs:35-105`).
- Context traversal is relation-profile-driven and returns neighboring graph
  nodes, not a document-oriented render model (`src/api/graph_read.rs:107-180`).
- Search results in `slim` mode contain `id`, `type`, `label`, optional `path`,
  optional `span`, and optional `summary`; standard mode adds
  `qualified_name`, score details, and `context`
  (`src/api/graph_read.rs:600-653`).
- Search ranking is optimized for generic code/entity retrieval, with
  `DocumentationSource` and `DocumentationChunk` treated as lower-priority than
  code declarations (`src/api/graph_read.rs:656-700`).
- `graph_query` is explicitly read-only; both keyword blocking and prepared
  statement read-only checks are enforced (`src/api/graph_read.rs:220-245`,
  `src/api/core.rs:587-616`, `SECURITY.md:21-31`).

Implication:

- The existing public API is good for retrieval and introspection, but it is
  not a page-delivery API for an OKF wiki. It has no concept-specific route or
  bundle projection surface.

### 4. Storage, refresh, and state

Confirmed facts:

- Repository runtime resolution derives `.codebaseGraph/config.json`,
  `.codebaseGraph/manifest.json`, and `.codebaseGraph/<repo>_graph.ldb` under
  the repository root (`src/api/context.rs:13-67`).
- Materialization persists manifests and graph state through a dedicated
  pipeline and manifest diff model (`src/api/materialization.rs:104-240`,
  `src/protocol.rs:72-170`).
- The source snapshot model tracks content hashes and changed paths, which is
  useful if an OKF compiler wants similar incremental behavior
  (`src/protocol.rs:126-170`).
- The README documents automatic freshness through `mcp start` or `watch`, and
  treats `build` as an explicit manual rebuild (`README.md:32-39`,
  `README.md:82-105`).

Implication:

- `codebaseGraph` already solves incremental graph state management for source
  files, but its persisted state is specific to syntax-materialized graph rows,
  not OKF bundle/page projections.

### 5. Transport and security posture

Confirmed facts:

- MCP specs are generated from the public API operation descriptors
  (`src/cli/mcp/tools.rs:14-48`).
- MCP tool handling maps only the graph operations listed in the registry and
  supports either block text or structured JSON payloads
  (`src/cli/mcp/tools.rs:63-220`).
- The README documents only graph-oriented MCP tools today
  (`README.md:54-80`).
- The documented security boundary is local-first; remote HTTP remains weakly
  scoped and `graph_query` must remain read-only (`README.md:62-70`,
  `SECURITY.md:21-31`).

Implication:

- Any wiki service exposed beyond local use needs its own security model; it
  should not inherit assumptions from the current local-first graph transport.

## OKF v0.1 Requirement Mapping

The table below compares the attached OKF v0.1 draft to the confirmed current
state of this repository.

| OKF draft requirement | Current `codebaseGraph` support | Status | Evidence |
| --- | --- | --- | --- |
| Bundle is a directory tree of markdown files | Markdown files are scanned and materialized | Partial | `README.md:3-5`, `src/profiles.rs:69-82` |
| Every concept has YAML frontmatter with required `type` | No frontmatter parser or typed frontmatter fields exist | Missing | `src/parser/markdown.rs:8-173`, `assets/graph_schema.json:3580-3698` |
| Optional `title`, `description`, `resource`, `tags`, `timestamp`, and unknown extensions must be preserved | No OKF metadata model exists in the graph schema | Missing | `assets/graph_schema.json:3580-3698` |
| Concept ID is bundle-relative path without `.md` | No concept identity rule exists; current IDs are graph IDs derived from capture/materialization keys | Missing | `src/syntax_materializer/builder/nodes.rs:52-97` |
| `index.md` and `log.md` are reserved and not concepts | Current markdown ingestion treats all `.md` files generically | Missing | `src/profiles.rs:69-82`, `src/parser/markdown.rs:8-173` |
| Exception: root `index.md` may contain `okf_version` frontmatter | No special root-index handling exists | Missing | `src/parser/markdown.rs:8-173` |
| Standard markdown body should remain readable and structured | Whole-file and heading-level markdown text is captured | Partial | `src/parser/markdown.rs:20-59` |
| Internal markdown links represent relationships and may be broken | Links are not extracted from markdown bodies today | Missing | `src/parser/markdown.rs:8-173`, `src/syntax_materializer/relations.rs:103-115` |
| Citations should be representable under `# Citations` | Headings are captured, but citations are not parsed as first-class data | Partial | `src/parser/markdown.rs:35-59`, `src/parser/markdown.rs:119-173` |
| Consumers must tolerate unknown types and broken links | No OKF consumer exists yet; current docs model avoids this by not parsing those semantics | Missing | `src/api/contracts.rs:18-157`, `src/parser/markdown.rs:8-173` |
| Index browsing and progressive disclosure | Graph search/context can retrieve docs, but no directory/index projection exists | Partial | `src/api/core.rs:57-163`, `src/api/graph_read.rs:35-180` |
| Optional logs for scoped history | No log-file parsing exists | Missing | `src/parser/markdown.rs:8-173` |
| Version declaration via root `index.md` frontmatter | No version parsing exists | Missing | `src/parser/markdown.rs:8-173` |
| Best-effort consumption rather than hard rejection | The graph API is permissive at graph-read time, but there is no OKF conformance layer | Partial | `src/api/core.rs:172-205`, `src/api/graph_read.rs:35-105` |

## Public API Capability Matrix

This matrix evaluates the already-written public API specifically for an OKF
wiki service.

| Public surface | What it does now | Useful for OKF wiki | Gap for OKF wiki |
| --- | --- | --- | --- |
| `CodebaseGraphApi::execute_operation` | Stable library entry point over public requests | Yes | Needs composition by a wiki service or extension with OKF-specific operations |
| `Health` | Confirms graph DB/manifest readability | Yes | Does not validate OKF bundle state |
| `Search` | FTS search over graph node types | Yes, for code/document retrieval sidebars | No concept-aware OKF search ranking or filtering |
| `Context` | Relation-based graph neighborhoods | Yes, for code-neighborhood panels | No concept page model or backlinks from markdown links |
| `Query` | Read-only graph queries | Yes, for admin/debugging | Unsafe to treat as end-user wiki contract; not concept-oriented |
| `Materialize` / `Plan` | Incremental graph build/plan for source files | Indirectly yes | Does not parse OKF semantics or produce wiki artifacts |
| `Catalog` (`schema`, `query-helpers`, `architecture-queries`) | Exposes graph metadata and canned queries | Indirectly yes | No OKF schema/catalog or bundle contracts |
| `Refresh` / `watch` | Keeps graph state fresh | Yes | No OKF projection refresh pipeline |
| MCP generated from descriptors | Makes graph tools consumable by agents | Yes, for codebaseGraph operations | No OKF-specific read/write tools |

## Prioritized Gap Analysis

### Priority 1: No OKF semantic model

Root problem:

- The current Markdown pipeline captures documents and heading chunks, but it
  never parses the semantic payload that OKF standardizes: frontmatter,
  concept-type metadata, reserved files, root-index exception, citations, or
  markdown cross-links (`src/parser/markdown.rs:8-173`,
  `assets/graph_schema.json:3580-3698`).

Why it matters:

- Without an OKF semantic model, a wiki cannot reliably answer basic questions
  such as “what concepts exist?”, “what type is this concept?”, “what tags does
  it have?”, “what links point here?”, or “is this `index.md` a directory page
  or a concept?”

### Priority 2: Public API is graph-oriented, not page-oriented

Root problem:

- The public API returns graph-search hits, graph neighborhoods, or raw query
  rows. It has no operation that returns a fully normalized concept page,
  directory listing, diagnostics set, or bundle manifest
  (`src/api/contracts.rs:18-157`, `src/api/core.rs:57-163`,
  `src/api/graph_read.rs:600-653`).

Why it matters:

- An OKF wiki needs stable concept/directory contracts, not just search hits.

### Priority 3: Search and ranking are wrong for OKF navigation

Root problem:

- The docs search index is generic (`label`, `path`, `summary`) and search
  ranking favors code entities over documentation nodes
  (`assets/graph_schema.json:6609-6620`, `src/api/graph_read.rs:656-700`).

Why it matters:

- OKF search should boost concept titles, concept IDs, tags, types, and
  descriptions. None of those are available in the current graph model.

### Priority 4: No reserved-file or history semantics

Root problem:

- `index.md` and `log.md` are not treated specially in current ingestion
  (`src/parser/markdown.rs:8-173`).

Why it matters:

- OKF requires both files to carry semantic meaning, and `index.md` at the
  bundle root has a special versioning exception that current parsing would miss.

### Priority 5: Security and transport assumptions differ

Root problem:

- `codebaseGraph` is explicitly local-first, and its HTTP mode is not positioned
  as a multi-user or production-grade remote API (`README.md:62-70`,
  `SECURITY.md:21-31`).

Why it matters:

- A wiki service may need browser access, sanitization, authentication, and a
  stronger remote boundary than the current graph transport.

## Recommended Target Architecture

### Recommendation

Build the OKF wiki as an adjacent subsystem that composes `codebaseGraph`
rather than modifying the current documentation graph model in place.

### Proposed boundaries

1. `codebaseGraph` remains the repository/code graph engine.
   - Ownership: syntax graph, incremental source scanning, read-only graph
     retrieval, architecture queries.
   - Interface used by the wiki: `CodebaseGraphApi` and existing MCP/CLI
     surfaces (`src/api/mod.rs:1-26`, `src/api/facade.rs:16-39`).

2. New `k_wiki` library/service owns OKF semantics.
   - OKF bundle discovery.
   - Frontmatter parsing and lossless extension preservation.
   - Concept ID derivation.
   - Reserved-file handling for `index.md` and `log.md`.
   - Internal-link, backlink, and citation extraction.
   - Conformance diagnostics against the OKF draft.
   - Directory and concept page projection.

3. Wiki backend/API composes both.
   - Primary page data comes from `k_wiki`.
   - Optional “code context” or “related implementation” panels use
     `CodebaseGraphApi::Search`, `Context`, and `Query`.

4. Wiki UI stays thin.
   - Render pre-normalized concept/directory payloads.
   - Do not make the browser responsible for interpreting raw OKF rules.

### Why this split is preferred

Pros:

- Preserves the existing graph API and schema contract.
- Avoids forcing OKF concepts into a code-search-first node taxonomy.
- Lets the OKF implementation evolve independently from syntax graph internals.
- Keeps repository/code retrieval reusable for non-OKF consumers.

Cons:

- Introduces another projection layer and state artifact.
- Requires coordination between two refresh paths if both code graph and OKF
  projection are kept hot.

### Rejected alternative

Do not treat `DocumentationSource`/`DocumentationChunk` as the canonical OKF
model.

Reason:

- Those nodes are too generic and too lossy. Retrofitting them would require
  stretching existing fields and search indexes far beyond their current
  meaning, with higher compatibility risk than a bounded adjacent module.

## Data Model, Storage, and Indexing Implications

### Recommended OKF data model

Add a normalized model owned by the wiki subsystem:

- `Bundle { id, root_path, okf_version, diagnostics[] }`
- `Directory { path, title, description, authored_index, children[], concepts[], logs[] }`
- `Concept { id, source_path, type, title, description, resource, tags[], timestamp, extensions, body_markdown, headings[], citations[], outbound_links[], backlinks[] }`
- `Link { raw_href, target_id?, fragment?, status }`
- `LogEntry { scope_path, date, category?, text, links[] }`

### Recommended storage

Recommended initial choice:

- Keep codebaseGraph state in `.codebaseGraph/` unchanged.
- Write OKF projection artifacts to a separate state root, for example
  `.okfWiki/`, to avoid accidental coupling to graph manifests or DB schema.

Rationale:

- `RepoPaths::derive` and runtime resolution are explicitly tied to
  `.codebaseGraph` and graph-specific files (`src/api/context.rs:13-67`).
- Materialization manifests are keyed to source snapshots and graph rows, not to
  OKF concepts (`src/protocol.rs:72-170`).

### Recommended indexing

- Build an OKF search index over `concept_id`, `title`, `type`, `tags`,
  `description`, headings, and body text.
- Keep backlinks and link diagnostics as first-class indexed data.
- Optionally mirror selected OKF concept summaries into `codebaseGraph` later,
  but do not make that a day-one dependency.

### Migration implications

- No migration to existing `codebaseGraph` graph schema is required for an
  initial release.
- If later integration into LadyBugDB is desired, add a versioned import path
  rather than mutating `DocumentationSource` semantics in place.

## APIs and Contracts

### Reuse existing public API as-is for

- Repository health checks.
- Code or document discovery by path/content terms.
- Graph neighborhood views for implementation cross-links.
- Read-only ad hoc inspection by maintainers.

### New OKF-facing contracts needed

Recommended library/service contracts:

- `LoadBundleRequest { root_path }`
- `ValidateBundleRequest { root_path, profile }`
- `BuildWikiProjectionRequest { root_path, incremental }`
- `GetConceptRequest { bundle_id, concept_id }`
- `GetDirectoryRequest { bundle_id, directory_path }`
- `SearchConceptsRequest { bundle_id, query, filters }`
- `GetDiagnosticsRequest { bundle_id, severity? }`
- `GetRecentChangesRequest { bundle_id, scope? }`

### Contract boundary recommendation

- Do not overload `OperationRequest::Search` or `OperationRequest::Context`
  with OKF semantics.
- Either:
  1. create a parallel `OkfWikiApi`, or
  2. add a separate top-level product surface after the OKF model stabilizes.

Option 1 is lower risk for the first implementation.

## Security, Concurrency, and Operational Considerations

### Security

Confirmed constraints:

- Current remote HTTP graph transport is intentionally minimal and not a
  production multi-user security model (`README.md:62-70`,
  `SECURITY.md:21-31`).

Recommendations for the wiki:

- Parse YAML with safe loaders only.
- Sanitize rendered markdown HTML.
- Treat all links and `resource` URIs as untrusted.
- If browser-served, add explicit authn/authz and do not rely on current MCP
  HTTP assumptions.

### Concurrency and refresh

Confirmed constraints:

- The graph system already has refresh/build state and locking concerns
  (`README.md:93-105`, `src/cli/mcp/tools.rs:141-153`).

Recommendations:

- Make OKF projection refresh independent from graph DB locking.
- If both systems watch the same repository, use one debounced source event
  stream feeding two consumers instead of two unrelated recursive watchers.
- Fail independently: a graph refresh failure should not make the OKF wiki
  unreadable, and an OKF parse failure should not stop code graph reads.

## Phased Implementation Plan

### Phase 0: Specification alignment

- Freeze the OKF v0.1 requirements set used for implementation.
- Encode the special root `index.md` frontmatter exception for `okf_version`.
- Write fixtures that cover concept docs, `index.md`, `log.md`, broken links,
  citations, and unknown extension keys.

Exit criteria:

- Requirement mapping is complete and test fixtures reflect every normative OKF
  rule used by the implementation.

### Phase 1: OKF core parser and model

- Implement bundle scanning.
- Parse YAML frontmatter and markdown body separately.
- Derive concept IDs from bundle-relative paths.
- Preserve unknown frontmatter keys.
- Distinguish concept docs from `index.md` and `log.md`.

Exit criteria:

- Bundle loading produces normalized concepts/directories/logs with diagnostics.

### Phase 2: Link, citation, and diagnostics layer

- Extract internal markdown links and fragments.
- Resolve absolute and relative links.
- Record broken links without rejecting the bundle.
- Extract citation sections and scoped log entries.

Exit criteria:

- The model supports backlinks, unresolved-link diagnostics, and changes views.

### Phase 3: Wiki API and projection

- Build concept/directory/search endpoints or library methods.
- Add concept-aware search ranking.
- Provide machine-readable diagnostics and recent-changes outputs.

Exit criteria:

- A consumer can request a concept page or directory listing without directly
  reading files.

### Phase 4: Adjacent composition with `codebaseGraph`

- Add optional “related code”, “implementation neighborhood”, or “architecture
  query” panels powered by `CodebaseGraphApi`.
- Keep this integration optional and additive.

Exit criteria:

- The wiki can show implementation context without changing OKF page semantics.

### Phase 5: Delivery and hardening

- Add browser UI or static rendering.
- Add authn/authz if remote-serving is required.
- Add watch/refresh integration and deployment packaging.

Exit criteria:

- The service is operationally independent, testable, and deployable.

## Testing and Verification Plan

### Unit tests

- Frontmatter parsing and extension preservation.
- Concept ID derivation.
- Reserved-file classification.
- Absolute and relative link resolution.
- Root `index.md` version exception handling.
- Citation and `log.md` parsing.

### Integration tests

- End-to-end bundle load and page projection.
- Broken-link diagnostics without hard failure.
- Search ranking over titles, concept IDs, and tags.
- Backlink recomputation on file changes.

### Composition tests

- `k_wiki` uses `CodebaseGraphApi` only for optional implementation context.
- A failing graph query does not prevent concept rendering.
- A malformed OKF bundle does not corrupt `.codebaseGraph` state.

### Non-functional verification

- Incremental rebuild correctness.
- HTML sanitization and link-scheme safety.
- Watch/refresh coalescing behavior.
- Concurrency tests if remote service mode is introduced.

## Acceptance Criteria

1. Every non-reserved markdown file becomes exactly one OKF concept with the
   correct path-derived concept ID.
2. `index.md` and `log.md` are never treated as normal concepts.
3. Bundle-root `index.md` may declare `okf_version` without breaking index
   semantics.
4. Unknown frontmatter keys survive parse and serialization.
5. Missing optional OKF fields do not block best-effort consumption.
6. Internal links produce backlinks and diagnostics for unresolved targets.
7. Search supports concept-aware ranking and filters.
8. `CodebaseGraphApi` integration remains optional and additive.
9. Existing `codebaseGraph` public API behavior remains backward compatible.
10. No change to `.codebaseGraph` schema or current MCP tool names is required
    for the first OKF wiki release.

## Rollout and Compatibility Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Overloading current documentation nodes with OKF semantics | Breaks existing graph consumers and search expectations | Keep OKF model adjacent and separate |
| Reusing current search ranking for wiki navigation | Poor concept discovery quality | Build dedicated OKF search index |
| Sharing transport/security assumptions with current MCP HTTP mode | Weak remote security posture | Give the wiki its own service boundary and auth model |
| Coupling OKF state to `.codebaseGraph` manifest/DB | Hard-to-reason upgrades and migration risk | Use separate projection state root |
| Dual watchers over the same repo | Duplicate work and race conditions | Centralize source change detection or coordinate refresh |

## Assumptions and Open Questions

Assumptions:

- The attached OKF v0.1 draft is the governing format for the first wiki
  implementation.
- The wiki service is intended to exist alongside `codebaseGraph`, not replace
  it.
- Backward compatibility of the current `codebaseGraph` public API matters more
  than collapsing everything into one surface immediately.

Open questions:

1. Should the first release be a library-only projection, a local HTTP service,
   or static artifact generation?
2. Does the wiki need authoring/editing in scope, or is read-only sufficient for
   the first release?
3. Should OKF projection state be persisted as JSON artifacts, SQLite, or a new
   LadyBugDB graph import?
4. Does search need to span only OKF concepts, or unified OKF plus code graph
   results?
5. Are OKF bundles expected to live inside the same repository as code, or in
   separate repositories that the wiki aggregates?

## Bottom Line

Confirmed current state: this repository already has a strong public API and a
working incremental graph engine, but its Markdown support is intentionally too
generic to serve as an OKF consumer out of the box.

Recommendation: implement OKF semantics in a dedicated adjacent subsystem,
compose `CodebaseGraphApi` for code-graph retrieval, and keep the initial OKF
wiki release independent from existing graph schema changes.
