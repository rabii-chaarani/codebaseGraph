<!-- codebaseGraph:start -->
## codebaseGraph workflow
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.
- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.
- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: "json"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.
- Use MCP `graph_search` with `detail: "slim"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.
- Use MCP `graph_context` with `profile: "<profile>"`, `detail: "slim"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.
- If MCP tools are unavailable, fall back to CLI: `codebase-graph codebase-search <query> --repo-root . --no-refresh --detail slim --context-limit 1`, `codebase-graph codebase-context <query> --repo-root . --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph codebase-architecture-queries`, `codebase-graph graph-query "<statement>" --repo-root .`, `codebase-graph schema`, and `codebase-graph query-helpers`.
- Do not rerun install to refresh the graph. The MCP server started from this setup config watches the repo and refreshes automatically; use `codebase-graph build --repo-root . --mode full` only for explicit manual rebuilds. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.
<!-- codebaseGraph:end -->

## Git Commit Convention
- When you finish your coding task, strictly use Conventional Commits 1.0.0 for commit message and commit your changes.

<!-- k-wiki:start -->
## k-wiki workflow
- Use the `k_wiki` MCP server for every wiki interaction; do not invoke the `k-wiki` CLI or edit generated state directly.
- Treat `knowledge/` as curated repository intent, not a substitute for current code. Start with `wiki_list_bundles`, then `wiki_search_concepts`; use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks`, and `wiki_get_neighborhood` to understand related decisions.
- Use the wiki for architecture, terminology, invariants, ownership, and prior decisions. Verify changeable details with codebase-graph MCP tools. If code and wiki conflict, identify the conflict and use `wiki_populate_page` to record clarified intent.
- Create missing pages with `wiki_create_page`; update existing pages with `wiki_populate_page`, supplying title, type, tags, useful Markdown, and `expected_content_hash`. Record durable decisions, public contracts, runbooks, invariants, and non-obvious trade-offs—not transient implementation noise or copied source.
- After meaningful wiki edits, call `wiki_validate` with `profile: recommended` and `include_structured_content: true`, then `wiki_check_links`. Call `wiki_build` with the configured `bundle_root` and `.kwiki/site` output root; it is a write operation.
- `knowledge/` is source and `.kwiki/` is generated state. Never manually edit generated projections.
- Use `wiki_get_diagnostics` to inspect remaining issues and `wiki_get_recent_changes` to understand recent work. In handoffs, cite updated concept paths and summarize decisions, uncertainties, and validation results.
<!-- k-wiki:end -->
