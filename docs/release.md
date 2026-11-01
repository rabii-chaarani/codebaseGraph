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

Normal pull requests target exactly `main`, `cod103-stack/cod-105`, `cod103-stack/cod-106`,
`cod103-stack/cod-56`, `cod103-stack/cod-107`, or `cod103-stack/cod-108`. Activities are opened,
synchronize, reopened, edited, labeled, and unlabeled. Pushes remain restricted to `main`.
Staging checks are not release qualification; a changed head/base needs fresh qualification.
No privileged PR trigger or PR artifact promotion/publication is added.

Every event requires successful `classify`, `fmt`, `supply-chain`, `repository-policy`, and
`native_retention` jobs. The policy job is the single ordinary `release-gate` invocation and runs
`cargo test -p xtask --locked` for classifier/workflow regressions. Formatting stays `cargo fmt --check`;
the existing advisory scan and time-parsing feature guard remain unchanged.

Only a positively verified metadata-only release PR may use lightweight verification. Current API and
event data must agree on the open, non-draft same-repository PR targeting current `main`, exact release
branch `release-please--branches--main--components--codebase-graph`, `autorelease: pending` label, and
creator ID41898282 / `github-actions[bot]` / Bot. Branch, label, author name, or token name alone is
not admission. A different proposal principal stays full until separately verified and approved.

The schema1 classifier runs code, manifests, lockfile, cwd, and build context from immutable protected
main base B. A separate Git object checkout supplies B→H data; H is never checked out or executed for
admission. Checkouts do not persist credentials, classifier builds use a private target directory, and
no untrusted compiler cache or publication secret is shared. The complete generated six-file version/
changelog form must pass the conservative semantic proof; other fields, dependencies, layouts, files,
or uncertain acquisition select explicit full mode. Missing/incompatible protected-base helper also
selects full: COD-108's own PR cannot use its head helper to admit itself.

Ordinary/untrusted/non-main-base PRs and every main push run full verification, including:

- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`;
- root `cargo publish --dry-run --locked`, exact-byte 10 MiB crate limit, and isolated wiki package checks;
- platform-aligned workspace tests and release-ready packages on macOS ARM64 and Windows x86_64,
  using the existing native CLI/profile/build/smoke recipes.

Lightweight mode skips exactly those three heavy jobs (`clippy`, `publish-dry-run`, `native`), not the
common policy/advisory/format/classification checks. It proves consistent metadata without building a
package in disguise. The stable `required` job always runs and needs every selector/common/heavy job.
It rejects missing, malformed, failed, cancelled, or unexpected results. Full requires all heavy jobs
successful; lightweight requires all three exactly skipped plus fresh positive proof. Immediately before
success, both PR modes re-read current identity and B/H; lightweight also rechecks current main and trust.
API failure or stale identity fails required, even after a full fallback. Run summaries record mode,
B/H, reason, and intentional skips. This final read is not a merge transaction: strict freshness through
an authorized non-bypass actor remains required. PR-head-editable workflow YAML is not claimed tamper-proof.

The separate COD-107 A03 selector is unchanged: only its qualifying same-repository head
`cod103-stack/cod-107` targeting `cod103-stack/cod-56` or `main` on its original four activities may retain
already-built native archives, sidecars, and provenance. Added bases/label events grant no COD-108 or
other PR retention. Main pushes retain both artifacts for 90 days; existing 3-day diagnostic retention
and native recipes are unchanged. Retention never skips required verification or promotes PR artifacts.

macOS ARM64 runs unit, generation-storage, and remaining integration/doctest cohorts independently.
Windows uses four serial groups per cohort, including generation-storage, so each case runs once.
Groups come from Cargo inventories; complete/selected inventories remain in diagnostics. Tests run
serially and release artifacts build only after all target test groups succeed.

Actual hosted lightweight scheduling must be qualified after protected-main deployment and its own
successful full main CI, before the final release-PR merge. Unit tests, configured jobs, and the full
bootstrap PR do not establish that rollout evidence. Every release merge still runs full main-push CI
and supplies its own exact-source native artifacts; there is no PR-artifact substitution.

## Release Flow

1. Merge ordinary PRs into `main`. Once current-main CI succeeds, release-please creates or updates the release PR.
2. Review and merge that release PR. Its merge commit must pass the entire `CI` push workflow.
3. Release verifies the run's repository, workflow path, event, branch, status, SHA, and exact trusted release PR identity.
   The merge SHA must still be in main history; it need not be the latest main commit.
4. Immutable metadata must agree across the root package, wiki package/dependency, release manifest, and changelog.
   Production checks and both native artifacts are validated before any tag or release is created.
5. The single publisher creates only `vX.Y.Z` at that SHA and creates or resumes its draft GitHub Release.
   It uploads the validated archives and checksum sidecars, reads every expected asset back, and compares its size and
   bytes with the staged validated set before publishing the draft. Only then may the crate publish.
   Existing matching tags/releases can be resumed. A complete matching immutable release is verified without writes;
   missing or mismatched immutable assets fail without attempting repair. Conflicting tags fail without being moved.
   Latest-version selection occurs when the draft is published, so delayed older versions do not replace newer ones.
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

### Optional local documentation helpers

Python helpers are permitted only under `docs/` for documentation, evidence, qualification, audit, or reproduction support.
They are optional local commands for explicit developer use with Python 3's standard library only. Path placement is
necessary, not sufficient: helper content must not implement product build, runtime, installation, or release
behavior.

Do not add Python setup, dependencies, package/tool manifests, or helper invocations in CI, Release, product builds,
runtime, or installation. Keep helpers out of native archives and published `.crate` payloads; ordinary Git source and
tag snapshots may retain documentation.
This policy does not restore or extract archived COD-99 helpers, and existing root-tooling, security, workflow, and
other repository checks remain in force. It adds no helper or command to run.

Release notes should list native smoke evidence, benchmark evidence used for rollout recommendations, and graph
compatibility changes that require users to refresh `.codebaseGraph` state.

Knowledge Wiki release evidence must also include deterministic projection,
malicious-content, localhost binding, MCP schema, authoring path-safety, and
package-owned fixture smoke results. Templates and assets must be loaded from
the packaged artifact; a smoke run that relies on the repository checkout is
not sufficient. Generated `.kwiki/` state is never included in an archive.

## Windows Cargo source installs

For Windows x86_64, ordinary `cargo install codebase-graph` uses the default
`bundled-windows-extensions` feature. The binary carries the existing Ladybug
0.19.0 JSON and FTS extension bytes; no extension feature flag or runtime
extension download is needed. Explicit `--no-default-features` opts out of these
bytes. The workspace `k-wiki` build inherits the root dependency's defaults;
`k-wiki` is not published as a separate crates.io package.

Cargo is still a native source build, not a self-contained Windows distribution.
It needs Rust and an MSVC C/C++ build environment, including the Windows SDK,
for the vendored grammar. Ladybug's source-build fallback additionally uses
CMake and Ninja and needs compatible native headers/libraries. The final
native link needs OpenSSL libraries; runtime extension loading needs compatible
OpenSSL 3 (`libssl-3-x64.dll`, `libcrypto-3-x64.dll`) and the required Visual C++
runtime libraries. Bundled extension bytes do not supply those DLLs. Missing
compiler, native-library or DLL prerequisites are errors, not permission to
download extensions or switch installation routes silently.

The default-feature change is tracked by [COD-53](https://linear.app/codebasegraph/issue/COD-53)
under [CBG-PLAT-01 revision 0.10, R02/A16](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md#a16).
Before claiming source-install qualification, execute the plain install on a
documented supported Windows 11 x86_64 build environment. Record the exact
source/package, compiler, native-library and DLL prerequisites. Then use an
isolated empty extension cache with runtime extension downloads disabled and
verify real JSON/FTS loading, graph build, reopen and search. A prepared Windows
Server CI build or a feature-selection check alone is not this qualification.

Prefer the official prebuilt route if you do not want a native build environment.
The separate D01 prebuilt-runtime work remains gated on permitted compatible
runtime supply and controlled loading. This source-install guidance neither
replaces that policy nor claims that the current prebuilt archive is qualified.
No exact Windows source-build recipe or clean-consumer execution is claimed by
the default-feature change alone.

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
  for successful main-push CI at its exact SHA. It requires an existing GitHub Release, never publishes a draft or
  crate, and never changes release PR labels. Immutable assets must already match the validated set.

`dry-run` defaults to `true`. It resolves identity, runs the production gate, acquires artifacts, and validates the full
archive/checksum/provenance set without creating tags, releases, labels, proposals, or publishing crates. After checking
the successful dry-run, dispatch the same target with `dry-run: false` to publish.

For existing-tag recovery, `artifact-source: rebuild-if-missing` rebuilds **both** targets with the shared native
workflow if any retained artifact is unavailable. It never mixes promoted and rebuilt artifacts. Automatic and CI-run
recovery never substitute a run or rebuild missing artifacts.

To recover a blocked release, locate its merge SHA and successful `CI` run, dispatch a `resume-ci-run` dry-run, then
publish that exact target. Do not clear the pending label to bypass the block. Once finalization succeeds, later merged
changes can enter the next release PR. Repeating the same target reconciles a partial draft upload or lost response;
accepted writes are not retried automatically. A failed upload or readback leaves a draft unpublished and blocks crate
publication and finalization. A published mutable release retains assets-only repair; a frozen release is verify-only.

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

### Unpublished 4.0.0 recovery

Release PR #156 merged as 4.0.0, but its immutable source failed the production repository gate before publication.
PR #159 corrected that gate on later main; it cannot change the old release source. PR #162 therefore proposes 4.0.1
from corrected main instead of retrying or tagging the failed 4.0.0 source. The old proposal remains unpublished, not tagged.

`release-please-config.json` uses that failed merge, `7038e66ff8271ffddc511f879cf8d28353a09b2d`, as its
`bootstrap-sha` commit cutoff. When the manifest's tag is missing, this prevents a full-history scan from replaying
an old `Release-As: 1.2.2` trailer. The cutoff is not evidence of publication and does not authorize a tag, CI run,
or artifact source. Once the manifest version has a real release tag, release-please ignores the bootstrap cutoff.
No permanent `release-as` version pin is used. The 4.0.1 notes explicitly include the unpublished storage change;
all existing exact-SHA CI, production, artifact, and crate-publication gates still apply to the new release merge.

Until this correction reaches main, another ordinary main merge can make the old generator overwrite the open PR.
Reapply the reviewed correction if that happens; do not publish a placeholder 4.0.0 tag or clear pending labels to
work around proposal generation. Publication and maintainer merge approval remain separate from this repair.

## Conda-Forge Release Path

This repository intentionally does not upload directly to Anaconda.org. Conda distribution should go through
conda-forge:

1. Ensure the GitHub Release has completed and download the tag source archive SHA256.
2. Verify the Rust toolchain requirements are available on conda-forge.
3. Copy `conda-forge/recipe/meta.yaml` into a new `recipes/codebase-graph/` directory in a fork of `conda-forge/staged-recipes`.
4. Replace `version` and `sha256` placeholders with release-specific values.
5. Open the staged-recipes pull request and let conda-forge CI validate macOS ARM64 and Windows x86_64 builds.
