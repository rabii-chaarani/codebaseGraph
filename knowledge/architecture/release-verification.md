---
description: Exact-commit publication, retained CI artifact promotion, proposal maintenance, and verified recovery.
resource: repository-architecture
tags:
- architecture
- artifacts
- ci
- provenance
- release
timestamp: 2026-09-24
title: Native Release Verification
type: architecture
---
# Native Release Verification

The Release Verifier owns the repository's build-once/promote contract. Platform-specific Cargo arguments and packaging behavior live in the typed `xtask` target model, while workflows select targets and provide GitHub orchestration only.

## Supported native targets

The public target identifiers are:

- `linux-x86_64`
- `macos-arm64`
- `macos-x86_64`
- `windows-x86_64`

Unsupported host and target combinations fail before Cargo is invoked. Windows is the only target that selects release-mode tests and the `bundled-windows-extensions` feature. The `k-wiki` feature forwards to the Graph Runtime feature so every binary in a Windows archive has the same native extension contract.

## Artifact contract

`cargo xtask native-artifact` derives the workspace version, builds both shipped binaries, stages the installer scripts, writes internal checksums, and creates a deterministic target archive plus its public SHA-256 sidecar. It then extracts the archive into a clean directory and runs installer dry-runs and packaged binary/wiki smoke checks.

Every internal artifact also contains `provenance.json` with the schema version, exact commit SHA, package version, target identifier, archive filename, and archive digest. Release validation accepts exactly one archive, checksum, and provenance document for each supported target; mismatched or unexpected files fail the complete set.

## CI contract

CI runs only for pull requests targeting `main` and pushes to `main`. Formatting, Clippy, audit, package dry-run, and the four native targets remain independent jobs. The native matrix calls the reusable workflow in `.github/workflows/native.yml`, which checks out the requested SHA, prepares native dependencies, optionally runs the typed platform tests, and always builds and smokes the typed artifact.

Pull requests validate artifacts without retaining them. Main pushes retain all four release-ready artifacts for 90 days. A final job named `required` depends on every mandatory job and is the sole stable repository-ruleset context.

## Automatic release orchestration

The Release workflow starts after successful main-push CI completes. It binds publication to the triggering run ID and SHA and revalidates the repository, workflow path, event, branch, status, and conclusion through GitHub. Exactly one repository-owned release-please PR must have that exact merge SHA, target main, and carry the pending or tagged release label. The commit must remain in main history; subsequent ordinary merges do not invalidate its release. Failed, unrelated, ambiguous, or removed commits cannot authorize publication.

Release-please only maintains version proposals and always has tag creation disabled. The targeted publisher handles one verified release, after production and complete-artifact validation. It derives version and release notes from the immutable checkout; the root package, wiki package and dependency, release manifest, and changelog must agree. Existing matching tags/releases can be retried, conflicting tags fail without being moved, and delayed older versions cannot replace newer versions as latest.

Only after native assets and the crate succeed does finalization mark the selected PR tagged. Proposal maintenance requires successful current-main CI and explicitly fails with recovery instructions when merged pending release PRs still block the sequence. It never silently tags those other releases. Automatic and manual runs share the non-cancelling release-main concurrency group with queue: max, avoiding replacement of pending release work.

The workflow implementation checkout uses github.workflow_sha; source identity always comes from verified CI or an existing tag. CI-run recovery can therefore validate old source commits using the current metadata helper. Artifact building and source package checks still use the immutable release source.

## Release promotion

Automatic publication downloads all four retained internal artifacts from the exact CI run that triggered Release. A single validation job verifies target completeness, provenance, versions, digests, extraction, installers, and packaged behavior. Missing or expired artifacts stop automatic publication.

The single asset publisher creates the exact tag and GitHub Release and uploads the validated complete set. Crate publication is allowed for automatic releases and explicit CI-run recovery, remains protected by the `cargo` environment, and starts after native asset publication succeeds. Existing-tag manual recovery remains native-assets-only. The environment restricts deployments to `main` but has no required reviewers, so publication remains unattended.

Crate upload is bounded and registry-aware. The publisher checks whether the exact immutable version already exists before uploading, retries transient failures with backoff, and checks again after every failed response so an accepted upload with a lost response is treated as success. Cargo's package verification may compile the extracted source package with the `dev` profile; that is not a distributed binary. Native release archives remain separate `--release` builds produced and smoked by the artifact contract.

The compressed crates.io source package must not exceed 10 MiB. CI and Release verify the generated `.crate` by exact byte count after Cargo's dry-run. Bundled Ladybug extension binaries are stored as lossless XZ streams in the source package and decompressed before cache seeding; checksum comparison preserves the official extension bytes, while native archives retain the same optimized runtime behavior.

## Recovery and dry-run

Manual dispatch on main requires exactly one target:

- resume-ci-run selects the release merge's own successful CI run, requires promotion of its retained artifacts, and can resume both native assets and crates.io, including creating a missing tag/release.
- publish-existing-tag selects an existing strict tag, locates exact-SHA successful main-push CI, and republishes native assets only. It retains manual-only rebuild-if-missing, which rebuilds all four targets without mixing artifact sources.

Dry-run defaults to true and executes identity, production, and artifact checks without creating tags, releases, labels, proposals, or publishing crates. Dispatch the same target with dry-run false only after validation. Automatic and CI-run recovery never substitute a run or rebuild missing artifacts. Complete recovery finalizes the release PR before the next proposal is maintained; do not remove a pending label merely to suppress release-please's outstanding-release warning.

The asset publisher prefers the cargo environment's optional RELEASE_PUBLISH_TOKEN, falling back to GITHUB_TOKEN. Historical targets that differ from current main's workflow files may require a repository-scoped token with Contents and Workflows write permissions. If no such token is configured, an owner can create the exact tag/release after a successful dry-run and then resume the same target. Never copy a local CLI login credential into Actions secrets as a recovery shortcut.

## Change discipline

A platform fix changes the typed target definition or native setup action for that target. Linux and macOS commands must not acquire Windows-only feature strings. Workflow policy tests enforce topology and permissions; direct `xtask` unit tests enforce target mapping, archive structure, checksums, provenance, and validation behavior.

Ruleset migration is a post-merge rollout step: first observe a successful `main` run exposing `required`, then make `required` the sole required context with strict branch freshness while preserving existing rules and bypasses.

Related: [Repository Ownership Map](./repository-map.md) and [Architecture Invariants](./invariants.md).
