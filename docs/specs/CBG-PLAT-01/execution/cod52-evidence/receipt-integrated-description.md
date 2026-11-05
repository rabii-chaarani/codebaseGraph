# COD-52 receipt regression — integrated fixture

The worker initially delivered a tests-only candidate fixture. The coordinator integrated and simplified it before final verification; the earlier plan/report are historical, not the final oracle.

Final test: `storage::maintenance_candidate::tests::terminal_receipt_selection_reaches_late_directory_after_child_receipt_veto`, macOS-only in `src/storage/maintenance_candidate.rs`.

- A canonical temporary fresh-domain Fixture registers a bare kind-2 worker with genuine fresh-root, owner, startup and reference-mutation controls. No RunWorkspace metadata file receipts bypass directory selection.
- Create161 real directory outputs and durable bound origin receipts. Initially every output contains a fixture-owned sentinel. The lexicographically smallest receipt becomes the parent: remove its sentinel, create/record a real child directory then remove only that fixture-owned child payload, retaining the genuine child receipt. The parent is now empty but must remain vetoed by that receipt.
- The other160 directory outputs stay nonempty. Observe actual registry order and choose a filler whose index is greater than128; one exists regardless of directory ordering because160fillers cannot fit in the first129slots. Remove only that filler sentinel so it is the sole eligible directory. The parent receipt is strictly smaller, so losing the child veto would select the parent instead of the expected target.
- Terminalize through real registration, release owner/mutation, and prepare the genuine terminal workspace. Explicitly assert `workspace_has_child_receipts`. Request selector cancellation and require Interrupted with parent retained, reprepare, then call actual `prepare_terminal_output` and require the late target's exact identity/name while the parent remains.
- No production admission change, handcrafted receipt, ordering seam, deletion-authority broadening, or fixed incidental85-entry expectation. Index128 is a conservative test-only proof that records cannot fit a4096-byte buffer given the existing parser's minimum32-byte record; output counts are fixture data, not production constants.
- Native execution results bind only to the final source hashes and command/JUnit evidence in the execution report. Earlier receipt variants' passes/fails are preserved separately, not claimed as final snapshot verification.
