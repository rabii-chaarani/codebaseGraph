# COD-110 shared Rust contract slice — backend handoff

## Status, authority, and snapshot

- **Status:** implementation slice complete; worker checks pass for the shared Rust core, checkout binding, CLI behavior, and package preflight process boundary. This is not independent acceptance, release-workflow policy/wiring, hosted scheduling, publication, PR handoff, Done, merge, or release. All source/Cargo/private writes have stopped; no processes remain and this three-file lease is released.
- **Worktree:** `/private/tmp/codebasegraph-cod110-20261101`, current documentary HEAD `754c82f4dc22c203c22a433f5bbae5f2f6e80231`, tree `fdf2f21db4c05965a7c4020075a9e66fddfa2db5`. The three Rust files were already dirty when this phase began; their initial bytes were preserved and this worker added only its assigned changes. No Git commit or staging occurred.
- **Authority:** five COD-103 contracts remain approved at `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; separate base approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`. The compatibility decision is `49612ec66814ad6b883a0c4c3c624911bd5a8825:docs/specs/COD-103/decisions/2026-11-01-cod110-compatibility.md` (SHA-256 `19f05279eba4e2f93709c3cc6c0aa58b3a37845f8236b5a5ee148e102ab525a0`) and separate record `754c82f4dc22c203c22a433f5bbae5f2f6e80231:docs/specs/COD-103/approvals/2026-11-01-cod110-compatibility.md` (SHA-256 `c70567850a096256dde58915fcc1e0e5c50ec01338f27e0f54d04021051a31ab`), recording the user's approval comment `2917ddb7-8746-407a-a37d-23b1e72396bf`. Existing Scryer `node-1` COD-110 DESIGN/EXECUTION and change `chg-wzt5hk` remain. No model, claim, ledger, signoff, verifier-fold, or Linear mutations were made.
- **Changed source:** `crates/xtask/src/ci.rs` SHA-256 `2f5f92b47692c23a12f35f95b6ac7b0c4eb79a1bc0adb861a1a93eb4050697fc` (85,950 bytes); `crates/xtask/src/main.rs` `c453cc9515665a43eafee9cd43b6f05c1bd7ffb746c3e5a3456389825fbec2d2` (240,119 bytes); `crates/xtask/src/release.rs` `f8ef9da0adfac3921991b462fd66c93ba8fe719ae6e7973766dc8529df57e734` (30,346 bytes). Final three-file diff SHA-256 `03a7a018b791680c4e0716bb66b5ed06c2c1980bad08b09cbf6541d2db9df8f9` (85,486 bytes).
- Non-owned files are unchanged: `release/workflow_tests.rs` `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450`; root Cargo manifest `5a08eb5f34916f35ee3e3964d6ab4c9a5c6f80b7fe374f6cf2d44daf441aeaa4`; lock `63d868f91aa52acbddf0eb7e9385a101c3a59c02f7e527ced82d1993299f5afb`; xtask manifest `206d3a9af5953963659c0cc292e1356c8905f44288c417568bb7f146c4716198`; CI YAML `b59d7028101120f6885870cec054a65d9788f55d364dc0f2eb2b53bd4fd9c339`; release YAML `e11c179705340bea6230dfac1a50c055d34fb7137d20f2c271e335c589653117`; `docs/release.md` `90d84f6614be6cd74b8340dd73b1a2c1000457af319475cf8939b1c316ebf486`.

## Delivered behavior

### Shared candidate metadata (`ci.rs`)

- Added `ReleaseCandidate { version, notes }` and one `validate_release_candidate(root, expected_version)` entry point. It reads the fixed six generated files; shares contextual TOML, manifest, Cargo.lock, and changelog readers; checks aligned root/wiki/local-dependency/xtask/manifest and three workspace package versions; requires exactly one source-less/checksum-less local package record for `codebase-graph`, `k-wiki`, and `xtask`; selects exactly one nonempty exact-version notes section; and optionally binds the selected version.
- Candidate/tag/final validation uses strict `X.Y.Z` grammar and rejects leading-zero forms such as `v01.2.3`. The already-approved decision is reflected exactly; ordinary compatibility readers do not inherit candidate-only restrictions.
- Candidate validity is snapshot consistency only. It does not require a B→H metadata transition, release-version increase, or changelog-history rewrite. Existing B→H classifier remains the additional strict six-file semantic proof. Lock parsing was factored so the classifier retains its prior behavior while candidate validation adds required checksum exclusion.
- Checkout binding retains full SHA/HEAD equality and required six tracked regular files. It independently rejects staged and unflagged tracked changes, examines clean/dirty `assume-unchanged` and `skip-worktree` paths by actual Git-normalized bytes without mutating the real index, and allows clean flags including tracked `src/lib.rs` outside the six metadata files. Real Git fixtures compare index/flag bytes before/after and cover staged/worktree cancellation, hidden dirty source, ordinary dirty source, and CRLF/Git text normalization. Benign untracked build output remains allowed.

