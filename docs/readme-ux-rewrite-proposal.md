# README UX Rewrite Proposal

Status: approved and implemented in `README.md` on 2026-08-24.

## Outcome

Rewrite the README around the reader's next task: understand the product, get a
working graph, run a first query, connect an MCP client, and recover from common
failures. Keep `codebaseGraph` as the primary journey and present `k-wiki` as an
optional, separate subsystem.

The proposed rewrite keeps the current commands and safety boundaries, removes
duplicated detail where a maintained guide already exists, and corrects the
obsolete flat graph-state layout shown in the current README.

## UX audit

### P0 — the primary journey starts too late

The opening introduces `k-wiki` at lines 9–12, then devotes lines 14–83 to its
setup and registration before the `codebaseGraph` quick start at line 85. A new
reader must understand a secondary subsystem before reaching the repository's
main activation path.

**Principles:** task orientation, progressive disclosure, aesthetic and
minimalist design.

**Proposal:** put the three-command graph quick start immediately after the
value proposition. Move `k-wiki` below the core graph workflows and label it as
optional.

### P0 — quick start stops before first value

The current quick start installs and checks the daemon, but does not show a
search or explain the success signal. Readers can complete setup without knowing
how to prove that the graph is useful.

**Principles:** visibility of system status, recognition rather than recall.

**Proposal:** add `check-health`, name the expected `health ok=true` signal, and
show one editable `codebase-search` example.

### P1 — two products are presented as one flow

`codebase-graph` and `k-wiki` have different purposes, state roots, refresh
models, and MCP registration behavior. The current order makes their
relationship harder to understand than the architecture requires.

**Principles:** match between the system and the user's mental model,
consistency and standards.

**Proposal:** use a compact comparison table and consistently distinguish the
product name (`codebaseGraph`), graph binary (`codebase-graph`), and wiki binary
(`k-wiki`).

### P1 — operational guardrails are buried in dense prose

The `k-wiki` registration rules at lines 45–72 and the remote HTTP warning at
lines 170–172 are important, but difficult to find while scanning. Migration,
naming, transport, and threat-boundary information compete with routine setup.

**Principles:** error prevention, help users recognize and recover from errors,
progressive disclosure.

**Proposal:** keep safety warnings beside the command they constrain. Summarize
advanced `k-wiki` behavior and link to `docs/k-wiki.md`, where those rules are
already maintained in detail.

### P1 — the documented state layout is stale

Lines 99–106 describe `manifest.json` and `<repositoryName>_graph.ldb` directly
beneath `.codebaseGraph/`. Current install tests assert schema-v3 configuration
without static database or manifest paths, and runtime health reports
`managed_v2` generations beneath `.codebaseGraph/storage/`.

**Principles:** trustworthy feedback, consistency with the real system.

**Proposal:** describe only the stable, user-relevant state roots. Do not expose
generation filenames as a user contract.

### P2 — navigation and recovery are feature-oriented

Headings such as “MCP Install,” “MCP Usage,” and “CLI Workflow” describe product
areas instead of reader goals. Troubleshooting appears only after all setup,
usage, development, and release content.

**Principles:** information scent, recognition rather than recall, user control.

**Proposal:** use verb-led headings, add a “Choose a workflow” table, and format
troubleshooting as symptom/action pairs.

## Audiences and top tasks

| Audience | Top tasks | Success signal |
| --- | --- | --- |
| First-time user | Install, initialize, verify, run a first query | Health is OK and search returns repository matches |
| MCP user | Register a client and confirm the shared local daemon | Client connects to the repository's loopback MCP endpoint |
| CLI user | Search, fetch context, inspect schema, and run bounded read-only queries | Commands return compact block output or requested JSON |
| Wiki user | Initialize curated knowledge and register the wiki MCP server | `knowledge/` is source-controlled and `.kwiki/` is generated |
| Maintainer | Develop, verify, release, and recover state | Repository checks pass and recovery guidance is easy to find |

## Proposed information architecture

