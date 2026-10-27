# PR 153 scoped independent review

**Outcome:** No remaining confirmed P0–P3 defects in the final reviewed snapshot. Both P2 issues found during review are corrected and verified. Proceed within this scoped review; platform and aggregate closure remain separate.

## Snapshot and reviewer

- Baseline: `6e1543b038961261555d35e55a4095afae02392e`.
- Worktree: `/private/tmp/codebasegraph-pr153-review`; review limited to the two PR comment fixes and their regressions in three source files.
- Reviewer: `openai-codex/gpt-6.1-sol`, `thinking=max` (parent-confirmed actual spawn metadata); fresh independent child, no implementation or source/Git writes. Read `adversarial-review-engineer/SKILL.md` and `references/engineering-lenses.md`.
- Canonical authority: wiki `architecture/graph-disk-cleanup-contract` revision 0.2; DC-S3-R3/R6/R7 and DC-S4-R3/R4, RAB-8/RAB-9, Scryer bugfix ledger `chg-ndq0yv`. Platform/aggregate/maintainer closure is separate.

Final source SHA-256:

| File | SHA-256 |
| --- | --- |
| `src/storage/maintenance.rs` | `061191affdacb910abe21070c25ae3cc2979db2f87ea0127274dce61e1e19c9e` |
| `src/storage/maintenance_candidate.rs` | `41917895967128bce8c7a5a3b4b6ad4ba55a2398232b8a3239c27e46795f47ba` |
| `src/storage/maintenance_legacy_generations_engine.rs` | `35fd9ff8a8c198226844664605b3dba54038b95d908554912200ef238e17642a` |

## Confirmed findings corrected during review

### P2 — Full advisory scan ignored cancellation

The first reviewed `select_generation_name` enumerated arbitrarily many entries with no cancellation callback after removing the old 100-entry stop. `prepare_work` checked cancellation only before that scan. Once stop/rebind happened, the joined worker would still start every remaining per-generation open. Revision 0.2 permits long preparation and no completed-shutdown deadline, but expressly requires cancellation to stop new work and check available interruption points.

**Correction verified:** `maintenance_legacy_generations_engine.rs:350–394` now accepts `cancel`; its per-entry check returns `Interrupted` before new directory/payload probes. `select_job` and `prepare_work` forward the real callback. The fourth-entry regression stops at exactly four callbacks and leaves all 153 entries intact. No shutdown deadline is imposed.

### P2 — Direct bypass exceeded the existing payload-descriptor cap

The first Direct fix bypassed `state.legacy_interest` in candidate preparation. Real Health confirmation can retain old default Direct DB/manifest descriptors through `PreparedLegacyConfirmationSource` (`maintenance_admission.rs:1355–1359,1435–1447`). The scheduler retains them until confirmation-consuming work claims them (`maintenance_scheduler.rs:393–396,881–886`). Modern Direct work does not consume confirmation. The bypass therefore let those two pins overlap artifact payload plus named recheck (`maintenance_candidate.rs:1404`, `maintenance_origin.rs:787–789`) or manifest scan/recheck (`maintenance_references.rs:702,716`), exceeding the unchanged three-payload-descriptor cap. These are payload files, not stable coordination handles. The original guard prevented this overlap. The initial interest-only test did not construct real request pins.

**Correction verified:** final `maintenance.rs:612–614,862–896,914–926,1199–1217` rejects inapplicable Direct confirmation before source capture/enqueue at all three production paths: ordinary request, transient confirm-and-wait, and controller startup. The candidate pending guard is restored byte-for-byte. Managed confirmation handling is unchanged. `RepositoryIdentity::maintenance_target` explicitly supplies a Direct pair for real Direct selections. The new real-ingress regression creates old default DB/manifest payloads and exercises startup confirmation, later `request(true)`, default-false request, `confirm_and_wait`, and interrupted modern metadata recovery. The baseline comparison fails the actual first committed-recovery assertion, not a helper-predicate assertion.

