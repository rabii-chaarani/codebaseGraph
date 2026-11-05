# PR158 Windows execution evidence — independent partial review

Review written 2026-10-29 UTC against the retained 2026-10-28 execution records. Exact candidate: **47808ac34bcff79882abee4ab796c16c0621ed05**, `C:\Users\rcha0053\Repositories\cbg-pr158`. This is a **partial execution review, not A16 or full S02 acceptance**. The addenda below incorporate the successful short-path trial and the final fresh installed build/reopen/search under a restart-resistant rejecting proxy, with before/after native negative controls. The initial timeout remains preserved and is not reclassified as a success.

## Verdict and blockers

- **Confirmed, narrow Windows D05d default-byte mechanism:** ordinary root defaults select `bundled-windows-extensions`; the enabled-branch byte/decompression assertion and cache-version test ran **2 passed, 0 failed, 799 filtered, 0 ignored**. No explicit extension feature flag was used.
- **Confirmed, exact-head ordinary default source install with supplied native prerequisites:** `cargo install --path ... --locked --offline --root ... --bin codebase-graph` completed. This is a path install, **not a published crates.io install**, extracted-package verification, or a zero-prerequisite/self-contained runtime.
- **Confirmed, direct native fixture:** JSON COPY, native FTS index creation/query, and a separate-process FTS reopen/query succeeded using product-preseeded candidate bytes. This fixture is **not the installed application** performing those operations.
- **Confirmed, downloader negative:** raw `INSTALL json` reached the rejecting loopback proxy, produced `Proxy connection failed`, and left no extension payload. The final assertion fixture intentionally exited **0** after checking that failure; this does not mean INSTALL succeeded.
- **Confirmed, final installed graph/reopen/search positive:** the later short-path trial wrote a 23-node/35-edge graph with native exit 0, and a new installed search process returned `SampleService` with native exit 0. Its JSON cache is exact. FTS is absent from that profile because the normal backend is `disk_bm25_v1`; installed native FTS use is **not claimed or required to explain this search**. The earlier READY-timeout/search failure remains preserved with unproven cause. See final addendum for attribution limits.
- **Blocking prerequisite coverage gap:** missing compiler is a clear expected failure; missing-runtime launch fails but does not supply a clear product diagnosis or isolate a named missing DLL. A later missing-native-import-library control now fails clearly with `LNK1181`; genuinely missing system VC-runtime coverage remains absent.
- **Full S02/A03/A04 remains absent:** no approved runtime-sidecar supply/controlled-loader implementation, clean-consumer archive proof, final dependency closure, installer pre-mutation native preflight or installed k-wiki/MCP coverage is established here.

No pass in this report authorizes closure, merge, release, publication, deployment or tracker mutation.

## Read-only method and source attribution

Read `qualification-criteria.md`, all specifically requested logs/JSON, fixture source/manifest/lock, command index, source snapshot, feature trees, runtime wrapper, prerequisite inventory, and build/search failure logs. Recomputed hashes from actual files, decompressed the two exact-worktree Windows XZ assets, and compared their bytes with both recorded caches. No build, test, install, service operation, process kill, product/source edit, tracker or GitHub mutation was performed by this reviewer. Only this report was created outside the worktree.

Read-only Git commands independently returned HEAD `47808ac34bcff79882abee4ab796c16c0621ed05`. Status was not wholly clean: `?? docs/specs/CBG-PLAT-01/execution/cod53-windows-qualification-evidence/`. The parent confirms this is its evidence-only addition. Do not rewrite that as a currently pristine worktree. Four recorded tracked-source byte hashes still match `source-snapshot.json`: `Cargo.toml`, `Cargo.lock`, `src/db_writer/extensions.rs`, and `docs/release.md`. CRLF raw sizes/hashes, not text-decoded character counts, were used. The original execution's clean-before/after claim is retained in the missing-compiler result, not inferred for all later activity.

## Commands and scope actually executed

Full commands/cwds/env overrides and outer exit codes are in `run-index-current.json` and `missing-compiler-install-result.json`.

