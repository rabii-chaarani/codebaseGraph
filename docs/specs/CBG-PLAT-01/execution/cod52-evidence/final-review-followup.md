# COD-52 / CBG-PLAT-01 S01 — final-review follow-up

## Outcome

**No unresolved confirmed P0–P3 defect in the final frozen production/test change. F1 is fixed.** Final source hashes independently match `final-frozen-snapshot.json`. The final strengthened receipt test has the intended old-production failure and passes alongside all 14 rooted cases (15/15) on native macOS. The source/test review gate is complete for this exact snapshot. Coordinator-owned final strict/full checks and required native Windows execution remain separate verification/A02 gates. This is not COD-52 closure, release approval or aggregate qualification.

## Reviewer and reviewed state

Same fresh read-only reviewer as `final-review.md`; actual execution was attested by the coordinator as `openai-codex/gpt-6.1-sol`, `thinking="max"`. Used the required adversarial-review-engineer skill; no inherited implementation work.

- Baseline: `7038e66ff8271ffddc511f879cf8d28353a09b2d`.
- Scope: approved spec rev0.10 at `1a9db7b558b60248888408daa6268a3b45f2f0df`, R01 / A01–A02 / S01 only; existing `resp-232`/`resp-233`, Scryer `chg-bfthrc`.
- Production + corrected rooted tests: `src/storage/locks.rs` SHA-256 `3334713035db60f24a86c34e3fbdaed0724943fcbdf96103c7bf1a39e163f25c`.
- Latest strengthened receipt-test source read: `src/storage/maintenance_candidate.rs` SHA-256 `8e539db80a81de8fb674270adaf608a551954db86352b3e87ad674d6badac765`.
- Final binding: `final-frozen-snapshot.json` with the two hashes above; independently reread source matches exactly. The earlier `integrated-snapshot.json`/`integrated-diff.patch` binds candidate hash `80ac795fbc95c49d7f09691ca96a33f21da6362690afe69bd36b1280c31e515a`, not the final variant. That interim mismatch was detected and resolved by the final manifest. Read `receipt-integrated-description.md` for the final oracle; original worker reports describe historical variants.
- Source-discovery evidence: initial `graph-discovery.txt` plus expanded `fixture-graph-discovery.txt`. Inspected only graph-discovered locks/candidate and fixture/control/origin source. Reviewed real receipt factories and canonical temporary fixture handling; mutable implementation facts were not inferred from recalled memory or older-root Scryer state.

## F1 — resolved

The broadened nested test now verifies each directory hint with `open_directory_readonly` before the filtered exact-set assertion (`locks.rs:2131–2133`). Windows intentionally returns advisory names for `visit_directories`, so this excludes the regular payload without changing product behavior. Missing real directories still fail the exact expected-set assertion. The unfiltered branch retains exact complete-set and nested-scan checks. No source path still has the initial deterministic Windows mismatch.

Evidence after correction: `rooted-postreview.log`/JUnit 14/14 macOS pass; the earlier integrated library run also passes all 14 rooted cases. Native Windows has not run, so this is static Windows verification plus native macOS execution, not a Windows passing claim.

## Latest production receipt regression reviewed

Location: `maintenance_candidate.rs:2108–2247` (latest hash above).

1. Uses the genuine canonicalized `maintenance_admission::tests::Fixture`, a fresh kind-2 worker root, exclusive lease, real reference mutation/startup gate and `WorkspaceRegistration::create`. It does not forge a decoded receipt or bypass registration with a mock.
2. Uses `WorkspaceCreatedDirectory::create_new` and `record_created_workspace_directory`; the factory checks live workspace scope and created-directory identity/nonce, creates genuine receipt files, writes/fsyncs and reads back durable origin frames. The test records real receipt names and physical identities from those outputs.
3. Creates 161 root directory outputs, initially all nonempty. It chooses the lexicographically smallest root receipt as the protected parent. It removes that directory's sentinel, creates/records a child and removes only the child payload. The parent is empty while its genuine child receipt remains.
4. The remaining 160 fillers guarantee at least one observed receipt index greater than128. Each native macOS record has at least32 bytes before name storage; more than129 records cannot fit the fixed4096-byte result batch. Thus the selected target is beyond one batch without asserting the incidental native85 count. Observed order is used instead of assuming APFS lexicographic enumeration.
5. Only the selected late filler becomes empty. The parent receipt is asserted lex-smaller than the target, so deleting/omitting the child-veto check from the otherwise-correct selector would select the parent and fail the target-identity assertion. This strengthens the earlier64-pair variant, which independently checked the veto helper but did not guarantee that same mutation was detected by the final selection oracle.
6. Terminalizes the real registration and releases actual owner guards before preparing the real terminal workspace. It explicitly confirms `workspace_has_child_receipts` for the empty parent, then calls the production `prepare_terminal_output`, not a test-only replacement selector.
7. The new cancellation check passes an always-true callback and asserts the actual existing `CandidateError::Io(Interrupted)` outcome. It checks the parent remains, reprepares the real workspace and retries successfully. This tests prompt callback cancellation/retry; it does not claim forced preemption during arbitrary native calls or every nested cancellation point.
8. Final assertions require the selected exact physical identity, directory flag and basename, and retain the parent. Preparation is read-only eligibility work; this test does not execute destructive retirement or claim full deletion-protocol coverage.

