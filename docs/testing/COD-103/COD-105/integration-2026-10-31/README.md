# COD-105 affected integration evidence — 2026-10-31

The [independent Verifier report](verification/report.md) records accepted local affected checks at **`5989dae3040d2bc17fc6c0b62cb62c5f0377c8c4`**, tree **`bca8192ae9a14317dff4b2184c03e326dc3a9e88`**, based on PR168 `b355d3d3751fdb5aa846181e6a4a49a5d140ada7` and main `6a98fb0ef4a6bbde02f1556683401eeded49574e`.

## Actual execution and reuse

New affected checks passed: formatting, complete **71/71 xtask** JUnit cases, workflow policy, ordinary release-gate and strict workspace/all-target/all-feature Clippy with no observed warnings. Actual commands, XML, exit codes, environment and source binding remain in the unchanged reports.

Prior **136 genuine JUnit passes plus 3 raw-log passes** are reused behavior evidence, supported by independent identity of all 378 relevant source/test/config/fixture blobs and the exact five corrected source hashes. The fresh 71 xtask executions overlap the earlier 71: **not 207 unique cases or new full native execution**. Preserve the original interrupted 15-case invocation limitation: no final exit/JUnit for that attempt, three earlier raw passes kept distinct, no synthetic full-run claim. [Prior warning-hygiene evidence](../warning-hygiene-2026-10-31/README.md) retains that history.

Generated metadata consistently advances 4.0.4 to 4.0.5. Parsed lock/manifests match after only the authorized workspace/version masks; no dependency resolution, source, checksum, feature or profile change is asserted. These local macOS results do **not** establish final-head Windows/full hosted native/archive smoke, Windows 11 desktop/shell, current-source Scryer ingestion/fold, staging, merge or release qualification. COD-110 remains unimplemented. The [final focused Final Reviewer report](final-review.md), SHA-256 `670b909f1e13e9d5517a0b192daabc28eccd78f7c918dd9a78c31d1b546614d2`, accepts the actual tested/reviewed HEAD `5989dae3040d2bc17fc6c0b62cb62c5f0377c8c4` plus the exact 32 persisted evidence documents, with no confirmed P0–P3 defects. This closes local integration/evidence review only. Fresh full hosted two-target/artifact/warning evidence, correct-root Scryer reconciliation and deferred first-child A05 proof remain pending. The evidence-only descendant is source-byte-bound to that reviewed HEAD; it is not a newly tested source revision.

## Exact retained-file mapping

Every file in [verification/](verification/report.md) is copied byte-for-byte from the original Verifier session directory:

`/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod105-qualified-integration-verification/`

[production-copy-manifest.json](production-copy-manifest.json) separately lists the copied filenames, sizes and SHA-256 identities. Original Verifier manifests/reports are unchanged and may refer to older external artifact paths or reused-source evidence; their references do not claim those artifacts are all committed adjacent to this index. All source files selected here are allowlisted UTF-8 small text below 2 MiB. No binary, archive, target output, executable, library or runtime cache is versioned.

Approval identity stays [spec revision 0.3 at `316be4c4…`](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/spec.md), with [separate approval record at `0e6008a1…`](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md). A later evidence-only commit does not advance that approved SHA or constitute a new test execution. Related source/spec/approval/evidence documentation remains for existing **PR168**, not a parallel PR. No remote update is authorized by this persistence.
