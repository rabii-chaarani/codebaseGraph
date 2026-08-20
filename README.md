# codebaseGraph

`codebaseGraph` is a local knowledge graph for AI coding agents. It builds a syntax-level searchable graph from
Python, Rust, Go, C, C++, Fortran, Markdown, and MDX files into a LadyBugDB-backed graph, then exposes search,
compact context, schema, query helpers, and read-only MCP tools.

The shipped CLI and MCP server are native Rust binaries.

The workspace also includes `k-wiki`, a local-first Open Knowledge Format
(OKF) 0.1 reader, compiler, searchable static wiki, preview server, and
controlled MCP authoring service. Its source and generated state remain
separate from the code graph; see [Knowledge Wiki](docs/k-wiki.md).

## Install k-wiki

Run the repository installer from its root to initialize managed wiki state:

```bash
k-wiki install
```

It creates a conformant starter source bundle at `knowledge/index.md` and the
generated-state layout under `.kwiki/`, independently of `.codebaseGraph/`:
`staging/`, `generations/`, `cache/`, and `site/`. Rerunning the command is
safe. It also maintains an MCP-only k-wiki workflow block in both `AGENTS.md`
and `CLAUDE.md`, preserving surrounding project instructions. Use `--repo-root
<directory>` to initialize another repository.

```bash
k-wiki install --repo-root /path/to/repository
k-wiki build /path/to/repository/knowledge --out /path/to/repository/.kwiki/site
```

The starter bundle is source-controlled Markdown, while `.kwiki/` is generated
state. For development from this checkout, use `cargo run -p k-wiki -- install`.

Register the bootstrapped wiki with an MCP client separately. This records a
stdio command for `k-wiki mcp /absolute/path/to/repository/knowledge`; it does
not start a persistent server process.

```bash
k-wiki mcp install --client codex
```

Supported clients are `codex`, `claude`, `claude-project`, `github-copilot`,
`lmstudio`, `hermes`, `openclaw`, `generic`, `copilot-studio`,
`microsoft-copilot`, and `all`. Use `--repo-root`, `--scope`, `--name`,
`--client-config-path`, or `--dry-run` as needed. Codex `local`/`project`,
Claude project, generic project, and GitHub Copilot targets are
`repository_local`; Codex user, Claude user/local, generic user/local, LM
Studio, Hermes, and OpenClaw targets are `shared`; Copilot Studio targets are
`manual`. For file-backed clients, an explicit config path is
`repository_local` only when it is inside the canonical repository root. Local
registrations keep `k_wiki`; shared and manual registrations derive
`k_wiki_<sanitized-repo>_<hash8>` from the
canonical path's SHA-256. Explicit names are preserved, except `k_wiki` is
rejected for shared/manual targets. Existing entries with different or
unparseable `command`/`args` fail closed. Legacy shared `k_wiki` entries are
removed when they target the same bundle; registrations for another valid
repository are renamed to that repository's deterministic shared name. The
installer preflights conflicts before writing, and failed cross-file cleanup
reports a partial migration without rolling back the local entry. `--dry-run`
reports registration and cleanup without writing, and
`--client all` resolves every client independently. Results include
`target_locality` and `legacy_cleanup`; manual targets include cleanup
instructions. The `k-wiki` executable must be on `PATH` when the client
launches it; set `K_WIKI_SERVER_COMMAND` before registration to record a
different executable path. The runtime remains single-bundle and
`wiki_list_bundles` does not accept caller-selected `repository_roots`. After
upgrading, replace both `codebase-graph` and `k-wiki` from the same release
archive, rerun `k-wiki mcp install --client codex --scope project --verify`
in each repository, and restart the MCP client.

Native release archives ship both binaries together plus `checksums.txt`,
`install.sh`, and `install.ps1`. Extract the archive and run the installer that
matches your platform to validate both binaries before replacing them in your
chosen bin directory.

Once registered, agents can use the `k_wiki` MCP server for the same wiki
maintenance operations without shelling out: `wiki_validate` (the equivalent
of `k-wiki validate`), `wiki_check_links` (the equivalent of `k-wiki
check-links`), and `wiki_build` (the equivalent of `k-wiki build`). The build
tool is marked as a write operation because it writes the static site.

## Quick Start

```bash
cargo install codebase-graph
codebase-graph install
codebase-graph mcp daemon status --config .codebaseGraph/config.json
```

For local development from this checkout:

```bash
cargo install --path . --bin codebase-graph
```

