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
- Writes a native MCP client config entry named `codebaseGraph`, unless skipped.

Useful options:

```bash
codebase-graph setup --repo-root /path/to/repo
codebase-graph setup --mcp-client codex
codebase-graph setup --mcp-client claude
codebase-graph setup --mcp-client claude-project
codebase-graph setup --mcp-client lmstudio
codebase-graph setup --mcp-client hermes
codebase-graph setup --mcp-client openclaw
codebase-graph setup --mcp-client generic
codebase-graph setup --mcp-config-path /tmp/client-config
codebase-graph setup --dry-run
codebase-graph setup --skip-mcp-config
codebase-graph setup --instructions-target claude
```

`--dry-run` returns the raw server descriptor plus the exact client patch or payload without writing the MCP client file. Repository graph state and instruction handling still run so the graph can be verified.

## MCP usage

Setup builds one canonical server descriptor and serializes it into the selected client format. When setup is run from a virtual environment, the command may be the absolute path to that environment's `codebase-graph` executable so the MCP client can launch it without relying on shell `PATH`.

Codex uses `~/.codex/config.toml`:

```toml
[mcp_servers.codebaseGraph]
command = "codebase-graph"
args = ["mcp", "serve", "--config", ".codebaseGraph/config.json"]
startup_timeout_sec = 60
```

Claude Desktop, Claude project config, LM Studio, and generic MCP JSON use an `mcpServers` shape:

```json
{
  "mcpServers": {
    "codebaseGraph": {
      "type": "stdio",
      "command": "codebase-graph",
      "args": ["mcp", "serve", "--config", ".codebaseGraph/config.json"]
    }
  }
}
```

OpenClaw uses JSON5-compatible JSON under `mcp.servers`, and Hermes emits YAML under `mcp_servers`. Use `--dry-run --mcp-client <client>` to inspect the exact emitted patch before writing a config file.

Client examples:

```bash
codebase-graph setup --repo-root . --mcp-client codex
codebase-graph setup --repo-root . --mcp-client claude
codebase-graph setup --repo-root . --mcp-client claude-project
codebase-graph setup --repo-root . --mcp-client lmstudio
codebase-graph setup --repo-root . --mcp-client hermes
codebase-graph setup --repo-root . --mcp-client openclaw
codebase-graph setup --repo-root . --mcp-client generic --dry-run
```

The server can also be run directly:

```bash
codebase-graph mcp serve --config .codebaseGraph/config.json
codebase-graph-mcp --config .codebaseGraph/config.json
```

Stdio is the default transport for local MCP clients. An optional local Streamable HTTP transport is available for clients that connect to an HTTP endpoint:

```bash
codebase-graph mcp http --config .codebaseGraph/config.json --host 127.0.0.1 --port 8765
```

Keep HTTP bound to `127.0.0.1` unless you have added authentication and understand the DNS rebinding risk for local MCP servers.

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
- Broken Codex config: rerun setup with `--mcp-client codex`, then check `codex mcp list`.
- Broken Claude config: rerun setup with `--mcp-client claude` for desktop config or `--mcp-client claude-project` for a repo-local `.mcp.json`.
- Broken LM Studio, Hermes, OpenClaw, or generic config: run setup with the matching `--mcp-client` and `--dry-run` first, then copy or write the emitted payload to the client path.
- PATH or executable issues: run setup from the virtual environment that contains `codebase-graph`; the descriptor prefers that absolute executable path.
- Direct smoke test: run `codebase-graph mcp serve --config .codebaseGraph/config.json` and send MCP `initialize`, `tools/list`, and `tools/call` JSON-RPC messages over stdio.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock/contention errors: stop other graph materialization or MCP processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`, then rerun setup.