1. Root default tree: `cargo tree --offline --locked -p codebase-graph -e features -i codebase-graph` shows both default and bundled features. Isolated root `--no-default-features` tree shows neither. This is opt-out **resolution evidence only**; no opt-out runtime/absence test is claimed.
2. Default tests: `cargo test --locked --offline --release --lib db_writer::extensions::tests:: -- --nocapture --test-threads 1`. `default-extension-tests-shared.log` identifies the built unit EXE. `default-extension-tests-shared-wrapped.log` repeats 2/2 and `NATIVE_EXIT_STATUS=0` through `run-native.ps1`. Its Cargo fingerprint records exactly `["bundled-windows-extensions", "default"]`. The executed profile is **release, not native-tests**. This does not replace the debug-assertion lifecycle suite required by the separate native-tests recipe, and is not an 801-test full-suite pass.
3. Install: `cargo install --path "C:/Users/rcha0053/Repositories/cbg-pr158" --locked --offline --root "C:/Users/rcha0053/pr158-qualification/installed" --bin codebase-graph`. `cargo-install-shared-resumed.log` reports release install success; the index records outer exit 0. No `--features`/`--all-features`/`--no-default-features` appears. The installed EXE's help/build-help runs succeed under documented supplied prerequisites.
4. Fixture: `probe/Cargo.toml` depends on the path candidate with defaults plus `lbug = "=0.19.0"`. Both source and probe locks pin lbug checksum `a2a843feea97d72124b0438ee6a02ae0ce9d3543da3c391989bfb1f39ec20486`. The fixture builds with `cargo build --offline --release --manifest-path .../probe/Cargo.toml`; it is not a product-source patch.

`LBUG_SHARED=1`, exact external Ladybug 0.19.0 headers/import library/DLL, MSVC 14.39.33519, SDK 10.0.22621.0, isolated Rust/Cargo 1.96.0, and existing Anaconda OpenSSL 3.3.2/libclang were supplied via `native-shared-environment.json`. This is Rust product source installation **against a supplied shared native dependency**, not a successful C++ Ladybug source build. The earlier official static-library link incompatibility and interrupted source fallback remain separate failed/incomplete trials; no shim or ABI upgrade is inferred. The shared ZIP actually measures 8,280,690 bytes, SHA256 `8ecb60bb5b3bec7ff263cd23a622c0892d7f605c42af19b4e06d75b1604bf612`, matching `ladybug-shared-download.json`. All four records in `shared-native-inputs.json` rehash correctly.

`cargo package --no-verify` is inventory only, not extracted-package compile proof. This review does not upgrade it to such proof.

## Positive native fixture and actual extension bytes

`probe/src/main.rs` has a distinct `download-negative` branch **before** product preseeding. Positive modes call `codebase_graph::db_writer::preseed_ladybug_extensions(true)`, then use the pinned native library. SQL errors propagate through `?`; JSON row and FTS result contents are checked, not just LOAD success. Create logs show:

- `LOAD json` and `LOAD fts` succeed.
- One JSON row is copied, and `MATCH` returns `six|Hello World`.
- `CREATE_FTS_INDEX` succeeds; `QUERY_FTS_INDEX(..., 'world')` returns `six`.
- `SHOW_LOADED_EXTENSIONS` identifies JSON and FTS paths under `profile-native-positive/.lbdb/extension/0.19.0/win_amd64/`.
- `CHECKPOINT` and `PASS_JSON_COPY_FTS_CREATE_QUERY`, native exit 0.

The reopen command is a new process in the command index. It loads both extensions again, queries the same disk database, returns `six`, and logs `PASS_FRESH_PROCESS_REOPEN_FTS_QUERY`, native exit 0. No native FTS claim is based on the misleadingly named product graph-search unit test; the direct fixture really invokes native FTS.

`profiles-before.json` records the native-positive/installed profiles as empty. Cache records and current files were rechecked against XZ-decompressed worktree bytes:

| Extension | XZ bytes / SHA256 | Decompressed bytes / SHA256 |
|---|---|---|
| JSON | 3,371,312 / `adf5e05d5a63c181a1be3e1e6c90a8ffb9901118171e7caf8f860af055868ada` | 14,178,304 / `bdecb5201dd6abccc06ddc1f6cb4fcf0be3adb7aa3ddaac2f383ed129fca2b1f` |
| FTS | 3,495,248 / `a8a9d74be93a54482b6bb6df7c44ead6cb751f398b87b10c990d63e2da77910d` | 14,779,904 / `7f82059c8149bb7851420a9bfd0c46859e92efaeda0bcd5c65418d8f174edac4` |