### CLI and compatibility (`main.rs`)

- Added existing `release-gate --release-candidate` mode; it requires `--source-sha`, and `verify_checkout` runs before the usual gate checks. Existing confirmations, conda handling, no-flag behavior, and ordinary checks stay intact.
- `verify-release-version` uses the approved strict tag parser and shared candidate validator. Existing ordinary functions `check_release_version_sync`, `cargo_version`, `dependency_version`, and `release_version` were byte-compared unchanged to their pre-phase bodies. A regression confirms the ordinary cargo-version readers still accept `01.2.3` values while candidate/tag validation rejects the leading-zero case.

### Release metadata and final package preflight (`release.rs`)

- `read_metadata(root, sha)` delegates to checkout verification and candidate validation while retaining the `ReleaseMetadata` fields `source_sha`, `version`, `tag_name`, and `notes`. Its former partial/minimal metadata fixture was replaced with a complete temporary-Git generated layout.
- Added `release-package-preflight --source-dir PATH --source-sha S --tag vX.Y.Z --output PATH`. It verifies selected source/version, requires output outside source and not already present, then queries the exact crates.io version status with existing curl and 10-second connect/30-second maximum/User-Agent behavior. Only 404 selects `cargo publish --dry-run --locked`; only a matching 200 selects `cargo package --locked` with normal verification. Other statuses, malformed/mismatched data, transport failure, Cargo failure, missing exact archive, oversize, or changed source fail without a report.
- Cargo runs with `current_dir` set to selected source and a fresh absolute owned temp `--target-dir`. It validates only `package/codebase-graph-<version>.crate`, measures the existing 10,485,760-byte limit, hashes it, rechecks source, emits schema-1 success-only report outside source, and removes its owned target on returned success/error. Tests inject executable paths to closed curl/Cargo doubles; no global PATH/config, real network, actual package/publication, or new dependency was used.

## Actual bounded worker checks

Environment: macOS, Cargo 1.96.0 / rustc 1.96.0, `native-tests` profile, offline/locked, no explicit features, Ladybug 0.19.0 `/private/tmp/codebasegraph-copy153-evidence/native019`, OpenSSL 3.6.2 `/opt/homebrew/Cellar/openssl@3/3.6.2/lib`, deployment target 13.3, `CARGO_BUILD_JOBS=2`; no RUSTFLAGS, no CARGO_TARGET_DIR override, no install.

- `cargo fmt --all --check`: passed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests ci::tests::`: **29 passed**, 0 failed. Includes classifier tests and checkout/index/F1 cases.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests release::tests::`: **16 passed**, 0 failed. Includes actual six-file candidate positives, strict grammar, lock checksum/source failures, ordinary metadata, and fake process package preflight branches/boundary/report checks.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::release_gate_`: **3 passed**, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::release_identity_validation_is_strict`: **1 passed**, 0 failed.
- `cargo test --offline --locked -p xtask --bin xtask --profile native-tests tests::ordinary_release_version_readers_keep_existing_permissive_manifest_behavior`: **1 passed**, 0 failed.
- Read-only final Git status plus `git diff --check`: passed; only the three leased source files changed.

Final tested xtask binary SHA-256 `445799fe403b5221129860cff65524b0a32f8208923eddc988da314e15f39bb1` at `target/native-tests/deps/xtask-eed5a74c0d8f01ef`, 3,372,080 bytes. This identifies the locally executed focused-test binary only.

No full 111-test suite after current changes, Clippy, full workspace tests, `release-gate` ordinary/product command, release workflow body tests, network, actual Cargo package/publish, hosted CI, independent verification, or final review was run. Workflow consumers and release/workflow tests are explicitly deferred until Production supplies actual YAML wiring. These limits are not acceptance.

## Development failures retained

Raw stdout for each actual failed development attempt is preserved in this phase directory: initial missing/import/type compile failure; initial F1 diagnostic and six-file flag-scope failures; release fixture/log/cleanup assertion errors; ordinary compatibility test import error. Subsequent focused checks at the final snapshot passed. Failures are not erased or presented as final passes; see `failed-*.stdout.txt` and `result.json` for exact commands, exits, durations, and reasons.

## Handoff and stop

Private phase directory: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-7b3f2b06/cod110-shared-rust-contract`. It contains full exact diff, per-command raw outputs, failure outputs, `report.md`, `result.json`, and `files.json`. The manifest lists every other artifact in the directory and intentionally excludes its own recursive hash. No Git commit, PR, model update, claim fold, workflow edit, release request, registry API call, platform qualification, or publication occurred. Constants changed: **none**.

**Next owner:** Engineer to review this three-file dirty diff and receipts; the same independent Evidence Verifier then independently checks the corrected shared Rust contract before Production starts any workflow wiring. I have stopped source/Cargo/private artifact writes; no outstanding Bash process remains.
