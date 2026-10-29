# PR158 Windows native fixture — independent static review

Reviewed: 2026-10-28T06:38:55.148314+00:00. Candidate: immutable isolated PR head `47808ac34bcff79882abee4ab796c16c0621ed05` (root-provided identity).

## Scope and verdict

Read-only review of `C:\Users\rcha0053\pr158-qualification\probe\src\main.rs`, its manifest, pinned lbug Rust query-result handling, and `C:\Users\rcha0053\pr158-criteria.md`. Reviewed main.rs SHA256: `82373ccf764ba554b50aedd325dd3642bb54d9468d0d4211781a5f96c9aa6f4a`. No source edits, build, test, runtime execution, tool installation, service or repository changes were made. This report is outside the repository.

**Static verdict: suitable small native JSON/FTS fixture, with important diagnostic/control gaps below. No runtime pass or qualification is granted by this review.** Runtime evidence and final built-source identity must be reviewed separately.

## Important gaps and simple fixes

1. **Module checks run too early.** `loaded_modules()` is called before `Database::new`, `LOAD json` and `LOAD fts`. A VC DLL required only by an extension may legitimately be absent at that point, causing a fixture failure before the native work is attempted. It also does not report extension-triggered DLL origins after loading. **Fix:** call it after successful JSON/FTS LOAD in each positive mode (or keep an optional non-failing startup snapshot and require the post-LOAD snapshot). Record `SHOW_LOADED_EXTENSIONS` output as well. The named-module list is useful source-environment evidence, not complete recursive PE/runtime dependency closure or controlled-loader proof.
2. **Proxy failure assertion does not bind itself to the held endpoint.** Negative mode requires only `127.0.0.1` and case-insensitive `connect` in the error. Another loopback connection failure could meet that check. **Fix:** have the wrapper retain the full error and verify it names the actual reserved endpoint `127.0.0.1:52256` (if the library reports the port); alternatively pass the expected endpoint to the fixture. The wrapper must preserve the held non-listening socket for all children, prove empty negative USERPROFILE cache before execution and no valid payload afterward, and record the effective proxy/no-bypass environment. Do not infer this from source alone.
3. **The fixture does not establish clean state or matching bytes itself.** Product preseeding only writes missing cache files, and `PRESEEDED_JSON_FTS_FROM_DEFAULT_LIBRARY` is printed after a successful function return even if pre-existing payloads were reused or an opt-out build supplied no bytes. **Fix:** the wrapper must prove the positive profile was empty, then compare both resulting payload sizes/SHA256 to decompressed exact-candidate XZ assets and retain exact loaded extension paths. The final fixture executable/source/manifest/lock and selected default features/native inputs must be bound to the execution record. No fixture source change is required if the wrapper provides these checks.

These are important before accepting runtime evidence, not requests to modify product behavior. Root reports a reserved bound non-listening loopback socket and child-only proxy/profile control; this review has not observed that execution. Do not treat the broad proxy-error substring alone as complete network-barrier proof.

## What the fixture correctly tests

- Its manifest depends on candidate `codebase-graph` through an absolute path with Cargo default features enabled, plus pinned `lbug = "=0.19.0"`. No explicit extension feature is added by the fixture. Resolved features and native inputs still need to be recorded from the actual build.
- Negative mode runs raw `INSTALL json` **before** any product preseeding. An unexpected successful installation returns an error. An expected matching download error returns `Ok(())`, so **fixture exit 0 means the negative assertion passed**, not that INSTALL succeeded. Retain the SQL error and wrapper status; do not label this as a nonzero fixture exit.
- Positive mode calls the candidate's public `preseed_ladybug_extensions(true)`, then really executes `LOAD json` and `LOAD fts`; it does not replace these with a disk-BM25 product search.
- Create mode writes a one-row JSON file, executes native `COPY Documents FROM ...`, then queries title/text and asserts `six` and `Hello World`. The JSON path escapes backslashes and SQL single quotes. With a new DB this is a real JSON COPY assertion, not LOAD-only evidence.
- It creates a native FTS index over title/text, queries `world`, and requires the returned title `six`. The success/error check in pinned lbug `Connection::query` rejects unsuccessful native query results, so `?` does not silently accept a failed SQL result.
- Reopen mode does not recreate the table/index: it reopens the same database, explicitly loads the extensions, queries the stored FTS index and asserts `six`. `CHECKPOINT` completes before create mode returns, and Rust local drop order releases connection before database. **A fresh-process claim requires the wrapper to launch create and reopen as separate child processes.**
- Both modes retain all SQL/output, including `SHOW_LOADED_EXTENSIONS`; the reviewer should check reported JSON and FTS paths against the isolated verified cache. Single-row substring assertions are adequate for this controlled fixture: the asserted values are not result headers. Structured exact-row checks would be a possible refinement, not a current blocker.
- Fixture resource settings are bounded to a 256 MiB buffer pool and two native query threads. These are fixture-only changes, not product defaults, and do not prove a total process memory ceiling. The data/index contain one record.

## Coverage limits and execution acceptance

This standalone harness links the candidate library and pinned native dependency. **It is not the installed product executable.** Its public-preseed/JSON-COPY/native-FTS/create/reopen results may supplement installed default-source evidence but cannot substitute for an actual exact-head private Cargo install, installed binary graph build, fresh installed binary reopen/search, supported desktop identity, or prerequisite negative cases. It does not implement or qualify A03/A04 prebuilt archive/runtime supply/installer/MCP/controlled-loader behavior and cannot close full S02/COD-53.

Before approving runtime evidence, retain: final compiled source and EXE hashes; actual build command/cwd/profile/target/default feature resolution and native input identity; distinct empty positive/negative USERPROFILE+HOME and cache before/after; held endpoint and exact child environment with all LADYBUG/generic proxy settings and NO_PROXY bypasses cleared; raw expected negative error; successful create/reopen commands, exit statuses and full transcripts; loaded native module/extension origins and cache hash comparisons. Cargo offline mode alone is not a runtime barrier. Expected negative-install failure must come from the reserved connection route, not TLS/certificate/configuration failure. A failed fixture must remain failed; no deletion or weakening of product assertions is authorized.

## Assumptions and changes

Assumed the root's exact-head worktree identity, child-only process environment, exclusively held non-listening socket at `127.0.0.1:52256`, isolated caches and same native input policy. None were runtime-verified here. No constants/settings changed by this reviewer. Root's fixture resource bounds and planned endpoint are recorded above; only this review file was created.

## Follow-up: shared Ladybug prerequisite

Root subsequently reported switching native inputs from incompatible static library to the official same-ABI 0.19.0 shared archive (reported archive SHA256 `8ecb60bb5b3bec7ff263cd23a622c0892d7f605c42af19b4e06d75b1604bf612`) through documented `LBUG_SHARED=1`, without candidate source changes. This is an unexecuted root-provided update, not a review of that build. **Add `lbug_shared.dll` to the actual module-origin evidence**, bind its DLL/import-lib/header identities and record runtime PATH. Earlier static input/environment hashes must not be misrepresented as the final successful shared-input identities. A successful shared-prerequisite source install would not be a self-contained runtime qualification.
