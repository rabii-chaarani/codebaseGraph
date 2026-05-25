# Release Process

`codebaseGraph` releases are tag-driven. The GitHub release workflow builds the source distribution and wheel from a `vX.Y.Z` tag, verifies that the package metadata version matches the tag, and publishes to PyPI with Trusted Publishing.

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
- A package build on Linux with `python -m build`, `twine check`, and console-script smoke tests from the built wheel.

## PyPI release

1. Confirm the release branch is green in CI.
2. Create an intentional release tag:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

3. The `Release` workflow checks out that tag, builds the distributions, verifies `Version: X.Y.Z`, and publishes to PyPI from the protected `pypi` environment.

For manual reruns, use the `Release` workflow dispatch input with an existing `vX.Y.Z` tag.

## Conda-forge release path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through conda-forge:

1. Ensure the PyPI release has completed and download the source distribution SHA256.
2. Before submitting `codebase-graph`, verify all runtime dependencies exist on conda-forge. If `real-ladybug` is not available, package that dependency first.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version`, `sha256`, and `license` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate Linux, macOS, and Windows builds.

After staged-recipes is merged, future conda releases are handled in the generated `codebase-graph-feedstock`.
