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