Both native-positive cache payloads match. The original installed-profile JSON matches and its FTS path remains absent, as `cache-audit.json` reports. The later successful installed trial also has exact JSON but no FTS; its normal backend is `disk_bm25_v1`. Thus installed FTS absence is **not a default-byte defect or, by itself, an acceptance blocker**. The fixture's `include_fts=true` path independently proves actual native FTS; it must not be described as installed-CLI native FTS use.

## Downloader barrier: exact final negative attribution

`runtime-download-policy-resumed.json` defines a loopback-only rejecting HTTP proxy on `127.0.0.1:59741`, returning 502 and never forwarding. It is an executed change from the earlier held non-listening socket proposal. Product behavior was not changed. All Ladybug-specific and generic HTTP/HTTPS/ALL proxy environment variables point there; NO_PROXY forms are cleared. `USERPROFILE`/`HOME` are isolated by command. This scopes the evidence to the **verified Ladybug downloader route**, not whole-machine network isolation, firewall isolation, Cargo offline semantics, or proof that every possible route is disabled.

`download-negative-assertions.json` records an empty final negative profile before, fixture exit 0, exactly one observed `CONNECT extension.ladybugdb.com:443 HTTP/1.1` at 11:46:30.323676Z, and no extension payload afterward. The final log reports `Failed to download extension: json ... (ERROR: Proxy connection failed)`. Current negative profile also contains no `.lbug_extension` file.

The final source intentionally accepts the **native query's expected error** only when both `Failed to download extension: json` and `Proxy connection failed` are present. Unexpected INSTALL success or another error returns `Err` and nonzero fixture exit. Therefore `NATIVE_EXIT_STATUS=0` means **the negative assertion passed**, not that a native INSTALL process failed nonzero. This is an honest refinement of the criteria's proposed nonzero-harness convention; report it explicitly. Earlier observed/non-listening attempts returned exit 1 because their assertion rejected the error text; they are not the final passing assertion.

`download-proxy-requests.json` contains two CONNECT requests attributable to the negative attempts and no recorded request attributable to positive create/reopen or installed operations. Positive commands use the same proxy policy and exact local bytes. This is consistent with local use without extension download. There is no independent packet capture or per-command listener-lifetime attestation here, so retain that scope instead of claiming blanket network impossibility.

## Resolved DLL locations and artifact identity

The create and reopen fixtures use `GetModuleHandleW`/`GetModuleFileNameW` and fail if any of the six expected loaded modules is missing. These observations describe the **fixture process**, not a controlled loader or the installed CLI's complete recursive import/delay/runtime-load closure.

| Module | Resolved location | Bytes | SHA256 |
|---|---|---:|---|
| lbug_shared.dll | `C:\Users\rcha0053\pr158-qualification\ladybug-shared-0.19.0\lbug_shared.dll` | 20,199,424 | `5b28bf744580c3a47aaeee7ece40a3511e2141268def5207e9beffe236c9b512` |
| libssl-3-x64.dll | `C:\Users\rcha0053\AppData\Local\anaconda3\Library\bin\libssl-3-x64.dll` | 1,247,744 | `4158ad85699d9c5428b62778883b16b6c21b3b7da11fe60a0cc24ea37687a9fd` |
| libcrypto-3-x64.dll | same Anaconda `Library\bin` | 6,459,392 | `124c5f40a6cf0e642f9d92784dd66314fd548b4fdf93c543bf478e71e1209f9d` |
| MSVCP140.dll | `C:\WINDOWS\SYSTEM32\MSVCP140.dll` | 553,552 | `def46aa6a8f72f27bafac0c43334419486a4d1dcdb6c479a8ef7034b3e1fa4cb` |
| VCRUNTIME140.dll | `C:\WINDOWS\SYSTEM32\VCRUNTIME140.dll` | 123,472 | `184146852727a9db4eea06178716bec3cdbb1015c911f6b0f915b184ad7775b2` |
| VCRUNTIME140_1.dll | `C:\WINDOWS\SYSTEM32\VCRUNTIME140_1.dll` | 47,264 | `e6bfb3662ab4b1969a73441dbe35c96d51441b6bff8cf1fe7430bd5b246ca605` |