The only changes in `maintenance_candidate.rs` are inside the macOS test module. Production receipt selection, child retention, bounded scratch, cancellation mapping, authority checks and deletion protocol are unchanged. The initial macOS production analysis in `final-review.md` remains valid: fresh pinned `openat` description, immediate RAII, identity check, no shared `lseek`, unchanged parser/API and native error propagation. No changed Rust operation exceeds MSRV1.82 by static API/stability analysis; actual MSRV execution remains absent.

## Executed evidence examined and its revision limits

- **Final negative control:** `receipt-final-old-production.log` / `.junit.xml`, final candidate hash `8e539db8…` with only the macOS production cursor restored to its old implementation: expected failure at `maintenance_candidate.rs:2237`, no selected candidate, 0 pass/1 fail (4.725s). Registration, child-veto and immediate-cancel assertions completed before that failure. This isolates the original cursor mechanism rather than invalid fixture setup.
- **Final fixed run:** `integrated-final-regressions.log` / `.junit.xml`, final candidate/locks hashes above, native library target: **15/15 pass**, 830 skipped by the focused filter, total 7.590s. The receipt case passes in 7.111s, including smallest-parent, actual selector cancellation/reprepare, late-target identity/name and retained-parent assertions. All 14 rooted cases pass, including F1's corrected test. The coordinator restored the fixed production bytes after the negative control; this review independently confirms the final source hashes.
- `format-final-frozen.log`: empty success as reported by the coordinator for the final frozen bytes. Final strict/full runs were still in progress at this report update; no passing result is inferred.
- **Historical earlier fixture:** `receipt-old-production.log` / `.junit.xml`: old macOS shared-cursor production with the earlier `80ac795f…` receipt fixture fails at `prepare_terminal_output(...).expect`, returning no candidate. Setup and explicit child-veto checks completed before the failure, so this is the intended selection-completeness negative control rather than an invalid fixture. Result:0 pass,1 fail,9.634s.
- `integrated-regressions.log` / `.junit.xml`: restored production plus the same earlier fixture,15/15 pass in the library target, including all14 rooted tests and actual receipt selection (18.206s receipt case). The storage-foundation harness does **not** include this candidate test; the earlier no-tests attempt is not passing coverage.
- `rooted-postreview.log` / `.junit.xml`:14/14 pass after F1 correction.
- `storage-maintenance-full.log` / `.junit.xml`:11/11 pass (338.450s), before final test-only changes. Includes real worker publication-birth, owner-death/retry, public-build/control and foreign-sentinel checks.
- Earlier storage-foundation267/267 (one ignored), strict Clippy and format results retain their original revision limits. `format-integrated.log` is empty success per coordinator; final full checks are still owned by the coordinator.
- Local execution environment remains macOS26.6.2 build25G83 ARM64, Rust1.96 with recorded native archive provenance. No native Windows, Rust1.82 or macOS13.3 execution was performed by this reviewer.

## Remaining gates and recommendation

- Final source binding and final receipt negative/positive checks are satisfied as recorded above; preserve the separate historical-variant labels.
- Complete coordinator-owned final strict/full verification at this frozen snapshot. Do not count an interrupted/partial full-library run as passing.
- Native Windows nested/storage verification is still required for A02. The final Windows test is statically sound, but neither source review nor macOS results qualify Windows.
- Preserve the older-root Scryer/isolated-model limitation; do not ingest worktree evidence as root implementation proof or silently declare architecture/delivery closure.

The final source/test review is satisfied for the recorded snapshot. Proceed within approved scope only after the remaining executed-verification gates; keep qualification/closure pending until its stated gates pass. No new confirmed defect or separate conditional product risk survived this scoped follow-up. No product constants/dependencies/API or supported platform scope changed. Assumptions: native order is observed before non-registry mutations; the fixture is isolated and no external actor mutates it. No builds, source edits, Git operations or external mutations were performed by this reviewer. Only requested review evidence was written; no reusable-memory candidate is warranted for transient review state.
