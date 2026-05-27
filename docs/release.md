# Release Process

`codebaseGraph` releases are managed by release-please. The release workflow opens and maintains a release pull request from Conventional Commit history. When that release pull request is merged, release-please creates the `vX.Y.Z` tag and GitHub Release, then the same workflow builds the source distribution and wheel from that tag, verifies that the package metadata version matches the tag, attaches the distributions to the GitHub Release, and publishes to PyPI with Trusted Publishing.

## One-time PyPI setup

Configure a PyPI Trusted Publisher for:

- PyPI project: `codebase-graph`
- Owner/repository: `rabii-chaarani/codebaseGraph`
- Workflow: `release.yml`
- Environment: `pypi`

Create the `pypi` GitHub environment before the first release. Use required reviewers on that environment when release approval should be manual.

Set these `pypi` environment variables to `true` only after the corresponding owner-controlled gate is verified:

- `CODEBASE_GRAPH_CONFIRM_TRUSTED_PUBLISHER`
- `CODEBASE_GRAPH_CONFIRM_PYPI_ENVIRONMENT`
- `CODEBASE_GRAPH_CONFIRM_HOSTED_CI_GREEN`
- `CODEBASE_GRAPH_CONFIRM_PRIVATE_VULNERABILITY_REPORTING`
- `CODEBASE_GRAPH_REQUIRE_CONDA`, only when conda-forge publication is part of the release

The release workflow runs `scripts/check_release_gate.py --production` in the protected `pypi` environment before building
or publishing release distributions. If one of these variables is missing or the repository-local gates fail, the release
stops before any package is uploaded.

## CI

Pull requests and pushes to `main` or `codex/**` run:

- `pytest` on Linux, macOS, and Windows for Python 3.10 through 3.14.
- `ruff check .` on Linux.
- Supply-chain checks on Linux with `pip check`, `pip-audit --strict` vulnerability advisory scanning, immutable
  GitHub Action pins, and CycloneDX SBOM generation.
- A package build on Linux with `python -m build`, `twine check`, console-script smoke tests from the built wheel and
  source distribution, packaged runtime smoke that runs `setup`, `graph-health`, `graph-search`, and stdio MCP handshake
  checks, and release SBOM generation.

## Release flow

1. Merge normal pull requests into `main` with Conventional Commit-style titles or squash commit messages such as `feat: add graph query helpers` or `fix: preserve MCP config`.
2. The `Release` workflow opens or updates a release pull request that updates `CHANGELOG.md` and `.release-please-manifest.json`.
3. Review and merge the release pull request when ready to publish.
4. The `Release` workflow creates the `vX.Y.Z` tag and GitHub Release, builds the distributions from that tag, verifies `Version: X.Y.Z`, uploads the distributions and SBOM to the GitHub Release, and publishes to PyPI from the protected `pypi` environment.

## Release gate

Before publishing a production release, confirm:

- Hosted CI is green for tests, ruff, package build, supply-chain, wheel smoke, and source-distribution smoke.
- `SECURITY.md` is present and vulnerability reporting expectations are current.
- The project owner has selected an SPDX license, added package license metadata, and included the corresponding license file.
- The PyPI Trusted Publisher, `pypi` GitHub environment, and release-please token posture have been verified in GitHub/PyPI settings.
- Conda-forge submission is either explicitly out of scope for the release or the recipe placeholders have been replaced with the release version, source-distribution SHA256, and chosen SPDX license.

Run the local release-gate checker before publishing:

```bash
python scripts/check_release_gate.py
python scripts/check_release_gate.py --production \
  --confirm trusted-publisher \
  --confirm pypi-environment \
  --confirm hosted-ci-green \
  --confirm private-vulnerability-reporting
```

Add `--require-conda` when conda-forge submission is in scope for the release.

Vulnerability advisory scans require an external advisory service. Hosted CI and release workflows run those scans and
fail on known vulnerable dependencies. Local setup stays offline-safe and must not call external advisory APIs
implicitly; run local advisory scans explicitly when that disclosure is acceptable.

The package version remains tag-derived through `setuptools_scm`; do not add a static `project.version` field to `pyproject.toml` just for release-please.

To force a specific next version, merge a commit whose body contains a `Release-As: X.Y.Z` trailer.

For manual maintenance, rerun or dispatch the `Release` workflow. If CI checks must run on release-please pull requests, configure a `RELEASE_PLEASE_TOKEN` secret backed by a personal access token or GitHub App token; the default `GITHUB_TOKEN` can create the pull request but does not trigger follow-up workflows from its own events.

## Conda-forge release path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through conda-forge:

1. Ensure the PyPI release has completed and download the source distribution SHA256.
2. Before submitting `codebase-graph`, verify all runtime dependencies exist on conda-forge. If `real-ladybug` is not available, package that dependency first.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version`, `sha256`, and `license` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate Linux, macOS, and Windows builds.

After staged-recipes is merged, future conda releases are handled in the generated `codebase-graph-feedstock`.
