# COD-110-FR-001 index no-refresh implementation

- Repository: `/private/tmp/codebasegraph-cod110-20261101`; assigned source ownership: `crates/xtask/src/ci.rs` only.
- Follow-up design handoff: `cod110-index-no-refresh-design/report.md`, SHA-256 `105cfbd358e5ae21314eecf4408bea21154501e0ebc4392b88ea23521ee125d3`.
- Existing CRLF correction retained. Parent baseline before this follow-up: `ci.rs` SHA-256 `3c52c2c8e01262cf530c1e2c286cd77c8b7c50aace8fbf532cdc605c48178570` (94601 bytes); `release.rs` retained at SHA-256 `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594`.
- Final `ci.rs`: SHA-256 `9cce67411b43c86a6797d35f5538bf2cc72227713c4dfa0baae5a02eb258e2fd` (95794 bytes).

## Change

Only the final worktree cleanliness probe in `verify_checkout` now invokes `git -c diff.autoRefreshIndex=false diff --quiet <expected_sha> --`. The staged `diff --cached` probe, Git helper, error mapping and exit handling are unchanged. This process-local override prevents Git porcelain diff from refreshing stat-only entries in the caller's real index; it persists no configuration and makes no weaker index/content assertion.

Added `verify_checkout_accepts_stale_stat_without_mutating_index`, using the existing private Git fixture. It moves tracked `Cargo.toml` mtime ten seconds before its original timestamp via the Rust standard-library `File::set_times` API (no sleep or dependency), confirms file bytes are unchanged, calls the actual checkout verifier, and compares the complete raw `.git/index` bytes before and after. The ten-second value is a test-only displacement, not a production constant.

## Commands and results

1. `rustfmt --edition 2021 --config skip_children=true crates/xtask/src/ci.rs` — exit 0.
2. `cargo fmt --all --check` — exit 0.
3. Exactly one assigned author test:
   `cargo test --offline --locked -p xtask --bin xtask --profile native-tests verify_checkout_accepts_stale_stat_without_mutating_index`
   with `LBUG_LIBRARY_DIR=LBUG_INCLUDE_DIR=/private/tmp/codebasegraph-copy153-evidence/native019`, `LBUG_VERSION=0.19.0`, `OPENSSL_LIB_DIR=/opt/homebrew/Cellar/openssl@3/3.6.2/lib`, `MACOSX_DEPLOYMENT_TARGET=13.3`, `CARGO_BUILD_JOBS=2`, and `GIT_OPTIONAL_LOCKS=0`.
   Exit 0: **1 passed, 0 failed, 157 filtered**. The raw output and exact commands are under `raw-results/`.

This is author evidence, not independent acceptance. No previous failure was replayed or waived. The independent macOS/Windows verifier and original Sol/MAX focused review must evaluate the final source before PR update.

## Private evidence

This new bundle contains exact before/after `ci.rs` snapshots, the incremental diff, raw format/test outputs, exact commands, and `checks.json`. The previously closed CRLF bundle was not modified.

## Stop/ownership

Source edit, private bundle and Cargo work are complete and released. No Git/PR/API/VM operation was performed. No other source file was changed. Further checks and review belong to the independent owners.
