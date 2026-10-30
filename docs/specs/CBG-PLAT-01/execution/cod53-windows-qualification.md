# COD-53 — native Windows qualification of the default-extension subset

## Verdict and exact scope

**Windows D05d default-byte execution is verified for the prepared environment below. Full S02 / A03 / A04 / A16 acceptance is not claimed.**

Tested product source: PR158 head **`47808ac34bcff79882abee4ab796c16c0621ed05`** (implementation subset `6d5de0c57d9311e3a5bf86f30a7767641d6b84bc`). This delivery adds evidence/documentation only. Tracked product source, dependency/ABI versions, extension assets, numeric constants and timeout values are unchanged. Original dirty user checkout is preserved; execution used a separate worktree and private build/install/profile roots.

Authority remains [approved spec rev0.10](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md), R02/A03/A04/A16, the [exact S02 approval](../approvals/cod53-windows-native-runtimes.md), and the [Community2022 licence decision](../decisions/cod53-vc-runtime-licence.md). Existing claims/ownership are unchanged. No merge, publication, deployment, service restart or aggregate closure is authorized.

## Executed evidence

Evidence directory: [cod53-windows-qualification-evidence/](cod53-windows-qualification-evidence/).

| Check | Executed result and limit |
| --- | --- |
| Native Windows defaults, no extra feature flag | **2/2 pass**, 0 failed/ignored, **799 filtered**. Existing JSON/FTS presence/decompression and cache-path tests. Ordinary `release` profile, not the debug-assertion `native-tests` lifecycle suite. [Log](cod53-windows-qualification-evidence/default-extension-tests-shared-wrapped.log). |
| Feature selection and explicit root opt-out | Cargo default tree selects `default` + `bundled-windows-extensions`; isolated root `--no-default-features` tree omits both. Opt-out resolution only, not an opt-out runtime/absence test. [Default](cod53-windows-qualification-evidence/default-feature-tree.log), [opt-out](cod53-windows-qualification-evidence/optout-feature-tree.log). |
| Exact-head ordinary default source install | **Pass**: `cargo install --path <exact-head-worktree> --locked --offline --root <private-root> --bin codebase-graph`, no `--features`. This is an unpublished candidate path install, **not a registry install**. Supplied same-version shared native prerequisite, not a C++ Ladybug source build or self-contained distribution. [Log](cod53-windows-qualification-evidence/cargo-install-shared-resumed.log), [recipe](cod53-windows-qualification-evidence/source-build-recipe.json). |
| Real JSON and native FTS, isolated empty cache | **Pass** in a standalone consumer of the candidate public preseeding API and pinned lbug: JSON `LOAD`/COPY/query; FTS `LOAD`/index/query; checkpoint and **fresh-process reopen/query** returning `six`. Every native query/error is checked. Both cached payload hashes equal the exact candidate XZ-decompressed assets. This is native fixture execution, not a claim that the CLI uses native FTS for new graphs. [Create](cod53-windows-qualification-evidence/native-json-fts-create.log), [reopen](cod53-windows-qualification-evidence/native-json-fts-reopen.log), [cache](cod53-windows-qualification-evidence/cache-audit.json), [fixture](cod53-windows-qualification-evidence/native-probe.rs). |
| Downloader barrier negative | **Pass**: raw `INSTALL json` without preseeding in a separate empty profile reaches the loopback rejecting proxy, fails with `Proxy connection failed`, and leaves no extension payload. Fixture exit 0 means its expected-negative assertion passed, **not INSTALL success**. Positive native create/reopen recorded no proxy request. Scope is this verified Ladybug downloader, not whole-machine network isolation. [Assertion](cod53-windows-qualification-evidence/download-negative-assertions.json), [requests](cod53-windows-qualification-evidence/download-proxy-requests.json). |
| Installed graph build/reopen/search | **Pass** in fresh scratch `q158`: 23 nodes, 35 edges, database written, native exit 0; a fresh installed process finds `SampleService`, native exit 0. New graphs use `disk_bm25_v1`; only JSON is seeded by that path, so absent CLI FTS cache is expected. Real native FTS was tested separately above. The long first retry crossed a kernel restart: uninterrupted proxy lifetime for this particular build is **not established**, although source/cache identities and successful behavior are. [Build](cod53-windows-qualification-evidence/installed-graph-build-short.log), [search](cod53-windows-qualification-evidence/installed-graph-search-short.log), [binding/limit](cod53-windows-qualification-evidence/installed-short-build-binding.json). |
| Fresh installed graph with restart-resistant rejecting proxy | **Pass** in fresh `q158b`/empty `q158bh`: installed build23nodes/35edges/native0, fresh-process `SampleService` search/native0; exact JSON cache; no positive proxy requests. Raw unseeded negatives before and after reach the same owned external rejecting proxy and fail as expected. This closes the older kernel-restart barrier gap without rewriting its history. [Results](cod53-windows-qualification-evidence/guarded-installed-results.json), [build](cod53-windows-qualification-evidence/installed-graph-build-guarded.log), [search](cod53-windows-qualification-evidence/installed-graph-search-guarded.log), [commands](cod53-windows-qualification-evidence/run-index-final-resume.json). |
| Missing configured compiler | **Expected-negative pass**: isolated locked/offline install fails 101 with cc-rs `ToolNotFound`, explicit absent compiler path / Windows error 3; no installed EXE, no fallback/download observed. Does not pretend the host's actual MSVC installation was removed. [Result](cod53-windows-qualification-evidence/missing-compiler-install-result.json). |
| Missing native import library | **Expected-negative pass**: isolated locked/offline install with an empty explicit external native directory fails101, linker `LNK1181` names missing `lbug_shared.lib`; no installed EXE. [Result](cod53-windows-qualification-evidence/missing-native-library-result.json). |
| Missing runtime prerequisite PATH | Nonzero pre-application Windows loader failure `0xC0000135` (`STATUS_DLL_NOT_FOUND`), outer Git Bash exit127/generic diagnostic. **Not a clear product prerequisite diagnostic or proof of missing VC**, which remains globally installed. This remains an A16 failure-diagnostic limit, not an A04 preflight pass. [Scope](cod53-windows-qualification-evidence/missing-runtime-results.json). |
| Windows `.crate` inventory | 8,460,008 bytes, SHA256 `4d924d1260d40d48e04b9cb2e7c41dc72b904687d9ec0b60d08bcaf23ed258e8`; all four XZ payloads match the candidate. `cargo package --no-verify`: **inventory only**, not extracted-package compile. [Audit](cod53-windows-qualification-evidence/package-audit.json). |

