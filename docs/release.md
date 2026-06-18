# Release Process

`codebaseGraph` releases are managed by release-please. The release workflow opens and maintains a release pull request from Conventional Commit history. When that release pull request is merged, release-please creates the strict `vX.Y.Z` tag and GitHub Release, then the same workflow builds Rust production binaries from that tag, verifies that the Rust crate version matches the tag, smoke-tests the native CLI/MCP surface, and attaches native archives plus per-archive `.sha256` checksum files to the GitHub Release.

## One-time release setup

Create the protected `release` GitHub environment before the first release. Use required reviewers on that environment when release approval should be manual.

Set these `release` environment variables to `true` only after the corresponding owner-controlled gate is verified:

- `CODEBASE_GRAPH_CONFIRM_RELEASE_ENVIRONMENT`
- `CODEBASE_GRAPH_CONFIRM_HOSTED_CI_GREEN`
- `CODEBASE_GRAPH_CONFIRM_PRIVATE_VULNERABILITY_REPORTING`
- `CODEBASE_GRAPH_REQUIRE_CONDA`, only when conda-forge publication is part of the release

The release workflow runs `scripts/check_release_gate.py --production` in the protected `release` environment before building
or uploading release artifacts. If one of these variables is missing or the repository-local gates fail, the release
stops before any artifact is uploaded.

## CI

Pull requests and pushes to `main` or `codex/**` run:

- `pytest` on Linux, macOS, and Windows for Python 3.10 through 3.14.
- `ruff check .` on Linux.
- Supply-chain checks on Linux with `pip check`, `pip-audit --strict` vulnerability advisory scanning, immutable
  GitHub Action pins, and CycloneDX SBOM generation.
- Native package builds on Linux, macOS, and Windows with
  `cargo build --manifest-path rust/Cargo.toml --locked --release --bin codebase-graph`, native archive creation,
  packaged runtime smoke that requires native CLI/MCP entrypoints and runs `graph-schema --json`, `setup --dry-run`,
  `setup`, `materialize`, `graph-health`, `graph-search`, and stdio MCP handshake checks.

## Release flow

1. Merge normal pull requests into `main` with Conventional Commit-style titles or squash commit messages such as `feat: add graph query helpers` or `fix: preserve MCP config`.
2. The `Release` workflow opens or updates a release pull request that updates `CHANGELOG.md` and `.release-please-manifest.json`.
3. Review and merge the release pull request when ready to publish.
4. The `Release` workflow creates the `vX.Y.Z` tag and GitHub Release, builds native archives from that tag, verifies the Rust crate version is `X.Y.Z`, smoke-tests each archived binary, and uploads `codebase-graph-X.Y.Z-linux-x86_64.tar.gz`, `codebase-graph-X.Y.Z-macos-universal.tar.gz`, `codebase-graph-X.Y.Z-windows-x86_64.tar.gz`, and matching `.sha256` files to the GitHub Release from the protected `release` environment.

## Release gate

Before publishing a production release, confirm:

- Hosted CI is green for tests, ruff, Linux/macOS/Windows native package builds, supply-chain checks, and native artifact smoke.
- Native Rust CLI and MCP entrypoints are required in production artifacts; missing helpers, unsupported platforms, or
  native runtime failures must fail the package smoke checks instead of falling back to Python behavior.
- Golden graph parity fixtures are current. Do not change stable graph node IDs, edge IDs, relation labels, source spans,
  or manifest compatibility semantics without updating the fixtures and documenting the compatibility impact.
- `SECURITY.md` is present and vulnerability reporting expectations are current.
- The project owner has selected an SPDX license, added Rust package metadata in Cargo, and included the corresponding license file.
- The protected `release` GitHub environment and release-please token posture have been verified in GitHub settings.
- Conda-forge submission is either explicitly out of scope for the release or the recipe placeholders have been replaced with the release version, GitHub source archive SHA256, and chosen SPDX license.

Run the local release-gate checker before publishing:

```bash
python scripts/check_release_gate.py
python scripts/check_release_gate.py --production \
  --confirm release-environment \
  --confirm hosted-ci-green \
  --confirm private-vulnerability-reporting
```

Add `--require-conda` when conda-forge submission is in scope for the release.

Vulnerability advisory scans require an external advisory service. Hosted CI and release workflows run those scans and
fail on known vulnerable dependencies. Local setup stays offline-safe and must not call external advisory APIs
implicitly; run local advisory scans explicitly when that disclosure is acceptable.

Release notes should list the native smoke evidence, benchmark evidence used for any rollout recommendation,
and graph compatibility changes that require users to refresh `.codebaseGraph` state.

The production package version is provided by the Rust crate metadata; keep
`rust/crates/codebase_graph_native/Cargo.toml` aligned with release-please.
The release-please config intentionally disables component-prefixed tags so production releases stay in strict `vX.Y.Z` format.

To force a specific next version, merge a commit whose body contains a `Release-As: X.Y.Z` trailer.

For manual maintenance, rerun or dispatch the `Release` workflow. If CI checks must run on release-please pull requests, configure a `RELEASE_PLEASE_TOKEN` secret backed by a personal access token or GitHub App token; the default `GITHUB_TOKEN` can create the pull request but does not trigger follow-up workflows from its own events.

## Conda-forge release path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through conda-forge:

1. Ensure the GitHub Release has completed and download the tag source archive SHA256.
2. Before submitting `codebase-graph`, verify the Rust toolchain requirements are available on conda-forge.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version`, `sha256`, and `license` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate Linux, macOS, and Windows builds.

After staged-recipes is merged, future conda releases are handled in the generated `codebase-graph-feedstock`.
