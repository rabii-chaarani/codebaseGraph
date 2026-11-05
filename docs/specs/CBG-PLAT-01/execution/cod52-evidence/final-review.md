# COD-52 / CBG-PLAT-01 S01 — independent final review (initial pass)

## Outcome

One confirmed P2 was found in the broadened Windows test. The coordinator has corrected it in the working tree; the correction is statically verified below, with execution still pending. No confirmed defect survives review of the macOS production cursor-isolation change. **The final review/completion gate is still pending:** the production receipt-selection regression is not yet integrated/reviewed, final-snapshot checks are pending, and native Windows execution is absent.

## Reviewer, scope and snapshot

- Fresh read-only reviewer, did not implement this patch. Used `/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md` and its engineering lenses.
- Actual reviewer execution attested by coordinator: `openai-codex/gpt-6.1-sol`, `thinking="max"`; spawn resolved to that model. The observer does not independently expose effort.
- Base: `7038e66ff8271ffddc511f879cf8d28353a09b2d`.
- Worktree: `/private/tmp/codebasegraph-cod52-macos-scans`.
- Initial review snapshot: `review-snapshot.json`; diff: `review-diff.patch`.
- Initial `src/storage/locks.rs` SHA-256: `7ad30072cb9ceed32391c95638dc3ade880db1251f1bd10cc034f498f3d941b5`.
- Initial `src/storage/maintenance_candidate.rs` SHA-256: `2d63f7e9950cd82784f84fb3cbffcb4ff266508949161ef65e5934d02ae4dd17` (unchanged at initial snapshot).
- Scope: approved rev0.10 at `1a9db7b558b60248888408daa6268a3b45f2f0df`, R01, A01–A02 / S01 only; approval record `docs/specs/CBG-PLAT-01/approvals/cod52-macos-nested-scans.md`; Scryer `chg-bfthrc`, existing `resp-232`/`resp-233`, no new claims.
- Followed the supplied isolated graph-discovery evidence before source inspection. Recalled repository memory, read the canonical cleanup contract through k-wiki, and read Scryer rules, Graph Store intent and the proportional ledger. The configured model is the older root, not SHA-bound worktree evidence; no model mutations or test-report ingestion were performed.

## Finding F1 — P2, confirmed static trace; corrected, execution pending

**New filtered nested-scan assertion fails on native Windows.**

Initial reviewed location: `src/storage/locks.rs:2128–2155`, especially the `directories_only` branch at 2141–2142 and expected-directory assertion at 2150–2151.

The fixture now creates 160 directories plus `regular-payload`. The test calls `visit_directories` and asserts that the outer results contain only directories. Windows `visit_entries_filtered` deliberately ignores `directories_only` (initial line 2764) and uses `FileNamesInformation`, passing every nondot name to the callback (2841–2843). The API documents these as advisory hints (2600–2601), not a cross-platform exact filter. Therefore the Windows outer set necessarily also includes `regular-payload`, causing a deterministic test/CI failure despite unchanged Windows production behavior. The old three-file test used only `visit_entries` and did not have this failure.

Smallest remedy: preserve production behavior and verify directory hints by opening them in the test before comparing the directory-only set. Keep exact-set nested traversal checks for real directories and the unfiltered case.

Coordinator correction inspected: callback now begins `if directories_only && root.open_directory_readonly(name).is_err() { return Ok(()); }`. This defeats the traced failure: a regular file cannot pass the existing directory-handle checks; real fixture directories still run the nested scan and are checked exactly once. Post-correction locks SHA-256: `3334713035db60f24a86c34e3fbdaed0724943fcbdf96103c7bf1a39e163f25c`. **Statically resolved; rerun the final macOS rooted test and native Windows test before claiming execution/qualification.**

## Production and test review