Install creates:

```text
.codebaseGraph/
  config.json
  manifest.json
  <repositoryName>_graph.ldb
```

The install command performs first-time setup: it materializes the initial graph, writes or updates one marked
codebaseGraph block in `AGENTS.md` or `CLAUDE.md`, and installs a Codex MCP client entry unless skipped. After setup,
the MCP server watches the repo and refreshes the graph automatically; rerunning `install` is not part of the refresh
workflow.

Use `codebase-graph reinstall` only when local setup state needs to be recreated. It moves aside the existing
`.codebaseGraph` state, runs install again, and refreshes the selected MCP registration without removing unrelated MCP
client entries.

## MCP Install

```bash
codebase-graph mcp install --client codex
codebase-graph mcp install --client all --mcp-transport http-daemon
```

Supported clients are `codex`, `claude`, `claude-project`, `github-copilot`, `lmstudio`, `hermes`, `openclaw`,
`generic`, `copilot-studio`, and `microsoft-copilot`.

`auto` is the default MCP transport and resolves to one repository-scoped Streamable HTTP daemon for every local
harness. Codex, Claude Code and `claude-project`, GitHub Copilot/VS Code, LM Studio, Hermes, OpenClaw, and generic
registrations all receive the same persisted `http://127.0.0.1:<port>/mcp` endpoint. Use
`--mcp-transport stdio` only for compatibility. `--mcp-daemon-port` overrides the stable repository-derived port.

`github-copilot` writes VS Code workspace configuration to `.vscode/mcp.json`. `claude` targets Claude Code;
`claude-project` targets the repository `.mcp.json`. An explicit Claude Desktop config rejects loopback HTTP and may
still be registered with explicit stdio. `copilot-studio` and `microsoft-copilot` report
`manual_remote_required`: their cloud runtimes require a publicly reachable HTTPS endpoint. The installer never
publishes the loopback URL or provisions a tunnel, TLS, OAuth, or remote deployment.

## MCP Usage

The managed daemon is the default transport for local MCP clients. Setup installs a user service through launchd on
macOS, a systemd user unit on Linux, or Task Scheduler on Windows. A repository lock prevents a second daemon from
starting, and every harness shares the daemon's coordinator and watcher.

```bash
codebase-graph mcp daemon status --config .codebaseGraph/config.json
codebase-graph mcp daemon start --config .codebaseGraph/config.json
codebase-graph mcp daemon stop --config .codebaseGraph/config.json
```

The daemon binds only to `127.0.0.1`, exposes MCP at `/mcp`, exposes local health metadata at
`/_codebasegraph/health`, and uses a rotating state-file token for authenticated shutdown. Reinstall and uninstall stop
and reap it before replacing or removing repository state. The lower-level transports remain available for diagnostics
and compatibility:

```bash
codebase-graph mcp start --config .codebaseGraph/config.json
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
codebase-graph check-health --repo-root .
codebase-graph codebase-context SampleService --repo-root . --profile definitions
codebase-graph graph-query "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1" --repo-root .
```

Retrieval commands emit block format by default for agent-facing output. Use `--json --pretty` or `--format json` for
structured inspection.

Freshness is automatic while the managed MCP daemon, explicit `mcp start`, or `watch` is running. Use `build` only for an
explicit manual rebuild, and use `plan` to inspect what a manual build would touch:

```bash
codebase-graph plan --repo-root . --json
codebase-graph plan --repo-root . --git-diff --git-base main --json
codebase-graph watch --repo-root . --debounce-ms 250
codebase-graph build --repo-root . --mode full --parallel --progress --json
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
- Stale graph: check `codebase-graph mcp daemon status --config .codebaseGraph/config.json`; use `codebase-graph watch --repo-root .` for an explicit foreground watcher or `build --mode full` only for a manual rebuild.
- Daemon service unavailable: setup fails closed instead of silently creating stdio registrations. Ensure launchd, the systemd user manager, or Task Scheduler is available, then rerun setup.
- Broken setup state: run `codebase-graph reinstall` to recreate `.codebaseGraph` and refresh the selected MCP registration.
- Broken client config only: rerun `codebase-graph mcp install --client <client> --verify`.
- PATH or executable issues: ensure the native `codebase-graph` binary is on `PATH`.
- Unsupported files: binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, and `.codebaseGraph` paths are skipped.
- Lock errors: stop other graph build or install processes using the same `.codebaseGraph/<repositoryName>_graph.ldb`.
