# COD-107 retention raw executed evidence export

This private export preserves the persisted BashResult output strings from the backend follow-up. Output streams were not separately exposed by the harness, so each `.combined-output.txt` contains the returned combined stdout/stderr without splitting. Each command's exact invocation and exit code are in its JSON sidecar. No commands were rerun to create this export.

Manifest: `manifest.json` (SHA-256 `30f21b375a504b0c66036033dbe6cb9d0ebd407dde4722d5012ebf8d2018d553`). Source/docs snapshot identities and read-only CI/native YAML identities are recorded there. Intermediate snapshots are not fabricated; capture-time per-attempt source hashes were unavailable except where separately retained in earlier failure evidence.

## Final evidence

- `18_full_xtask_final_pass`: `cargo test --locked -p xtask`, exit 0, 83 passed.
- `17_scoped_clippy_final_pass`: `cargo clippy --locked -p xtask --all-targets -- -D warnings`, exit 0, no warnings.
- `20_final_fmt_pass`: `cargo fmt --package xtask -- --check`, exit 0.
- `19_workflow_and_ordinary_gate_final_pass`: actual `check-workflows` and ordinary `release-gate`, both exit 0.
- `13_retention_focus_pass`, `14_wiring_focus_pass`, `15_required_focus_pass`: final focused body, typed wiring, and required job tests all exit 0.

## Earlier failures and warnings

Files 01–12 and 16 preserve selected intermediate failures/diagnostics and their exact observed command/result. These are not final passing checks. The initial feature regression (docs helper root-prefix failure) remains separately preserved under `cod107-failed-attempt-1.md`, `cod107-failed-attempt-1-main.rs`, and `cod107-failed-attempt-1.diff`; do not treat it as a retention-fix attempt. This export does not claim missing outputs/streams or Windows execution.
