# Release Process

Release-please manages version pull requests and changelogs. Publication is bound to the successful `main` CI run
for the exact release-PR merge commit. Subsequent merges do not invalidate that release while its commit remains
on `main`. The workflow validates both retained native artifacts before creating its tag and GitHub Release,
then publishes the crate from the same source SHA. Release-please itself always runs with tag creation disabled.

Failed CI, ordinary commits, ambiguous release PRs, and commits removed from main history cannot authorize publication.
After native assets and the crate succeed, the specific release PR is marked tagged and proposal maintenance resumes.
An outstanding merged pending-release PR is reported as a blocked workflow with its recovery instruction, not a silent
successful no-op. All automatic and manual runs serialize in `release-main`, with cancellation disabled and `queue: max`.

## One-Time Setup

Create the protected `cargo` GitHub environment before the first release. Keep its deployment policy restricted to
`main`. Automatic publication is unattended, so the environment must not require reviewers.

Set these `cargo` environment variables to `true` only after the corresponding owner-controlled gate is verified:

- `CODEBASE_GRAPH_CONFIRM_RELEASE_ENVIRONMENT`
- `CODEBASE_GRAPH_CONFIRM_PRIVATE_VULNERABILITY_REPORTING`
- `CODEBASE_GRAPH_REQUIRE_CONDA`, only when conda-forge publication is part of the release

Add a `CARGO_REGISTRY_TOKEN` secret with permission to publish the `codebase-graph` crate.

The publisher uses `GITHUB_TOKEN` unless `RELEASE_PUBLISH_TOKEN` is configured in the `cargo` environment.
Historical tags whose workflow files differ from current main may require a repository-scoped token with Contents and
Workflows write permissions. The workflow fails clearly if GitHub refuses the exact tag; it never substitutes a newer
commit. An owner can instead create the exact verified tag/release after a successful recovery dry-run, then resume
asset and crate publication. Never copy a local CLI login token into repository secrets as part of recovery.

## CI

Pull requests targeting `main` and pushes to `main` run:

- `cargo fmt --check`
- platform-aligned workspace tests on macOS ARM64 and Windows x86_64
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- Rust advisory scanning with `cargo audit`
- `cargo run -p xtask --locked -- release-gate`, before the package dry-run, to reject repository-policy violations before release.
- `cargo publish --dry-run --locked`
- An exact-byte check that the generated `.crate` does not exceed crates.io's 10 MiB upload limit.
- `cargo package -p k-wiki --locked --no-verify` and the isolated Knowledge Wiki smoke. The
  unpublished wiki binary uses the in-tree codebase-graph registrar, while the preceding root
  package check verifies that publishable shared API in isolation.
- Release-ready package builds on macOS ARM64 and Windows x86_64. Pull requests build and smoke
  them without retention; `main` retains both artifacts for 90 days.
- A stable `required` aggregate check that succeeds only when every mandatory job succeeds. Repository rules should
  require this check after it has appeared on `main` once.

macOS ARM64 runs unit, generation-storage, and remaining integration/doctest cohorts independently. Windows uses four
serial groups per cohort, including generation-storage, so each case runs once. Groups are discovered from
Cargo's test inventories, with complete and selected inventories retained in diagnostic artifacts. Each runner executes
tests serially, and a target's release artifact is built only after all its test groups succeed.

## Release Flow

1. Merge ordinary PRs into `main`. Once current-main CI succeeds, release-please creates or updates the release PR.
2. Review and merge that release PR. Its merge commit must pass the entire `CI` push workflow.
3. Release verifies the run's repository, workflow path, event, branch, status, SHA, and exact trusted release PR identity.
   The merge SHA must still be in main history; it need not be the latest main commit.
4. Immutable metadata must agree across the root package, wiki package/dependency, release manifest, and changelog.
   Production checks and both native artifacts are validated before any tag or release is created.
5. The single publisher creates only `vX.Y.Z` at that SHA, uploads the validated archives, and publishes the crate.
   Existing matching tags/releases can be resumed. Conflicting tags fail without being moved. Delayed older versions
   do not replace newer versions as GitHub's latest release.
6. After both publishers succeed, mark that PR `autorelease: tagged`, remove `autorelease: pending`, and run proposal
   maintenance if current main has successful CI. Ordinary runs never tag an outstanding release as a side effect.

Cargo's package verification compiles the extracted source package with the `dev` profile by default. That compile is
not a distributed binary. The native GitHub Release archives are built separately with `cargo build --release`, and
crates.io distributes source for downstream users to compile with the profile they select.

Bundled Ladybug extensions are stored as lossless XZ streams in the source package and decompressed before the runtime
cache is seeded. This keeps the publishable `.crate` below the registry limit without changing the extension bytes or
the optimized native archive contract.

If the release pull request merge fails CI, later successful commits cannot publish its stale tag. A corrected release
must be represented by a new release pull request whose own merge commit passes CI, preserving the exact-run artifact
and provenance contract.

