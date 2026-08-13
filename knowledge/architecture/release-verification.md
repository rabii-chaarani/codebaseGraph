---
description: Build-once/promote contract, exact-SHA gating, native target isolation, artifact provenance, and release recovery.
resource: repository-architecture
tags:
- architecture
- ci
- release
- artifacts
- provenance
timestamp: 2026-08-13
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

## Release promotion

Release resolution produces a tag, package version, and exact commit SHA once. Before any publication, the workflow finds a completed successful `push` run of `ci.yml` whose `head_sha` equals that release SHA. A concurrently running match may be awaited for a bounded period; branch-level or manually confirmed green status is never sufficient.

The default path downloads all four internal artifacts from that exact CI run. A single validation job verifies target completeness, provenance, versions, digests, extraction, installers, and packaged behavior. Only the single asset publisher receives `contents: write`; it uploads the already validated complete set. Crate publication is automatic-release-only, remains protected by the `cargo` environment, and starts after native asset publication succeeds.

## Recovery and dry-run

Manual dispatch always requires an existing tag and exact-SHA successful CI:

- `promote` fails if any retained artifact is unavailable.
- `rebuild-if-missing` is manual-only and rebuilds all four targets through the same reusable workflow. Promoted and rebuilt targets are never mixed.
- `dry-run` performs resolution, gating, artifact acquisition, validation, extraction, and smoke checks without modifying a GitHub release or publishing crates.
- Manual execution never publishes a crate.

Release concurrency is scoped to the tag with cancellation disabled so duplicate attempts cannot race publication.

## Change discipline

A platform fix changes the typed target definition or native setup action for that target. Linux and macOS commands must not acquire Windows-only feature strings. Workflow policy tests enforce topology and permissions; direct `xtask` unit tests enforce target mapping, archive structure, checksums, provenance, and validation behavior.

Ruleset migration is a post-merge rollout step: first observe a successful `main` run exposing `required`, then make `required` the sole required context with strict branch freshness while preserving existing rules and bypasses.

Related: [Repository Ownership Map](./repository-map.md) and [Architecture Invariants](./invariants.md).
