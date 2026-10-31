# COD-105 — independent local verification, final handoff

**Local checks complete.** Formatting, strict workspace Clippy and source-policy checks passed. **136 genuine JUnit testcases passed**, plus **3 earlier raw-log passes**. All15 macOS integration cases are accounted, but the original interrupted15-case invocation has **no final exit/JUnit**. No single full15 pass/report is claimed.

| Field | Identity / outcome |
| --- | --- |
| Snapshot | `/private/tmp/codebasegraph-cod105-resume-20261030`; HEAD `e06d7404955abeb7a002b54ad740399dfe10c931`, tree `7e4fbdb7834dbed308a233c40d2af6acaa09a82c`, assigned base `c60a00c1d747b73789cd6edc7ee20601182a35ca`, plus exact5-file dirty diff. HEAD alone is not tested source identity. |
| Scope | Existing COD-105 warning cleanup only:4 cfg additions plus Unix test-import split/rustfmt reflow. No bodies/assertions/features/manifests/native runner changed. |
| Authority | Original rev0.2 approved at `7e79f7e924536f29d3212e83899b01c38afd39de`, approval record `383a481bd5a71c58dab7dd796ace92a365760da5`. Coordinator confirmed current rev0.3 approved exactly `316be4c4bb81d4f6e025016b12374836b4dcc1a8`, COD103 comment `c8690556-e836-428d-bc81-079c701d2395`; unchanged COD105 oracle. Exact rev0.3 spec independently read; approval is the later user decision, not its historical preparation prose. |
| Briefs / criteria | `chg-ahwwmf`/`chg-cgjfa4`, isolated `chg-ndhhtx`; C103-R01/A01 and first-PR R05/A05 local scope retained. No new claims. |
| Settings | Evidence Verifier `inherit/inherit`; original accepted dispatch reused, overrides omitted. Actual runtime model/effort unexposed. No other roles dispatched. |
| Preservation | All five before/after hashes and complete status/diff identical. Final read-only `git diff --check` exit0.5 files,12 insertions,6 deletions, including rustfmt reflow. |

## Executed checks

Exact runnable commands, filters, timing, environment and output paths are in both directories' `command-records.json`. All commands were serial with exclusive Cargo ownership; no test retries, zero-test passes or synthetic XML.

| Check | Actual method / result | Evidence |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check`, exit0 | Earlier `../cod105-independent-verification-final/cargo-fmt-check.log`; original failed pre-repair report remains separately unchanged. |
| Complete xtask | nextest, entire xtask binary, Cargo `native-tests`, retries0, one thread: **71/71**,0skips | `../cod105-independent-verification-final/xtask-junit.xml`; run7c34f6f2-a8ac-45ef-bf25-5b7b3ec25ea0. |
| Affected native library | daemon/API lifecycle/storage controls/process lifecycle: **52/52** | Earlier `native-focused-junit.xml`;796 non-selected cases are exclusions, not passes. |
| Real FIFO producer/collector | `storage::maintenance_collector_tests::dc_s5_real_containment_fifo_retires_with_pin_receipt_and_terminal_root`: **1/1** | Earlier `fifo-focused-junit.xml`;847 non-selected cases excluded. |
| Strict warnings | `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings`, exit0,71.340s, **no warnings observed** | `strict-workspace-clippy.log`. Normal documented dev Clippy profile. |
| Workflow/source policy | `cargo run --offline --locked -p xtask -- check-workflows`; ordinary `release-gate`: both exit0 | `check-workflows.log`, `release-gate.log`; no production confirmations/publication. |
| k-wiki bundle_reader | All10 native-profile cases, **10/10**,0skips | `bundle-reader-junit.xml`, run1574e1a9-23ac-48df-9a71-88b78c7e80fd. |
| Unix permission-failure helper | `managed_publish_failure_preserves_active_generation`: **1/1**,11.703s | `generation-permission-junit.xml`, runa8ed3ddf-66f4-45ba-91b6-b1d05fb2ea32. |
| Ten-update/leak | `managed_materialization_survives_ten_updates_without_generation_leaks`: **1/1**,1122.295s testcase,1122.641s wall | `generation-churn-junit.xml`, run9b26f4b0-f13e-4455-bfe7-2ed870672999. SLOW display was not a failure; no timeout/deadline/oracle changed. |

### Interrupted attempt and exact coverage

Original run44488b22-47f0-40bc-9549-c4291935121c/pid41162 ended without recoverable completion after kernel/daemon restore. Its unchanged raw log records these passes:

- `direct_materialization_uses_custom_paths_and_cleans_shadow_files` —3.314s.
- `direct_search_sidecars_publish_with_the_database_and_remain_queryable` —3.531s.
- `managed_materialization_activates_generations_atomically` —317.397s.

It then recorded the ten-update case SLOW>660s, with no final exit/summary/XML. One bounded process observation found no original nextest/test/owned child; no kill or cleanup occurred. Engineer explicitly authorized safe continuation of only unfinished12 cases. Those12 now passed with genuine XML above. The three already-completed cases were **not repeated** or converted into fabricated JUnit. All5 macOS generation cases and10wiki cases are accounted across those records, **not** one uninterrupted suite pass. Existing `#[cfg(windows)] managed_reads_survive_deferred_retired_generation_deletion` is not compiled on macOS and remains Windows coverage, not a dropped/ignored test.

