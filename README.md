# codebaseGraph

`codebaseGraph` is a local knowledge graph for AI coding agents. It builds a syntax-level searchable graph from
Python, Rust, Go, C, C++, Fortran, Markdown, and MDX files into a LadyBugDB-backed graph, then exposes search,
compact context, schema, query helpers, and read-only MCP tools.

The shipped CLI and MCP server are native Rust binaries.

## Quick Start

```bash
cargo install codebase-graph
codebase-graph setup --repo-root .
codebase-graph graph-search SampleService --repo-root . --no-refresh
```

For local development from this checkout:

```bash
cargo install --path . --bin codebase-graph
```

Setup creates:

```text
.codebaseGraph/
  config.json
  manifest.json
  <repositoryName>_graph.ldb
```

The setup command materializes the graph, writes or updates one marked codebaseGraph block in `AGENTS.md` or
`CLAUDE.md`, and installs a Codex MCP client entry unless skipped.

## MCP Install

```bash
codebase-graph mcp install --client codex
```

Supported clients are `codex`, `claude`, `claude-project`, `github-copilot`, `lmstudio`, `hermes`, `openclaw`,
`generic`, `copilot-studio`, and `microsoft-copilot`.

`github-copilot` writes VS Code workspace configuration to `.vscode/mcp.json`. `copilot-studio` and
`microsoft-copilot` are metadata-only targets: they print stdio and local HTTP connection details for manual Copilot
Studio onboarding and do not provision a hosted connector, TLS, OAuth, or remote deployment.

## MCP Usage

Stdio is the default transport for local MCP clients:

```bash
codebase-graph mcp serve --config .codebaseGraph/config.json
```

HTTP is available for local endpoint clients:

```bash
codebase-graph mcp http --config .codebaseGraph/config.json --host 127.0.0.1 --port 8765
```

Keep HTTP bound to `127.0.0.1` for normal use. Remote binding requires `--allow-remote` and a bearer token, but does
not provide TLS, rate limiting, authorization scopes, or a multi-user security model. HTTP clients must initialize first
and send the returned `Mcp-Session-Id` header on later requests.

Available MCP tools:

- `graph_health`
- `graph_search`
- `graph_context`
- `graph_schema`
- `graph_query_helpers`
- `graph_architecture_queries`
- `graph_query` with write-like statements blocked

## CLI Workflow

```bash
codebase-graph graph-health --repo-root .
codebase-graph graph-context SampleService --repo-root . --profile definitions
codebase-graph graph-query "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1" --repo-root .
```

Retrieval commands emit block format by default for agent-facing output. Use `--json --pretty` or `--format json` for
structured inspection.

Freshness commands use the same manifest hashing as setup/materialize, with Git as an optional file-selection layer:

```bash
codebase-graph plan --repo-root . --json
codebase-graph plan --repo-root . --git-diff --git-base main --json
codebase-graph watch --repo-root . --debounce-ms 250
codebase-graph materialize --repo-root . --parallel --progress --json
```

Use `.codebaseGraphignore`, `--include`, `--exclude`, or `.codebaseGraph/config.json` materialization include/exclude
arrays to tune scanned paths. Git discovery respects `.gitignore` by default and falls back to filesystem scanning when
Git is unavailable.

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked --release --bin codebase-graph
cargo run -p xtask -- release-gate --production \
  --confirm release-environment \
  --confirm hosted-ci-green \
  --confirm private-vulnerability-reporting
cargo run -p xtask -- smoke-artifact target/release/codebase-graph
cargo publish --dry-run --locked
```

## Release and Security

CI runs Rust formatting, linting, tests, advisory scanning, package dry-run checks, native package builds, and artifact
smoke tests. See [docs/release.md](docs/release.md) for the full release process and conda-forge checklist.

Report suspected vulnerabilities privately. See [SECURITY.md](SECURITY.md) for supported versions, reporting
expectations, and the local-first MCP security boundary.

## Troubleshooting

- Missing LadyBugDB: install `codebase-graph` from crates.io, a release artifact, or this checkout.
- Stale graph: rerun `codebase-graph setup --repo-root .` after material source or documentation changes.
- Broken client config: rerun `codebase-graph mcp install --client <client> --verify`.
- PATH or executable issues: ensure the native `codebase-graph` binary is on `PATH`.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock errors: stop other graph materialization or setup processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`.
