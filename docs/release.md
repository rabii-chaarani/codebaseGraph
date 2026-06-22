# Release Process

`codebaseGraph` releases are managed by release-please. Merging a release pull request creates a strict `vX.Y.Z` tag and
GitHub Release. The release workflow then runs the Rust release gate, builds native archives from the tag, smoke-tests
the archived binary surface, uploads `.tar.gz` archives plus `.sha256` files, and publishes the same version to
crates.io as `codebase-graph`.

## One-Time Setup

Create the protected `cargo` GitHub environment before the first release. Use required reviewers when release approval
should be manual.

Set these `cargo` environment variables to `true` only after the corresponding owner-controlled gate is verified:

- `CODEBASE_GRAPH_CONFIRM_RELEASE_ENVIRONMENT`
- `CODEBASE_GRAPH_CONFIRM_HOSTED_CI_GREEN`
- `CODEBASE_GRAPH_CONFIRM_PRIVATE_VULNERABILITY_REPORTING`
- `CODEBASE_GRAPH_REQUIRE_CONDA`, only when conda-forge publication is part of the release

Add a `CARGO_REGISTRY_TOKEN` secret with permission to publish the `codebase-graph` crate.

## CI

Pull requests and pushes to `main` or `codex/**` run:

- `cargo fmt --check`
- `cargo test --workspace --locked` on Linux, macOS, and Windows
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- Rust advisory scanning with `cargo audit`
- `cargo publish --dry-run --locked`
- Native package builds and `xtask` artifact smoke on Linux, macOS, and Windows

## Release Flow

1. Merge normal pull requests into `main` with Conventional Commit-style titles or squash commit messages.
2. The `Release` workflow opens or updates a release pull request that updates `CHANGELOG.md` and `.release-please-manifest.json`.
3. Review and merge the release pull request when ready to publish.
4. The `Release` workflow creates the `vX.Y.Z` tag and GitHub Release.
5. The protected release jobs verify the tag against root `Cargo.toml`, run `xtask release-gate`, build native archives,
   run `xtask smoke-artifact`, upload archives/checksums, and publish the crate with `cargo publish --locked`.

## Release Gate

Before publishing a production release, confirm:

- Hosted CI is green for Rust tests, formatting, linting, native package builds, advisory scanning, package dry-run, and artifact smoke.
- Native Rust CLI and MCP entrypoints are required in production artifacts.
- Golden graph fixtures or expected graph-contract tests are current.
- `SECURITY.md` is present and vulnerability reporting expectations are current.
- Root `Cargo.toml` has complete crates.io package metadata and matches the release tag.
- The protected `release` GitHub environment and release-please token posture have been verified in GitHub settings.
- Conda-forge submission is either out of scope or the recipe placeholders have been replaced with the release version,
  GitHub source archive SHA256, and chosen SPDX license.

Run the local release-gate checker before publishing:

```bash
cargo run -p xtask -- release-gate
cargo run -p xtask -- release-gate --production \
  --confirm release-environment \
  --confirm hosted-ci-green \
  --confirm private-vulnerability-reporting
```

Add `--require-conda` when conda-forge submission is in scope for the release.

Release notes should list native smoke evidence, benchmark evidence used for rollout recommendations, and graph
compatibility changes that require users to refresh `.codebaseGraph` state.

To force a specific next version, merge a commit whose body contains a `Release-As: X.Y.Z` trailer.

## Conda-Forge Release Path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through
conda-forge:

1. Ensure the GitHub Release has completed and download the tag source archive SHA256.
2. Verify the Rust toolchain requirements are available on conda-forge.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version` and `sha256` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate Linux, macOS, and Windows builds.
