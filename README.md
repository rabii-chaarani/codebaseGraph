# codebaseGraph

`codebaseGraph` turns a local source repository into a searchable code graph for
AI coding agents. It indexes Python, Rust, Go, C, C++, Fortran, CSS, JavaScript,
JSX, TypeScript, TSX, Markdown, and MDX, then exposes compact context, schema information, query
helpers, and bounded read-only graph queries through a native CLI and MCP server.

This workspace also ships `k-wiki`, an optional subsystem for curated repository
knowledge. The graph and wiki have separate source and generated state.

## Quick start

For a crates.io install, you need Rust 1.82 or newer and Cargo. Run these commands
from the repository you want to index:

```bash
cargo install codebase-graph
codebase-graph install
codebase-graph check-health --repo-root .
```

Setup is healthy when the first output line includes `health ok=true`. Try a
search by replacing the example text with a symbol or concept from your
repository:

```bash
codebase-graph codebase-search "your search term" --repo-root .
```

For development from this checkout:

```bash
cargo install --path . --bin codebase-graph
```

### What setup changes

`codebase-graph install`:

- materializes the first graph;
- creates repository-local configuration and managed graph state under
  `.codebaseGraph/`;
- writes or updates one marked `codebaseGraph` block in `AGENTS.md` or
  `CLAUDE.md`; and
- installs a Codex MCP client entry unless you skip registration.

The managed MCP service refreshes the graph automatically. Do not rerun
`install` to refresh it. Use `codebase-graph reinstall` only when setup state
must be recreated; reinstall preserves unrelated MCP client entries.

## Choose a workflow

