# Release PR #162 repair evidence

Work: **COD-100 / GH-162** · Scryer change: `chg-9sernm`.

The user approved “make it 4.0.1 and resolve the issues first” on 2026-10-27. This is a proportional repair under the existing [release process](../../../docs/release.md) and the canonical wiki `knowledge/architecture/release-verification.md`; no new architecture or publication contract is introduced. Merge, exact-source main CI, publication and maintainer-owned closure remain separate.

## Acceptance evidence

| Criterion | Evidence |
| --- | --- |
| Stop replaying the old `Release-As: 1.2.2` and unrelated historical notes | `generator-audit.md`, `generator-4.0.1.json`: actual release-please 17.6.0, writes blocked, manifest 4.0.0 plus failed-release bootstrap cutoff yields 4.0.1. `generator-future.json` distinguishes a local overlay from synthetic future tagged states. |
| Keep all package/dependency/lock/manifest versions consistent at 4.0.1 | `verification.json` snapshot hashes; locked Cargo metadata; `verify-release-version v4.0.1`; immutable metadata helper after commit. |
| Preserve unreleased storage changes and report 4.0.0 truthfully | `CHANGELOG.md` 4.0.1 includes the storage feature and breaking change; comparison starts from published v3.0.4. Historical 4.0.0 section stays. Failed PR156 is not tagged or described as published. |
| Stage the complete supported artifact set without accepting an incomplete set | New `staged_assets_match_the_supported_native_targets` executes actual workflow Bash with two target archives and checksums. `asset-staging-baseline.log` demonstrates failure before 8→4 fix; `xtask.junit.xml` records 44 passing tests, including incomplete-set rejection. |
| Preserve release gates | `local-checks.log`: format, warnings-denied xtask Clippy, workflow policy, normal and production repository gates, and v4.0.1 version validation pass. Confirmation arguments reproduce existing job settings, not new environment approval. |

## Reproduction

From this checkout, with an isolated `CARGO_TARGET_DIR` if sharing other worktrees:

```text
cargo fmt --check
cargo nextest run -p xtask --locked --config-file docs/testing/release-pr-162/nextest.toml --profile cod100
cargo clippy -p xtask --all-targets --locked -- -D warnings
cargo run -p xtask --locked -- check-workflows
cargo run -p xtask --locked -- release-gate
cargo run -p xtask --locked -- release-gate --production --confirm release-environment --confirm private-vulnerability-reporting
cargo run -p xtask --locked -- verify-release-version v4.0.1
cargo metadata --no-deps --locked --format-version 1
cargo run -p xtask --locked -- release-metadata --source-sha <this-checkout-HEAD> --source-dir .
```

The generator check is a separate isolated Node project, not a repository dependency. Install `release-please@17.6.0`
there and copy `read-only-generation.cjs` into that directory. Supply a GitHub read token on stdin through the normal
CLI credential flow; never save it to a file or log it. The script guards REST writes and GraphQL mutations, only
builds candidates/change sets, and writes the two scenario reports in its current directory. It tests current main,
so its output changes after this PR merges. The committed JSON records the exact live-main baseline named above.

The recorded environment is local Darwin ARM64, Cargo 1.96.0, nextest 0.9.140. These checks do not qualify native Windows/macOS consumer archives or substitute for PR/main CI. No tag, release, crate, workflow dispatch, merge or release-label mutation is part of this repair.

The original working tree has unrelated edits and an older checkout. Scryer is configured against it, so these isolated-snapshot passing reports must not be fingerprinted as evidence for those different source files. Source/test hashes and JUnit are retained here for correct-snapshot reconciliation. No whole-project drift advancement was performed.

Until this configuration reaches main, a new ordinary main CI completion can regenerate the old proposal over the open PR. There is no supported open-PR hold label in release-please 17.6.0. Do not bypass recognition by removing pending labels, fabricate a v4.0.0 tag, or change workflow permissions to mask that risk.
