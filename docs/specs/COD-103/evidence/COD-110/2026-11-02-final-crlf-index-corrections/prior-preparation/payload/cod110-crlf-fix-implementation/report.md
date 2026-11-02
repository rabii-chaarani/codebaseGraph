# COD-110 CRLF implementation handoff

- Source: `/private/tmp/codebasegraph-cod110-20261101` (parent-bound baseline `862bb3b3c40c36b21cdcd388f596d52ad482530b`)
- Settled design: `cod110-crlf-correction-design/report.md`, SHA-256 `4e1b42334f078eb25a3913155d080fd30276fcce26d171dcb1fa2825fdbe32e0`
- Scope: only `crates/xtask/src/ci.rs` and `crates/xtask/src/release.rs`; no source files other than these were edited by this owner.

## Source hashes

| File | Before SHA-256 | After SHA-256 |
| --- | --- | --- |
| `crates/xtask/src/ci.rs` | `7db803caf383175b0842c9b01e2725588a96794fc8e9b021db550fd4e129139a` | `3c52c2c8e01262cf530c1e2c286cd77c8b7c50aace8fbf532cdc605c48178570` |
| `crates/xtask/src/release.rs` | `34090159b154bb5a9676f50b441313ba11e70d43211ea5d24748322b0afe9b62` | `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594` |

Exact two-file snapshots are in `before/` and `after/`; the complete unified diff is `diff.patch`.

## Change summary

After the existing raw `MAX_BLOB_SIZE` check, the candidate read loop now converts CRLF pairs to LF in a private owned validation buffer for all six metadata files. Lone CR and all other bytes are copied. The production recognizers, classifier/raw spans, checkout proof, and ordinary helpers were not changed. Candidate regressions cover all-six LF/CRLF/mixed equivalence, no filesystem writes, retained bare-CR/duplicate/ambiguous/mismatch rejection, and unchanged raw-CRLF classifier rejection. The ordinary LF classifier positive and real product-root positive remain in the executed set.

## Checks

1. `rustfmt --edition 2021 --config skip_children=true crates/xtask/src/ci.rs crates/xtask/src/release.rs` — exit 0; raw output: `raw-results/rustfmt.stdout.txt`.
2. `cargo fmt --all --check` — exit 0; raw output: `raw-results/cargo-fmt-check.stdout.txt`.
3. Environment:
   - `LBUG_LIBRARY_DIR=/private/tmp/codebasegraph-copy153-evidence/native019`
   - `LBUG_INCLUDE_DIR=/private/tmp/codebasegraph-copy153-evidence/native019`
   - `LBUG_VERSION=0.19.0`
   - `OPENSSL_LIB_DIR=/opt/homebrew/Cellar/openssl@3/3.6.2/lib`
   - `MACOSX_DEPLOYMENT_TARGET=13.3`
   - `CARGO_BUILD_JOBS=2`
   - `GIT_OPTIONAL_LOCKS=0`
   - default target; no `RUSTFLAGS`, added features, or profile overrides.

   Command/filter: `cargo test --offline --locked -p xtask --bin xtask --profile native-tests metadata`.
   Exit 0. **32 passed, 0 failed, 0 ignored, 125 filtered**; output: `raw-results/cargo-test.stdout.txt`. This is the assigned author check only, not independent acceptance.

## Ownership boundary

Author source, private-evidence, and Cargo/Bash work is stopped and released. Independent macOS/Windows qualification and the focused original-Sol/MAX review remain pending; no PR update, CI rerun, Git operation, or merge was performed.