All six current files match `loaded-module-audit.json`. The host's globally installed VC runtime is 14.50.35719.0; narrowing PATH does not remove System32. This is documented developer-prerequisite execution, **not app-local sidecar independence or clean-consumer A03 evidence**. Module paths avoid misattribution to Git's mingw OpenSSL. In fact `native-versions.log` contains only a Git/mingw OpenSSL 3.2.1 banner; it does not establish the loaded native OpenSSL version or all tool/shell banners. Use the DLL hashes/host inventory for Anaconda 3.3.2. A later retained `exact-native-versions.log` now supplies rustc/Cargo 1.96.0 banners, PowerShell 5.1.22621.7649, CIM Windows identity, compiler resource version, SDK and actual Anaconda DLL versions; the earlier `native-versions.log` is still not the source of those identities.

All five `tested-artifacts.json` identities match current bytes, including the fixture's actual CRLF source size 4,554 bytes. Key EXEs:

| Artifact | Bytes | SHA256 |
|---|---:|---|
| installed `codebase-graph.exe` | 34,350,080 | `95ad3781beadab8842c137a884a1e8d8f534b1d3c1af1288bffd968ba3fdf665` |
| fixture `pr158-windows-probe.exe` | 23,448,064 | `b98c93ac8b57353f8ba53e47f27043f0f767f3e9805e451f43ee52fce33dadb7` |
| tested unit EXE `codebase_graph-3cda96f058e84b64.exe` | 47,810,560 | `08d5bd486383c81d2eed8f022cd76617d545e61923a919e60db47843e0e22c02` |

The unit EXE was additionally hashed by this reviewer; it was not in the original `tested-artifacts.json` inventory.

## Missing-prerequisite evidence and remaining limits

**Compiler expected-negative passes:** `missing-compiler-install-result.json` records exact path install, isolated fresh negative target/root, `CC=CXX` pointing at an absent qualification-owned `cl.exe`, Cargo exit 101, no installed or target product EXE, and unchanged HEAD/source status at that test. The log hash rechecks. Its `link-cplusplus`/cc-rs error clearly names the configured nonexistent compiler and Windows error 3; no fallback/download is observed. This proves rejection of a missing explicitly configured compiler, **not physical absence of the installed MSVC toolchain**. Compiler PATH entries remain present, and no missing-library/runtime category follows from this result. Completion-observed wall timestamps and handle duration differ; do not infer precise elapsed time or cause from those clocks.

**Missing runtime is incomplete as a clear-diagnostic test:** `missing-native-runtime.log` prints native status `-1073741515` (`0xC0000135`, DLL not found), followed by a generic Git Bash/shared-library diagnostic mentioning PowerShell. The command index's outer exit is 127. It shows launch failure with native prerequisite PATH removed, but does not isolate whether Ladybug/OpenSSL/transitive DLL was absent, prove missing VC (which remains globally installed), or demonstrate a clear product prerequisite error. `missing-openssl-runtime.log` has the same generic limitation. Do not call these A04 preflight, trusted-loader, pre-mutation safety or full missing-runtime qualification.

**Preserved first installed attempt, not a current missing-positive blocker:** `installed-graph-build.log` says `build guardian READY timed out`, preserves its lifecycle path, and reports cleanup `containment_unknown`; native exit 1. `installed-graph-search.log` says `Cannot create an empty database under READ ONLY mode`, native exit 1. The later final addendum establishes an independent successful installed build/reopen/search trial. It does not erase the first attempt or prove its cause. Host wall-time jumps were observed across calls, so the timeout's underlying product/environment cause is **unproven**. This reviewer neither reproduced nor fixed it and changed no timeout/constants.

**OS scope:** host inventory establishes Microsoft Windows 11 **Enterprise 23H2**, AMD64, build **22631.7652**; the stale registry ProductName `Windows 10 Enterprise` must not override CIM/build evidence. Enterprise/Education 23H2's published support end is **2026-11-10** (Microsoft lifecycle page: https://learn.microsoft.com/en-us/lifecycle/products/windows-11-enterprise-and-education). The 2026-10-28 execution precedes that date; this is not the already-expired Home/Pro 23H2 lifecycle and not a broad or post-end-of-support Windows qualification. A fresh successful lifecycle-page response was not retained by this reviewer. A later qualification must use a then-supported client version.

