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
- Installs an MCP client entry named `codebase_graph`, unless skipped.

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

## MCP installation

The user-facing installer is:

```bash
codebase-graph mcp install
```

By default this installs Codex with a repository-specific server name, for example `codebase_graph_my_service`. It builds the server descriptor from `.codebaseGraph/config.json`, uses the supported native client CLI when available, and falls back to the adapter file writer when the CLI is missing or fails.

Useful installer options:

```bash
codebase-graph mcp install --client codex
codebase-graph mcp install --client claude --scope user
codebase-graph mcp install --client claude-project
codebase-graph mcp install --client lmstudio
codebase-graph mcp install --client hermes
codebase-graph mcp install --client openclaw
codebase-graph mcp install --client generic
codebase-graph mcp install --client all --dry-run --json
codebase-graph mcp install --name codebase_graph
codebase-graph mcp install --config-path /path/to/.codebaseGraph/config.json
codebase-graph mcp install --verify
```

Native CLI installers are attempted first for Codex, Claude, Claude project scope, and OpenClaw:

```bash
codex mcp add <name> -- <command> <args...>
claude mcp add --transport stdio --scope <scope> <name> -- <command> <args...>
openclaw mcp set <name> '<json>'
```

If native installation is unavailable, codebaseGraph writes the client config file directly. `setup --mcp-client ...` remains supported and delegates to the same installer behavior after materializing graph state and updating instructions. The default MCP server name is `codebase_graph`, which avoids mixed-case tool namespace issues in clients that normalize or validate MCP labels strictly.

`--dry-run` reports the native command or emitted file patch without calling native CLIs or writing files. `--verify` runs a direct stdio MCP smoke test and, where available, asks the client CLI whether it can see the server.

## MCP usage

Setup and install build one canonical server descriptor and serialize it into the selected client format. When run from a virtual environment, the command may be the absolute path to that environment's `codebase-graph` executable so the MCP client can launch it without relying on shell `PATH`.

Codex uses `~/.codex/config.toml`:

```toml
[mcp_servers.codebase_graph]
command = "codebase-graph"
args = ["mcp", "serve", "--config", ".codebaseGraph/config.json"]
startup_timeout_sec = 60
```

Claude Desktop, Claude project config, LM Studio, and generic MCP JSON use an `mcpServers` shape:

```json
{
  "mcpServers": {
    "codebase_graph": {
      "type": "stdio",
      "command": "codebase-graph",
      "args": ["mcp", "serve", "--config", ".codebaseGraph/config.json"]
    }
  }
}
```

OpenClaw uses JSON5-compatible JSON under `mcp.servers`, and Hermes emits YAML under `mcp_servers` in `~/.hermes/config.yaml`. LM Studio reads `~/.lmstudio/mcp.json` and requires enabling "Allow calling servers from mcp.json" in the app. Use `codebase-graph mcp install --dry-run --client <client> --json` to inspect the exact emitted command or patch before installation.

Client examples:

```bash
codebase-graph mcp install --client codex
codebase-graph mcp install --client claude
codebase-graph mcp install --client claude-project
codebase-graph mcp install --client lmstudio
codebase-graph mcp install --client hermes
codebase-graph mcp install --client openclaw
codebase-graph mcp install --client generic --dry-run --json
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
- `graph_architecture_queries`
- `graph_query` with write-like statements blocked

For coding-task architecture orientation, call `graph_architecture_queries` first to fetch the grouped read-only Cypher catalog, then run selected statements with `graph_query`.

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

## CI and releases

GitHub Actions runs pytest across Linux, macOS, and Windows for Python 3.10 through 3.14, plus ruff and package-build validation. Releases are driven by `vX.Y.Z` tags, use tag-derived package versions, and publish to PyPI through Trusted Publishing.

Conda distribution uses the conda-forge staged-recipes path rather than direct Anaconda.org uploads. See [docs/release.md](docs/release.md) for the release workflow and conda-forge submission checklist.

## Troubleshooting

- Missing LadyBugDB: install a package build that includes `real_ladybug`; setup will fail before creating `.codebaseGraph`.
- Stale graph: rerun `codebase-graph setup --repo-root .` after material source or documentation changes.
- Broken Codex config: rerun `codebase-graph mcp install --client codex --verify`, then check `codex mcp list`.
- Broken Claude config: rerun `codebase-graph mcp install --client claude --scope user --verify` or `codebase-graph mcp install --client claude-project --verify`.
- Broken LM Studio, Hermes, OpenClaw, or generic config: run `codebase-graph mcp install --client <client> --dry-run --json` first, then inspect the emitted payload and target path.
- PATH or executable issues: run setup from the virtual environment that contains `codebase-graph`; the descriptor prefers that absolute executable path.
- Direct smoke test: run `codebase-graph mcp serve --config .codebaseGraph/config.json` and send MCP `initialize`, `tools/list`, and `tools/call` JSON-RPC messages over stdio.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock/contention errors: stop other graph materialization or MCP processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`, then rerun setup.
