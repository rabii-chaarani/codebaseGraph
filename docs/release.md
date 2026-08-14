# Release Process

`codebaseGraph` releases are managed by release-please. Main-branch CI builds and smoke-tests the complete native
archives. The Release workflow starts only after that entire CI workflow completes. A successful current-tip `main`
push lets release-please create or update its release pull request; merging that pull request creates a strict `vX.Y.Z`
tag, validates and promotes the triggering CI run's retained artifacts, and publishes `codebase-graph` to crates.io.
Failed, cancelled, pull-request, and non-main completions perform no release mutation. A completion that is already stale
is skipped before release-please; if `main` advances while release-please is running, a post-action guard stops all asset
and crate publication.

## One-Time Setup

Create the protected `cargo` GitHub environment before the first release. Keep its deployment policy restricted to
`main`. Automatic publication is unattended, so the environment must not require reviewers.

Set these `cargo` environment variables to `true` only after the corresponding owner-controlled gate is verified:

- `CODEBASE_GRAPH_CONFIRM_RELEASE_ENVIRONMENT`
- `CODEBASE_GRAPH_CONFIRM_PRIVATE_VULNERABILITY_REPORTING`
- `CODEBASE_GRAPH_REQUIRE_CONDA`, only when conda-forge publication is part of the release

Add a `CARGO_REGISTRY_TOKEN` secret with permission to publish the `codebase-graph` crate.

## CI

Pull requests targeting `main` and pushes to `main` run:

- `cargo fmt --check`
- platform-aligned workspace tests on Linux, macOS ARM, and Windows
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- Rust advisory scanning with `cargo audit`
- `cargo publish --dry-run --locked`
- `cargo package -p k-wiki --locked --no-verify` and the isolated Knowledge Wiki smoke. The
  unpublished wiki binary uses the in-tree codebase-graph registrar, while the preceding root
  package check verifies that publishable shared API in isolation.
- Release-ready package builds on Linux x86_64, macOS ARM/x86_64, and Windows x86_64. Pull requests build and smoke
  them without retention; `main` retains all four artifacts for 90 days.
- A stable `required` aggregate check that succeeds only when every mandatory job succeeds. Repository rules should
  require this check after it has appeared on `main` once.

## Release Flow

1. Merge normal pull requests into `main` with Conventional Commit-style titles or squash commit messages.
2. After the complete `CI` push workflow succeeds, `Release` verifies that its triggering run is the current `main` tip
   and passes that exact run ID and SHA to release-please. It rechecks the tip after release-please before continuing.
3. Release-please opens or updates a release pull request that changes `CHANGELOG.md`, `.release-please-manifest.json`,
   root `Cargo.toml`, and `crates/k-wiki/Cargo.toml` together.
4. Review and merge the release pull request when ready to publish. Its `main` CI must complete successfully like any
   other merge.
5. The resulting Release run creates the `vX.Y.Z` tag, proves that the tag resolves to the triggering CI SHA, validates
   all four archives/checksums/provenance records from that exact run, and uploads the public assets from one publisher.
6. `cargo publish --dry-run --locked` runs at the immutable tag, then the crate publishes automatically after native
   assets succeed. Manual recovery never publishes the crate.

## Release Gate

Before publishing a production release, confirm:

- The exact tagged commit is the current `main` tip and matches the completed successful `ci.yml` push run that triggered
  Release, including Rust tests, formatting, linting, native package builds, advisory scanning, package dry-run, and
  artifact smoke.
- Native Rust CLI and MCP entrypoints are required in production artifacts.
- Golden graph fixtures or expected graph-contract tests are current.
- `SECURITY.md` is present and vulnerability reporting expectations are current.
- Root `Cargo.toml` has complete crates.io package metadata and matches the release tag.
- `crates/k-wiki/Cargo.toml` matches the root version and points its `codebase-graph` dependency at the same release version.
- The protected `cargo` GitHub environment and release-please token posture have been verified in GitHub settings.
- Conda-forge submission is either out of scope or the recipe placeholders have been replaced with the release version,
  GitHub source archive SHA256, and chosen SPDX license.

Run the local release-gate checker before publishing:

```bash
cargo run -p xtask -- check-workflows
cargo run -p xtask -- release-gate
cargo run -p xtask -- release-gate --production \
  --confirm release-environment \
  --confirm private-vulnerability-reporting
```

Add `--require-conda` when conda-forge submission is in scope for the release.

Release notes should list native smoke evidence, benchmark evidence used for rollout recommendations, and graph
compatibility changes that require users to refresh `.codebaseGraph` state.

Knowledge Wiki release evidence must also include deterministic projection,
malicious-content, localhost binding, MCP schema, authoring path-safety, and
package-owned fixture smoke results. Templates and assets must be loaded from
the packaged artifact; a smoke run that relies on the repository checkout is
not sufficient. Generated `.kwiki/` state is never included in an archive.

Each native archive must contain:

- `codebase-graph` / `codebase-graph.exe`
- `k-wiki` / `k-wiki.exe`
- `checksums.txt` for the packaged binaries
- `install.sh`
- `install.ps1`

The internal Actions artifact also carries `provenance.json`, which binds the public archive checksum to the exact
commit SHA, version, and target. `provenance.json` is validation metadata and is not uploaded as a public release asset.

## Manual validation and recovery

Run the `Release` workflow manually with an existing strict tag:

- `artifact-source: promote` requires all four exact-SHA CI artifacts to remain available.
- `artifact-source: rebuild-if-missing` rebuilds **all four** targets through the same native workflow when any retained
  artifact is missing or expired. It never mixes promoted and rebuilt targets.
- `dry-run: true` performs exact-SHA gating, promotion or recovery, archive/checksum/provenance validation, extraction,
  and smoke checks without modifying a GitHub Release or publishing Cargo.

Every manual mode still searches for and requires successful CI for the exact tag commit. Automatic mode never searches
for a substitute run: it validates and consumes the triggering run directly. Use dry-run first when exercising recovery.

The packaged installer validates both binaries against `checksums.txt`, runs
`codebase-graph --help` plus `k-wiki --version`, and only then atomically
replaces the selected target binaries.

After upgrading from a release archive:

1. Replace both binaries together from the same archive.
2. Rerun `k-wiki mcp install --client codex --scope project --verify` in each repository that uses k-wiki.
3. Restart Codex or the relevant MCP client so it reloads the updated repository-local registration.

To force a specific next version, merge a commit whose body contains a `Release-As: X.Y.Z` trailer.

## Conda-Forge Release Path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through
conda-forge:

1. Ensure the GitHub Release has completed and download the tag source archive SHA256.
2. Verify the Rust toolchain requirements are available on conda-forge.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version` and `sha256` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate Linux, macOS, and Windows builds.
