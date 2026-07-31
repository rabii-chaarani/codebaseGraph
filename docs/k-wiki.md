# Knowledge Wiki

`k-wiki` turns one or more Open Knowledge Format (OKF) 0.1 bundles into a
safe, searchable, deterministic repository wiki. Source Markdown remains the
canonical record; generated state is disposable and lives under `.kWiki/`.

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
k-wiki build <bundle> --out <directory> [--base-url <path>]
k-wiki serve <bundle> [--host 127.0.0.1] [--port 4321]
k-wiki inspect <bundle> --concept <concept-id>
k-wiki check-links <bundle> [--include-external]
k-wiki mcp [bundle]
```

`serve` binds to the local machine by default and refuses remote hosts.
Generated HTTP responses include restrictive content, framing, referrer, MIME,
and cache policies.

## State and rollback

Successful builds publish complete versioned generations under `.kWiki/`.
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
