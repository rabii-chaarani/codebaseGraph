<!-- codebaseGraph:start -->
## codebaseGraph workflow
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth.
- Use `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-search <query> --repo-root . --no-refresh --json` before answering repo-structure questions or performing coding tasks.
- Use `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-context <query> --repo-root . --profile <profile> --no-refresh --json` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.
- For architecture orientation, run `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-architecture-queries`, then execute selected read-only statements with `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-query "<statement>" --repo-root .`.
- Use `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-schema` or `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph graph-query-helpers` before writing raw graph queries, and keep `graph-query` read-only.
- Refresh the graph with `/Users/rabii/Projects/Repositories/codebaseGraph/.venv/bin/codebase-graph setup --repo-root . --mcp-client none` when files change materially. Setup config: `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/config.json`.
<!-- codebaseGraph:end -->

## Git Commit Convention
- Strictly use Conventional Commits 1.0.0 for commit message. 
