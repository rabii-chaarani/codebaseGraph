# Troubleshooting

Use this guide when setup, graph freshness, or an MCP connection is not
working as expected. Start with the least invasive checks, then follow the
matching symptom below.

## Start with health and service status

Run these commands from the repository you want to inspect:

```bash
codebase-graph check-health --repo-root .
codebase-graph mcp daemon status --config .codebaseGraph/config.json
```

The first command checks the graph and manifest. The daemon status command is
the next check for MCP or freshness problems; it reports service state, the
latest bounded failure, and a recommended action when those details are
available.

If status recommends starting the daemon, run:

```bash
codebase-graph mcp daemon start --config .codebaseGraph/config.json
```

Retry the original command after the health check is successful. For MCP
transport, service-manager, or client-registration details, see the [MCP
guide](mcp.md). For first-time setup, see the [README onboarding
guide](../README.md). The local-first boundary and HTTP security limitations
are documented in the [security policy](../SECURITY.md).

## Fix the symptom

| Symptom | Action |
| --- | --- |
| Missing LadyBugDB | Install `codebase-graph` from crates.io, a release archive, or this checkout. |
| Stale graph | Run `codebase-graph mcp daemon status --config .codebaseGraph/config.json`. Use `watch` for an explicit foreground watcher or `build --mode full` for a manual rebuild. |
| MCP HTTP transport send error | Run `codebase-graph mcp daemon status --config .codebaseGraph/config.json`. Inspect `service`, `latest_failure`, and `recommended_action`, then run the reported `start_daemon` command. |
| Daemon service unavailable | Ensure launchd, the systemd user manager, or Task Scheduler is available, then run `codebase-graph mcp daemon start --config .codebaseGraph/config.json`. Setup fails closed instead of silently creating stdio registrations. |
| Broken setup state | Run `codebase-graph reinstall` to recreate `.codebaseGraph/` and refresh the selected registration. |
| Broken client configuration only | Run `codebase-graph mcp install --client <client> --verify`. |
| Binary not found | Ensure the native `codebase-graph` binary is on `PATH`. |
| Expected file is missing from the graph | Check `.gitignore`, `.codebaseGraphignore`, configured include/exclude rules, and whether the path is binary, vendor, cache, virtualenv, build, dist, `.codebase_graph`, or `.codebaseGraph`. |
| Repository lock error | Stop other graph build, install, or daemon processes using the same repository state, then retry. |

## Refresh versus reinstall

The managed MCP service refreshes the graph automatically while it is running.
Do not rerun `install` just to refresh a graph. Use an explicit foreground
watcher or a manual full rebuild when needed:

```bash
codebase-graph watch --repo-root . --debounce-ms 250
codebase-graph build --repo-root . --mode full --parallel --progress --json
```

Use `codebase-graph reinstall` only when setup state must be recreated. It
recreates `.codebaseGraph/` and refreshes the selected registration; it is not
the ordinary graph-refresh command.

## If the first checks do not resolve the issue

Keep the output from `check-health` and daemon `status`, including any
`latest_failure` and `recommended_action` fields, when reporting the problem.
Then consult the [MCP guide](mcp.md) for transport and service-manager
diagnosis, or return to the [README](../README.md) for the supported setup
paths.

## Agent-loop hooks

Agent-loop hooks are deliberately advisory. A hook failure should not stop an
agent from working; use the checks below to restore graph context when it is
missing.

### Check the generated configuration

Start with a read-only verification:

```bash
codebase-graph agent-hooks verify --client all
```

If a project file is malformed or truncated, restore the file's valid JSON
shape, preserve unrelated settings, and reinstall only the managed entries:

```bash
codebase-graph agent-hooks install --client <client> --verify
```

The generated files are `.codex/hooks.json`, `.claude/settings.json`, and
`.github/hooks/codebase-graph.json`. Do not copy a project hook file into a
user-level configuration file: hosts may merge both layers and run handlers
twice. Inspect the generated command for the stable
`--managed-id codebase-graph-v1` marker, and remove only codebaseGraph's
managed entries with:

```bash
codebase-graph agent-hooks remove --client <client>
```

### The hook cannot find the binary

The generated command must resolve the installed `codebase-graph` executable.
Check the client environment and shell `PATH`, then rerun verification. A
missing executable is a successful no-op from the hook's point of view, so MCP
registration and direct graph commands remain usable while the path is fixed.

### The daemon is unavailable or the graph is stale

Inspect the existing managed daemon; do not create a second database or force a
rebuild from the hook:

```bash
codebase-graph mcp daemon status --config .codebaseGraph/config.json
codebase-graph mcp daemon start --config .codebaseGraph/config.json
```

The hook reports health or freshness uncertainty and exits successfully. The
managed watcher refreshes the graph after source changes. Use an explicit
`build --mode full` only when a manual rebuild is intentional.

### The hook reports a repository mismatch

Hooks resolve identity from the repository's fixed
`.codebaseGraph/config.json`. Confirm the client starts in the repository that
owns that file and that the endpoint is the matching managed daemon. An
identity mismatch is fail-open: the hook returns a warning rather than using
another repository's graph.

### Nothing appears in the prompt

Check, in order:

1. The client or workspace trusts project hooks.
2. The host has not disabled lifecycle hooks through policy or settings.
3. The generated project file is valid JSON and contains the managed command.
4. The client has been restarted or reloaded after installation.
5. `agent-hooks verify` reports the expected client and path.

An empty prompt intentionally produces no search. A non-empty prompt can still
produce no context when the graph is unavailable, the input exceeds the bounded
limit, or no semantic matches exist.

### Copilot-specific behavior

GitHub Copilot CLI and VS Code use the project hook file at
`.github/hooks/codebase-graph.json`. Local Copilot delivery may use the bounded
session cache under `.codebaseGraph/agent-hooks/sessions/`; it never stores raw
prompts. A cloud-agent invocation is intentionally a successful no-op because
the local loopback daemon is not available there.