1. Value proposition and two-tool distinction
2. Quick start with an observable success signal
3. Workflow selector
4. Core graph tasks: query, connect, refresh
5. Optional `k-wiki` workflow
6. Development, release, and security
7. Symptom-led troubleshooting

This order follows progressive disclosure: first value, routine operation,
optional capability, then maintainer and recovery detail.

## Content rules

- Lead sections with outcomes and actions, not internal implementation terms.
- Use one term for each concept and distinguish product names from binary names.
- Keep one primary action per code block where sequencing is not required.
- Place expected results immediately after setup commands.
- Put security constraints next to transport commands.
- Prefer short tables for repeated task-to-command and symptom-to-action mappings.
- Use descriptive link text; do not rely on color, badges, or icons for meaning.
- Preserve a valid heading hierarchy and plain-language link destinations.
- Link to maintained detail instead of duplicating long option catalogs.
- Treat generated paths as implementation detail unless users must act on them.

## Content migration map

| Current content | Proposed destination |
| --- | --- |
| Product overview, lines 3–12 | Short opening plus “Understand the two tools” |
| `k-wiki` setup, lines 14–83 | “Add curated knowledge with k-wiki (optional)” plus `docs/k-wiki.md` |
| Quick start and install effects, lines 85–115 | “Quick start” and “What setup changes” |
| MCP installation and usage, lines 117–183 | “Connect an MCP client” with adjacent transport warning |
| CLI workflow, lines 184–207 | “Use the graph” task table and refresh note |
| Development and release, lines 209–230 | “Develop and verify” and “Release and security” |
| Troubleshooting, lines 232–241 | Symptom/action table at the end, linked near the top |

## Proposed README

The following approved candidate is retained as the review record. `README.md`
matches this content.

~~~markdown
# codebaseGraph

`codebaseGraph` turns a local source repository into a searchable code graph for
AI coding agents. It indexes Python, Rust, Go, C, C++, Fortran, Markdown, and
MDX, then exposes compact context, schema information, query helpers, and
bounded read-only graph queries through a native CLI and MCP server.

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

Setup installs the user service through launchd on macOS, a systemd user unit on
Linux, or Task Scheduler on Windows. A repository lock prevents a second daemon
from starting.

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
| Daemon service unavailable | Ensure launchd, the systemd user manager, or Task Scheduler is available, then rerun setup. Setup fails closed instead of silently creating stdio registrations. |
| Broken setup state | Run `codebase-graph reinstall` to recreate `.codebaseGraph/` and refresh the selected registration. |
| Broken client configuration only | Run `codebase-graph mcp install --client <client> --verify`. |
| Binary not found | Ensure the native `codebase-graph` binary is on `PATH`. |
| Expected file is missing from the graph | Check `.gitignore`, `.codebaseGraphignore`, configured include/exclude rules, and whether the path is binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, or `.codebaseGraph`. |
| Repository lock error | Stop other graph build, install, or daemon processes using the same repository state, then retry. |
~~~

## Acceptance criteria

- The primary graph quick start appears before optional `k-wiki` setup.
- A first-time user reaches an observable success signal and first search without
  following another document.
- The graph product, graph binary, wiki binary, curated source, and generated
  state use distinct, consistent terms.
- Every heading communicates a user goal or decision.
- Security constraints appear beside the affected transport commands.
- Troubleshooting uses symptom/action language and contains no obsolete flat
  database path.
- All links are descriptive, relative, and valid from the repository root.
- Heading levels are sequential; tables have headers; instructions do not rely
  on color or icons.
- Every proposed command is checked against current CLI behavior as part of the
  implementation.
- `README.md` was not modified before the proposal was approved.

## Verification notes

After replacing `README.md` with the approved candidate, run the repository's
Markdown/link checks and exercise the quick-start, daemon-status, search, and
registration dry-run commands. Keep deeper `k-wiki` registration behavior in
`docs/k-wiki.md` to prevent two long copies from drifting.