## Host, prerequisites and native identities

Microsoft **Windows 11 Enterprise 23H2 x86_64, build22631.7652**; CIM identifies Windows11 despite the stale registry ProductName. Enterprise23H2 is supported through 2026-11-10; these results are limited to the recorded supported client/date, not all Windows releases. Rust/Cargo1.96.0 (MSVC target), existing VS2022 Community17.9.6 / cl19.39.33523.0, toolset14.39.33519, SDK10.0.22621.0, Windows PowerShell5.1.22621.7649. [Exact versions](cod53-windows-qualification-evidence/exact-native-versions.log).

The official Ladybug0.19.0 **shared** archive SHA256 is `8ecb60bb5b3bec7ff263cd23a622c0892d7f605c42af19b4e06d75b1604bf612`. `LBUG_SHARED=1` plus explicit headers/import-lib/DLL supplied the native prerequisite. The official static archive could not link against this older local MSVC toolset (newer STL internal symbols); no shim/version upgrade was made. The source fallback was interrupted, not validated. [Native inputs](cod53-windows-qualification-evidence/shared-native-inputs.json), [trial notes](cod53-windows-qualification-evidence/native-trial-notes.md).

Actual fixture module origins: Ladybug from the private verified archive; OpenSSL3.3.2 x64 from existing Anaconda `Library/bin`; VC14.50.35719 from System32. Exact hashes/paths are in [loaded-module-audit.json](cod53-windows-qualification-evidence/loaded-module-audit.json). Import-library aliases `ssl.lib`/`crypto.lib` point to copies of existing `libssl.lib`/`libcrypto.lib` in a private link directory. This is declared source-runtime preparation, **not an approved packaged DLL provider, redistribution licence proof, controlled loader or clean-consumer evidence**. No native DLL bytes are committed or distributed.

