# Knowledge Wiki

`k-wiki` turns one or more Open Knowledge Format (OKF) 0.1 bundles into a
safe, searchable, deterministic repository wiki. Source Markdown remains the
canonical record; generated state is disposable and lives under `.kwiki/`.

## Trust boundary

The service reads only configured bundle roots. Its three authoring operations
create bundles, create concept pages, and populate existing pages beneath those
roots. They reject absolute paths, traversal, escaping symlinks, reserved-file
misuse, stale revisions, and existing destinations. Browser routes are
read-only. Remote HTTP binding, arbitrary file editing, Git operations, and
multi-user authorization are not part of this release.

Graph context is optional. When enabled, the wiki calls only the public
`CodebaseGraphApi` search, context, and health operations. Graph failures
degrade related context without blocking concept reading or publication.

## Source layout

An OKF bundle is a directory whose root `index.md` declares `okf_version`.
Every other non-reserved Markdown file is a concept whose stable ID is its
bundle-relative path without `.md`.

- `index.md` describes its directory and never becomes a concept.
- `log.md` contributes scoped dated history and never becomes a concept.
- A concept requires YAML frontmatter containing a non-empty `type`.
- Unknown types and unknown frontmatter fields remain consumable.

## Commands

```text
k-wiki validate <bundle> [--profile consume|conformant|recommended] [--json]
k-wiki install [--repo-root <directory>]
k-wiki build <bundle> --out <directory> [--base-url <path>]
k-wiki serve <bundle> [--host 127.0.0.1] [--port 4321]
k-wiki inspect <bundle> --concept <concept-id>
k-wiki check-links <bundle> [--include-external]
k-wiki mcp install --client <client> [--repo-root <directory>] [--scope local|user|project] [--name <name>] [--client-config-path <path>] [--dry-run]
k-wiki mcp [bundle]
```

`serve` binds to the local machine by default and refuses remote hosts.
Generated HTTP responses include restrictive content, framing, referrer, MIME,
and cache policies.

`install` creates a conformant starter source bundle at `knowledge/index.md`
and initializes the repository-local `.kwiki/` state layout. It defaults to the
current directory and accepts `--repo-root` for another repository. The starter
bundle remains separate from generated state and can be passed to the remaining
commands. It also creates or updates a managed MCP-only k-wiki workflow block
in both `AGENTS.md` and `CLAUDE.md`, preserving all instructions outside the
`<!-- k-wiki:start -->` and `<!-- k-wiki:end -->` markers.

## MCP client registration

After bootstrapping the repository, register the wiki's stdio server with a
chosen MCP client:

```text
k-wiki mcp install --client <client> [--repo-root <directory>] [--scope local|user|project] [--name <name>] [--client-config-path <path>] [--dry-run]
```

`--client` is required and accepts `codex`, `claude`, `claude-project`,
`github-copilot`, `lmstudio`, `hermes`, `openclaw`, `generic`,
`copilot-studio`, `microsoft-copilot`, or `all`. The command resolves the
canonical `knowledge/` bundle and records `k-wiki mcp <absolute-bundle-path>`
with the client. It fails safely when the starter bundle is absent; run
`k-wiki install` first.

Registration is separate from repository bootstrap and does not start a
persistent server. The registered command uses `k-wiki` by default; set
`K_WIKI_SERVER_COMMAND` before registration to write a different executable
path. It registers as `k_wiki` unless `--name` is supplied.

## MCP maintenance tools

The MCP server exposes the same maintenance operations as the CLI, so agents
can validate, check links, and build without invoking a shell:

| CLI command | MCP tool | Required inputs |
| --- | --- | --- |
| `k-wiki validate knowledge --profile recommended --json` | `wiki_validate` | `bundle_root`, `profile` |
| `k-wiki check-links knowledge` | `wiki_check_links` | `bundle_root` |
| `k-wiki build knowledge --out .kwiki/site` | `wiki_build` | `bundle_root`, `output_root` |

`wiki_build` is marked as a write operation because it writes the static site.
All three tools accept `include_structured_content: true` when an agent needs
the typed result as well as the standard MCP text response. `bundle_root` must
identify the bundle configured when the server was registered; use its absolute
`knowledge/` path when the MCP client's working directory is not the repository.

## AI-agent workflow

Use the `k_wiki` MCP server for every wiki interaction; do not invoke the
`k-wiki` CLI or edit generated state directly.

1. **Orient before acting.** Treat `knowledge/` as curated repository intent,
   not a substitute for current code. Start with `wiki_list_bundles`, then
   `wiki_search_concepts`. Read the most relevant entries with
   `wiki_get_concept`; use `wiki_list_directory`, `wiki_get_backlinks`, and
   `wiki_get_neighborhood` to understand related decisions.
   `wiki_list_bundles` accepts no bundle-selection arguments and lists only the
   bundle roots configured when the MCP server was registered. It never scans
   caller-supplied repository paths.
2. **Ground implementation work.** Use the wiki for architecture, terminology,
   invariants, ownership, and prior decisions. Verify changeable details with
   the repository's codebase-graph MCP tools. When code and wiki conflict,
   identify the conflict and use `wiki_populate_page` to record the clarified
   intent rather than silently following stale content.
3. **Author knowledge deliberately.** Create missing pages with
   `wiki_create_page`; update existing pages with `wiki_populate_page`, always
   supplying title, type, tags, useful Markdown, and `expected_content_hash`.
   Record durable decisions, public contracts, runbooks, invariants, and
   non-obvious trade-offs—not transient implementation noise or copied source.
4. **Validate and publish after edits.** Call `wiki_validate` with
   `profile: "recommended"` and `include_structured_content: true`, then call
   `wiki_check_links`. Call `wiki_build` with the configured `bundle_root` and
   the repository's `.kwiki/site` `output_root`; it is a write operation.
   `knowledge/` is source, while `.kwiki/` is generated state and must never be
   edited manually.
5. **Close the loop.** Use `wiki_get_diagnostics` to inspect remaining issues
   and `wiki_get_recent_changes` to see changes since prior work. In a handoff,
   cite the updated concept paths and summarize decisions, uncertainties, and
   validation results.

## State and rollback

Successful builds publish complete versioned generations under `.kwiki/`.
Compilation or rendering failures leave the last valid generation readable.
An unchanged input is a cache hit; generation tokens prevent stale concurrent
work from replacing newer output.

To roll back, stop the preview process and restore the previous manifest and
generation pointer from normal repository backup tooling. Never edit generated
projection JSON as source.

## Validation profiles

- `consume`: retains readable content and reports diagnostics.
- `conformant`: fails required OKF 0.1 semantics.
- `recommended`: adds advisory authoring guidance.

Diagnostics use stable codes and repository-relative paths. Public errors never
include absolute host paths, source contents, secrets, or raw parser failures.
