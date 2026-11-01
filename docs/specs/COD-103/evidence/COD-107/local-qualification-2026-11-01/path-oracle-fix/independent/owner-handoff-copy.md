# COD-107 Windows diagnostic-oracle portability fix handoff

## Authority and scope

- Existing approved COD-107/C103-R03→A03 work and retention supplement; existing source change `chg-xd2jtt`. This is a narrow test-only correction to the confirmed Windows diagnostic-path oracle issue reported by Final Reviewer. No production behavior, requirements, path policy, diagnostics, test selection, or packaging logic changed.
- Exclusive writable file: `crates/xtask/src/main.rs`. No edits to `docs/release.md`, `ci.yml`, `native.yml`, or any other file.
- Worktree root `/private/tmp/codebasegraph-cod107-20261031`, branch `cod103-stack/cod-107`, HEAD `08d2c060bba8bf45db069fe53185f06f5b3d6cd9`, tree `09552870917f232f59005cd73e41c788b0163dca`; no Git operation.
- Before source SHA-256 `e4d256e451c03f447a4e374ed361d0d0b23d2898977d51953995fa307d896f1e`; after SHA-256 `076dd22a1da16a6d651016b7227536f1d65de766ff6c09a3dcb02631c51af5da`. `docs/release.md` `1e67f4949647c10a48f9184db230454572d666a756b68c203690398f525c7954`, `ci.yml` `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9`, `native.yml` `cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683` unchanged.

## Defect and minimal correction

The production gate formats each non-eligible path with `relative.to_string_lossy()`, which uses native separators. The Windows test expected slash-separated strings, so its `report.contains(expected)` oracle could miss real Windows rejection diagnostics. The corrected test derives exactly these path-specific expectations with `Path::new(...).join(...).display()`: `docs-extra/neighbor.py`, `src/docs/neighbor.py`, `scripts/helper.py`, `src/codebase_graph/helper.py`, `docs/allowed.py` (the expected-forbidden allowance guard), and outside-symlink `outside/linked.py`. Fixed tooling/security/workflow diagnostic prefixes stay literal because production formats those fixed strings, not PathBufs. No output normalization or alternate separator allowance was added.

Exact source-only diff is `/private/tmp/cod107-backend-output/cod107-windows-path-oracle-fix.diff`; frozen final source is `cod107-windows-path-oracle-fix-main.rs` in the same private output directory. Raw outputs and metadata are in `retention-raw-evidence/windows-path-oracle-fix/`; manifest SHA-256 `6178c62a2ce121f891aa48fd172648385cb4ffec9b08f080e1889346506d4112`.

## Checks after the edit

Environment: macOS ARM64, Rust/Cargo 1.96.0, Bash 3.2.57, jq 1.7.1, `xtask` dev/test profile, Cargo commands locked; no dependencies installed.

- `cargo fmt --package xtask -- --check` — exit 0.
- `cargo test --locked -p xtask docs_python_gate_rejects_neighbors_root_tooling_and_workflow_python -- --nocapture` and `cargo test --locked -p xtask docs_python_exception_rejects_component_aliases_and_symlinks -- --nocapture` — both exit 0.
- `cargo clippy --locked -p xtask --all-targets -- -D warnings` — exit 0, no warnings.
- `cargo test --locked -p xtask` — exit 0, **83 passed, 0 failed, 0 ignored**.
- `cargo run -p xtask --locked -- check-workflows` and `cargo run -p xtask --locked -- release-gate` — both exit 0.

These are macOS results only. No Windows compile/execution or platform CI occurred; the new path oracle is derived from native `Path::display()` source semantics but actual Windows qualification remains pending. No native package/archive, `.crate`, hosted CI, A03 acceptance, PR, merge, or release is claimed.

## Stop

All assigned source/Cargo/private evidence writes are stopped at this snapshot. Independent Evidence Verifier and Final Reviewer own their gates; Production owns Git/PR/CI. The prior handoffs and evidence export were left unchanged. No reusable memory was warranted.
