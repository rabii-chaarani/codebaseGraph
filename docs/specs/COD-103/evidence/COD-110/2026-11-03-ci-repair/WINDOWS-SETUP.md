# Local Windows test environment

Prepared directly under the user's Windows-readiness approval. No product source or global PATH change.

## Entry

Normal launcher: `C:\Users\rabii\codebaseGraph-COD111-tests\dependencies-20261102\windows-ready.cmd`

It validates hashes for the two OpenSSL3 x64 runtime DLLs, adds their directory to process-local PATH, then calls the unchanged `windows-test.cmd`. `--preflight` passed with actual exit0. For Cargo tests set the existing caller-owned paths first:

```bat
set "CBG_TEST_SOURCE=C:\Users\rabii\c110y-b37cb6\repo"
set "CBG_TEST_TARGET=C:\Users\rabii\c110y-b37cb6\target"
set "CBG_TEST_PROFILE=C:\Users\rabii\c110y-b37cb6\c110-win-follow-20261102T234408Z-26a017\profile"
set "CBG_TEST_HOME=C:\Users\rabii\c110y-b37cb6\c110-win-follow-20261102T234408Z-26a017\home"
call "C:\Users\rabii\codebaseGraph-COD111-tests\dependencies-20261102\windows-ready.cmd" test --offline --locked <normal Cargo arguments>
```

The original dependency metadata and static OpenSSL/Ladybug SDK remain unchanged. Runtime metadata is adjacent `windows-runtime.json`. Keep source/target/profile/home explicit; do not use installed or guessed old binaries.

## Actual results

- OpenSSL3.6.0#3 `x64-windows` dynamic package built from the verified retained source and pinned vcpkg registry. Actual host0/inner0; 25 minutes. No new source/tool download in the captured build log.
- Both runtime DLLs measured AMD64; provenance, hashes, PE dependencies, ABI and Apache-2.0 license retained beside the raw logs.
- Frozen captured source CLI `03be889f09a87bc87032bcfe7653be71f12b1df916c34efa2c56fcd358ecaab9`: real JSON database build returned0, wrote15nodes/22edges and no build diagnostics; ordinary sidecar search returned `helper` in `service.py`, and identical warm search did too.
- Existing compiled real native FTS tests each passed exactly1 actual case first and warm: `adapters::cli::tests::graph::graph_search_reads_native_fts_indexes` (8.92/8.16s) and `api::graph_read::tests::manifest_without_backend_metadata_uses_legacy_ladybug_fts` (5.58/4.01s). No Cargo rebuild/full-test replay.
- Earlier actual affected Windows15 and bundled-extension1 pass remain valid, separate from these additional runtime checks.

## Preserved limitation

The additional manual null-backend `helper` query on the ordinary direct-build fixture returned empty results (CLI0, assertion1). It is preserved, not reported as a pass. Passing real FTS fixture tests independently establish working native JSON/FTS loading and query execution; the empty manually constructed fixture is a separate behavior/test-fixture issue, not silently repaired or used to block basic environment provisioning. The DB build's maintenance status also reports `maintenance target changed during preparation`; no successful maintenance/whole-product qualification is claimed.

This is Windows11 ARM64 running x64 Rust/MSVC artifacts through emulation, not native ARM64 product qualification or a native-x64 OS result. Native build compiler was Hostarm64/x64. No global PATH, system DLL copy, policy bypass, source change, merge or release. PR implementation review/CI are separate.

Raw evidence: `/Users/rabii/Documents/c110-win-ready-direct-20261103`. Original failed smoke and closed87/135 historical evidence stay unchanged. Final postcheck actually returned0: clean exact candidate HEAD/tree, no Cargo/compiler/CLI/package processes, original static dependency metadata unchanged, and CLI/JSON/FTS/runtime DLL hashes all match. The environment is ready for local Windows tests; the manual fixture limitation above is not a passing result.
