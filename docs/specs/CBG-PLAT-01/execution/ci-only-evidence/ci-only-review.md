# CI-only review — COD-57/COD-58/COD-59

## Result

No confirmed correctness, contract, or portability findings in the stable five-file S06–S08 diff.

Baseline: `862a8f88d485d5bd257ebd9456c194156fd4ee94`.
Approved scope supplied by parent: spec0.10 S06–S08 at `1a9db7b558b60248888408daa6268a3b45f2f0df`.
Reviewed parent-produced `working-diff.patch`, SHA-256 `5f1a99d7168d2934bf683c80f45c6e57ebdddbdfa73c4d7987ae360135fe66cf`, against current source. No repository edits, Git, remote/CI calls, Cargo, or build/test execution by this reviewer.

## Checks

- **S06:** Only watch help text changes. `WatchOptions::parse` still delegates to `MaterializeOptions`, whose unchanged default is `parallel: true`; `--single-thread` still opts out. The help states existing behavior, not a new default.
- **S07:** The trace test creates an actual `rows with spaces.json` file containing exactly `b"[{}]"`. It renders the path with the same `files::copy_path` implementation used by production COPY generation, requires metadata resolution, compares canonical paths, and separately asserts **4 bytes**. This avoids treating Windows slash/prefix spelling as file identity. Outside that test, only crate-internal helper visibility and a `#[cfg(test)]` re-export change. The production renderer body, metadata parser, and COPY execution/mutation paths are unchanged.
- **S08:** Only the fixture observer's contention predicate changes. It accepts existing `WouldBlock` or a **present** raw OS code equal to `fs2::lock_contended_error().raw_os_error()`, not every `Other`/permission error. fs2 0.4.3 uses `ERROR_LOCK_VIOLATION` on Windows and `EWOULDBLOCK` on Unix. The **5-second** deadline, **10-ms** retry interval, error propagation, held shared-lock lifetime, epoch envelope/checksum/version/coverage, creation nonce, publication binding, receipt checks, and epoch-advance assertions remain unchanged.
- The Windows-only regression uses a real exclusive fs2 lock and a separate probe handle, observes contention, checks no result while held, then unlocks and requires the observer's unchanged invalid-epoch check. The scoped thread borrows only shareable fixture/path/JSON/sender values; receivers stay on the caller thread. Result types support the assertions, and these standard-library APIs fit Rust 1.82. No static Windows compile or locking-semantics defect found. The cross-platform missing-lock case exercises immediate error propagation.

## Limits and assumptions

Windows compile/runtime execution was not performed by this reviewer; parent owns builds/tests, including any native Windows validation. Approval scope and baseline are the supplied ones, and `working-diff.patch` is treated as the complete parent-generated baseline diff. No production constants changed; the existing 5-second bound and 10-ms interval were preserved. New regression synchronization waits are test-only (100 ms held-result check; 2-second channel waits).

## Reviewed file hashes

- `src/adapters/cli/format/help.rs`: `e54503d12a43a5f8696ddc9d72b14dda8083fb862a65f063af57410ce043d9b5`
- `src/db_writer/phase.rs`: `a300d4a31f0117e52288de82b9407a6db45d11973b73cc17ad9f0a4111288c83`
- `src/staging_writer/files.rs`: `b725202bdcf4390666b80fdb1f7a3fde0e8542b13c46b9be655b2dbff63c82c7`
- `src/staging_writer/mod.rs`: `86560ac2d5d9c28178a9149093da63179bb39fdb049413635f94f1152e0a6ebd`
- `tests/support/storage_maintenance/process.rs`: `afe27d7a70da62177242bc477ab391a7926d428e52418665e33ae00da1a72a31`
