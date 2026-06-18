<!-- codebaseGraph:start -->
## codebaseGraph workflow
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.
- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.
- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: "json"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.
- Use MCP `graph_search` with `detail: "slim"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.
- Use MCP `graph_context` with `profile: "<profile>"`, `detail: "slim"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.
- If MCP tools are unavailable, fall back to CLI: `codebase-graph graph-search <query> --repo-root . --no-refresh --detail slim --context-limit 1`, `codebase-graph graph-context <query> --repo-root . --profile <profile> --no-refresh --detail slim --context-limit 2`, `codebase-graph graph-architecture-queries`, `codebase-graph graph-query "<statement>" --repo-root .`, `codebase-graph graph-schema`, and `codebase-graph graph-query-helpers`.
- Refresh the graph with `codebase-graph setup --repo-root . --mcp-client none` when files change materially. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.
<!-- codebaseGraph:end -->

## Git Commit Convention
- When you finish your coding task, strictly use Conventional Commits 1.0.0 for commit message and commit your changes.
