# Security Policy

## Supported Versions

Security fixes are prepared against the current `main` branch and included in the next package release.

## Reporting a Vulnerability

Report suspected vulnerabilities privately through GitHub security advisories or private vulnerability reporting for this repository. Do not open a public issue for exploitable behavior, dependency vulnerabilities with an available proof of concept, credential exposure, or a bypass of the read-only MCP/query contract.

Include:

- affected version or commit
- reproduction steps
- expected impact
- relevant logs or proof of concept
- whether the report can be disclosed publicly after a fix is available

Maintainers should acknowledge reports within 7 days, triage severity, and coordinate disclosure timing with the reporter.

## Security Scope

The production security boundary is local-first:

- The stdio MCP transport is intended for local MCP clients.
- The HTTP MCP transport binds to localhost by default.
- `--allow-remote` requires a bearer token. It does not add TLS, rate limiting, authorization scopes, or a multi-user session model.
- HTTP tool calls require an initialized `Mcp-Session-Id`; one client's initialize request must not unlock tools for another client.
- `graph_query` is intended to remain read-only. Do not relax query restrictions without a parser-level read-only proof or an explicit safe-procedure allowlist.

Dependency vulnerability scanning runs in hosted CI and release workflows. Local setup commands must not call external advisory services implicitly.
