# COD-105 — independent current source review

## Outcome and handoff

**No confirmed P0–P3 defects found. Integrated source review and focused acceptance of current local execution evidence are complete for the identified source plus 81 evidence documents. Ready for Engineer-authorized commit of the reviewed five-file correction and related evidence, then the local PR update/push path for fresh hosted qualification.** This does not authorize a remote write, staging-only handoff, Done, merge or release; current hosted/platform/model gates remain. No source fix is requested.

| Field | Result |
| --- | --- |
| Status | Integrated source/caller review and same-snapshot local execution-evidence acceptance complete. Local commit/PR-update readiness only; hosted qualification remains pending. |
| Scope | Read-only COD-105 at `/private/tmp/codebasegraph-cod105-resume-20261030`, integrated change versus assigned main baseline `c60a00c1d747b73789cd6edc7ee20601182a35ca`; C103-R01/A01 and first-stack R05/A05. Five-file correction, formatter repair and 81 related evidence documents included. No later-base qualification is claimed. |
| Canonical inputs | `docs/specs/COD-103/spec.md` revision 0.2 and design/architecture/delivery/verification at `7e79f7e924536f29d3212e83899b01c38afd39de`; separate exact approval `383a481bd5a71c58dab7dd796ace92a365760da5:docs/specs/COD-103/approvals/2026-10-29-combined.md`. All five current contract files independently match those approved bytes. |
| Scryer / tracking | Existing `chg-ahwwmf`, ancillary `chg-cgjfa4`, recovery `chg-ndhhtx`; unchanged resp-15/16/17/256. [COD-105](https://linear.app/codebasegraph/issue/COD-105), parent [COD-103](https://linear.app/codebasegraph/issue/COD-103). No new claims. Aggregate closure stays with its existing maintainer/user owner. |
| Next owner | Engineer decides the bounded local commit/PR-update handoff. Production alone may commit these exact reviewed source/evidence bytes after Engineer grant, prove committed bytes, refresh authorized PR head/base/ownership and obtain fresh hosted qualification. |

## Exact reviewed snapshot

- Root: `/private/tmp/codebasegraph-cod105-resume-20261030`.
- HEAD: `e06d7404955abeb7a002b54ad740399dfe10c931`; HEAD tree: `7e4fbdb7834dbed308a233c40d2af6acaa09a82c`.
- Product baseline main: `c60a00c1d747b73789cd6edc7ee20601182a35ca`; baseline tree: `51d5403b9f74bcf6c2725daae2355541ec226fcf`.
- Candidate is HEAD **plus five uncommitted files**, not HEAD alone: daemon, maintenance_controls, lifecycle, generation_storage and bundle_reader below. `git status --short` lists exactly those five; `git ls-files --others --exclude-standard` returned none.
- Complete integrated `git diff --no-ext-diff --binary --full-index c60a00c1d747b73789cd6edc7ee20601182a35ca` identity: SHA-256 `f37a580c47a1ddea01d7c2d5b507b1a386d48368ae5f5b7e7b417ec50b720acf`, 305972 UTF-8 bytes as captured. The 47-path diff contains the eight implementation/support paths below plus committed approved-contract/history/evidence paths. Historical reports were inspected as provenance, not accepted as new execution.
- Original dirty canonical root and old worktree were not changed. COD-110 implementation and future stack-base extension remain excluded. Later exact revision 0.3 queue approval is acknowledged below without restarting unchanged COD-105 review.

| Implementation/support path | Reviewed SHA-256 |
| --- | --- |
| `crates/xtask/src/main.rs` | `88867a05e430c02645c1935ff548b4795c4d8388142f9fc724859d814156b7ff` |
| `.github/workflows/ci.yml` | `98f659e98ad2060f503785390bc810b453a3eb23518b372e85122d2090f1fcda` |
| `docs/release.md` | `20196cefbff6e99fe8959ee36cc78f067ad25753b22c88a0721247ade3aafee9` |
| `src/adapters/mcp/daemon.rs` | `89fe617cff0e9fcaecb40d4ca726b738f5f99c53817bef04c23fed13317d6ea0` |
| `src/storage/maintenance_controls.rs` | `2ef7e403c9a991c485920ddebce89e047506ac48fa01a44b93b08b46b01973e4` |
| `src/api/lifecycle.rs` | `83214d386bc9e99705a5e3a44a769f71e93f73933b4d80c2a2a9a041814929e8` |
| `tests/generation_storage.rs` | `4734dd94adfc61f4d1783731d7b655fa257a87937aa73866a3b0adcfc3d2a4a0` |
| `crates/k-wiki/tests/bundle_reader.rs` | `464d98c91303e9fb98a766a51f90b3084e2f9d759d9e3c104dcb63f1e92af015` |

Hashes were independently read and rechecked after the import-format repair. No source movement was observed during this review. `src/materialization_worker.rs` has **no diff from main**; its inherited import guard is not attributed to this change.

## Confirmed findings

**None.** No consequential hypothesis survived the inspected source, callers, guards and baseline comparison. This is a bounded source review, not exhaustive assurance or an execution pass.

## Causal paths challenged

### Packaged MCP smoke — C103-R01/A01

- `crates/xtask/src/main.rs:2369–2406` preserves the existing two requests, IDs 1/2 and protocol value. The write closure owns and drops stdin on success/error; a write failure attempts kill/reap and returns failure. `wait_with_output` collects both pipes before the one actual validator call. The installed Rust 1.96.0 primary `std::process::Child::wait_with_output` documentation confirms simultaneous wait/collection and stdin closure. No new timeout, protocol state machine or product behavior is introduced.
- `main.rs:2409–2551` rejects non-success child status before looking at stdout. It then requires UTF-8, JSON objects and JSON-RPC 2.0, rejects response errors/request fields, enforces the expected numeric IDs without duplicates, requires the approved initialize field types and an actual tools-array object with `name == "graph_search"`. Missing responses, decoy descriptions, stderr strings and valid notifications cannot fulfill required success. Unexpected response IDs fail rather than becoming evidence.
- Compatible producer traced: `src/adapters/mcp/stdio.rs:9–23,106–112` reads messages until EOF and emits/flushed newline-delimited JSON; `src/adapters/mcp/protocol.rs:21–59,85–88` preserves request IDs and emits the accepted initialize/list result shape. `src/adapters/mcp/tools.rs:10–43` produces a tools array with tool names from operation descriptors. The unchanged producer defeats the hypothesis that the stricter validator rejects its normal output due to a new framing/handshake mismatch.
- Reachable artifact path: `main.rs:464–535` smokes the installed macOS or extracted Windows graph executable; `main.rs:1827–1900` reaches this exact MCP smoke. Fixture success cannot replace real artifact execution.
- New regression seams at `main.rs:2771–2843` call the production validator. The nonzero test's status assertion prevents malformed trailing text alone from satisfying that regression. The grouped negative test checks meaningful rejection triggers, not just line coverage. These tests were source-inspected here, **not executed by this reviewer**.
- Baseline comparison: the old path ignored stdout read failure, did not drain piped stderr, rejected nonzero only when stdout was empty and accepted a `graph_search` substring. The integrated diff replaces those exact false-green mechanisms without changing callers or requests.

### Literal stack foundation — first C103-R05/A05 scope

- `.github/workflows/ci.yml:3–18` admits exactly the approved five literal PR bases and opened/synchronize/reopened/edited activities. Main-only push, read-only permissions, both supported native targets, push-only native archive retention and the full mandatory `required` graph remain unchanged (`:20–25,102–141`). No privileged event, wildcard, new secret or release permission is present.
- `main.rs:1218–1265,1718–1732` enforces exact PR/push sets separately. The new sequence matcher requires exact length and string elements before equality, so duplicate/junk entries cannot stand in for a missing approved member. `pull_request_target` is rejected. Actual-YAML mutation tests at `:3166–3237` exercise these checks; existing release/native/publication safeguards remain.
- The unchanged release workflow still restricts CI-completion publication to main. The unchanged reusable native workflow requires native test success before artifacts. No COD-108 classifier/lightweight admission was introduced.
- A stack-base advance may need a new event; the approved design explicitly requires current base/head qualification rather than accepting an old-event rerun. The code/docs do not claim this operational proof has executed. Actual first-child/retarget evidence remains pending, not a new demonstrated code defect.

### Five-file platform warning restoration

| Changed location | Reachable callers / strongest safeguard |
| --- | --- |
| `src/adapters/mcp/daemon.rs:1838–1844` | `home_dir` exists only on macOS/Linux, matching its two callers inside `PlatformService::for_spec`'s compile-time macOS/Linux branches (`:1347–1366`). Windows service construction uses `spec.state_dir` (`:1367–1373`). Body and defaults are unchanged. |
| `src/storage/maintenance_controls.rs:3434–3444` | `workspace_fifo_pin_name` is Unix-only, matching local `WorkspaceCreatedFifo` definition/impl (`:3447–3456`) and its calls (`:3504,3532`). Cross-module calls are inside explicit Unix blocks in `maintenance_candidate.rs:3050–3065,3357–3360`; collector call `:2076` belongs to Unix-only test starting `:2016`. No caller was removed or rewritten. |
| `src/api/lifecycle.rs:5113–5126` | Only the test import is Unix-gated. Its test use is inside the Unix block (`:6166–6182`). The portable production function (`:2255–2259`) and production uses at `:841,1569,2229,2239` are unchanged. The formatter repair moves/reflows imports only. |
| `tests/generation_storage.rs:427–440` | `materialize_err` matches its only caller (`:135`) in the already Unix-gated permission-failure test (`:116–146`). Shared imports/request helper retain other uses; test selection and assertions are untouched. |
| `crates/k-wiki/tests/bundle_reader.rs:39–52` | `copy_dir_all`'s only nonrecursive use (`:164`) is in the existing Unix-only symlink block (`:158–184`). Other shared imports still have portable uses. No test body or assertion changed. |

The actual dirty diff is restricted to the four helper cfg attributes plus the test import split/reflow. No body, assertion, test removal/ignore, lint suppression, dependency, feature/default, manifest, runtime or native-runner change was found.

## Evidence available at the initial source checkpoint (historical status)

- Reviewer executed **no Cargo, compiler, build, test, formatter, native artifact, CI or remote publication command**. Review methods were read-only Git identity/diff/baseline checks and exact source/document/tool reads. `git diff --check` returned exit 0. This whitespace check is not `cargo fmt` or compilation.
- Initial independent formatting failed on lifecycle hash `de43c2c62b92c8989a8ed701356ebb959eb499f19bcf1ac16fda47df75e37e65`. Its failure log is preserved by the Verifier. Source owner corrected only the import placement/reflow to the final `83214d…` hash. I inspected the actual corrected diff and the owner's formatting-only exit-0 report. That is not substituted for the independent final rerun.
- Engineer reports the independent Verifier has restarted checks on the final snapshot and retains exclusive Cargo execution. **No current independent local execution is claimed passed in this report.**
- Committed reports under `docs/testing/COD-103/COD-105` bind older source/baseline hashes and runs. They remain historical, not passing execution on this rebased candidate. In particular current main.rs hash `88867a…` differs from historical tested `652dbe…`.

## Retained limits and gates

1. **Local evidence review completed in the final addendum below.** The actual final Verifier reports, commands, inventories, XML, logs and source binding were inspected. The earlier fmt failure remains preserved and the final independent formatter check passed. Source movement still requires affected rechecks/focused review. This closes the local-evidence review limit only; it does not qualify hosted/native archives or a later base.
2. **Current hosted qualification pending.** Full first main-base PR CI, current native tests/debug/test cohorts and actual macos-arm64/windows-x86_64 release graph/wiki artifact smoke plus warning scan remain required. Prior hosted green runs are not current evidence. Configured jobs/source traces are not execution.
3. **Exact-source Scryer reconciliation pending.** Read governing/scoped rules, `orient`, all three assigned `get_pending` views, committed `read_model(node-1)` and `get_drift` using the existing canonical project. They supply intended claims/briefs only. The original immutable brief's proposal wording is governed by the later exact approval; no intent change is inferred. The canonical fingerprint root is not this worktree. No old-root test attachment/ingestion/fold, copied model or global drift reconciliation was performed or endorsed. Unrelated pending claims, including COD-110 proposal intent, are excluded.
4. **Deferred first-child A05 proof remains pending.** After all staging-only gates, the actual fresh COD-106 → COD-105 event must establish full CI/stable required for the current base/head/run/attempt. Missing/failed/stale proof blocks full qualification, next handoff, every actual main integration, Done and release. No permission to open/stage COD-106 is granted by this report.
5. **Later delivery limits unchanged.** Actual Windows 11 desktop PowerShell 5.1/supported 7 A09 is COD-56 qualification owned by the user/later slice; Server CI is not a substitute. A compliant non-bypass merge actor is separate. No merge/release or aggregate closure is certified.
6. **Tool limits.** Graph MCP explicitly binds the old canonical root, so search/context supplied discovery only. `maintenance_controls` is not directly indexed; I independently followed graph-discovered `src/storage/mod.rs`'s exact module declaration, with its exact candidate module/test path references. Direct search/context returned unrelated siblings; no graph rebuild/CLI bypass was used. One busy graph request was retried later successfully for other targets. Linear connection reports pending verification, so no live Linear status is certified and no tracking write occurred.

## Execution settings, authority and memory

- Required and Engineer/host-attested accepted pair: **`gpt-6.1-sol` / `max`**, matching current `models.toml` Final Reviewer and required adversarial-review-engineer method. No substitution, nested dispatch or map change.
- RLM does **not expose accepted `agent_type` / `fork_turns` fields or independent runtime model/effort metadata here**. I do not claim literal `agent_type: default` / `fork_turns: none` host keys were accepted. The assignment supplied fresh independent context and I did not author this implementation. Host-level capability attestation, if required beyond that supplied dispatch, remains Engineer's check; no known mismatch is asserted merely from missing self-inspection.
- Read current repository instructions (candidate has no AGENTS.md; supplied/canonical instructions govern), Engineer/Final Reviewer/adversarial skills and fix-verification method, relevant wiki discovery/memory, exact approved contracts and Scryer rules/intent before judgments. Read-only limits preserved; only this review report is written in this reviewer's artifacts.
- Assumptions are the explicitly approved same-scope protocol/result-shape and literal stack contracts; no new product policy is imposed. **No constants or reviewed files were changed by the reviewer.** Reusable-knowledge assessment: no new durable claim is warranted for this transient snapshot; no memory/wiki write is authorized or needed.

### Final same-snapshot execution-evidence acceptance

**Accepted for the bounded local commit/PR-update handoff; no confirmed P0–P3 defect remains.** This completes the existing independent source/local-evidence review. Engineer may hand the exact five-file correction and related text evidence to Production for an authorized commit, followed by the PR-update/push path for fresh hosted qualification. This does not grant remote write/ownership authority or imply staging, Done, main integration or publication.

#### Authority update without a changed COD-105 oracle

Read exact current `docs/specs/COD-103/spec.md` revision **0.3 at `316be4c4bb81d4f6e025016b12374836b4dcc1a8`**, and separate approval **`0e6008a1f46679a86aba85ac513beb05af94b0c0:docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md`**. The record identifies the user's **“i already approved revision 0.3”** confirmation and COD-103 comment `c8690556-e836-428d-bc81-079c701d2395`. Its later exact approval governs historical preparation wording. COD-105 R01/A01 and current first-stack R05/A05 behavior are unchanged; no approval-driven rerun or new review gate was added. COD-110 implementation/future foundation remain excluded. Original revision 0.2 approval/history is preserved.

#### Actual evidence inspected

Original artifact parent: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7`.

- Final `cod105-independent-verification-resumed/report.md`: SHA-256 **`d524266f76662f54ca05fd66a7b199db630e9bda3d44f412ed1291cf00080d58`**.
- Final `result.json`: **`9087d052e13117e5ed419042ef0202225cb3a8c973b172ce19eadcfc8d42aa86`**.
- Final raw `evidence-file-manifest.json`: **`e209722736cb233eb0a5d1f0ab8b01e697035f5811132d8f51c3cbb748fef24e`**. Independently hashed all **68 listed files** and checked sizes: all match, including the actual captured executable.
- Initial/final command-record SHA-256: **`406a6d5604ca0796b36a3dfb213ff289d0600208fdc1570a44f0006b0fc89699`** (`cod105-independent-verification-final/command-records.json`) and **`657e7790389f946b3caffaf39f5fc59abbeab5aa3341b5ebe6219945785ef5c1`** (`cod105-independent-verification-resumed/command-records.json`). Actual commands, cwd, profiles/features, environment, exits, timing and interrupted status were inspected.
- Independently parsed all six genuine nextest XML reports: **136 testcase elements**, zero failure/error/skipped elements, no duplicate testcase names. Actual selected inventories match every XML case set, including the exact 12-case continuation. Nonselected cases are exclusions, not passes.

| Genuine XML | Passed testcase elements | SHA-256 |
| --- | ---: | --- |
| `xtask-junit.xml` | 71 | `f0d4efbc9c009e5c6a961565a6034832a5e4d984b177be70425d128f7d7df1f5` |
| `native-focused-junit.xml` | 52 | `5ee9fc153bf6fe00129f95f6bc324da9cadcc29ee6366e0ab16a40b37871b718` |
| `fifo-focused-junit.xml` | 1 | `25d41e9ebd1fd2c032f793f05dfcfb4a99b437020a632310a68acbf568706a39` |
| `bundle-reader-junit.xml` | 10 | `31709887f6692e20edc4bfb3bbe750c29a4ec663759e33a26c9867068072885d` |
| `generation-permission-junit.xml` | 1 | `8ed056b38c672f4173055cc77caf8a0ca2552d9a52d5cf954d80229cb50e980f` |
| `generation-churn-junit.xml` | 1 | `a0c76183662dbb9daf828bd6b920c0baf1524d271f122cd44297e86fcdae7531` |

#### Accepted local execution and limits

Environment: macOS **26.6.2 / 25G83 ARM64**, Cargo/Rust **1.96.0**, nextest **0.9.140**. Native runs preserve Cargo **`native-tests`**, opt-level 3, debug assertions/overflow checks, the compiler-reported unified features (`default`, `bundled-windows-extensions`), retries 0 and one test thread. No custom Rust flags, shared target, alternate-checkout executable or native-runner/profile edit was used. Jobs 2 is documented resource concurrency, not a product constant. Clippy used its ordinary dev profile.

| Actual check | Accepted result |
| --- | --- |
| `cargo fmt --all --check` | Independent final exit 0 after the final lifecycle repair. Original failed formatter log remains separate and unchanged. |
| Full xtask nextest, no test filter | **71/71 passed**, including smoke-validator and literal-workflow cases; no skipped cases. |
| Affected native library tests | **52/52 passed**; 796 nonselected cases excluded. |
| Real FIFO producer/collector | **1/1 passed** with the captured current-source CLI; 847 nonselected cases excluded. |
| Complete k-wiki bundle_reader | **10/10 passed**. |
| Unix permission-failure generation case | **1/1 passed**, 11.703 s testcase. |
| Ten-update/leak generation case | **1/1 passed**, 1122.295 s testcase / 1122.641 s command wall time. SLOW notices are not failures; no timeout, resource budget, assertion or oracle changed. |
| `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` | Exit 0, all three workspace packages; no compiler warnings observed. Does not compile another platform's cfg paths. |
| `cargo run --offline --locked -p xtask -- check-workflows` and ordinary `release-gate` | Both exit 0; actual success output inspected. No production confirmations/publication. |

Original integration run **`44488b22-47f0-40bc-9549-c4291935121c`** remains **interrupted/incomplete**, with no final exit, summary or XML. Its unchanged raw log records three passes: direct custom-path materialization (3.314 s), direct searchable-sidecar publication (3.531 s), atomic managed activation (317.397 s). Engineer explicitly authorized continuation after the no-owned-process checkpoint. Only the unfinished two generation and ten wiki cases continued. Independently verified: **3 raw-log passes +12 resumed XML cases exactly equal the original 15-case macOS inventory with no overlap**. This is **not a single uninterrupted 15-case suite pass**, not 139 JUnit cases and not a full workspace/native-cohort run. Interruption cause is unestablished, not a confirmed product defect. Existing Windows-only generation coverage is not compiled on macOS and is not silently counted.

#### Source/native executable binding

All eight reviewed implementation/support hashes still match the source manifest above; Verifier before/after records match the exact five dirty hashes. HEAD/tree remain **`e06d7404955abeb7a002b54ad740399dfe10c931` / `7e4fbdb7834dbed308a233c40d2af6acaa09a82c`**. Complete tracked diff versus assigned base `c60a00c1…` retains SHA-256 **`f37a580c47a1ddea01d7c2d5b507b1a386d48368ae5f5b7e7b417ec50b720acf`**. Read-only `git diff --check` returned 0. Source did not move during this evidence review.

Compiler JSON has exactly one root-manifest-bound normal `codebase-graph` executable with `profile.test=false`, opt-level 3 and debug assertions/overflow checks; build-finished is success and no compiler-message warning/error exists. Captured CLI hash **`b6e604c354a12d683ee9e0aa32f3b878c0f6d1049930da28f951a013f0cf0f12`** was independently checked against the actual preserved bytes and source-bound compiler/provenance record. `CARGO_BIN_EXE_codebase-graph` targets this unchanged diagnostic copy for native runs. This is test-profile evidence, **not optimized release/archive smoke**.

Actual local LBUG0.19.0 static library/header files were independently hashed and match the recorded official member identities (`d2e9a5…`, `7da5b7…`, `0b463b…`). The outer cached archive is absent; no current archive verification/download/fallback is claimed. Documented local OpenSSL3.6.2 and deployment13.3 are developer test prerequisites, not clean-consumer qualification.

#### Related durable evidence included

Production added only untracked text evidence under **`docs/testing/COD-103/COD-105/warning-hygiene-2026-10-31/`**. The earlier no-untracked observation remains the original source-review checkpoint, not the final documentation inventory. Inspected the new README/mapping and independently verified all **79 copied files byte-for-byte against originals**, the exact **81-file /920767-byte** set including the two Production documents. Copies preserve initial failed formatting and interrupted execution, with no false full-suite/hosted pass. The Production mapping is separate from original Verifier manifests, which may reference excluded raw binaries. The 49,764,112-byte CLI remains only in session artifacts; no executable/archive/library is versioned.

- Production README SHA-256: **`f5f8c39fc0d81e7f33f74174a7c0c0c995e58f12294129c298650205c5e8123f`**.
- Production copy-manifest SHA-256: **`caa32042e9633f157e3b87fd30d9050e79ee8311a2106f72c7c2510fc867d353`**; binds all 79 copy paths/sizes/hashes, all matched.

The README's review-pending wording is historical persistence status, superseded by this handoff, not a hosted-pass claim. Thus local commit readiness includes the **exact five source edits plus these 81 evidence documents**, not unexamined later work. Production may commit source first and records next as Engineer directs; it must prove committed source/evidence bytes equal the reviewed snapshot. Such a commit is not new execution at that commit. A faithful copy of this report may accompany the related PR without rewriting its evidence.

#### Remaining gates, not waived

1. **Fresh current-head/base hosted qualification:** full macos-arm64/windows-x86_64 native tests/cohorts, actual optimized packaged graph/wiki smoke, compiler-warning inventory and stable required result remain pending. Historical hosted results/local macOS runs do not substitute.
2. **Current-main/PR freshness:** review/test baseline is assigned `c60a00c1…`, not coordinator-reported later main metadata snapshot `6a98…`. That short object was unavailable in this worktree; no fetch/remote action was performed. Production must identify the full current main/base/head and authorized PR ownership, assess the delta and obtain affected fresh evidence. No current-main diff/merge readiness is asserted. Source/contract changes need affected checks/review, not a speculative extra stage now.
3. **Legitimate exact-source Scryer binding** remains pending; no old dirty-root attachments/ingestion/fold/global reconciliation occurred or is endorsed.
4. **Actual deferred first-child C103-A05 proof** remains pending; no COD-106 stage advancement, Done, merge or release is granted here.
5. **Later COD-56 Windows11 desktop PowerShell5.1/supported7 A09**, compliant non-bypass strict-freshness merger, complete queue/publication and aggregate closure remain with existing owners.

Accepted reviewer pair remains **`gpt-6.1-sol` / `max`**, explicitly host/Engineer-attested. Fresh independent RLM context and no implementation-authoring history are preserved. Host `agent_type` / `fork_turns` fields and independent runtime model/effort metadata are **unexposed**, not guessed or claimed accepted. No substitution/nested dispatch. Reviewer ran no Cargo, tests/builds, source edits, tracking/model/Git writes or remote actions. Only this own-session report was edited. No new assumptions, product constants or reusable memory were introduced.
