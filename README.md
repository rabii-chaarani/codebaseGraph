# codebaseGraph

`codebaseGraph` builds a repo-local knowledge graph for coding agents. It materializes Python source, `AGENTS.md`,
`CLAUDE.md`, Markdown, and MDX files into a LadyBugDB-backed graph, then exposes search, compact context, schema, query
helpers, and read-only MCP tools.

Using `codebaseGraph` helps agents orient and reason faster, reduce guesswork, keep prompts focused, and make changes with better
impact awareness. Because the graph stores local source, documentation, spans, and relationships together, it gives
AI agents a compact evidence layer for safer edits, architecture review, dependency tracing, and onboarding while reducing token consumption and tool calling.

Requires Python 3.10+

## Quick start

```bash
python -m pip install cbasegraph
codebase-graph setup --repo-root .
codebase-graph graph-search SampleService --repo-root . --no-refresh
```

Setup creates:

```text
.codebaseGraph/
  config.json
  manifest.json
  <repositoryName>_graph.ldb
```

For a repository named `my-service`, the database path is `.codebaseGraph/my-service_graph.ldb`.

The setup command materializes the graph, writes or updates one marked codebaseGraph block in `AGENTS.md` or
`CLAUDE.md`, and installs a Codex MCP client entry unless skipped.

Useful setup options:

```bash
codebase-graph setup --repo-root /path/to/repo
codebase-graph setup --mcp-client claude
codebase-graph setup --mcp-client lmstudio
codebase-graph setup --skip-mcp-config
codebase-graph setup --instructions-target claude
codebase-graph setup --dry-run --pretty
```

## MCP install

```bash
codebase-graph mcp install --client codex
```

Supported clients are `codex`, `claude`, `claude-project`, `lmstudio`, `hermes`, `openclaw`, and `generic`.

Server naming:

- `codebase-graph setup` installs the default MCP server as `codebase_graph`.
- Standalone `codebase-graph mcp install` defaults to `codebase_graph_<repo>`.
- Use `--name codebase_graph` to override the standalone installer name.

The installer builds the server descriptor from `.codebaseGraph/config.json`, uses a supported native client CLI when
available, and falls back to writing the client config file directly. Use `--dry-run --json` to inspect the emitted
command or config patch before writing, and `--verify` to run a stdio smoke test after installation.

```bash
codebase-graph mcp install --client claude --scope user
codebase-graph mcp install --client claude-project
codebase-graph mcp install --client all --dry-run --json
codebase-graph mcp install --config-path /path/to/.codebaseGraph/config.json
codebase-graph mcp install --verify
```

## MCP usage

Stdio is the default transport for local MCP clients:

```bash
codebase-graph mcp serve --config .codebaseGraph/config.json
codebase-graph-mcp --config .codebaseGraph/config.json
```

HTTP is available for local endpoint clients:

```bash
codebase-graph mcp http --config .codebaseGraph/config.json --host 127.0.0.1 --port 8765
```

Keep HTTP bound to `127.0.0.1` for normal use. Remote binding requires `--allow-remote` and a bearer token, but does not
provide TLS, rate limiting, authorization scopes, or a multi-user security model. HTTP clients must initialize first and
send the returned `Mcp-Session-Id` header on later requests.

Available MCP tools:

- `graph_health`
- `graph_search`
- `graph_context`
- `graph_schema`
- `graph_query_helpers`
- `graph_architecture_queries`
- `graph_query` with write-like statements blocked

`graph_query` returns at most 1,000 rows per call. Add a narrower `MATCH` pattern or a query-side `LIMIT` for broader
graph exploration.

## CLI workflow

The CLI mirrors the MCP tools for clients that do not surface MCP directly:

```bash
codebase-graph graph-health --repo-root .
codebase-graph graph-context SampleService --repo-root . --profile definitions
codebase-graph graph-query "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1" --repo-root .
```

Retrieval commands emit block format by default for agent-facing output. Use `--json --pretty` or `--format json` for structured inspection. MCP callers can request the typed payload with `include_structured_content: true`.
Retrieval commands also support `--detail standard|slim`; `slim` drops score diagnostics and duplicate or empty summary fields.

For coding-task architecture orientation, call `graph_architecture_queries` first, then run selected statements with
`graph_query`.

## Development

```bash
python -m pip install -e .[dev]
python -m pytest
ruff check .
```

## Release and security

CI runs pytest across Linux, macOS, and Windows for Python 3.10 through 3.14, plus ruff, package-build checks,
supply-chain validation, and smoke tests. See [docs/release.md](docs/release.md) for the full release process and
conda-forge checklist.

Report suspected vulnerabilities privately. See [SECURITY.md](SECURITY.md) for supported versions, reporting
expectations, and the local-first MCP security boundary.

## Troubleshooting

- Missing LadyBugDB: install a package build that includes `real_ladybug`; setup fails before creating `.codebaseGraph`
  if the runtime cannot open a graph database.
- Stale graph: rerun `codebase-graph setup --repo-root .` after material source or documentation changes.
- Broken client config: rerun `codebase-graph mcp install --client <client> --verify`.
- PATH or executable issues: run setup from the virtual environment that contains `codebase-graph`; the descriptor
  prefers that absolute executable path.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are
  skipped.
- Lock errors: stop other graph materialization or setup processes using the same
  `.codebaseGraph/<repositoryName>_graph.ldb`. Stale locks with dead writer PIDs are removed automatically; if the error
  remains, inspect the `.ldb.lock` file before removing it manually.
- Diagnostics: set `CODEBASE_GRAPH_LOG_LEVEL=INFO` to include setup start/completion events on stderr.
