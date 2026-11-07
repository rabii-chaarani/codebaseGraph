# COD-21 final test-helper extension approval

Work CBG-C4-01/F02/S02 / COD-21; actual human decision, 7 November 2026, Australia/Darwin.

Approved immutable proposal: full Git SHA **`fa4328d913c10ab3925d40804464e24272a9efc3`**, path `docs/specs/CBG-C4-01/execution/COD-21-rust198-process.md`. Question `request_user_input_async`, `call_1ba2c431bec84a679ef724c0ad5e7be8`, item0 named that full SHA and the single remaining `tests/support/storage_maintenance/process.rs:813` change. Exact human answer: **Approve the one-line extension (Recommended)**.

The existing sole technical owner may replace `origin.chunks_exact(512).enumerate()` with `origin.as_chunks::<512>().0.iter().enumerate()` in that test helper, preserving all validation, borrowing, checksum, length, remainder and error behavior. This adds only this file to the previously approved cleanup; no other source scope, suppression, refactor, dependency, merge, release or gate waiver is granted. The source was left untouched until this answer.

Record the expanded scope/sign-off on existing empty Scryer chore ledger `chg-w9q2zb` before editing. No new architectural claim or directive is proposed and unrelated intent/drift remains preserved. Final unsuppressed Clippy, actual independent verification and focused final-snapshot review remain required. Earlier approvals and their exact subjects retain their identities.
