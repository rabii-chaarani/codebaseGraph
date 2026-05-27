# Release Process

`codebaseGraph` releases are managed by release-please. The release workflow opens and maintains a release pull request from Conventional Commit history. When that release pull request is merged, release-please creates the `vX.Y.Z` tag and GitHub Release, then the same workflow builds the source distribution and wheel from that tag, verifies that the package metadata version matches the tag, attaches the distributions to the GitHub Release, and publishes to PyPI with Trusted Publishing.

## One-time PyPI setup

Configure a PyPI Trusted Publisher for:

- PyPI project: `codebase-graph`
- Owner/repository: `rabii-chaarani/codebaseGraph`
- Workflow: `release.yml`
- Environment: `pypi`

Create the `pypi` GitHub environment before the first release. Use required reviewers on that environment when release approval should be manual.

## CI

Pull requests and pushes to `main` or `codex/**` run:

- `pytest` on Linux, macOS, and Windows for Python 3.10 through 3.14.
- `ruff check .` on Linux.
- Supply-chain checks on Linux with `pip check`, `pip-audit --dry-run --strict` dependency collection, and CycloneDX
  SBOM generation.
- A package build on Linux with `python -m build`, `twine check`, console-script smoke tests from the built wheel,
  packaged runtime smoke that runs `setup`, `graph-health`, `graph-search`, and stdio MCP handshake checks, and release
  SBOM generation.

## Release flow

1. Merge normal pull requests into `main` with Conventional Commit-style titles or squash commit messages such as `feat: add graph query helpers` or `fix: preserve MCP config`.
2. The `Release` workflow opens or updates a release pull request that updates `CHANGELOG.md` and `.release-please-manifest.json`.
3. Review and merge the release pull request when ready to publish.
4. The `Release` workflow creates the `vX.Y.Z` tag and GitHub Release, builds the distributions from that tag, verifies `Version: X.Y.Z`, uploads the distributions and SBOM to the GitHub Release, and publishes to PyPI from the protected `pypi` environment.

Vulnerability advisory scans require an external advisory service. Keep them in the hosted CI/release environment or
run them explicitly from a development machine; do not make local setup call external advisory APIs implicitly.

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