## Coverage and checks

Source trace covered public Health ingress, actual target selection, request-time pin ownership/scheduler lifetime, candidate metadata recovery, fixed-buffer generation discovery, current/modern/shape filters, downstream eligibility guards, authority boundaries and cancellation. Full advisory discovery retains a 4096-byte stream buffer and one fixed 128-byte selected name, rather than a catalog. It stays off authority; the 100-entry/100-ms mutation grant is unchanged.

An apparent ingress-test hang was traced to the existing scheduler's 60-second cooldown. The sampled worker was in `run_driver -> Condvar::wait_timeout`, while the request waited for a new sequence; the unchanged two-wait positive passed in 120.26 seconds. The final test removes only the redundant second wait. This is not a new product defect and no invocation deadline is claimed.

Final execution evidence inspected:

- Serial native Nextest run `f58f802c-25fd-401e-b79e-ec18806a307f`: **50/50 passed**, zero failures/errors/skipped cases; 790 tests excluded by the filter. Actual run duration 95.603 s. The Direct ingress regression passed in 60.230 s, Managed pending confirmation in 0.112 s, and generation discovery/cancellation in 0.045 s. Scheduler/source-pin regressions also passed.
- Command (parent-executed, Darwin ARM64, native-tests profile):
  ```text
  CARGO_TARGET_DIR=/Users/rabii/Projects/Repositories/codebaseGraph/target cargo nextest run --config-file /private/tmp/pr153-nextest-final.toml --profile pr153 --cargo-profile native-tests -p codebase-graph --lib -E 'test(storage::maintenance::tests) or test(storage::maintenance_candidate::tests) or test(storage::maintenance_legacy_) or test(storage::maintenance_scheduler::)' --test-threads 1 --no-fail-fast
  ```
- Final JUnit: `/private/tmp/pr153-review-final-junit.xml`, SHA-256 `429f5c9a889766dd8122d19f8a898eff63ea11ead38b2a90c79369154db055db`; log `/private/tmp/pr153-review-final.log`. Durable copies are under `docs/testing/pr153-review-fixes/`.
- Final workspace/all-targets `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`: passed; reviewer inspected `/private/tmp/pr153-ingress-clippy.log`. Final `cargo fmt --all -- --check`: parent-reported pass. Reviewer-executed `git diff --check`: exit 0.
- Final three source hashes above independently rechecked after the final run and match the frozen inputs.
- Baseline evidence inspected: `pr153-ingress-baseline.log` fails actual committed recovery (generation 0 versus expected 1); `pr153-legacy-baseline.log` fails with `WouldBlock`. These behavior-restoration probes were parent/implementer-executed, not reviewer-executed. The 7/7 and 30/30 intermediate results are from the earlier candidate-only snapshot and are not substituted for final-source qualification.

No constants changed by the final fix: only the erroneous advisory 100-entry cutoff is removed. The 60-second scheduler interval, authoritative 100-entry/100-ms grant, aggregate memory accounting and three-payload-descriptor cap remain unchanged.

## Risks and limitations

- No remaining source-based conditional defect or open behavioral question is identified in the scoped final source.
- Configured code graph lacks the PR cleanup modules; the review worktree has no local graph DB. Exact reviewed source was used as an explicit read-only discovery fallback, not another project's implementation. Scryer intent was read only from the parent-designated same-project model; no global drift reconciliation was performed.
- Reviewer ran no Cargo command to avoid shared-cache contention. Parent/implementer command logs and JUnit reports are independently inspected; they are not reviewer-executed tests.
- This review does not rerun the full repository suite, native Linux/Windows qualification or whole >100-generation retirement. The >100 fixture proves deterministic read-only selection and cancellation, not whole retirement convergence. No platform, release, milestone or aggregate cleanup closure is inferred.
- Reusable-knowledge assessment: the source-pin/advisory-preparation interaction is a useful invariant already stated in the governing Scryer brief and contract; this read-only review creates no duplicate wiki memory or external state.
