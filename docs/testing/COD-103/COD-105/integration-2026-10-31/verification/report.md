# COD-105 — affected integration verification

**PASS for assigned local affected checks.** No new native behavior/platform or delivery qualification is claimed.

## Snapshot and authority

- Clean worktree `/private/tmp/codebasegraph-cod105-qualified-resume-20261031`.
- HEAD `5989dae3040d2bc17fc6c0b62cb62c5f0377c8c4`; tree `bca8192ae9a14317dff4b2184c03e326dc3a9e88`.
- PR168 base `b355d3d3751fdb5aa846181e6a4a49a5d140ada7`; main base `6a98fb0ef4a6bbde02f1556683401eeded49574e`.
- Approved COD103 revision0.3: `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; separate approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`. Exact approved document/approval bytes preserved. Existing COD105 oracle unchanged.
- Evidence Verifier `inherit/inherit`; original accepted dispatch reused, no overrides/role dispatch. Runtime metadata unexposed.

## Actual new checks

| Check | Exit | Actual wall time |
| --- | ---: | --- |
| `format` | 0 | 2.009s |
| `xtask-inventory` | 0 | final duration unavailable |
| `full-xtask-run` | 0 | 19.302s |
| `check-workflows` | 0 | 12.338s |
| `ordinary-release-gate` | 0 | 0.455s |
| `strict-workspace-clippy` | 0 | 66.238s |
| `final-whitespace-and-state` | 0 | 0.055s |

Exact commands/environment/results are in `command-records.json`. Formatting, workflow policy and ordinary release-gate passed. Strict `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` passed with **no warnings observed**. No production flags/confirmations/publication.

**Fresh complete xtask:71/71 passed,0failures/errors/skips.** Cargo `native-tests`; nextest retries0/one thread/`--no-tests fail`; actual inventory matches all71 JUnit names. Run `1b334cdc-f4d2-4e67-8da4-1557111bd933`. Genuine `xtask-junit.xml` SHA256 **`d41a8b53e7a5536def1c9e61fb8bf6e6962e925992accd7d791a941214a46b71`**. Actual binary is under this new worktree's target/native-tests; source package4.0.5 and binary hash are recorded in `execution-source-identity.json`. No old executable/shared target reused.

## Byte identity, version consistency and earlier evidence

Independent Git-tree/working-byte audit verified **all378 relevant source/test/config/fixture blobs unchanged** versus accepted/tested `ef7988439bb26f1d9914ea8d007196f638c2f33b`. Exact change partition:6generated metadata+94docs/evidence,0unexpected. All83 preserved prior-evidence files,10approved documents and the separate approval byte-match their recorded identities. Prior review SHA256 `3a1049c36ab0f88f52a15f228091c330187bc18ab65bd1cb4af24f9a1c37c2b1` is preserved.

Root/k-wiki/xtask versions, wiki root dependency, release-please manifest and3workspace lock packages are consistently **4.0.5**. Parsed manifests equal tested content after masking only package versions/wiki dependency version; parsed lock equals tested content after masking only3no-source workspace package versions. No dependency edge/source/checksum/features/profile changed. Full audit and actual metadata diff are retained.

Prior **136 genuine JUnit passes+3raw-log passes** remain reused behavior evidence; no wholesale native/cohort/1122s rerun. The new71xtask executions overlap the same71 already in prior136: **do not claim207unique tests or new native behavior execution.** The prior interrupted15-case attempt remains incomplete with no final exit/JUnit; its3raw passes stay distinct, no synthetic full15 report. Canonical prior report: `docs/testing/COD-103/COD-105/warning-hygiene-2026-10-31/cod105-independent-verification-resumed/report.md`.

## Final source preservation

Clean tracked/untracked status, HEAD/tree and all378relevant working blobs remained unchanged; final read-only diff check passed.

| File | SHA256 before/after |
| --- | --- |
| `src/adapters/mcp/daemon.rs` | `89fe617cff0e9fcaecb40d4ca726b738f5f99c53817bef04c23fed13317d6ea0` |
| `src/storage/maintenance_controls.rs` | `2ef7e403c9a991c485920ddebce89e047506ac48fa01a44b93b08b46b01973e4` |
| `src/api/lifecycle.rs` | `83214d386bc9e99705a5e3a44a769f71e93f73933b4d80c2a2a9a041814929e8` |
| `tests/generation_storage.rs` | `4734dd94adfc61f4d1783731d7b655fa257a87937aa73866a3b0adcfc3d2a4a0` |
| `crates/k-wiki/tests/bundle_reader.rs` | `464d98c91303e9fb98a766a51f90b3084e2f9d759d9e3c104dcb63f1e92af015` |

## Environment and limits

macOS26.6.2/25G83ARM64; Rust/Cargo1.96.0, nextest0.9.140. Actual direct Ladybug0.19.0 library+both headers rehashed to trusted previous identities. Documented LBUG paths/version, developer OpenSSL3.6.2, deployment13.3 and jobs2resource recipe reused. No customRustflags/LBUG_SHARED/shared target/oldCLI; new default target initially absent. Project config supplies serial tests and own target/test-home. No download/install/source fallback or constant/policy changes.

Current graph search/context returned busy; no retry/rebuild. Earlier same-path discovery/context served only orientation; actual source identity came from Git/working bytes. Older graph/Scryer root is not candidate evidence binding; **no model attachment/ingestion/fold**. Inventory exit0 crossed a kernel restore; exact final duration unavailable, no timing repeat.

**Fresh full both-target head-bound hosted CI/native artifact smokes/warnings remain post-push gates.** Local macOS Clippy does not qualify Windows/desktop/shell/clean consumers/final optimized archives. No candidate-preflight command was run; COD110 is unimplemented. No Done/stage/merge/release/aggregate closure claimed.

No source/docs/tests/Git refs/Linear/Scryer/settings/credentials/memory/CI writes. Only own reports/config and normal isolated build outputs. Reusable-memory assessment: transient evidence; no write warranted/authorized.

**Next owner:** Engineer/Production integration/review handoff under existing authority. Correct-root Scryer, stack/platform/candidate/release/aggregate gates remain. **Cargo slot released; execution stopped.**

Machine result: `result.json`; exact commands: `command-records.json`; full file/hash manifest: `evidence-file-manifest.json`.