**Other retained gaps:** no exact native-tests/debug-assertion suite execution in this evidence slice; no opt-out runtime cache absence proof; no extracted `.crate` compile; the later missing-import-library control is reviewed below; no clean machine without VC runtime; no complete final runtime/extension dependency/export closure, legal redistribution inventory or controlled loader; no A04 sidecar/hash/mutation/PS5.1+7 installer probes or installed k-wiki/MCP smoke. These are not silently passed by a green default test or direct fixture.

## Safe interpretation and future addendum

**Narrow D05d Windows default-byte verification is complete for the recorded prepared/shared-prerequisite environment. Exact-head default path install, direct native JSON COPY/FTS reopen, and final installed graph/reopen/search under a verified rejecting downloader guard are supported. A16 acceptance remains partial because clear missing-runtime prerequisite reporting/scope is not established. The final fresh guarded trial closes the earlier listener-continuity evidence gap without rewriting that history. Full S02/A03/A04 remains open.** No promise of self-contained source installation, native C++ source-build success, installed native FTS, or clean-consumer readiness follows.

Assumptions: retained logs describe the bound commands; the documented rejecting proxy remained effective during the positive commands; source-snapshot/Git evidence ties candidate inputs to the tested binaries. Independent current hashes strengthen that attribution but are not a clean-room reproduction. Runtime barrier values, DLL PATH, fixture 256 MiB buffer/two threads, and compiler-negative overrides were execution-harness choices, not product constants changed by this reviewer. This reviewer changed no product constants or global settings; its sole environment adjustment was process-local Git Bash selection for two read-only Git checks.


## Final addendum — installed graph build and fresh-process search

Reviewed the completed `installed-graph-build-short.log`, `installed-graph-search-short.log`, `installed-short-cache-audit.json`, `run-index-final-resume.json`, and `runtime-download-policy-search.json`. **This addendum supersedes the baseline's pending installed-positive status, not the preserved first failure.** No new build/runtime action was performed by this reviewer.

### Final positive outcome and correct FTS attribution

- Fresh fixture: `C:\Users\rcha0053\q158\service.py`, 63 bytes, SHA256 `778d750c2381edc466065d2a9e4d092207ed7c4e7a431820459e479741544b8d`, containing class `SampleService` and method `helper`. The current file identity matches the build's source-snapshot `content_hash`/byte count.
- Build reports `database_written=true`, `skipped=false`, 23 node rows, 35 edge rows, `pending_runs=0`, `cleanup_pending=false`, empty diagnostics and **`NATIVE_EXIT_STATUS=0`**. It lists 55 COPY calls, including JSON node-staging COPY statements, and a real `disk_bm25_v1` search index with 18 documents. This is a successful normal installed graph workflow, not the direct FTS fixture.
- Separate process command, bound in `run-index-final-resume.json`, cwd `C:\Users\rcha0053\q158`:
  `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "C:/Users/rcha0053/pr158-qualification/run-native.ps1" "C:/Users/rcha0053/pr158-qualification/installed/bin/codebase-graph.exe" codebase-search SampleService --repo-root "C:/Users/rcha0053/q158" --json`
  It returns the `SampleService` class from `service.py` (plus `helper`) with **native exit 0 and outer exit 0**. A new process reading the completed disk graph establishes the requested installed reopen/search positive; no concurrent build success is inferred.
- Same installed EXE still measures 34,350,080 bytes with SHA256 `95ad3781beadab8842c137a884a1e8d8f534b1d3c1af1288bffd968ba3fdf665`. Parent attests no tracked source, EXE or supplied native input change; docs-only evidence addition remains separate.
- Isolated `C:\Users\rcha0053\q158home` contains JSON 14,178,304 bytes / SHA256 `bdecb5201dd6abccc06ddc1f6cb4fcf0be3adb7aa3ddaac2f383ed129fca2b1f`, independently rechecked. FTS is absent, matching `installed-short-cache-audit.json`. This is **expected for the normal `disk_bm25_v1` path**, not evidence that bundled FTS bytes are absent. Search JSON's `score_components.fts` name does not prove Ladybug FTS extension use. Actual native FTS LOAD/index/query/reopen is established only by the separately attributed direct fixture.
- The build's maintenance observation remains partial (`measurement_complete=false`, partial maintenance attempt). Do not convert `cleanup_pending=false` for this operation into a universal claim that every retained artifact was measured/reclaimed or that the old failed tree was cleaned.

