# Missing compiler source-install negative

- **PASS (expected negative):** Cargo exited `101` after `27.953` seconds.
- Exact PR158 HEAD: `47808ac34bcff79882abee4ab796c16c0621ed05`. Worktree clean before and after; no source changes.
- Failure: `link-cplusplus v1.0.12 custom build -> cc-rs ToolNotFound -> explicit missing CXX cl.exe (Windows os error 3)`. The log shows `CXX = Some(C:\Users\rcha0053\pr158-qualification\missing-compiler\cl.exe)` and `failed to find tool`.
- No installed EXE under `C:\Users\rcha0053\pr158-qualification\negative-compiler-install`; no `codebase-graph.exe` in the negative release target. The compiler path remained nonexistent.
- One command: `cargo install --path 'C:/Users/rcha0053/Repositories/cbg-pr158' --locked --offline --root 'C:/Users/rcha0053/pr158-qualification/negative-compiler-install' --bin codebase-graph > 'C:/Users/rcha0053/pr158-qualification/missing-compiler-install.log' 2>&1`
- Start (UTC): `2026-10-28T11:45:33.275761+00:00`. Completion observed (UTC): `2026-10-28T11:46:12.690109+00:00`.
- Environment assumption: only prerequisite values from `C:\Users\rcha0053\pr158-qualification\native-shared-environment.json` were loaded. Overrides: `CARGO_TARGET_DIR=C:\Users\rcha0053\pr158-qualification\negative-compiler-target`; `CC=CXX=C:\Users\rcha0053\pr158-qualification\missing-compiler\cl.exe`. `CARGO_HOME` was retained and `CARGO_BUILD_JOBS=2`. The configured Git Bash was used.
- `--locked --offline` was enforced. No download lines or compiler fallback were observed. The root native target was not used.
- Scope: one missing-compiler assertion only. This does **not** establish missing runtime or native-library categories. No tools installed, DLLs copied, services changed, or positive/full suite run.

## Evidence

- Full stdout/stderr: `C:\Users\rcha0053\pr158-qualification\missing-compiler-install.log`
- Command, environment, timing, exit, and no-EXE result: `C:\Users\rcha0053\pr158-qualification\missing-compiler-install-result.json`

Wiki recall was completed before repository conclusions. No durable wiki memory was created: this is transient PR qualification evidence.
