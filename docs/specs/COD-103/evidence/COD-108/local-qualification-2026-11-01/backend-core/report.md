# COD-108 Backend Core Handoff

Status: implementation slice closed; source/Cargo writes stopped. This private artifact records the actual Codex backend results already observed in the session. It is not approval, hosted workflow evidence, or independent verification.

## Snapshot and scope

- Branch: `cod103-stack/cod-108`; baseline HEAD `90af76d3453a6f5cc0f032045ed48c4fd09c275c`.
- Working tree showed only new `crates/xtask/src/ci.rs` and modified `crates/xtask/src/main.rs`; no other project path changed (0).
- `ci.rs`: 48671 bytes, SHA-256 `b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f`.
- `main.rs`: 183023 bytes, SHA-256 `c175e0df3591a195cb38e89167bec160a880951718c9c431b73bf01d9d392957`.
- Build executable: `/private/tmp/codebasegraph-cod108-20261101/target/debug/xtask`, SHA-256 `f04b82d9548c283de31da9b994000a4775952e6a8b69d40da1b2e9a70ef26be6`.
- Environment: Darwin 25.6.0 arm64; rustc 1.96.0 (ac68faa20 2026-05-25); cargo 1.96.0 (30a34c682 2026-05-25); git version 2.52.0.

## Implemented core

CLI: `cargo run -p xtask --locked -- ci-mode --repo <git-object-checkout> --base-sha B --head-sha H`. Success emits one schema-version-1 JSON line with `mode`, bounded `reason_code`, and exact input SHAs. Invalid invocation/output failure is nonzero. Classifier and Git acquisition are private to xtask. Acquisition validates full commit identities, requires B ancestor of H, reads bounded immutable blobs, uses argument-array Git calls and NUL-delimited raw paths/modes, and selects full on unsafe or uncertain inputs. Metadata eligibility requires the full six-file release-generated set and byte-identical remainder after only unique contextual version spans are masked. No dependency was added. Workflow API trust, protected-base provenance, job scheduling, aggregate required-state logic, and workflow tests remain outside this backend slice.

## Executed checks

All results are transcript-derived from `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-7f3e2b93/01a196de-89e6-7479-9883-1ff6993d2751.jsonl` (snapshot SHA-256 `9a7b6d98c445fe07434143ea1038818e3d7c89e825d6c7288033433f4c9ca499`). No separate raw command log was saved. Bash records combined stdout/stderr for its commands; exact transcript captures are in `result.json`. No check was rerun to prepare these files.

- `cargo test -p xtask --locked ci::tests:: -- --nocapture`: exit 0, 13 passed / 0 failed. Covers generated PR165/167-shaped positive cases, ambiguity/history/version negatives, immutable Git ancestry/path/mode/add-delete/symlink/submodule/acquisition failures, and NUL/newline paths.
- `cargo test -p xtask --locked`: exit 0, 96 passed / 0 failed.
- `cargo fmt --check`: exit 0; no output.
- `cargo clippy -p xtask --all-targets --all-features --locked -- -D warnings`: exit 0; no warnings.
- `cargo build -p xtask --locked`: exit 0.

## Built-process CLI evidence

All three used executable `/private/tmp/codebasegraph-cod108-20261101/target/debug/xtask` (SHA-256 `f04b82d9548c283de31da9b994000a4775952e6a8b69d40da1b2e9a70ef26be6`) and the disposable Git fixture `/private/tmp/cod108-real-layout-b0af51a183ba468b9be4ca384f410228`. Raw bytes are preserved as repr, UTF-8, and base64 in `result.json`; the strings below are copied from the transcript tool result.

1. Eligible generated six-file metadata transition. B `e71c436d0207c2b03269b17d6b1a3a784faba22f`, H `12cb291412bb4417ece5961bdf41f38473c95c79`. Exact argv: `/private/tmp/codebasegraph-cod108-20261101/target/debug/xtask ci-mode --repo /private/tmp/cod108-real-layout-b0af51a183ba468b9be4ca384f410228 --base-sha e71c436d0207c2b03269b17d6b1a3a784faba22f --head-sha 12cb291412bb4417ece5961bdf41f38473c95c79`. Status 0; stdout `{"schema_version":1,"mode":"lightweight","reason_code":"metadata-only-version-bump","base_sha":"e71c436d0207c2b03269b17d6b1a3a784faba22f","head_sha":"12cb291412bb4417ece5961bdf41f38473c95c79"}\n`; stderr empty.
2. Ordinary source-code change. B `e71c436d0207c2b03269b17d6b1a3a784faba22f`, H `42450d3297a9673c59038f57e6256d5d29d979d6`. Exact argv: `/private/tmp/codebasegraph-cod108-20261101/target/debug/xtask ci-mode --repo /private/tmp/cod108-real-layout-b0af51a183ba468b9be4ca384f410228 --base-sha e71c436d0207c2b03269b17d6b1a3a784faba22f --head-sha 42450d3297a9673c59038f57e6256d5d29d979d6`. Status 0; stdout `{"schema_version":1,"mode":"full","reason_code":"changed-path-set","base_sha":"e71c436d0207c2b03269b17d6b1a3a784faba22f","head_sha":"42450d3297a9673c59038f57e6256d5d29d979d6"}\n`; stderr empty.
3. Invalid base SHA `bad`. Exact argv: `/private/tmp/codebasegraph-cod108-20261101/target/debug/xtask ci-mode --repo /private/tmp/cod108-real-layout-b0af51a183ba468b9be4ca384f410228 --base-sha bad --head-sha 12cb291412bb4417ece5961bdf41f38473c95c79`. Status 1; stdout empty; stderr `ci-mode requires full 40-character hexadecimal commit SHAs\n`.

## Historical failures retained

`result.json` preserves transcript-derived outputs and event references for early implementation failures. In particular, the attempted Cargo unit-test CLI launch using `CARGO_BIN_EXE_xtask` failed: Cargo did not define that variable for this binary test target, then a runtime environment lookup also returned `NotPresent`. That unit test was removed; no production fallback or skipped test was introduced. The actual built-process checks above replace that failed approach. Early parser failures were fixed and the final focused/full runs passed. These prior failures are not final check failures.

## Boundaries and next owner

The registered graph MCP/CLI did not bind to the disposable checkout; the prescribed target CLI initially returned OS error 2. Engineer then supplied exact same-project Git identity evidence and explicitly authorized this narrow source path. The canonical graph was used only for navigation; no graph was built, installed, copied, or rebound. Local CLI tests do not establish hosted workflow/API trust, protected-base behavior, aggregate CI state, platform qualification, independent review, or PR delivery. Production remained STOP pending the checked core handoff; Engineer/Production own subsequent workflow/docs/workflow-test stages.