## Release Gate

Before publishing a production release, confirm:

- The exact tagged commit remains in `main` history and matches the completed successful `ci.yml` push run that triggered
  Release, including Rust tests, formatting, linting, native package builds, advisory scanning, package dry-run, and
  artifact smoke.
- Native Rust CLI and MCP entrypoints are required in production artifacts.
- Golden graph fixtures or expected graph-contract tests are current.
- `SECURITY.md` is present and vulnerability reporting expectations are current.
- Root `Cargo.toml` has complete crates.io package metadata and matches the release tag.
- The generated `.crate` passes `cargo run -p xtask -- verify-crate-size <archive>` and remains at or below 10 MiB.
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
- `checksums.txt` for the complete target payload
- `install.sh`
- `install.ps1`

The macOS ARM64 archive additionally includes the version-qualified
`libcodebasegraph-ssl-3.6.2.dylib`, `libcodebasegraph-crypto-3.6.2.dylib`, and
`openssl-3.6.2-LICENSE.txt`. These three files are covered by `checksums.txt`
and installed beside both binaries. Do not copy a binary without its matching
runtime payload. Windows archive contents are unchanged by this macOS repair.

The macOS artifact builder verifies the official OpenSSL 3.6.2 source archive
SHA-256, source-builds its dynamic libraries for ARM64 with deployment target
13.3, and uses that library directory for both release builds. Packaging rewrites
the binary imports to executable-relative names and the SSL-to-crypto import
to a loader-relative name. It rejects unexpected non-system imports, runpaths,
architectures or minimum-OS metadata above 13.3. After load-command edits it
renews and verifies ad-hoc code signatures, then hashes the final files. This
is not Developer ID signing or notarization.

Qualification covers only the current stable macOS release on Apple Silicon.
The exact installed archive must run on that release without relying on
separately installed Homebrew/OpenSSL. Current-release hosted CI can supply this
evidence when it records the actual OS/build and demonstrates those conditions;
a `macos-latest` label or deployment metadata alone is not proof. The earlier
macOS13.3 execution requirement is dropped. The existing13.3 deployment target
remains build metadata, not an older-version support guarantee. See the
[current-only policy amendment](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/decisions/d05a-current-macos-only.md).

The internal Actions artifact also carries `provenance.json`, which binds the public archive checksum to the exact
commit SHA, version, and target. `provenance.json` is validation metadata and is not uploaded as a public release asset.

## Manual validation and recovery

Dispatch `Release` on `main` with exactly one target:

- `resume-ci-run`: the successful main-push CI run of a trusted release PR merge. This can create a missing tag/release
  and resumes both native assets and crates.io. It reuses the exact CI run and requires `artifact-source: promote`.
- `publish-existing-tag`: an existing strict `vX.Y.Z` tag. This legacy mode publishes native assets only and searches
  for successful main-push CI at its exact SHA. It never publishes a crate or changes release PR labels.

`dry-run` defaults to `true`. It resolves identity, runs the production gate, acquires artifacts, and validates the full
archive/checksum/provenance set without creating tags, releases, labels, proposals, or publishing crates. After checking
the successful dry-run, dispatch the same target with `dry-run: false` to publish.

For existing-tag recovery, `artifact-source: rebuild-if-missing` rebuilds **both** targets with the shared native
workflow if any retained artifact is unavailable. It never mixes promoted and rebuilt artifacts. Automatic and CI-run
recovery never substitute a run or rebuild missing artifacts.

To recover a blocked release, locate its merge SHA and successful `CI` run, dispatch a `resume-ci-run` dry-run, then
publish that exact target. Do not clear the pending label to bypass the block. Once finalization succeeds, later merged
changes can enter the next release PR. Repeating the same target is safe after a partial upload or lost response.

The macOS installer validates the exact five-file payload and every checksum,
then probes `codebase-graph --help` and `k-wiki --version` without inherited
runtime-library overrides. All validation precedes destination writes. Missing,
corrupt or incompatible runtime files fail without replacing the existing
installation. Dry run writes no destination. Real installation copies the two
dylibs and license before the binaries, using a temporary file and rename for
each file; this is not an all-file transaction or rollback guarantee. Older
version-qualified runtime files are retained. Windows installer behavior is
unchanged by this slice.

The macOS artifact smoke performs a real install into a relocated directory
with spaces. Every runtime child receives an isolated home, no inherited
`DYLD_*` or OpenSSL configuration overrides, and a system-only PATH. It checks
graph build/reopen/search, bundled JSON/FTS bytes, wiki/MCP and invalid runtime
preflight while preserving an installed payload. These are local/native checks,
not, by themselves, proof of the required current-release runtime isolation.

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
5. Open the staged-recipes pull request and let conda-forge CI validate macOS ARM64 and Windows x86_64 builds.
