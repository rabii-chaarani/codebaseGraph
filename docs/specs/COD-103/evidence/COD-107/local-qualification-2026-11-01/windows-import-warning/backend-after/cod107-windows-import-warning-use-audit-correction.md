# COD-107 Windows unused-import handoff use-audit correction

The original handoff was restored byte-for-byte: SHA-256 `14678b927dc40aab1fd2047bbe1e2fe60955fca9420d211f1aadcb1f03e3f17d`. A temporary wording-only version (`ae672fac9776eab8d9f2c71f21e8206d6a3043b25a80681e7f0ee3bad0b90f09`) is superseded; it did not alter source or checks. The original handoff’s claims about test-module callers also being used by platform-independent tests were inaccurate.

Read-only audit of source SHA-256 `26837ce559fd1bd973584321ab1fc7cd912ea6dd939b1714b9c956fe76be9594`:

- Production parent-module references are unconditional: `yaml_path` is used in `native_retention_policy_valid` and `check_workflow_policy`; `yaml_step_by_id` and `yaml_step_by_name` are used in `native_retention_policy_valid` and `check_workflow_policy` release-policy checks.
- Inside `#[cfg(test)] mod tests`, `yaml_path` references the Unix-gated `native_retention_step` helper and `required_rejects_retention_selector_failure` test. `yaml_step_by_id` is referenced only by `native_retention_step`. `yaml_step_by_name` is referenced only by `required_rejects_retention_selector_failure`.
- `native_retention_step`, `assert_retention_result`, `run_required_result`, and the three tests that call these helpers are all immediately `#[cfg(unix)]`. Thus the extra test-module imports are used only in Unix builds; they are not needed by platform-independent tests on Windows.

This audit is source-reading only. It does not claim local Windows compiler execution or qualification. Structured caller/cfg details are in the adjacent JSON appendix. No source/Cargo/Git/test command was run for this audit correction.
