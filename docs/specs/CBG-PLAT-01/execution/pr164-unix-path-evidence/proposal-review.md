# PR164 Unix PATH executable-selection proposal review

**Verdict: PROCEED WITH SPECIFIED CONDITIONS. No decision-blocking findings.**

Reviewed fresh, read-only using `adversarial-proposal-review-engineer`, GPT-6.1 Sol / High. Decision: authorize the bounded Unix PATH repair before source edits, not approve merge or platform qualification. Proposal: `/tmp/pr164-ci-debug/unix-review-fix/proposal.md`. Baseline: `afcb83feb98c7a9cffeb2a942dd6f30b6ed23271` in `/private/tmp/codebasegraph-pr164-ci-clean`; Git was clean at review.

## Recommendation

Use a Unix-only `libc::access(path, libc::X_OK) == 0` preflight, combined with the existing ordinary-file check, before accepting a PATH candidate. Keep the first eligible exact filename in PATH order. Convert native path bytes through `OsStrExt` and `CString`; never use lossy Unicode conversion. Keep the CString alive across the narrowly documented unsafe call. Keep explicit-path handling, Windows behavior and dependency manifests unchanged. `libc = "0.2"` already exists in Cargo.toml.

Prefer checking the absolute candidate that will actually be pinned (derive it using `absolute_path` first), so metadata, permission check and returned path refer to the same spelling. This is a local implementation detail, not a request to broaden relative-path contracts.

Before final acceptance, execute the proposed deterministic regressions: first 0644 candidate skipped for later 0755, all nonexecutable returns None, first executable wins, exact-name fixtures made executable on Unix, and actual execution of the selected script with exact expected argv. The red-before-green regression must exercise `ClientExecutable::discover_in_path`, not only reproduce bare Command behavior. Add a focused nonexecutable explicit-path assertion to prove the PATH filter does not leak into that branch. No process-global PATH mutation is needed. Preserve ordinary spawn errors; an access preflight is not execution assurance.

## Assessment and evidence

- **Established defect:** baseline `src/client_process.rs:111–134` accepts the first PATH ordinary file without checking execute access. Supplied `baseline.log` records bare search exiting 0 with `stdout=runnable`, but the pinned candidate producing PermissionDenied. This directly supports the narrow repair; keeping status quo fails the requested behavior.
- **Supported minimal design:** checking a permission API rather than merely `mode & 0o111 != 0` respects ordinary caller permission and directory-search checks. The latter alternative can select an executable-bit file that the caller cannot execute. Existing symlink-following ordinary-file behavior can remain.
- **Supported scope boundary:** canonical spec `docs/specs/CBG-PLAT-01/spec.md:160–181`, especially line179, requires the Windows adapter not incidentally change macOS behavior. The repair restores the demonstrated Unix PATH case without changing Windows suffix/batch policy or adding a general shell fallback.
- **Primary-source verification:** native `man 2 access` (macOS26.6, manual dated September15,2014; read during review) documents X_OK, real-user/real-group credentials and supplementary groups, failure when file or path-component permissions deny access, and the warning that successful access is only a preflight. It documents `faccessat(..., AT_EACCESS)` as the effective-credential alternative. The source and proposal were reviewed after branch-local semantic graph search plus explicit change_impact context; connected graph MCP is unavailable for this checkout. k-wiki memory recall/search and Scryer proportionality/change-ledger rules were read. No architectural model edits or external mutations were made.

## Conditional risks, not blockers

**U01 — Credential mismatch (conditional; outside approved scope).** `access` checks real rather than effective IDs, while execution uses effective credentials. It is not literally a universal “current user's ability to execute” predicate. With different real/effective identities, it can accept or reject the wrong candidate. The parent confirms no setuid/deployment requirement; the proposal explicitly limits support to ordinary unprivileged execution. Therefore do not expand this repair to setuid handling. State that limitation precisely, and revisit `faccessat`/AT_EACCESS only if such support becomes an actual requirement. No privileged experiment was performed.

**U02 — Preflight is not a launch guarantee (accepted existing limitation).** Filesystem changes, interpreter/format errors and platform execution policy can still make an accepted path fail at spawn. Pinning only the first permission-eligible candidate is not a full execvp emulation. This does not defeat the reproduced 0644-before-0755 repair, and the proposal correctly excludes a new retry/fallback framework. Keep claims narrow; do not describe this change as restoring every possible execvp failure-path behavior.

## Remaining gates and limits

This review approves proceeding with implementation, not its correctness before it exists. Required: actual diff review by the fresh Sol/Max final reviewer; executed focused Unix tests and relevant MCP/lifecycle/client suite; strict formatting/Clippy; native final-head CI and existing delivery/evidence reconciliation. Supplied baseline log was inspected, not independently rerun here. No product source was edited and no tests were executed in this read-only proposal review. No new reusable repository memory is warranted: this is a bounded proposal and the durable platform caveat is already primary-source documentation.
