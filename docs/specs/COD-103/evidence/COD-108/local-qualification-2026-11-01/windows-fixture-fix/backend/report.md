# COD108 Windows fixture portability correction — Backend handoff

## Result

Applied the narrowly assigned test-only fix in `crates/xtask/src/ci.rs`; no production/classifier code changed. The original all-platform Git extra-path test now uses the portable filename `unrelated-extra.txt` and still verifies the actual B→H classifier selects `Full { reason_code: "changed-path-set" }`. Separate Unix-only `git_newline_path_names_remain_nul_delimited` preserves real Git newline filename coverage where the filesystem supports it. The unguarded raw byte parser regression `raw_diff_parser_accepts_newline_names_and_rejects_unknown_or_truncated_records` remains active on every OS.

## Root cause and preserved Windows evidence

The actual hosted run was 41317523731, attempt1, Windows unit job124880650882. Test `ci::tests::git_path_names_are_nul_delimited_and_extra_paths_force_full` passed normal Git commits then failed before reaching its classifier assertion at `fs::write` of a filename with an embedded newline. Windows reported `Os { code: 123, kind: InvalidFilename, message: "The filename, directory name, or volume label syntax is incorrect." }`. This is a test fixture portability defect, not a production classifier failure or flake. The preserved log is `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-hosted-41317523731-attempt1/availability-checkpoint03-failure/job-124880650882.log` (SHA-256 `cd42b3e23fc476ba38ae99c2aa023a77d3e3b77136babb9487a96ef0f51af9a6`); the frozen source view is `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod108-hosted-41317523731-attempt1/availability-checkpoint03-failure/actual-failing-case-frozen-source.txt` (SHA-256 `55bce2bf03adc13e57ea10a8c0a9e6a08ee2091acce96d2369d52d96c0c2618b`). No hosted rerun was requested or performed.

## Snapshot/scope

- Branch `cod103-stack/cod-108`, HEAD `157a4170f21d8f75d7d02611dd4e4bdfc5ca1dfa`, tree `2f8d78fab4fb9a11a21290e92da7f894fff0fd81`.
- Only path changed: `crates/xtask/src/ci.rs`, now 49894 bytes, SHA-256 `52595249047f49a2b8c9dece9fb48fbbafda6cd5e70849c5b8ffaf252c0f400c`; baseline was 48,671 bytes SHA-256 `b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f`.
- The whole production prefix before `#[cfg(test)] mod tests` is byte-identical: 28536 bytes, SHA-256 `1233773bf939f15216e20e61e42e55c4676edd75aa316baa2240852f0379c9e7`.
- `main.rs` remains `94e36cb3ceed27f45b552e42c9d6a52aa7cfef8dd70876f6e6ab4e562ec8891d`; `release/workflow_tests.rs` remains `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450`. Consumer YAML/docs, manifests, lock, native/release workflow/actions and other paths unchanged.
- No Git operation, hosted run/retry/cancel, dependency install, config/profile change, JUnit creation, model/API/tracker/wiki mutation.

## Final executed results

Environment: Darwin 25.6.0 arm64; rustc 1.96.0 (ac68faa20 2026-05-25); cargo 1.96.0 (30a34c682 2026-05-25); git version 2.52.0.

- `cargo test -p xtask --locked ci::tests::git_path_names_are_nul_delimited_and_extra_paths_force_full -- --nocapture`: exit0, 1 passed.
- `cargo test -p xtask --locked ci::tests::git_newline_path_names_remain_nul_delimited -- --nocapture`: exit0, 1 passed on current Unix host; test is cfg(unix).
- `cargo test -p xtask --locked ci::tests::raw_diff_parser_accepts_newline_names_and_rejects_unknown_or_truncated_records -- --nocapture`: exit0, 1 passed; test has no platform guard.
- `cargo test -p xtask --locked ci::tests::`: exit0, 14 passed / 0 failed.
- `cargo test -p xtask --locked`: exit0, 103 passed / 0 failed.
- `cargo fmt --check`: exit0.
- `cargo clippy -p xtask --all-targets --all-features --locked -- -D warnings`: exit0, no warnings.

An intermediate `cargo fmt --check` returned1 with one expected rustfmt formatting diff in the new Unix fixture. I applied the suggested bounded formatting using `edit`; the final formatting check passes. Exact raw outputs/status are stored in `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-7f3e2b93/windows-fixture-fix/raw`; result.json lists 16 raw logs including interim failures. The existing failed hosted attempt and fixture failures are preserved, not erased or declared fixed on Windows. No Windows result is claimed.

## Editing-method disclosure and next gate

This source correction used exact-unique `edit` calls only. The earlier disclosed initial `ci.rs` creation using `Path.write_text` and one main.rs diagnostic rewrite remain recorded in the prior handoff; this change does not retroactively claim that method complied. The prior correction/handoff continues to contain that incident. Source/Cargo edits are stopped now. Production and Verifier retain the actual Windows 41317523731 follow-up and all hosted/independent review gates.