- `openat(retained_fd, c".", O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)` opens a fresh file description relative to the retained actual directory, not a substituted public pathname. The fresh descriptor is immediately owned by `File::from_raw_fd`; all normal, callback-error, parse-error and identity-error exits drop it. No `dup`/`try_clone` isolation or shared-cursor `lseek` remains in the macOS branch.
- The new handle's physical identity is checked against the pin before enumeration. The native error from failed `openat` is preserved; identity mismatch is `InvalidInput`; existing callback and native traversal errors still propagate. The root itself stays borrowed/live through callbacks.
- The 4096-byte scratch/parser and borrowed-name API are unchanged. Rust borrowing requires disjoint mutable scratch for nesting; callback data cannot escape its borrow. The change does not add a heap entry catalog, public-path join or deletion authority.
- Native `open(2)`/`getattrlistbulk(2)` documentation from the installed Apple SDK confirms relative-FD resolution, fresh initial position and reopening as a valid restart mechanism. Both APIs predate the approved macOS13.3 floor (introduced 10.10).
- Rust's primary 1.77 announcement confirms C-string literals are stable from 1.77, below the declared 1.82 minimum: https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/ . Other added Rust operations already exist in this file/baseline. No Rust1.82 execution was performed; current checks use 1.96.
- The multi-batch fixture is independent of incidental native batch sizes: 160 long directory names require more than 4096 bytes, and exact sorted vectors detect omissions and duplicates in both outer and inner visits. The supplied baseline-fail evidence shows the test detects the old cursor bug rather than merely passing both versions.
- The new interrupted-callback/retry test returns `Interrupted` after exactly one callback, replaces the public root name, then repeats complete scans of the original pinned object and checks a foreign replacement sentinel. It tests restart/pin behavior, not arbitrary OS-call preemption or mutation authority.
- Windows and Linux production branches are unchanged. The initial Windows test defect is isolated to new expectations, not a new production enumeration contract. Linux is outside this approved platform scope.
- Existing receipt-selection source reaches nested scans of the same registry in `prepare_terminal_output` → `workspace_has_child_receipts` (maintenance candidate lines 2717–2822, 2651–2697). These calls use separate fixed buffers and preserve conservative veto behavior. The required actual receipt regression is still pending and must be reviewed separately before final acceptance.

## Evidence examined (coordinator runs, not independently rerun)

- `regression-baseline.log` / `.junit.xml`: two new cases, one passes and the nested exact-set case fails against old production source.
- `fixed-rooted-tests.log`: 14/14 pass on macOS before the Windows-test correction and pending receipt integration.
- `storage-foundation-full.log` / `.junit.xml`: 267/267 pass, one ignored/skipped, before the final pending changes.
- `clippy-fixed.log`: successful strict workspace/all-target/all-feature command as supplied by coordinator; the short log alone does not encode flags.
- `baseline-provenance.json`: macOS26.6.2 build25G83 ARM64, Rust1.96, cached lbug archive hash and target dir recorded. This is local evidence, not minimum-macOS or Windows support qualification.
- `storage-maintenance-full.log` is still in progress; no full-suite passing claim is made.

## Remaining gates / limitations

1. Integrate and inspect the actual multi-batch production receipt-selection regression, including later-eligible selection, child-parent protection and cancellation behavior; bind its executed result to final source hashes.
2. Recheck final integrated hashes/diff and the Windows correction; rerun affected checks after the last source/test edit.
3. Execute required native Windows nested traversal/storage checks. Static resolution and macOS passes do not satisfy A02's Windows gate.
4. Rust1.82 and minimum-macOS13.3 were not executed here; do not imply platform/consumer qualification from API-age/static compatibility analysis.
5. Keep unavailable isolated Scryer/Linear reconciliation honest. No older-root model state or root test report was used as exact worktree proof.

No additional conditional product defect survived the scoped review. No builds, source edits, Git operations or external mutations were performed by this reviewer. Only this requested evidence report was written. Reusable-knowledge assessment: no new durable repository memory is warranted from this bounded initial review; transient finding/gate state belongs in this report and delivery evidence.