### Barrier and command-capture limits retained

`runtime-download-policy-search.json` records a **new search-only** rejecting loopback proxy at `127.0.0.1:53712`, generic/Ladybug proxy variables set, NO_PROXY cleared, same native prerequisite PATH and **requests=[]**. The search command uses `USERPROFILE=HOME=C:\Users\rcha0053\q158home`. This is not port 59741, and no second raw-INSTALL negative was recorded for this new search listener. Earlier observed CONNECT negative proves the linked downloader honors the proxy mechanism; search's no-request record is consistent with local cache use.

The exact build command and before-cache evidence were subsequently recovered from retained root tool-call details into `installed-short-build-binding.json`, which this reviewer read:
`powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "C:/Users/rcha0053/pr158-qualification/run-native.ps1" "C:/Users/rcha0053/pr158-qualification/installed/bin/codebase-graph.exe" build --repo-root "C:/Users/rcha0053/q158" --mode full --no-git --single-thread --max-parallelism 1 --json` (stdout/stderr redirected to the retained short build log), cwd `C:\Users\rcha0053\q158`.
The retained launch code asserts `q158home` does not exist before creating it, binding a genuinely fresh profile. It records `USERPROFILE=HOME=...\q158home`, original proxy port 59741, same prerequisites and native exit 0. This resolves the earlier command/environment/fresh-profile capture gap. At that historical read `run-index-final-resume.json` only indexed the short search; it has since been expanded to bind the final guarded operations and controls.

**Historical limitation of the first short-path trial (later closed by an independent fresh guarded trial below):** the REPL kernel restarted while this long installed build was still running, losing the in-kernel proxy listener. The earlier fixture negative/positive runs used a verified live guard, and no build request was recorded before the restart, but **uninterrupted rejecting-proxy lifetime for the installed build cannot be established**. Consequently this successful installed build must **not** be called fully blocked-download execution. Exact fresh-profile JSON bytes support candidate preseeding but do not replace proof of a live downloader barrier for the whole process. Later search used its separate live port-53712 guard; that cannot retroactively repair the build's lost listener. No download is demonstrated by this gap, but no whole-build no-download conclusion is accepted either. No long build rerun was requested or performed by this reviewer.

### First failure remains separately attributed

Read `guardian-ready-investigation.md`, `guardian-startup-investigation.md` and JSON. Independently rechecked all three retained phase proof hashes and three receipt hashes. Phase 0/1 have successful receipts; phase 2 has no READY, `cancelled=true`, code 1 and `containment_empty=true`. The peer's exact-source ordering analysis attributes code 1 to owner-cancellation teardown before START, **not a demonstrated Ladybug/native-loader failure**. The original `containment_unknown` diagnostic is retained as the parent's observation at that time; a later receipt is not permission for manual cleanup.

The successful short-path trial **does not prove path length caused the original timeout**, nor exonerate/establish a PR regression. File timestamps span observed clock/host pauses and are not active I/O measurements. The protected failed tree remains preserved; no timeout, product constant, DLL, service or runtime state was changed by this reviewer.

### Final acceptance boundary

**Complete:** narrow D05d default-byte Windows verification for the recorded Windows 11 Enterprise 23H2/shared-prerequisite environment: default selection, actual enabled JSON/FTS bytes/decompression, 2/2 narrow tests, exact-head default path install, real JSON/native-FTS direct create/reopen positives, downloader negative, and normal installed graph/reopen/search positive. This is a bounded execution conclusion, not maintainer acceptance or full native-tests lifecycle-suite coverage.

