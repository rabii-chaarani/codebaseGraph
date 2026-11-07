# COD-21 final Clippy site — proposed one-file extension

Status: **awaiting human approval before source edits**. Work CBG-C4-01/F02/S02 / COD-21, 7 November 2026, Australia/Darwin.

The approved15-site/nine-file cleanup is applied. A subsequent complete `cargo +1.98.1 clippy -p codebase-graph --all-targets --keep-going -- -D warnings` run reports exactly one remaining diagnostic, in a separately compiled test-support target: `tests/support/storage_maintenance/process.rs:813`. The earlier approval was explicitly limited to nine files, so it does not silently authorize this tenth file.

Proposed sole additional change, by the existing complete technical owner `/root/cod21_implementation_2`:

```rust
// Before
for (phase, frame) in origin.chunks_exact(512).enumerate() {
// After
for (phase, frame) in origin.as_chunks::<512>().0.iter().enumerate() {
```

Preserve the existing2048-byte origin-length check, four512-byte borrowed frames, phase/magic/checksum validation, error order, subsequent payload/key checks, zero-copy behavior and all test semantics. No other source change, suppression, refactor, file scope, dependency or permission is included. This is the same Rust1.98 idiom compatibility requirement, not a change to a storage or C4 contract.

Graph CLI fallback located the file; Scryer orientation places it under Graph Runtime with no new claim or directive proposed. Existing ledger `chg-w9q2zb` remains the proportional empty chore record; all unrelated pending intent/drift stays unchanged. Explicit approval of this committed proposal plus recorded scope/sign-off is required before the owner edits this file. The prior nine-file approval and separate compiler checkpoint fix remain valid.

Verification: repeat unsuppressed keep-going all-target Clippy, formatting and the real affected storage-maintenance process/collector tests; independently verify the complete corrected delivery and focus-review all changes against the final snapshot. The obsolete Scryer-attached semantic-enrichment test name is unavailable in current source and must not be reported as executed. Session logs/evidence remain outside the PR; no merge/release or issue closure is granted.
