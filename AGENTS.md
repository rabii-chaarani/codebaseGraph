<!-- codebaseGraph:start -->
## codebaseGraph workflow
- Use the `codebaseGraph` MCP server for repository graph search, schema, and compact context before answering repo-structure questions.
- Prefer `graph_search` for symbols, paths, docs, and setup instructions; follow with `graph_context` when relationships or nearby evidence matter.
- Use `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.
- Refresh the graph with `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph setup --repo-root .` when files change materially. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.
<!-- codebaseGraph:end -->
