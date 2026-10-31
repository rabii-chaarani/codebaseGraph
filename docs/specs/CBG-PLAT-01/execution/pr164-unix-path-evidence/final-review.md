# PR164 Unix PATH repair — final adversarial review

**Verdict: no confirmed P0–P3 findings. Proceed within this bounded repair; final-head native CI and delivery/qualification gates remain separate.**

## Reviewer and snapshot

- Fresh, independent, read-only reviewer using `adversarial-review-engineer` and its engineering lenses; actual model `openai-codex/gpt-6.1-sol`, thinking `max`. Settings verified in this session's `model_change` / `thinking_level_change` records and confirmed by the coordinator. Session `01a180c0-84ee-71cd-8a48-9b3676c6f67f`, child `sub-908b1f5d` (`pr164-unix-final-review`). This reviewer did not implement the change.
- Baseline/HEAD: `afcb83feb98c7a9cffeb2a942dd6f30b6ed23271`; reviewed working-tree source `/private/tmp/codebasegraph-pr164-ci-clean/src/client_process.rs`, SHA-256 `dae066b97109bd2aad214a43ab6c62836bc4952d3d0263b3f39940f3da231db3`. Rechecked unchanged after reading completed execution evidence.
- Boundary: this one-file Unix PATH repair for [review4396573116](https://github.com/rabii-chaarani/codebaseGraph/pull/164#discussion_r4396573116), against CBG-PLAT-01 revision0.11 S04/R04 non-Windows preservation (spec lines160–181). Not a whole-PR audit, merge approval or aggregate closure.
- Read proposal, proposal review, implementation report, actual baseline/current diff, complete current source, relevant real callers, baseline reproduction and corrected RED/GREEN logs. Called branch-local semantic graph search and explicit change-impact/callgraph context before source inspection; connected graph MCP is unavailable for this branch. Recalled wiki memory and read Scryer proportionality/change-ledger rules. Configured Scryer describes the main checkout, not SHA-bound branch implementation evidence; no unrelated model state was substituted.

## Findings and causal checks

**Confirmed defects: none.** The reported non-executable-first PATH regression is fixed, not hidden by weaker assertions.

- `src/client_process.rs:125–144`: each PATH candidate is made absolute before both ordinary-file and execute-access checks; the same path is returned and later passed to `Command::new`. A 0644 earlier file is skipped for a later0755 file under the supported ordinary credentials. PATH order and exact filename selection are retained. Relative and empty PATH-directory spellings resolve against the supplied absolute current directory rather than testing a different path from the one pinned.
- `src/client_process.rs:135–144`: `OsStrExt::as_bytes` preserves non-UTF-8 Unix path bytes. `CString::new` rejects interior NUL, owns the terminated buffer, and remains alive across the narrowly scoped `libc::access(..., X_OK)` call. The unsafe pointer/lifetime boundary is sound. `is_file` follows symlinks as before; no canonicalization changes path spelling.
- Explicit-path lines116–123 and the complete Windows discovery/helpers are byte-for-byte unchanged from baseline. The new Unix helper and tests are Unix-gated; the other non-Windows/non-Unix helper preserves the old file-only predicate. No public signature, dependency manifest, retry, timeout, generic shell fallback or resource policy changed.
- Reachability checked at `src/api/lifecycle.rs:2876–2899` and `src/adapters/cli/install/verify.rs:268–295`: registration and verification both use shared discovery and construct the command from the selected object. There is no second bare-name lookup in these paths.
- Tests at lines361–445 exercise the actual helper, not only a mode predicate: 0644-before0755 selection plus actual script execution and exact two-argument stdout, all-nonexecutables yielding None while an explicit non-executable file remains selected, and first-executable order with relative-directory pinning. Tests supply PATH entries directly without process-global environment mutation. Existing exact-filename assertions remain; only their Unix fixture becomes executable (lines464–468).

## Completed execution evidence reviewed

Tests were executed by the coordinator in the project's Cargo environment, not rerun by this read-only reviewer. I inspected the completed files, independently parsed the JUnit counts, and checked source identity. Exact commands/exits are recorded in `docs/specs/CBG-PLAT-01/execution/pr164-unix-path-evidence/commands.json`.

| Evidence | Completed result |
| --- | --- |
| `baseline.log` | Old bare lookup runs the later client; baseline pinned lookup fails PermissionDenied on the earlier file. |
| `regression-red.log` | Same three helper regressions fail with baseline discovery logic; earlier0644 selected, all-nonexec not rejected, relative PATH not resolved against supplied current directory. Exit101. The separate initial relative-fixture attempt is not used as permission-regression proof. |
| `regression-green.log` | `cargo test --locked --lib client_process::tests::unix_path_ -- --test-threads=1`: 3/3 passed, exit0. |
| `affected-tests.log` and `affected.junit.xml` | Nextest affected MCP/install/lifecycle/client selection: 61/61 passed; XML has61 testcase elements, zero failures/errors/skips.796 other tests excluded by selection, not executed. Run `3398b6a6-2e67-4857-a4df-f7bb95287156`. |
| `fmt.log` | `cargo fmt --all --check` passed. Empty log alone is not proof; coordinator handle87921 and recorded `&&` validation chain completed exit0. |
| `clippy.log` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` finished successfully; same chain exit0. No unresolved warning reported. |
| `windows-helper-check.log` / wrapper | Coordinator reports Windows helper metadata compilation with `-D warnings` exit0; inspected empty log/wrapper. Compile-only evidence, not native Windows execution. |

## Conditional risks and limits

- **Different real/effective credentials:** `access` checks real user/group IDs and supplementary groups, whereas execution may use effective credentials. Assumption: ordinary unprivileged application execution, not setuid/setgid or another credential-switching deployment. Such a deployment could choose/reject the wrong candidate; its support would require a separate contract decision and effective-credential verification. This is an explicitly accepted out-of-scope condition, not a confirmed supported-path defect.
- **Preflight is not execution assurance:** filesystem races, interpreter/format failures and OS execution policy can still fail at spawn. The patch is not complete `execvp` emulation and does not promise fallback/replay after a selected candidate fails. I independently read native macOS26.6 `man 2 access`, which confirms real-credential semantics and the preflight/race limitation. Existing command error handling is not bypassed.
- Local executed evidence is macOS, not final-head native Windows CI or full consumer/platform qualification. Prior-head Windows/COPY passes must not be relabeled as this snapshot's native qualification. No new final-head CI result was inferred.
- No production source edits, Git writes, external mutations or new test runs by this reviewer. Only this requested local report was written. No constants changed by this reviewer. Reusable-memory assessment: no new memory warranted; this is transient review/evidence status, and the durable API caveat is already primary documentation and recorded in the proposal review.