**A16 remains partial:** the missing compiler negative is clear and category-specific. Runtime-negative records consistently show OS `STATUS_DLL_NOT_FOUND` before application startup and shell exit 127, **not a clear named product missing-runtime prerequisite diagnosis**; global VC DLLs remain installed. The later distinct missing external native-import-library control is now accepted: log hash matches `missing-native-library-result.json`; linker `LNK1181` explicitly names absent `lbug_shared.lib`, Cargo exits 101, the configured external directory is empty and no EXE exists in its private install root. This proves an explicit external-library override failure, not source/compiler/runtime absence or a product preflight. Compiler, external library and runtime DLL are separate categories; do not use one to silently pass the others. This review does not invent an exhaustive matrix beyond the approved clear-prerequisite-failure requirement. The original short-path trial's lost listener remains an honest limitation of that attempt; the later independent fresh guarded trial below now closes installed-build downloader-blocking evidence. The current A16 blocker in this reviewed slice is clear missing-runtime prerequisite diagnosis/scope, not listener lifetime, missing compiler/import-library coverage, normal graph behavior, or absent CLI FTS.

**Still separate/open:** full S02/A03/A04 controlled loading, permitted sidecar supply/licences/final dependency closure, clean-consumer archive execution, installer pre-mutation failure handling under PS5.1/7, and installed k-wiki/MCP smoke. Successful D05d/installed workflow does not authorize release, merge or closure. A fully C++ source-built Ladybug dependency, extracted-package build, opt-out runtime absence control and native-tests/debug-assertion suite are not newly proved; do not present these optional/separate capture limits as if this review added new acceptance promises.

Machine summary `qualification-status.json` was initially stale (short trial `running`, A16 pending installed search); the parent has now updated it. The final reviewed result is the new guarded trial below, with A16 still partial for missing-runtime diagnosis.

Final-log byte fingerprints at review:

| File | SHA256 |
|---|---|
| `installed-graph-build-short.log` | `d21e041eefd8334c4238017d101830941858f242d40fbcef901d38180f990550` |
| `installed-graph-search-short.log` | `9a1f16ebf1372160fca12216aa296f65e5399a745eea2b9bbf0825c5e2d836ba` |
| `installed-short-cache-audit.json` | `dca1dad27997a7e0e55a0c708f3d2b9ef4df23444dc35fe40bf2724d5e62152a` |
| `runtime-download-policy-search.json` | `e3073ce8cf0d7797ae1676e32aa2adf261ad5dfa2799c6e6e9a73d1eb048fa1d` |
| `run-index-final-resume.json` | `068a24ca55d3d1dfccf8297f904dafdd108276e131493b4e69861a53fade995c` |


## Final independent addendum — fresh installed workflow under a restart-resistant guard

This is the **current final execution verdict**. It supersedes the earlier listener-lifetime gap for acceptance purposes through a new independent trial; it does not erase the old gap or the first guardian timeout. Reviewed `guarded-installed-results.json`, guarded build/search logs, completed `run-index-final-resume.json`, `guarded-build-profile-before.json`, proxy source/ready/request files, and native negative logs before and after. No test/build, process signal or runtime operation was performed by this reviewer.

### Verified result and barrier attribution

- Harness-only `reject-proxy.py` runs as a separately owned external process, not in the REPL kernel. The reviewed code binds only `127.0.0.1`, records requests and returns 502; it contains no forwarding implementation. Ready record: port **55319**, native process PID **29812**. Root retained the process handle, observed it still running after search, and stopped it **only after** the final negative control. The guard's recorded final exit 1 is owner teardown, not a native test/product failure or proof it died during positive execution.
- Exact build and fresh-process search commands, cwds, isolated `USERPROFILE`/`HOME`, identical supplied native PATH, six proxy variables at `http://127.0.0.1:55319`, cleared NO_PROXY forms, outer exit codes and immediate native statuses are retained in `run-index-final-resume.json`. Fresh fixture/root: `C:\Users\rcha0053\q158b`; fresh empty profile recorded before: `C:\Users\rcha0053\q158bh`.
- Guarded installed build exits **native 0 / outer 0**, writes the graph, has **23 nodes / 35 edges**, empty diagnostics, `pending_runs=0`, and `cleanup_pending=false`. A new installed process returns class **SampleService** from `service.py`, native 0 / outer 0. Backend is **disk_bm25_v1**, not the Ladybug FTS extension.
- Installed EXE rehashes to the same `95ad3781beadab8842c137a884a1e8d8f534b1d3c1af1288bffd968ba3fdf665`. Current `q158bh` JSON is 14,178,304 bytes and SHA256 `bdecb5201dd6abccc06ddc1f6cb4fcf0be3adb7aa3ddaac2f383ed129fca2b1f`, equal to decompressed exact-candidate bytes. FTS is absent **as expected for this CLI backend**; actual native FTS remains separately proved by the direct library fixture.
- Raw unseeded `INSTALL json` native negative controls execute before **and after** the positive build/search using the same owned guard. Both logs show `Failed to download extension: json ... Proxy connection failed`; each fixture/native wrapper exits **0 because its expected-negative assertion passed**. Neither negative profile (`q158neg`, `q158neg2`) contains any `.lbug_extension` payload; this reviewer checked current files.
- `proxy-guard-requests.jsonl` has exactly **two CONNECT records** to `extension.ladybugdb.com:443`: one at 01:11:14.417133Z before the build, one at 01:14:33.394695Z after search. No positive request is recorded. Native and outer exit statuses, before-profile evidence, owned external guard and bracketing controls together support the recorded blocked-downloader installed workflow. The evidence remains scoped to the verified Ladybug downloader route, **not whole-machine network isolation**.

