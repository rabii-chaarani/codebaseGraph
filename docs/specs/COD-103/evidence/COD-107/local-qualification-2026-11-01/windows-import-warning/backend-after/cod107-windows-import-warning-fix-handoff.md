# COD-107 Windows unused-import correction handoff

- Authority: existing COD-107/C103-R03→A03 scope, approved COD-103 revision0.3 (`316be4c4bb81d4f6e025016b12374836b4dcc1a8`, separate record `0e6008a1f46679a86aba85ac513beb05af94b0c0`), approved retention method (`e5adebfa1196a2074527cec4935522c43c773e39`, separate approval `08d2c060bba8bf45db069fe53185f06f5b3d6cd9`, user decision `5b4b5aad-cbe8-4265-8db9-5ea351b7f386`). Bound only to existing source change `chg-xd2jtt`; no model/claim/Linear/JUnit/fold work.
- Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch `cod103-stack/cod-107`, HEAD `996967eb50d5f84f477525e4dccdc2f73421a029`, tree `301ec679880ca35888258dc7f4993f1a06b91959` (read-only task snapshot). Starting source SHA `076dd22a1da16a6d651016b7227536f1d65de766ff6c09a3dcb02631c51af5da`; final `crates/xtask/src/main.rs` SHA `26837ce559fd1bd973584321ab1fc7cd912ea6dd939b1714b9c956fe76be9594`. Final read-only hashes: docs/release.md `1e67f4949647c10a48f9184db230454572d666a756b68c203690398f525c7954`, ci.yml `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9`, native.yml `cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683`. Only main.rs changed.
- Compiler warning authority/evidence: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod107-hosted-41269015161-attempt1/availability-checkpoint02/actual-compiler-warning-evidence.json` (digest `cd6cae82575afdb3783b9ffcd3d44cbcbb2b0ed8981f0b051212770e40f77c8c`), actual Windows artifact `13069030952` records unused `yaml_path`, `yaml_step_by_id`, and `yaml_step_by_name` in the test import at prior line 3021. It is a zero-warning blocker, not a claimed CI failure.
- Fix: remove those three helpers from the unconditional `#[cfg(test)]` `use super` block and add one `#[cfg(unix)] use super::{yaml_path, yaml_step_by_id, yaml_step_by_name};` immediately after. Their unconditional non-test use in policy code remains unchanged; test-only retention calls are inside Unix-gated test helpers/cases; other uses in tests are already required by non-Unix workflow tests. No warning suppression, API, body, behavior, oracle, cfg-disabled test, dependency, profile, package, workflow, or other-file change.
- Static use audit: `yaml_path` remains used in production policy and test code; `yaml_step_by_id` and `yaml_step_by_name` remain used in unconditional production policy and other platform-independent tests. Their extra retention-test uses are under `#[cfg(unix)]`. The compiler warning source is specifically the unconditional test import on Windows, which the cfg-split addresses.
- Exact source delta: `/private/tmp/cod107-backend-output/cod107-windows-import-warning-fix/source-delta.diff`; final source freeze adjacent. Raw combined outputs/command sidecars are under the same directory. Manifest SHA256 `21321f2cb5224d9b2cd879ac6cdb23502dcc87ddacfb267b93b7a0b3346a7cec`. This handoff and logs are new; earlier reports remain unchanged.

## Final actual commands/results after the edit

Environment: macOS ARM64, Rust/Cargo 1.96.0, xtask dev/test profile, commands locked; no dependency installation.

- `cargo fmt --package xtask -- --check` — exit 0.
- `cargo clippy --locked -p xtask --all-targets -- -D warnings` — exit 0, no warnings.
- `cargo test --locked -p xtask` — exit 0, **83 passed, 0 failed, 0 ignored**.
- `cargo run -p xtask --locked -- check-workflows` — exit 0.
- `cargo run -p xtask --locked -- release-gate` — exit 0.

These are Darwin results. No Windows compiler run is claimed; actual hosted Windows full CI is still required. No native/package/artifact build or inventory, PR/push, merge, publication, A03 acceptance, or aggregate completion is claimed. All source/Cargo/private writes are stopped.
