# CBG-PLAT-01 — CI-only S06–S08 execution

## Identity and authority

- Source baseline: PR153 `862a8f88d485d5bd257ebd9456c194156fd4ee94`. Delivered fixes: [`d6dd34493e9da6f80847240bafcc92dbcaed2c38`](https://github.com/rabii-chaarani/codebaseGraph/commit/d6dd34493e9da6f80847240bafcc92dbcaed2c38) in [existing PR153](https://github.com/rabii-chaarani/codebaseGraph/pull/153).
- Exact user approval: [CI-only S06–S08 record](https://github.com/rabii-chaarani/codebaseGraph/blob/7650f264fe0cbfa7fa328840722929feac1f32df/docs/specs/CBG-PLAT-01/approvals/ci-only-s06-s08.md), approved spec0.10 full SHA `1a9db7b558b60248888408daa6268a3b45f2f0df`. User: **“I approve CI-only implementation scope”**.
- Issues: [COD-57](https://linear.app/codebasegraph/issue/COD-57), [COD-58](https://linear.app/codebasegraph/issue/COD-58), [COD-59](https://linear.app/codebasegraph/issue/COD-59) were delivered In Review pending native CI. The subsequent exact-head [hosted verification](ci-only-hosted-qualification.md) records their final affected-native results/status. COD-51 remains Backlog for other implementation scope. No other slices, packaging, runtime policy changes, full-platform closure, merge/deployment/restart included.
- Changed source: only `src/adapters/cli/format/help.rs`, `src/db_writer/phase.rs` test, `src/staging_writer/files.rs` internal visibility, `src/staging_writer/mod.rs` test-only re-export and `tests/support/storage_maintenance/process.rs` fixture/tests.

## Implemented behavior

| Slice/criterion | Delivered change |
|---|---|
| S06 / R06 / A10 / COD-57 | Watch help states existing parallel default; parser/default/opt-out behavior unchanged. Existing test retained. |
| S07 / R07 / A11 / COD-58 | Fixture renders the actual production COPY path via an internal test re-export, creates a spaced filename, checks actual canonical file identity and exact4-byte metadata. No production renderer/parser/COPY mutation change. |
| S08 / R08 / A12 / COD-59 | Test observer accepts WouldBlock or a present raw OS code matching fs2 contention. Original5-second deadline,10-ms interval and complete epoch/birth/integrity assertions preserved. Added Windows real independent-handle contention/release regression and cross-platform missing-lock propagation. |

No production constants changed. The new regression's100-ms held-result receive and2-second channel receives are test-only synchronization, not new observer timeouts. The black-box Windows test has an ordinary scheduling caveat: its independent contention handshake establishes the owned lock state but cannot instrument the observer's exact internal try-lock instant. No helper instrumentation/general framework was added purely for this test.

## Executed local checks

Host: aarch64-apple-darwin, Rust/Cargo1.96.0, source native Cargo environment in isolated worktree. A prior exact-PR review target directory was reused for dependency compilation; Cargo rebuilt this checkout's changed crate/CLI. Normal CLI SHA-256: `a6c8042acc1bbc1f8b709cfe53fa009dbf4edff081a504e41b001ccb28ed491e`. This is not a frozen previous-branch binary.

| Command/method | Actual result | Evidence |
|---|---|---|
| `cargo test --locked -p codebase-graph --lib -- adapters::cli::tests::dispatch_materialize:: db_writer::phase::tests:: staging_writer::tests::copy_path_ --test-threads=1` |42 passed|[focused-unit-tests.log](ci-only-evidence/focused-unit-tests.log)|
| `cargo test --locked -p codebase-graph --test storage_maintenance covered_epoch_observer -- --test-threads=1` |1 passed on Mac; Windows-only contention case not compiled/executed here|[observer-fixture-tests.log](ci-only-evidence/observer-fixture-tests.log)|
| `cargo test --locked -p codebase-graph --test storage_maintenance isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth -- --exact --test-threads=1`, with explicit newly built `CARGO_BIN_EXE_codebase-graph` |1 passed on final complete fixture snapshot (69.46s); earlier same case also passed69.96s, not double-counted|[epoch-publication-integration-final.log](ci-only-evidence/epoch-publication-integration-final.log)|
| `cargo build --locked --bin codebase-graph` |passed|[cli-build.log](ci-only-evidence/cli-build.log)|
| `cargo fmt --check` and `git diff --check` |passed after worker formatting; no deadline/assertion relaxation|[format.log](ci-only-evidence/format.log)|
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |passed final snapshot|[clippy-final.log](ci-only-evidence/clippy-final.log)|
| Fresh independent read-only focused review |no confirmed stable-diff findings; native Windows execution not performed by reviewer|[ci-only-review.md](ci-only-evidence/ci-only-review.md)|

**44 distinct selected tests passed locally.** Full workspace and native Windows runtime were not executed locally. Stable source hashes/diff are in [review](ci-only-evidence/ci-only-review.md), [worker report](ci-only-evidence/epoch-worker.md), [exact delivered code diff](https://github.com/rabii-chaarani/codebaseGraph/commit/d6dd34493e9da6f80847240bafcc92dbcaed2c38.diff) and [provenance](ci-only-evidence/provenance.json). Saved text logs omit redundant trailing blank lines only.

## Qualification and delivery limits

Push updated existing PR153 to the exact code SHA above; no force push or merge. Initial post-push discovery had no runs yet. The subsequent [head-bound hosted verification](ci-only-hosted-qualification.md) records run40426019260 and actual named testcase/native cohort/artifact outcomes. No old green/failed run or skipped artifact counts as current passing execution. A normal automatic push trigger is not a manual CI rerun.

If actual contention still exceeds five seconds after classification is fixed, investigate separately; do not silently increase deadline or weaken assertions. Archive/clean-consumer/milestone qualification remains outside this CI-only implementation approval. Do not close issues until their native criteria/review/verification are satisfied.

## Architecture/procedure reconciliation

Scryer ledger `chg-pzt87f` was opened/signed off for this approved docs/test-restoration scope, with0 model entries. `locate` confirmed these fixture/help edits add no modeled responsibilities and do not change anchored `run_isolated_phase` behavior. The empty ledger was closed under proportional rules (tool labels empty closure abandoned); no intent implementation, current-PR source-map sync or broad drift reconciliation is claimed. Configured graph/model remained bound to the older main checkout; exact source/test evidence governs this PR worktree. No unrelated pending resource-ceiling claims were consumed.

Reusable-knowledge assessment: existing fs2 contention and platform-path lessons already cover the narrow procedure; this record preserves executed evidence rather than duplicating transient status into memory. No generated wiki state or implementation source outside the five files changed.
