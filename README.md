# codebaseGraph

`codebaseGraph` is a generic project/code knowledge graph engine for coding repositories. It materializes Python source, `AGENTS.md`, `CLAUDE.md`, Markdown, and MDX files into a LadyBugDB-backed graph, then exposes search, compact context, schema, query helpers, and a read-only MCP tool surface for coding agents.

LadyBugDB is a required runtime dependency. A normal production install must include `real_ladybug`; setup fails before creating repository state if the runtime cannot open a graph database.

## Production install

```bash
python -m pip install codebase-graph
```

From a repository root, run:

```bash
codebase-graph setup --repo-root .
```

Setup creates:

```text
.codebaseGraph/
  config.json
  manifest.json
  <repositoryName>_graph.ldb
```

For a repository named `my-service`, the database path is exactly `.codebaseGraph/my-service_graph.ldb`.

The setup command also:

- Materializes the repository graph into the repo-local database.
- Writes or updates one marked codebaseGraph block in `AGENTS.md` or `CLAUDE.md`.
- Writes a Codex or Claude-compatible MCP JSON config entry named `codebaseGraph`, unless skipped.

Useful options:

```bash
codebase-graph setup --repo-root /path/to/repo
codebase-graph setup --mcp-client claude
codebase-graph setup --mcp-config-path /tmp/mcp.json
codebase-graph setup --dry-run
codebase-graph setup --skip-mcp-config
codebase-graph setup --instructions-target claude
```

`--dry-run` returns the MCP config patch without writing the MCP client file. Repository graph state and instruction handling still run so the graph can be verified.

## MCP usage

Setup writes an MCP server entry equivalent to the block below. When setup is run from a virtual environment, the command may be the absolute path to that environment's `codebase-graph` executable so the MCP client can launch it without relying on shell `PATH`.

```json
{
  "mcpServers": {
    "codebaseGraph": {
      "command": "codebase-graph",
      "args": ["mcp", "serve", "--config", ".codebaseGraph/config.json"]
    }
  }
}
```

The server can also be run directly:

```bash
codebase-graph mcp serve --config .codebaseGraph/config.json
codebase-graph-mcp --config .codebaseGraph/config.json
```

Available MCP tools:

- `graph_health`
- `graph_search`
- `graph_context`
- `graph_schema`
- `graph_query_helpers`
- `graph_query` with write-like statements blocked

## CLI search

The legacy materializer/search commands are still available. Setup reports the explicit database and manifest paths to use with them:

```bash
codebase-graph search SampleService \
  --source-root . \
  --db .codebaseGraph/<repositoryName>_graph.ldb \
  --manifest .codebaseGraph/manifest.json
```

## Development install

```bash
python -m pip install -e .[dev]
```

Run checks:

```bash
python -m pytest
ruff check .
```

## Troubleshooting

- Missing LadyBugDB: install a package build that includes `real_ladybug`; setup will fail before creating `.codebaseGraph`.
- Stale graph: rerun `codebase-graph setup --repo-root .` after material source or documentation changes.
- Broken MCP config: rerun setup with `--mcp-config-path` pointing at the client JSON file, or use `--dry-run` to inspect the server block.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock/contention errors: stop other graph materialization or MCP processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`, then rerun setup.
