# MCP integration

This guide covers connecting `codebaseGraph` to an MCP client and operating the
local service that serves the graph. Start with the [README](../README.md) for
installation and the product overview. For symptom-led recovery, see
[Troubleshooting](troubleshooting.md).

## Connect a local MCP client

After running `codebase-graph install` in the repository you want to index,
Codex is registered by default. To add or refresh another client explicitly:

```bash
codebase-graph mcp install --client codex
codebase-graph mcp install --client all --mcp-transport http-daemon
```

The installer supports these clients:

`codex`, `claude`, `claude-project`, `github-copilot`, `lmstudio`, `hermes`,
`openclaw`, `generic`, `copilot-studio`, and `microsoft-copilot`.

The full command accepts `--scope local|user|project`, `--name`,
`--mcp-transport auto|stdio|http-daemon`, `--mcp-daemon-port`,
`--config-path`, `--client-config-path`, `--repo-root`, `--dry-run`, and
`--verify`. Use `--verify` when you want the installer to read back the
registration and check the transport and graph-tool schemas.

### Default local transport

For local clients, `auto` resolves to one repository-scoped Streamable HTTP
daemon. Every local harness for that repository shares the persisted
`http://127.0.0.1:<port>/mcp` endpoint, coordinator, and watcher. The port is
stable for the repository; pass `--mcp-daemon-port <port>` to override it.

This is the normal local setup. The installer writes the client registration
and installs the platform service; it does not create a public endpoint.

## Operate the managed daemon

Use the repository's setup configuration when checking or controlling its
daemon:

```bash
codebase-graph mcp daemon status --config .codebaseGraph/config.json
codebase-graph mcp daemon start --config .codebaseGraph/config.json
codebase-graph mcp daemon stop --config .codebaseGraph/config.json
```

`status` retains its original top-level fields and also reports service-manager
state, running and controller versions, manifest drift, the latest bounded
failure from `.codebaseGraph/mcp-daemon-failure.json`, and a directly
executable `recommended_action`. If a service is loaded but inactive, or its
binary or supervisor manifest is stale, rerunning `start` repairs it without
changing the MCP URL.

The service manager is selected by platform:

- macOS: a user `launchd` service owns the configured loopback listener and
  starts or restarts the daemon when a client connects, including in
  on-demand-only user sessions.
- Linux: a `systemd --user` unit manages the repository daemon.
- Windows: Task Scheduler manages the repository daemon.

A repository lock prevents a second daemon from starting for the same graph
state. If the service manager is unavailable, setup fails closed rather than
silently writing a stdio registration. Check the service state first, then run
the `start` command above.

## Client-specific behavior

- `github-copilot` writes workspace configuration to `.vscode/mcp.json`.
- `claude` targets Claude Code; `claude-project` targets the repository
  `.mcp.json`.
- Claude Desktop's explicit configuration rejects the managed loopback HTTP
  registration. Register it with `--mcp-transport stdio` instead, or target
  Claude Code.
- `copilot-studio` and `microsoft-copilot` return
  `manual_remote_required`: their cloud runtimes require a publicly reachable
  HTTPS endpoint.

For cloud clients, manual metadata is intentional. The installer does not
provide a tunnel, TLS termination, OAuth, authorization service, or hosted
deployment. Deploy and secure a public HTTPS endpoint yourself before
registering one of those clients. See the [security policy](../SECURITY.md)
for the local-first MCP security boundary.

## Use stdio or direct HTTP

The managed daemon is the normal local path. Use the lower-level transports for
compatibility, diagnostics, or clients that cannot use the managed HTTP
registration.

### Stdio

Register stdio explicitly:

```bash
codebase-graph mcp install \
  --client claude \
  --mcp-transport stdio \
  --config-path .codebaseGraph/config.json
```

To start the stdio server directly for a compatible harness, run:

```bash
codebase-graph mcp start --config .codebaseGraph/config.json
```

The registered command starts `codebase-graph mcp start` and communicates over
the process's stdin/stdout. `mcp start` is a blocking server; run it through
the `codebase-graph` binary rather than as a daemon-management subcommand.

### Direct HTTP

To run a blocking HTTP server yourself:

```bash
codebase-graph mcp http \
  --config .codebaseGraph/config.json \
  --host 127.0.0.1 \
  --port 8765
```

The HTTP endpoint path defaults to `/mcp`; use `--path <path>` only when the
client requires another path. HTTP clients must initialize first and send the
returned `Mcp-Session-Id` header on later requests.

## HTTP endpoints and security boundary

The managed daemon exposes:

- `/mcp` — Streamable HTTP MCP transport.
- `/_codebasegraph/health` — local health metadata.
- `/_codebasegraph/shutdown` — authenticated daemon shutdown using the rotating
  state-file control token.

Keep HTTP bound to `127.0.0.1` for normal use. A remote bind requires
`--allow-remote` and a bearer token. Set `CODEBASEGRAPH_MCP_TOKEN` in the server
process environment, then run:

```bash
codebase-graph mcp http \
  --config .codebaseGraph/config.json \
  --host 0.0.0.0 \
  --port 8765 \
  --allow-remote \
  --auth-token-env CODEBASEGRAPH_MCP_TOKEN
```

Remote binding does not provide TLS, rate limiting, authorization scopes, or a
multi-user security model. Treat the token and endpoint as sensitive and put a
properly secured HTTPS gateway in front of any intentionally remote deployment.

## Available graph tools

Once connected, an MCP client can call these read-oriented graph tools:

| Tool | Purpose |
| --- | --- |
| `graph_health` | Check database and manifest health. |
| `graph_search` | Find graph entities with compact context. |
| `graph_context` | Retrieve definitions, dependencies, call graphs, docs, runtime, or change impact. |
| `graph_schema` | Inspect the ontology and indexes. |
| `graph_query_helpers` | Discover named query helpers. |
| `graph_architecture_queries` | Discover architecture-oriented queries. |
| `graph_query` | Execute one bounded, read-only graph statement. |

`graph_query` rejects write-like graph statements. Use the schema and query
helper tools to discover the supported read model before composing a raw query.

## Refresh registration after an upgrade

Upgrading the binary does not rewrite an existing client registration. If a
client still invokes an old `codebase-graph mcp start` command, rerun the
installer for that registration and restart the client:

```bash
codebase-graph mcp install --client codex --scope local \
  --config-path .codebaseGraph/config.json \
  --mcp-transport http-daemon \
  --verify
```

Repeat for each client whose registration should use the new binary. If the
daemon itself is stale, `codebase-graph mcp daemon start --config
.codebaseGraph/config.json` reconciles the service manifest and runtime before
the client reconnects.