## Native environment and executable identity

- macOS26.6.2/build25G83 ARM64; Cargo/Rust1.96.0, nextest0.9.140.
- Existing actual `/private/tmp/codebasegraph-copy153-evidence/native019` static library and both headers independently match committed official0.19.0 identities: lib`d2e9a5c37617b0affb01f18f6ef48bb1531f5744f5ba06c849226842617ad142`, header`7da5b7e36d6610e43a1512e4997ac123ba0886cf5598ae44aca5449aa02a8aa3`, hpp`0b463bef0fa7a9ba1bb14cee12b23bf5bc7c2d6cfc4b3779d6f7cbe92bf4831d`. Outer cached archive is absent now; no new archive hash/download/source fallback claimed.
- Documented local LBUG library/include/version, OpenSSL3.6.2 lib dir, deployment13.3 and jobs2 were used. Jobs2 is reported test-resource concurrency, not a changed product constant. No custom Rust flags/shared target/LBUG_SHARED. Default isolated `target/`; project config supplies serial tests and its own `target/test-home`.
- Actual Cargo compiler messages identify this root/manifest and opt3/assertions/overflow enabled. Normal CLI copied into diagnostics and hash-bound **`b6e604c354a12d683ee9e0aa32f3b878c0f6d1049930da28f951a013f0cf0f12`**; `CARGO_BIN_EXE_codebase-graph` remained bound to that unchanged copy. No other checkout executable reused. Details: earlier `native-executable-provenance.json`, `environment-current.json` and compiler JSON.

## Final five-file identity

| File | SHA-256 before / after | Result |
| --- | --- | --- |
| `src/adapters/mcp/daemon.rs` | `89fe617cff0e9fcaecb40d4ca726b738f5f99c53817bef04c23fed13317d6ea0` | unchanged |
| `src/storage/maintenance_controls.rs` | `2ef7e403c9a991c485920ddebce89e047506ac48fa01a44b93b08b46b01973e4` | unchanged |
| `src/api/lifecycle.rs` | `83214d386bc9e99705a5e3a44a769f71e93f73933b4d80c2a2a9a041814929e8` | unchanged |
| `tests/generation_storage.rs` | `4734dd94adfc61f4d1783731d7b655fa257a87937aa73866a3b0adcfc3d2a4a0` | unchanged |
| `crates/k-wiki/tests/bundle_reader.rs` | `464d98c91303e9fb98a766a51f90b3084e2f9d759d9e3c104dcb63f1e92af015` | unchanged |

## Limits and next owner

- **Fresh Final Reviewer, actual head-bound full macos-arm64/windows-x86_64 CI and optimized native artifact smoke remain pending.** Local macOS tests/Clippy do not qualify Windows cfg paths, Windows11 desktop/PowerShell, clean consumer/runtime installation or final archive behavior.
- Source-model reconciliation remains pending: no attachment/JUnit ingestion/fold against the older dirty6aa5 Scryer root, no force/global reconciliation. Graph was discovery only; current source/compiler evidence identifies this candidate.
- No source/tests/Git refs/Linear/wiki/memory/CI/settings/credentials/dependency-install writes. Only own reports/config/captured CLI and authorized standard isolated build/test outputs. No behavior/profile/features/budget constants changed. No reusable memory warranted for transient status.
- Engineer owns the next fresh-review assignment; Production may persist this evidence unchanged with its related work under existing authorization. Cargo slot is released. **No Done, staging, merge, release or aggregate closure is asserted.**

Machine result: `result.json`; genuine-report audit: `junit-audit.json`; exact commands: `command-records.json`; final source: `source-snapshot-final.txt`, `source-five-after.json`. Earlier interrupted evidence remains in `../cod105-independent-verification-final/`; original formatting failure remains in `../cod105-independent-verification/`. `evidence-file-manifest.json` binds all three final evidence layers by file SHA-256.
