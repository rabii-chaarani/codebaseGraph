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

`--dry-run` returns the raw server descriptor plus the exact client patch or payload without writing repository graph state, instruction files, or MCP client files.

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

The HTTP transport rejects non-local bind hosts unless `--allow-remote` is passed. Keep it bound to `127.0.0.1`
for normal use. Remote binding requires a bearer token:

```bash
CODEBASE_GRAPH_MCP_TOKEN="$(openssl rand -hex 32)"
codebase-graph mcp http --config .codebaseGraph/config.json --host 0.0.0.0 --allow-remote --auth-token-env CODEBASE_GRAPH_MCP_TOKEN
```

Clients must send `Authorization: Bearer <token>`. The token gate does not add TLS, rate limiting, authorization scopes, or
a multi-user session model; put remote HTTP behind a trusted network boundary and TLS-terminating proxy.

Available MCP tools:

- `graph_health`
- `graph_search`
- `graph_context`
- `graph_schema`
- `graph_query_helpers`
- `graph_architecture_queries`
- `graph_query` with write-like statements blocked

`graph_query` returns at most 1,000 rows per call and fetches only one extra row to determine whether the result was
truncated. Add a narrower `MATCH` pattern or a query-side `LIMIT` for broader graph exploration.

For coding-task architecture orientation, call `graph_architecture_queries` first to fetch the grouped read-only Cypher catalog, then run selected statements with `graph_query`.

## Operational diagnostics

Runtime warning and error paths emit structured JSON events to stderr. Set `CODEBASE_GRAPH_LOG_LEVEL=INFO` to include
setup start/completion diagnostics; the default level is `WARNING`.

Examples of emitted events include:

- `setup.failed`
- `mcp.tool_error`
- `mcp.stdio_parse_error`
- `mcp.http_forbidden_origin`
- `materializer.lock_exists`
- `materializer.stale_lock_removed`

## CLI graph workflow

The CLI exposes the same graph workflow as the MCP tools, which is useful in clients that do not surface MCP tools directly:

```bash
codebase-graph graph-health --repo-root .
codebase-graph graph-search SampleService --repo-root . --no-refresh --detail slim --context-limit 1 --json
codebase-graph graph-context SampleService --repo-root . --profile definitions --no-refresh --detail slim --context-limit 2 --json
codebase-graph graph-schema
codebase-graph graph-query-helpers
codebase-graph graph-architecture-queries --group overview
codebase-graph graph-query "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1" --repo-root .
```

CLI JSON output is minified by default to reduce tokens. Add `--pretty` to JSON-producing commands when you want indented output. Retrieval commands support `--detail standard|slim`; `standard` keeps the full payload, while `slim` drops score diagnostics and duplicate or empty summary fields.

`graph-query` blocks write-like statements and should be used read-only. The older `search` and `context` commands remain available. Setup reports the explicit database and manifest paths to use with them when needed:

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

GitHub Actions runs pytest across Linux, macOS, and Windows for Python 3.10 through 3.14, plus ruff, supply-chain, and package-build validation. Supply-chain checks include dependency consistency, vulnerability advisory scanning, Dependabot update coverage, immutable GitHub Action pins, and CycloneDX SBOM generation. Built wheels and source distributions are smoke-tested with `setup`, `graph-health`, `graph-search`, and a stdio MCP handshake before release. Releases are managed by release-please, use tag-derived package versions, create GitHub Releases with distribution assets and SBOMs, and publish to PyPI through Trusted Publishing.

Run `python scripts/check_release_gate.py` for local release-gate checks. Use the `--production` confirmations documented in [docs/release.md](docs/release.md) before publishing.

Conda distribution uses the conda-forge staged-recipes path rather than direct Anaconda.org uploads. See [docs/release.md](docs/release.md) for the release workflow and conda-forge submission checklist.

## Security

Report suspected vulnerabilities privately. See [SECURITY.md](SECURITY.md) for supported versions, reporting expectations, and the local-first MCP security boundary.

## Troubleshooting

- Missing LadyBugDB: install a package build that includes `real_ladybug`; setup will fail before creating `.codebaseGraph`.
- Stale graph: rerun `codebase-graph setup --repo-root .` after material source or documentation changes.
- Broken Codex config: rerun `codebase-graph mcp install --client codex --verify`, then check `codex mcp list`.
- Broken Claude config: rerun `codebase-graph mcp install --client claude --scope user --verify` or `codebase-graph mcp install --client claude-project --verify`.
- Broken LM Studio, Hermes, OpenClaw, or generic config: run `codebase-graph mcp install --client <client> --dry-run --json` first, then inspect the emitted payload and target path.
- PATH or executable issues: run setup from the virtual environment that contains `codebase-graph`; the descriptor prefers that absolute executable path.
- Direct smoke test: run `codebase-graph mcp serve --config .codebaseGraph/config.json` and send MCP `initialize`, `tools/list`, and `tools/call` JSON-RPC messages over stdio.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock/contention errors: stop other graph materialization or setup processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`. Stale locks with dead writer PIDs are removed automatically; if the error remains, inspect the `.ldb.lock` file before removing it manually.
