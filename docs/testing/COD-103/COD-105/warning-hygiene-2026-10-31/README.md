# COD-105 warning-hygiene local evidence — 2026-10-31

This Production index preserves accepted independent **local macOS** evidence. It does not assert staging readiness, Windows/hosted/artifact qualification, Scryer reconciliation, Done, merge, release or aggregate closure. Related source, evidence and approvals belong in **PR #168** after authorized ownership/fresh-head checks.

## Source and actual results

Tested checkout HEAD was **`e06d7404955abeb7a002b54ad740399dfe10c931`**, tree `7e4fbdb7834dbed308a233c40d2af6acaa09a82c`, **plus the exact five-file dirty diff below**. HEAD alone is not tested source identity. No bodies/assertions/features/manifests/native runner were changed by the warning cleanup.

| Tested file | Final SHA-256 |
| --- | --- |
| `src/adapters/mcp/daemon.rs` | `89fe617cff0e9fcaecb40d4ca726b738f5f99c53817bef04c23fed13317d6ea0` |
| `src/storage/maintenance_controls.rs` | `2ef7e403c9a991c485920ddebce89e047506ac48fa01a44b93b08b46b01973e4` |
| `src/api/lifecycle.rs` | `83214d386bc9e99705a5e3a44a769f71e93f73933b4d80c2a2a9a041814929e8` |
| `tests/generation_storage.rs` | `4734dd94adfc61f4d1783731d7b655fa257a87937aa73866a3b0adcfc3d2a4a0` |
| `crates/k-wiki/tests/bundle_reader.rs` | `464d98c91303e9fb98a766a51f90b3084e2f9d759d9e3c104dcb63f1e92af015` |

The lifecycle hash is the final rustfmt-reflow snapshot. The first layer retains the original failed formatting check unchanged; it is not the later pass. Final formatting passed. Strict `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` passed with no warnings observed; workflow policy and ordinary release-gate passed. These are local checks, not final hosted warning/artifact evidence.

The [final Verifier report](cod105-independent-verification-resumed/report.md) accounts for **136 genuine passed JUnit cases** (71 xtask, 52 affected library, 1 FIFO, 10 bundle-reader, 1 permission failure, 1 ten-update/leak) plus **3 earlier raw-log passes**. The original 15-case integration invocation was interrupted without final exit/JUnit. Its three recorded passes were not repeated or fabricated into XML; only the unfinished 12 cases continued with actual reports. All 15 cases are accounted across these records, **not a single full uninterrupted 15-case pass**. Preserve failed/interrupted attempts and exclusions honestly.

Actual environment/provenance is in the preserved reports: macOS 26.6.2 ARM64, Rust/Cargo 1.96.0, nextest 0.9.140, existing locally verified native 0.19.0 library/headers, documented local OpenSSL/deployment environment and resource jobs=2. The captured current-source CLI SHA-256 was **`b6e604c354a12d683ee9e0aa32f3b878c0f6d1049930da28f951a013f0cf0f12`**. The executable is **not** versioned here. Native tests used the captured CLI and source-bound compiler provenance; no different checkout or Windows execution is inferred.

## Unchanged evidence layers and mapping

All selected Verifier-authored files are copied **byte-for-byte**, preserving these directory names and `../` cross-references:

1. [cod105-independent-verification/](cod105-independent-verification/report.md): original formatting failure and initial snapshot.
2. [cod105-independent-verification-final/](cod105-independent-verification-final/result.json): repaired formatting, initial serial checks and interrupted integration evidence.
3. [cod105-independent-verification-resumed/](cod105-independent-verification-resumed/report.md): safe unfinished-case continuation and final local result.

Original raw/session parent:

`/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/`

Each copied relative layer/file maps to the same path under that original parent. [production-copy-manifest.json](production-copy-manifest.json) lists precisely the selected Git-candidate files, source locations, sizes and SHA-256 values. This is a separate Production mapping, not a modified Verifier manifest. Original Verifier manifests may enumerate binary/raw artifacts that are **not** committed adjacent to the report; this index does not claim otherwise. Only allowlisted UTF-8 text (`md`, `json`, `jsonl`, `log`, `xml`, `txt`, `toml`, `diff`) below 2 MiB/file was selected. No selected text exceeded that bound. The 49,764,112-byte `cod105-independent-verification-final/captured-codebase-graph` stays only in the original artifact directory. No native/test binaries, target outputs, archives, libraries, secrets or runtime caches are versioned.

## Approval and remaining gates

- [Original revision 0.2 spec](https://github.com/rabii-chaarani/codebaseGraph/blob/7e79f7e924536f29d3212e83899b01c38afd39de/docs/specs/COD-103/spec.md) and [original approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md).
- [Approved revision 0.3 spec](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/spec.md) and [separate current approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md). The record commit is not the approved-spec SHA.

The [accepted current Final Reviewer report](final-review.md) completed integrated source review and focused local evidence acceptance for the same five-file snapshot plus the 81 persisted evidence documents, finding no confirmed P0–P3 defects. Production committed those exact tested/reviewed source bytes at **`ef7988439bb26f1d9914ea8d007196f638c2f33b`**, parent `e06d7404955abeb7a002b54ad740399dfe10c931`; [committed-byte-proof.json](committed-byte-proof.json) binds the five hashes and unchanged execution evidence. A commit is not a new test execution. Final-head full two-target hosted CI/optimized packaged smoke, current-source warning inventory and applicable source-model evidence reconciliation remain pending. Later base/integration movement requires affected rechecks and focused review; this local acceptance is not qualification of a later tree. No XML attachment/ingestion/fold against the older dirty Scryer root is claimed. The user's actual Windows 11 desktop PowerShell 5.1/supported 7 tests follow reviewed PR push and still need independent acceptance; local macOS evidence supplies no Windows pass. No eligibility, stage advancement, publication or endpoint waiver follows from copying these files. This evidence persistence changes no tested source bytes; final Git/evidence review must inspect the added documentation before commit.

Raw-format limitation: the complete staged evidence `git diff --cached --check` exits **2**, confined to seven byte-preserved original `.txt`/`.log` help, source-snapshot and failed-format output files. This is recorded tool-output whitespace, not source defects or compiler warnings. Raw bytes are not trimmed; scoped authored/source checks are clean. The committed-byte proof lists affected paths and hashes. The full raw diff is not claimed whitespace-clean.
