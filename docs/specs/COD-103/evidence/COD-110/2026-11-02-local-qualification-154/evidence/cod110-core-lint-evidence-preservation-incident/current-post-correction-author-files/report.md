# COD-110 shared Rust contract slice — backend handoff

## Status, authority, and snapshot

- **Status:** bounded implementation is complete and worker checks pass for shared Rust candidate validation, checkout binding, CLI behavior, and final package preflight. This is not independent acceptance, release-workflow/YAML integration, hosted scheduling, publication, PR handoff, Done, merge, or release. I stopped all source/Cargo/private writes; no process remains and this three-file lease is released.
- **Worktree:** `/private/tmp/codebasegraph-cod110-20261101`, docs-only HEAD `754c82f4dc22c203c22a433f5bbae5f2f6e80231`, tree `fdf2f21db4c05965a7c4020075a9e66fddfa2db5`. Modified only `ci.rs`, `main.rs`, `release.rs`; no commit or staging.
- **Authority:** approved five contracts remain SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8` / approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`. Compatibility decision is `49612ec66814ad6b883a0c4c3c624911bd5a8825:docs/specs/COD-103/decisions/2026-11-01-cod110-compatibility.md`, separate record `754c82f4dc22c203c22a433f5bbae5f2f6e80231:docs/specs/COD-103/approvals/2026-11-01-cod110-compatibility.md`; user decision `2917ddb7-8746-407a-a37d-23b1e72396bf`. Existing Scryer design/execution carrier node-1 / `chg-wzt5hk`; no model, claim, signoff, ingest, fold, or tracking mutation.

## Implementation summary

- `ci.rs`: one `ReleaseCandidate`/`validate_release_candidate(root, expected_version)` core over the six generated files, using contextual TOML/manifest/Cargo.lock/changelog readers. It verifies all generated version identities, unique source-less/checksum-less local lock entries, and one nonempty exact-version changelog section. Strict candidate/tag grammar rejects leading zeroes including `v01.2.3`; ordinary compatibility callers keep their old behavior. Candidate snapshot consistency is not the classifier's B→H transition/history proof.
- `verify_checkout` retains full expected-SHA/HEAD binding, regular tracked six metadata files, and fail-closed tracked cleanliness. It checks index and worktree separately, compares Git-normalized bytes using `hash-object --path`, and allows clean `assume-unchanged` / `skip-worktree` paths only after actual indexed/tree/worktree byte checks without changing the real index. Clean flagged tracked `src/lib.rs` outside the six metadata files is allowed; staged cancellations and dirty bytes hidden by flags are rejected. Tests compare actual flags/index entries before and after and include CRLF Git normalization.
- `main.rs`: release-gate `--release-candidate` requires `--source-sha`, and checkout validation precedes ordinary gate checks. `verify-release-version` delegates to strict tag parsing/shared candidate validation. Ordinary `check_release_version_sync`, `cargo_version`, `dependency_version`, and `release_version` bodies are unchanged; an ordinary reader regression confirms those callers still accept `01.2.3`.
- `release.rs`: `read_metadata` delegates to shared verification while preserving the existing release JSON shape. New `release-package-preflight` checks the selected source and tag, requires a new output file outside source, queries exact crates.io status with existing curl time bounds/User-Agent, selects 404 → `cargo publish --dry-run --locked` or matching 200 → `cargo package --locked` with normal verification, uses a fresh absolute temp target and exact crate path, checks the existing 10,485,760-byte limit and SHA, rechecks source, writes a success-only schema-1 report, and cleans its temp target. Closed fake-process tests cover status/error/Cargo/output/size paths. No live network, Cargo package or publication was run.

## Worker checks executed

Environment: macOS, Cargo 1.96 / rustc 1.96, `native-tests` profile, offline and locked, no explicit features; Ladybug 0.19.0 `/private/tmp/codebasegraph-copy153-evidence/native019`, OpenSSL 3.6.2 `/opt/homebrew/Cellar/openssl@3/3.6.2/lib`, deployment target 13.3, jobs 2. No install, dependency/config/manifest/lock/feature/profile changes, RUSTFLAGS or target-dir override.

- `cargo fmt --all --check`: pass.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests ci::tests::`: 29 passed, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests release::tests::`: 16 passed, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::release_gate_`: 3 passed, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::release_identity_validation_is_strict`: 1 passed, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::ordinary_release_version_readers_keep_existing_permissive_manifest_behavior`: 1 passed, 0 failed.
- `git diff --check`: passed. No warnings in final scoped outputs.

The exact focused-test binary was observed at `target/native-tests/deps/xtask-eed5a74c0d8f01ef`, SHA-256 `445799fe403b5221129860cff65524b0a32f8208923eddc988da314e15f39bb1`, 3,372,080 bytes. This identifies local worker execution only.

Not run: post-change full 111-test suite, workspace Clippy, full CI, production YAML policy/actual-body tests (they await real Production wiring), hosted proof, real registry/network, actual Cargo package/publish, independent acceptance, or Final Reviewer. These remain pending and are not passes.

## Snapshot and handoff

- `ci.rs`: SHA-256 `2f5f92b47692c23a12f35f95b6ac7b0c4eb79a1bc0adb861a1a93eb4050697fc`, 83771 bytes.
- `main.rs`: SHA-256 `c453cc9515665a43eafee9cd43b6f05c1bd7ffb746c3e5a3456389825fbec2d2`, 240119 bytes.
- `release.rs`: SHA-256 `34090159b154bb5a9676f50b441313ba11e70d43211ea5d24748322b0afe9b62`, 30161 bytes.
- Current exact 3-file diff: SHA-256 `680d5bdd164f9aea8c1ddb559e2d75d56cd619a90d7c8d8fc6d54b870d92f30e`, 84304 bytes.
- Protected `release/workflow_tests.rs`, Cargo manifests/lock, workflows/docs remain unchanged; exact hashes are in `files.json`.

Private handoff directory: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-7b3f2b06/cod110-shared-rust-contract`. It contains the exact source diff, raw outputs, preserved failed attempts, `report.md`, `result.json` and `files.json`; the manifest excludes itself. Constants changed: **none**. No integrated acceptance or PR/Done/merge/release is claimed.

**Next owner:** Engineer reviews this dirty snapshot and receipts, then the same independent Evidence Verifier rechecks the corrected Rust contract. Production consumers remain blocked until that handoff.
