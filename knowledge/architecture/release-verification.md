---
description: Build-once promotion, CI-completion release orchestration, exact-run gating, and manual recovery.
resource: repository-architecture
tags:
- architecture
- artifacts
- ci
- provenance
- release
timestamp: 2026-08-22
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

The Release workflow is triggered by completion of the `CI` workflow on `main`, not independently by a branch push. Release-please runs only when the triggering workflow was a completed successful `push` run on `main` and its `head_sha` is still the current `main` tip. Before invoking release-please, the workflow classifies whether that SHA belongs to exactly one merged pull request targeting `main` from the repository-owned release-please branch with the pending-release label; ambiguous or untrusted identities fail closed. Ordinary successful commits run release-please with GitHub Release and tag creation disabled, allowing proposal maintenance without publishing a stale pending release. Only a successful release-merge commit enables tag creation. Failed, cancelled, pull-request, and non-main completions may create a skipped Release workflow record but cannot mutate release state. A completion that is already stale is rejected before release-please. Because GitHub does not provide an atomic branch-tip check plus action invocation, the workflow rechecks the current tip, release classification, and release SHA immediately after release-please; if `main` advanced during the action, all artifact and crate publication stops.

Automatic mode binds release identity directly to `github.event.workflow_run.id` and `github.event.workflow_run.head_sha`. It revalidates that run's workflow path, event, branch, status, conclusion, and SHA, and it requires any release-please tag to resolve to that same SHA. If a release-merge commit fails CI, a later ordinary commit cannot publish its pending tag; the corrected release must be represented by a new release pull request whose merge commit passes CI. Automatic mode never uses `github.sha`, polls for a substitute CI run, or rebuilds missing artifacts. Automatic runs serialize in the `release-main` concurrency group without cancellation, so only a successful current-tip completion owns orchestration.

## Release promotion

Automatic publication downloads all four retained internal artifacts from the exact CI run that triggered Release. A single validation job verifies target completeness, provenance, versions, digests, extraction, installers, and packaged behavior. Missing or expired artifacts stop automatic publication.

Only the single asset publisher receives `contents: write`; it uploads the already validated complete set. Crate publication is automatic-release-only, remains protected by the `cargo` environment, and starts after native asset publication succeeds. The environment restricts deployments to `main` but has no required reviewers, so publication remains unattended.

Crate upload is bounded and registry-aware. The publisher checks whether the exact immutable version already exists before uploading, retries transient failures with backoff, and checks again after every failed response so an accepted upload with a lost response is treated as success. Cargo's package verification may compile the extracted source package with the `dev` profile; that is not a distributed binary. Native release archives remain separate `--release` builds produced and smoked by the artifact contract.

## Recovery and dry-run

Manual dispatch always requires an existing tag and exact-SHA successful CI. Unlike automatic mode, manual recovery may search for the successful CI run for that tag SHA and wait for a concurrently running match for a bounded period:

- `promote` fails if any retained artifact is unavailable.
- `rebuild-if-missing` is manual-only and rebuilds all four targets through the same reusable workflow. Promoted and rebuilt targets are never mixed.
- `dry-run` performs resolution, gating, artifact acquisition, validation, extraction, and smoke checks without modifying a GitHub release or publishing crates.
- Manual execution never publishes a crate.

Manual release concurrency is scoped to the tag with cancellation disabled so duplicate attempts cannot race publication.

## Change discipline

A platform fix changes the typed target definition or native setup action for that target. Linux and macOS commands must not acquire Windows-only feature strings. Workflow policy tests enforce topology and permissions; direct `xtask` unit tests enforce target mapping, archive structure, checksums, provenance, and validation behavior.

Ruleset migration is a post-merge rollout step: first observe a successful `main` run exposing `required`, then make `required` the sole required context with strict branch freshness while preserving existing rules and bypasses.

Related: [Repository Ownership Map](./repository-map.md) and [Architecture Invariants](./invariants.md).
