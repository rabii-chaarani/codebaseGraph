<!-- codebaseGraph:start -->
## codebaseGraph workflow
- Treat the `codebase_graph` MCP server knowledge graph as the project operating source of truth.
- Use the `codebase_graph` MCP server for repository graph search, schema, and compact context before answering repo-structure questions or performing coding tasks.
- Prefer `graph_search` for symbols, paths, docs, and setup instructions; follow with `graph_context` when relationships or nearby evidence matter.
- For coding tasks that requires architecture orientation, call `graph_architecture_queries` first, then execute selected statements through `graph_query` that relevant to your task.
- Use `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.
- Refresh the graph with `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph setup --repo-root . --mcp-client none` when files change materially; install or update MCP with `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph mcp install --client codex`. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.
<!-- codebaseGraph:end -->

## Git Commit Convention
- Strictly use Conventional Commits 1.0.0 for commit message. 