The installed EXE SHA256 is `95ad3781beadab8842c137a884a1e8d8f534b1d3c1af1288bffd968ba3fdf665`; native fixture SHA256 `b98c93ac8b57353f8ba53e47f27043f0f767f3e9805e451f43ee52fce33dadb7`. [Artifact/source identities](cod53-windows-qualification-evidence/tested-artifacts.json), [source snapshot](cod53-windows-qualification-evidence/source-snapshot.json). Windows CRLF working-copy hashes are distinguished from LF/Git identities. Harness-only settings are a 256MiB fixture buffer/two query threads, build jobs2, CLI max parallelism1, isolated profiles and process-local prerequisite PATH/proxies. No product constants/global PATH/firewall were changed.

## Preserved failed trial and review

The first installed full build timed out before a phase guardian READY, then retained unknown-containment evidence; its search had no readable graph. This failure is retained, not hidden by the later success. Code1 in the phase receipt was owner-cancellation teardown, not proof of native worker code failure. A large proof-file timestamp gap and host wall-time jumps do **not** establish disk, path or product root cause. The later short-path success is an independent result, not causal proof. Protected failed runtime state was not deleted. [Failure](cod53-windows-qualification-evidence/installed-graph-build.log), [investigation](cod53-windows-qualification-evidence/guardian-startup-investigation.md).

Independent read-only [static fixture review](cod53-windows-qualification-evidence/probe-static-review.md) and [execution review](cod53-windows-qualification-evidence/execution-review.md) distinguish actual commands, checked cache/native bytes, supplied prerequisites and remaining acceptance gaps. They did not rerun the build or modify product source.

## Historical helper archive — CI repair

The original `reject-proxy.py` is retained as historical evidence in
[historical-reject-proxy.tar.gz](cod53-windows-qualification-evidence/historical-reject-proxy.tar.gz),
not maintained Python source. [archived-inputs.json](cod53-windows-qualification-evidence/archived-inputs.json)
records the source commit, original member path, byte length and SHA-256 digests.
The original [evidence index](cod53-windows-qualification-evidence/evidence-index.json),
raw reports and recorded commands are unchanged. Its `reject-proxy.py` entry now
resolves to that archived member with the same 1,311 bytes and digest.
Verify both the archive and member before extracting into a separate historical
checkout; do not extract Python files into this checkout. This archival repair
adds no Windows qualification and does not change any verdict above.

## Still open; no aggregate closure

- **A16 remains partial** while clear missing-runtime diagnostic evidence is incomplete. A successful prepared-environment path install, default/native tests and installed behavior do not erase its failure-condition limits. Registry publication was neither required nor performed for this unpublished candidate.
- **Full S02/A03/A04 remains unimplemented/unqualified**: permitted exact runtime supply and notices, final PE/export/recursive load closure, trusted app-local loading, archive/installer sidecars and real pre-replacement native probe, clean consumer environment without separately installed runtimes, installed k-wiki/MCP and applicable installer-shell negatives. Existing Community installation evidence does not invent publisher eligibility/term acceptance.
- No extracted Windows `.crate` compile, full Windows native-tests lifecycle rerun, opt-out runtime absence proof, clean runtime-free desktop, release/merge/publication or aggregate COD-53 closure is claimed.
- Reusable-knowledge checkpoint: results are preserved as SHA-bound qualification evidence. No new wiki memory is needed for transient PR status or an unproven failure cause.