The restart-resistant guard therefore resolves the old installed-build live-listener evidence gap. No claim relies on the old lost listener. This new success does not establish the cause of the first guardian cancellation or permit removal of its protected runtime state.

### Current remaining acceptance gaps; no new blockers invented

**Supported-environment narrow D05d execution verification is complete.** Exact default `--path` product source install with recorded shared native prerequisites, 2/2 default byte tests, actual candidate byte/cache checks, direct native JSON COPY/FTS create/reopen, clear missing compiler/import-library negatives, and final installed graph/reopen/search under the verified rejecting downloader guard are supported.

**A16 still remains partial for the missing-runtime failure-diagnostic/scope requirement.** Runtime removal from process PATH produces pre-application `STATUS_DLL_NOT_FOUND` with generic shell diagnostic/outer exit 127, not a clear named missing-runtime prerequisite diagnosis. System VC runtime remains installed, so genuinely missing VC is not established. This is the remaining A16 blocker in the reviewed execution slice; the review does not silently promote the good compiler/import-library negatives to runtime coverage or invent an exhaustive new negative matrix.

**Full S02/A03/A04 is still open**, including permitted runtime supply/notices, final dependency/export closure, trusted app-local controlled loading, clean-consumer archive operation, installer pre-mutation native/sidecar probes under PS5.1/7, and installed k-wiki/MCP smoke. No extracted-package compile, full debug-assertion native-tests rerun, opt-out runtime absence control or C++ Ladybug source-build success is newly claimed. These limits are separate from the proven narrow D05d outcome; no aggregate closure, merge, release or publication authorization is given.

### Final delivery-report no-overclaim review

Independently read the current `docs/specs/CBG-PLAT-01/execution/cod53-windows-qualification.md`. The final guarded row is correctly a pass; older short-path continuity limitation and initial failure are preserved; actual fixture vs installed CLI FTS, source path vs registry, shared prerequisite vs native C++ source build, release vs native-tests, and OS-supported-date scope are kept distinct. All local Markdown evidence links resolved at review. No blocking overclaim found in this final report. `qualification-status.json` now reflects the completed guarded positive and partial A16 runtime-diagnostic scope.

This reviewer added/updated only this external `execution-review.md`; parent owns the evidence/doc copying, build/guard lifetime, and delivery report. No product/source/asset/dependency/ABI/timeout/numeric constant was changed by this reviewer.

Final guarded-evidence byte fingerprints:

| File | SHA256 |
|---|---|
| `guarded-installed-results.json` | `568babf5fa55ee890f35118ebcce12f66522dadf5b43f0dc8f30bf03e64c82d1` |
| `installed-graph-build-guarded.log` | `d912be6f074f17efcc40cb369f47d9a3e52be96f0ccf5a74624322e2f67f864e` |
| `installed-graph-search-guarded.log` | `9a1f16ebf1372160fca12216aa296f65e5399a745eea2b9bbf0825c5e2d836ba` |
| completed `run-index-final-resume.json` | `3f48af72140b63f734c320dfe90987c6c957e4d832a7806dc13631291464c427` |
| `proxy-guard-requests.jsonl` | `8a4d879060675c3e73b5b06f647a05c78cacc55dea2a1b4324529e6288dbc19b` |
| `reject-proxy.py` | `ca7ada29f99cb0a6da0807d6f468c22ad81d1bce59abe5c9f743c1b03b83588f` |