| Goal | Start here |
| --- | --- |
| Search or inspect the graph | [Use the graph](#use-the-graph) |
| Connect an AI coding client | [Connect an MCP client](#connect-an-mcp-client) |
| Add curated repository knowledge | [Add curated knowledge with k-wiki](#add-curated-knowledge-with-k-wiki-optional) |
| Work on this repository | [Develop and verify](#develop-and-verify) |
| Recover a broken setup | [Troubleshoot](#troubleshoot) |

## Understand the two tools

| Tool | Purpose | Authored source | Generated state |
| --- | --- | --- | --- |
| `codebase-graph` | Builds and queries the source-code graph | Your repository source | `.codebaseGraph/` |
| `k-wiki` | Builds and publishes curated OKF knowledge | `knowledge/` | `.kwiki/` |

`knowledge/` is curated intent. `.kwiki/` is disposable wiki projection state,
and `.codebaseGraph/` is graph runtime state. Do not edit either generated root
as source.

## Use the graph

| Task | Command |
| --- | --- |
| Check graph health | `codebase-graph check-health --repo-root .` |
| Search for a symbol or concept | `codebase-graph codebase-search "SampleService" --repo-root .` |
| Fetch compact context | `codebase-graph codebase-context SampleService --repo-root . --profile definitions` |
| Inspect a manual rebuild | `codebase-graph plan --repo-root . --json` |
| Inspect a Git diff rebuild | `codebase-graph plan --repo-root . --git-diff --git-base main --json` |
| Start an explicit foreground watcher | `codebase-graph watch --repo-root . --debounce-ms 250` |
| Run an explicit full rebuild | `codebase-graph build --repo-root . --mode full --parallel --progress --json` |

Retrieval commands emit compact block output by default. Use `--json --pretty`
or `--format json` when you need structured output.

Freshness is automatic while the managed MCP daemon, explicit `mcp start`, or
`watch` is running. `build` is for an explicit manual rebuild; use `plan` first
when you want to see what it would touch.

To tune discovery, use `.codebaseGraphignore`, `--include`, `--exclude`, or the
materialization include/exclude arrays in `.codebaseGraph/config.json`. Git
discovery respects `.gitignore` by default and falls back to filesystem scanning
when Git is unavailable.

### Run a read-only graph query

```bash
codebase-graph graph-query \
  "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1" \
  --repo-root .
```

Write-like graph statements are blocked.

## Connect an MCP client

Setup registers Codex by default. To add or refresh registrations explicitly:

```bash
codebase-graph mcp install --client codex
codebase-graph mcp install --client all --mcp-transport http-daemon
```

Supported clients are `codex`, `claude`, `claude-project`, `github-copilot`,
`lmstudio`, `hermes`, `openclaw`, `generic`, `copilot-studio`, and
`microsoft-copilot`.

For local clients, the default `auto` transport resolves to one
repository-scoped Streamable HTTP daemon. Local harnesses share the persisted
`http://127.0.0.1:<port>/mcp` endpoint, coordinator, and watcher. Use
`--mcp-transport stdio` only for compatibility, or `--mcp-daemon-port` to
override the stable repository-derived port.

Check or control the daemon with:

```bash
codebase-graph mcp daemon status --config .codebaseGraph/config.json
codebase-graph mcp daemon start --config .codebaseGraph/config.json
codebase-graph mcp daemon stop --config .codebaseGraph/config.json
```

`status` keeps its original top-level fields and also reports the service
manager state, running/controller versions, manifest drift, the latest bounded
failure from `.codebaseGraph/mcp-daemon-failure.json`, and a directly executable
recommended action. Rerunning `start` repairs an inactive service and reconciles
stale daemon versions or supervisor manifests without changing the MCP URL.

Setup installs the user service through launchd on macOS, a systemd user unit on
Linux, or Task Scheduler on Windows. A repository lock prevents a second daemon
from starting. On macOS, launchd owns the configured loopback listener and
starts or restarts the daemon when a client connects, including in
on-demand-only user sessions.

### Client-specific behavior

- `github-copilot` writes workspace configuration to `.vscode/mcp.json`.
- `claude` targets Claude Code; `claude-project` targets the repository
  `.mcp.json`.
- An explicit Claude Desktop configuration rejects loopback HTTP, but may be
  registered with explicit stdio.
- `copilot-studio` and `microsoft-copilot` report `manual_remote_required`
  because their cloud runtimes require a public HTTPS endpoint.

The installer does not publish a loopback URL or provision tunnels, TLS, OAuth,
or remote deployment.

### Use a lower-level transport

The managed daemon is the normal local path. Stdio and direct HTTP remain
available for diagnostics and compatibility:

```bash
codebase-graph mcp start --config .codebaseGraph/config.json
codebase-graph mcp http \
  --config .codebaseGraph/config.json \
  --host 127.0.0.1 \
  --port 8765
```

> **Security:** keep HTTP bound to `127.0.0.1` for normal use. Remote binding
> requires `--allow-remote` and a bearer token, but does not provide TLS, rate
> limiting, authorization scopes, or a multi-user security model. HTTP clients
> must initialize first and send the returned `Mcp-Session-Id` header on later
> requests.

The daemon exposes MCP at `/mcp`, local health metadata at
`/_codebasegraph/health`, and authenticated shutdown using a rotating state-file
token.

### Available graph tools

| Tool | Purpose |
| --- | --- |
| `graph_health` | Check database and manifest health |
| `graph_search` | Find graph entities with compact context |
| `graph_context` | Retrieve definitions, dependencies, call graphs, docs, runtime, or change impact |
| `graph_schema` | Inspect the ontology and indexes |
| `graph_query_helpers` | Discover named query helpers |
| `graph_architecture_queries` | Discover architecture-oriented queries |
| `graph_query` | Execute one bounded, read-only graph statement |

### Refresh a registration after upgrading

Upgrading the binary does not rewrite an existing client registration. If a
client still invokes `codebase-graph mcp start`, rerun the installer for that
registration, then restart the client:

```bash
codebase-graph mcp install --client codex --scope local \
  --config-path .codebaseGraph/config.json \
  --mcp-transport http-daemon \
  --verify
```

## Add curated knowledge with k-wiki (optional)

Use `k-wiki` when your repository needs curated, searchable knowledge alongside
the generated code graph.

```bash
k-wiki install
k-wiki mcp install --client codex
```

`k-wiki install` creates a starter source bundle at `knowledge/index.md` and
generated state beneath `.kwiki/`. Rerunning it is safe. It also maintains the
MCP-only `k-wiki` workflow block in `AGENTS.md` and `CLAUDE.md` while preserving
surrounding instructions.

Wiki MCP registration records a stdio command; it does not start a persistent
server process, so `k-wiki` must be on `PATH` when the client starts it.

See the [Knowledge Wiki guide](docs/k-wiki.md) for other repository roots, static
site builds, supported clients, registration locality and naming, validation,
authoring, and upgrade steps.

## Develop and verify

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked --release --bin codebase-graph
cargo publish --dry-run --locked
```

Release maintainers also use the policy and packaged-artifact checks:

```bash
cargo run -p xtask -- release-gate --production \
  --confirm release-environment \
  --confirm hosted-ci-green \
  --confirm private-vulnerability-reporting
cargo run -p xtask -- smoke-artifact target/release/codebase-graph
```

## Release and security

CI runs formatting, linting, tests, advisory scanning, package dry-run checks,
native package builds, and artifact smoke tests. See the
[release process](docs/release.md) for the full workflow and conda-forge
checklist.

Report suspected vulnerabilities privately. See the
[security policy](SECURITY.md) for supported versions, reporting expectations,
and the local-first MCP security boundary.

## Troubleshoot

| Symptom | Action |
| --- | --- |
| Missing LadyBugDB | Install `codebase-graph` from crates.io, a release archive, or this checkout. |
| Stale graph | Run `codebase-graph mcp daemon status --config .codebaseGraph/config.json`. Use `watch` for an explicit foreground watcher or `build --mode full` for a manual rebuild. |
| MCP HTTP transport send error | Run `codebase-graph mcp daemon status --config .codebaseGraph/config.json`, inspect `service`, `latest_failure`, and `recommended_action`, then run the reported `start_daemon` command. |
| Daemon service unavailable | Ensure launchd, the systemd user manager, or Task Scheduler is available, then run `codebase-graph mcp daemon start --config .codebaseGraph/config.json`. Setup fails closed instead of silently creating stdio registrations. |
| Broken setup state | Run `codebase-graph reinstall` to recreate `.codebaseGraph/` and refresh the selected registration. |
| Broken client configuration only | Run `codebase-graph mcp install --client <client> --verify`. |
| Binary not found | Ensure the native `codebase-graph` binary is on `PATH`. |
| Expected file is missing from the graph | Check `.gitignore`, `.codebaseGraphignore`, configured include/exclude rules, and whether the path is binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, or `.codebaseGraph`. |
| Repository lock error | Stop other graph build, install, or daemon processes using the same repository state, then retry. |
