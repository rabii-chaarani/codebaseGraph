# COD-103 — bounded software design

**Status:** proposed design for combined approval, not implementation authorization. 2026-10-29.
**Governing requirements:** [spec.md](spec.md), revision 0.2, C103-R01–R06 / A01–A06, plus unchanged **CBG-PLAT-01 rev0.11 S05/R05/A09** at `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`. Canonical new policy home is `docs/specs/COD-103/spec.md`; this report is a design input, not a second spec or execution-brief authority.
**Architecture:** [architecture.md](architecture.md). Reuse Release Verifier / Release Gate. Keep `resp-15` on static workflow validation; GitHub orchestration is not a responsibility of `check_workflows`. No new topology. The two proposed component-level lightweight admission/full-fallback claims fit this design, pending approval; draft Scryer IDs are `chg-9xdkd4`, `resp-1rnbs9` and `resp-tddbkj` as recorded in architecture.md.

## 1. Role, baseline and limits

- Read Engineer, Software Designer and role map. The user's task-only Sol/xhigh override is honored: persisted child metadata exposes `openai-codex/gpt-6.1-sol`, and the session records `thinkingLevel: xhigh`. No role-map/model/settings changes or dispatches.
- Read source through immutable `git show 5889ed09954a4a07efd2590dd1bd380ee77be06a:<path>` after semantic graph search and explicit change-impact/callgraph orientation. Graph MCP is unavailable; CLI fallback was used without refresh/rebuild. The graph is tied to the old root and supplies discovery, not current behavior proof. Read wiki bundles/search/release concept and active memory; inspected committed Scryer Release Verifier, rules and `locate(check_workflows)`. No own/inherited directive set surfaced.
- Current source still has the COD-105 stdout/nonzero false-green, legacy existing-tag ancestry omission, and native-probe exit omission. CI and typed policy still admit only PR bases/main pushes `main` and require five mandatory jobs. Supported targets are exactly `macos-arm64`, `windows-x86_64`.
- Preserve original dirty root, Production's proposal worktree and unrelated work. Writes on this assignment are this `/tmp` report only. No tests, builds, product/Git/Linear/Scryer/wiki/settings writes; no constants changed.
- Main `5889ed0…` is externally merged PR167, not proof of a published 4.0.3 baseline. Follow Production's independent publication refresh; do not retry/dispatch/relabel the live release. Existing PR158 inclusion is approved, but no unknown later product delta is authorized.

## 2. Module and contract map

| Concern | Existing/proposed implementation boundary | Contract / coupling |
|---|---|---|
| Graph packaged MCP smoke | Existing `crates/xtask/src/main.rs::smoke_mcp_stdio`; one small output validator beside it | Preserve callers and `Result<(), String>`; no product MCP API changes or wiki smoke rewrite. |
| Legacy release ancestry | Existing `.github/workflows/release.yml` resolution/upload Bash; retain `.github/actions/verified-release/action.yml` automatic/resume behavior | Strict source SHA in current main history, not tip equality; no release-PR requirement for legacy. Extend actual-body workflow fixtures. |
| Installer probes | Existing `release/install/install.ps1::Assert-Binary` | Immediate native exit capture, before existing two validations complete and before either replacement. |
| Docs Python boundary | Existing `main.rs::check_rust_only_files` plus a small path eligibility helper | `docs/` is a component boundary, not substring match or a product exemption. Same check in ordinary/production gate. |
| Semantic CI classifier | Proposed private `crates/xtask/src/ci.rs` and one `main.rs` command dispatch | Pure conservative semantic diff decision; no GitHub mutations/API client or Cargo dependency resolution. New internal command only. |
| CI selection/freshness/aggregate | Existing `.github/workflows/ci.yml` | Read-only event/API identity, trusted-base helper execution, scheduling and final freshness; one stable `required`. |
| Workflow invariants/tests | Existing `main.rs::check_workflow_policy`, adjacent tests; actual Bash-body fixture pattern in `release/workflow_tests.rs` | YAML, required needs/conditions, allowed events/permissions, classifier provenance and skip policy must change together. |

These are ownership **boundaries for Planner**, not writable-file assignments. Planner records the exact one-writer/file ownership, prerequisites and detailed briefs in Scryer. `main.rs` is shared across COD-105, first-PR stack support, COD-107 and COD-108; serialize those edits. Do not rewrite/extract the whole xtask. Production alone owns Cargo.lock/manifests if a real need emerges; this design adds **no dependencies** or lockfile changes.

## 3. COD-105 — child completion plus parsed response success (R01 / A01)

Keep `smoke_mcp_stdio(executable: &Path, repo_root: &Path) -> Result<(), String>` and the two current requests/IDs (initialize=1, tools/list=2). After writing, drop stdin, then use `wait_with_output()` rather than taking stdout/read-to-string while leaving piped stderr unread. This collects both streams and checks the actual child completion; remove the ignored read result. A write/spawn/wait/UTF-8/protocol error fails. On a write failure, kill/reap the spawned child before returning so this new early path does not orphan it. Do not add a new protocol, handshake state machine, timeout framework or wiki-tool scope.

Proposed pure seam: `validate_mcp_smoke_output(output: &std::process::Output) -> Result<(), String>`. Check nonzero status first, unconditionally. Include status and useful stderr/stdout in failure diagnostics. Parse the current newline-delimited stdout into JSON-RPC objects; blank lines may be ignored, non-JSON/non-object protocol data fails. Valid notifications do not fulfill either response. For each expected ID, require one JSON-RPC 2.0 response, an object `result`, no `error`, and no duplicate/conflicting response. An unexpected response ID is a protocol diagnostic, not initialization/tool evidence. Initialization must contain the existing successful initialize result shape (protocol version and capability/server-info objects); do not introduce a narrower newly supported product protocol version. The tools result must contain a `tools` array with an object whose `name` is exactly `graph_search`. Arbitrary JSON strings, error messages, tool descriptions and stderr cannot supply that evidence.

Tests: feed actual `Output` values to the same validator (platform-native ExitStatus construction is test-only); positive valid responses/zero; nonzero with otherwise convincing success text; zero with error; missing init; wrong ID; malformed/truncated JSON; duplicate error/success ID; missing/description-only graph_search. Confirm `smoke_mcp_stdio` uses this seam, then execute real corrected packaged smoke on both retained native targets. This restores resp16/17/256; attachment or unit parsing alone is not real artifact qualification.

## 4. COD-106 — ancestry at legacy selection and mutation (R02 / A02)

Preserve current exact tag resolution, successful exact-SHA main-push CI selection, provenance/artifact checks, dry-run and assets-only mode. Add a small explicit guard **inside the existing-tag selection branch** after its source SHA is resolved/validated and before publication authorization is emitted:

1. Read `refs/heads/main` through `gh api`; require a full commit SHA.
2. Compare `SOURCE...MAIN` using the existing verified-action direction. Accept only API status **`identical` or `ahead`**. (`ahead` means main is ahead of source, so source is its ancestor.) Reject `behind`, `diverged`, malformed/empty results and API errors.
3. Emit selected source/output only after this guard succeeds. No current-tip equality and no PR lookup needed for legacy.

Replace the existing single-line upload body with a short `set -euo pipefail` body. Immediately before `gh release upload`, re-read current main and repeat that same ancestry predicate for the fixed source. For legacy, also re-resolve the selected tag and require it still identifies the selected source before mutating its release assets; never move it. Automatic/resume retains its existing stronger verified-release action; applying the same final ancestry predicate to all uploads is harmless and keeps one final publication barrier. Keep guards local to these two existing workflow bodies rather than invent a release service/action hierarchy for a few API reads. Existing automatic/resume identity handling is not redesigned.

Failures stop before upload and preserve the tag/release/assets. A normal main fast-forward remains valid. Comparison failure is not a reason to upload, and retry redoes current identity, not a cached success. Dry-run executes the early guard but never the uploader; no crate publish/finalization is introduced for legacy.

Extend `release/workflow_tests.rs`'s closed `gh` fixture to execute the **actual** changed resolve and upload bodies, not recreated algorithms. Cover lightweight/annotated existing tags, identical/ancestor/diverged/behind/API error, main membership removed between selection and upload, changed tag identity, and a call log proving upload was never reached on rejection. Include legacy source with no release PR. Retain existing automatic/resume main-advance, removed-history, exact-run, dry-run and idempotency regressions. A new mock branch matching every request is not evidence; unknown requests must keep failing.

## 5. COD-56 — immediate native exit (external S05/R05/A09)

In `Assert-Binary`, invoke the existing native probe, capture `$LASTEXITCODE` immediately after that invocation/pipeline, and throw when it is nonzero. Diagnostic identifies binary, probe argument and captured numeric exit. Keep PowerShell 5.1-compatible syntax; do not rely on PowerShell 7-only `$PSNativeCommandUseErrorActionPreference` or global preference changes. If a supported shell already throws on the native invocation, propagate that failure; no replacement follows.

Both `Assert-Binary` calls stay before `DryRun` success and before `New-Item`/`Install-Binary`. Existing checksum/path/host checks and per-file temporary replacement remain. No pairwise rollback/atomic upgrade, client `.cmd` change or shell support expansion.

A09 uses checksum-valid **real controlled native executables**, not `.cmd` stand-ins for the native-exit case, for each binary's success/nonzero outcome. Run the identical DryRun/real cases under built-in Windows PowerShell5.1 and a supported PowerShell7 on actual Windows11 x86_64 desktop. Missing/corrupt candidate and either probe failure must preserve both old destination hashes. Record exact OS edition/release/build, shell versions, source/archive/candidate hashes, exits, commands and before/after bytes, including failed attempts. Existing Server CI and source tests are supplementary only. No desktop capability is established; do not close COD-56 or release the partial queue until this evidence and COD-51 S05 approval exist.

## 6. COD-107 — docs-only Python eligibility (R03 / A03)

Keep prohibited root manifests/directories, legacy checks, security checks and workflow Python prohibitions. Do not create or restore helpers in this fix. Existing root Cargo `include` excludes docs, and the native archive has an explicit inventory; preserve those facts and verify actual archives rather than changing packaging speculatively.

Use a helper such as `is_docs_helper_path(root: &Path, candidate: &Path) -> Result<bool, String>` for the **exception only**:

- Work with `Path::components`, not rendered `/` strings. Require an actual first relative component `docs`, at least one following component, and a regular `.py` file. `docs-extra`, `mydocs`, `src/docs`, `..`, drive/root escape and lexical aliases do not qualify. Keep existing case/extension semantics; do not silently broaden to executable Python-like suffixes.
- Require the canonical candidate to remain under the real repository's literal `docs` directory, and reject the exception if `docs` or any candidate path component is a symlink. A Python symlink in docs or a docs directory symlink cannot launder outside code into the allowance; a Python path outside docs pointing inside still does not qualify. A metadata/canonicalization failure grants no exception and produces an actionable policy failure.
- Preserve the existing generated-directory exclusions (`.git`, `target`, `.codebaseGraph`, `.kwiki`) by component, cross-platform, rather than adding non-product folders as implicit exceptions. No permission for Python in knowledge, testing, scripts or archived source outside docs is inferred.

The current walker follows directory symlinks and ignores some I/O errors. Do **not** claim this small allowance repairs the whole walker or makes hostile/cyclic filesystem traversal safe. The maintained tree at the reviewed SHA has no symlinks. Keep the allowance fail-closed for acyclic symlink paths; if implementation requires a broader traversal/error/symlink policy, return it as a scoped contract question instead of silently banning every repository symlink or adding a scanner framework. No symlink target becomes exempt merely because traversal found it below docs.

The gate enforces this path permission; it cannot infer that arbitrary Python content is documentation rather than product code. The remaining proposed local-only/optional/stdlib-only/no-CI/no-package contract is enforced by reviewed uses, unchanged workflow prohibition, no product/build/install invocation, and actual package inventories. Do not install/run Python in CI to parse helpers or add a dependency/import auditor. Documentation should clearly say that path placement is necessary, not sufficient, and give the optional local invocation only when an actual helper exists.

Tests use an isolated complete gate fixture, not mutations to the real checkout: docs root/nested positives and neighboring/outside/root-tooling/workflow negatives through both ordinary and confirmed-production entry points. Add portable component tests; acyclic symlink tests on capable hosts cover docs→outside, outside→docs and symlinked docs directory (report unavailable Windows symlink privilege rather than claim execution). Inspect actual `.crate` and native inventory for `.py`/docs-helper absence; ordinary Git source snapshots may retain docs. Preserve COD-99 bytes/history; no extraction/restoration.

## 7. First PR prerequisite — literal stack CI (R05 / A05)

Deliver stack-enabling CI/policy/tests/docs **with COD-105**, before its staging-only handoff permits COD-106 to open. That limited handoff requires COD-105's applicable local/real-artifact checks, full main-base PR CI and independent final review/fix verification, not the first child event that cannot yet exist. The [delivery stage rule](delivery.md#first-child-staging-clarification-pr-01) assigns Production/Evidence Verifier/Engineer the subsequent first COD-106 non-main-base full-CI proof and blocks full qualification, further handoff and every main integration until it passes. Per-PR retarget checks still occur at their actual stage. This is an explicit prerequisite in COD-105, not an extra backlog item, waived acceptance or an unapproved broad trigger change.

Proposed branch namespace and fixed literal chain:

| PR head | Initial base |
|---|---|
| `cod103-stack/cod-105` | `main` |
| `cod103-stack/cod-106` | `cod103-stack/cod-105` |
| `cod103-stack/cod-56` | `cod103-stack/cod-106` |
| `cod103-stack/cod-107` | `cod103-stack/cod-56` |
| `cod103-stack/cod-108` | `cod103-stack/cod-107` |

Prefer an exact PR-base list `{main, cod103-stack/cod-105, cod103-stack/cod-106, cod103-stack/cod-56, cod103-stack/cod-107}` over an unrestricted wildcard. This is the bounded COD103 namespace; do not include unrelated branches or a push wildcard. Add normal `pull_request` activity `edited` for retargeting (and labeled/unlabeled in COD-108 for trust changes), retaining opened/synchronize/reopened. No `pull_request_target`, new privileged trigger, secret inheritance, artifact retention on PRs or publication from stack branches.

Typed `check_workflow_policy` must validate the exact PR-base set separately from push `{main}`, permit only the approved activity set, reject privileged PR events, and retain release workflow-run `{main}`. At COD-105, every qualifying stack PR still runs the current **full** jobs and stable required; no classifier exists yet. A normal main-base first PR can exercise its head workflow with the same read-only PR posture. Do not require a prior separate CI PR or merge child PRs into their parent branches.

Stack-base advancement is not guaranteed to emit a fresh PR event. On any base/head update, staging evidence for the old pair is obsolete: update/rebase/retarget/reopen as needed to obtain a new event and fresh qualification; a rerun tied to old event SHAs must not be treated as current. Intermediate branches are not protected like main and are never merge destinations. Before each actual integration retarget only the next ready slice to current main, transplant its slice if squash changed ancestry, review the resulting exact diff, and run fresh CI. Use an authorized non-bypass actor under strict main rules. Require full successful main CI for that actual merge before the next integration. The literal stack is a bounded **staging exception**, not permission to integrate later items early.

## 8. COD-108 — small semantic classifier, trusted execution (R04/R05 / A04/A05)

### 8.1 Responsibilities and interfaces

Add private `ci.rs`; proposed CLI:

`cargo run -p xtask --locked -- ci-mode --repo <git-object-checkout> --base-sha <B> --head-sha <H>`

The helper reads immutable Git blobs, never checks out/executes H, never invokes dependency resolution, and never calls GitHub. `--repo` is explicit; do not rely on compile-time workspace paths or inherited cwd. Validate full SHAs, read exact objects with argument arrays, disable external diff/text conversion, and use NUL-delimited name/status/mode data so filenames cannot inject commands. Reject symlink/submodule modes. Require B ancestor of H for the positive release case; use actual **B→H** changes, not merely merge-base→H while the release branch lags current main. Added/deleted/renamed/type-changed files or unknown data select full.

Small data seam:

- `classify_metadata_diff(base/head blob sets) -> MetadataDecision`, where `MetadataDecision` is `Eligible {old_version,new_version}` or `Full {reason_code}`.
- CLI success emits one valid versioned JSON object `{schema_version:1, mode:"lightweight"|"full", reason_code, base_sha, head_sha}`. `reason_code` is a bounded code, not untrusted text in a GitHub-output assignment. A valid negative/unsupported/uncertain comparison is **full**, not a green lightweight result. Invalid invocation/unrecoverable output failure is nonzero; the workflow may explicitly convert a failed eligibility attempt to a fresh, valid full result, but missing/invalid outputs can never imply lightweight or a green required.

GitHub workflow owns the enclosing identity: repository ID, PR number, B/H, event/merge SHA and classification reason recorded with the run. Do not turn local CLI arguments into claims that the helper authenticated a PR.

### 8.2 Exact semantic proof without a new parser dependency

The current xtask has JSON/YAML support, no TOML parser. Use a **conservative recognizer of the current generated layouts**, not a general TOML parser and not a file-only allowlist. Unsupported legal syntax simply runs full CI. Find unique allowed field spans in their exact table/package context, validate simple quoted X.Y.Z values, replace only those old/new value spans with the same sentinel, and require the remainder byte-identical. Base data is the current protected source; preserved base syntax plus recognized valid replacement values cannot smuggle an extra field. The recognizer must positively identify an actual allowed key/value in its unique approved table/package context, not a matching line inside a comment, literal/multiline string or another table. Reject duplicate keys/tables, ambiguous quoting/comment context and unsupported syntax in either blob; do not attempt ad-hoc syntax generalization. Whole-file unchanged remainder comparison remains mandatory after masking only those proven value spans. Do not strip all lines named version, whitespace-normalize arbitrary TOML, or use the first `version =` anywhere. Unsupported valid TOML safely selects full; supporting all valid TOML is explicitly not required.

Positive contract requires one consistent strict new release version, different and greater than the base version, across the complete generated set:

| File | Sole permitted change |
|---|---|
| `.release-please-manifest.json` | Parse JSON; unique root `.` string changes old→new. Other values unchanged. Reject duplicate/ambiguous keys; for current generated layout, byte-identical remainder after the sole recognized value also provides this cheaply. |
| `Cargo.toml` | Unique `[package].version` old→new. Every other byte unchanged. |
| `crates/k-wiki/Cargo.toml` | `[package].version` and `version` of the existing local `dependencies.codebase-graph` entry old→new; path/features/default-features/other fields unchanged. Unsupported dependency representation selects full rather than guessing. |
| `crates/xtask/Cargo.toml` | Unique `[package].version` old→new. No xtask code/dependency change exemption. |
| `Cargo.lock` | Exactly the unique source-less workspace `[[package]]` records `codebase-graph`, `k-wiki`, `xtask` change their own version old→new. Package names/count/order, all dependency arrays/edges (including version-qualified edges), external versions, sources, checksums, metadata and every other byte remain unchanged. Do not regenerate the lockfile to prove this. |
| `CHANGELOG.md` | Insert one nonempty new-version section at the existing release-section boundary; fixed preamble and the complete previous changelog suffix remain byte-identical. No old section deletion/reordering/editing. Version header agrees with new version; malformed/duplicate new section or arbitrary text outside it selects full. Do not interpret release-note prose as shell code. |

Requiring the complete current generated six-file form is intentionally narrower than every conceivable semantic metadata edit; partial/reformatted/novel-but-harmless metadata changes use full CI. Include real PR165/167-shaped fixtures, not string-presence tests. This satisfies the intended case without a new dependency or accepting any external dependency-resolution change.

### 8.3 Current trust and protected-base provenance

All pushes (including main release merges), non-main-base PRs and ordinary/untrusted PRs select full **before** metadata eligibility. Lightweight is only possible for the current open non-draft release PR where current GitHub API and event agree on:

- Repository ID/full name, PR number, base repository/ref `main`, head repository ID/full name equal the base repository, exact configured head ref `release-please--branches--main--components--codebase-graph`, and expected `autorelease: pending` label. Label/title/branch alone never authorize reduction.
- Exact current base SHA B equals the current `refs/heads/main`; exact current head SHA H equals the event head; B/H also match the event under test. A synchronize/retarget/update requires a new decision. No ambiguous/missing/partial response qualifies.
- Verified proposal creator identity for the configured token route. Current #167 was authored by **GitHub user ID41898282, `github-actions[bot]`, type Bot**, and its generated release ref matches the above. The action uses `RELEASE_PLEASE_TOKEN || github.token`; a token-name's existence does not prove its principal. Proposed initial admission pins this observed bot **ID**, with login/type consistency, not a generic username match. If a future/custom token produces another principal, select full until that identity is independently verified and approved; do not auto-learn trust from a newly observed PR. This is a design constant for combined approval, not a settings change.

Run the classifier code and its Cargo manifests/lock/build context from immutable **protected-main B**, never H, a PR artifact, a mutable release branch or the PR checkout's modified xtask. Use a separate trusted-base checkout at B with `persist-credentials:false`; read H's Git objects as data in a separate checkout/object store. The command path, cwd and manifest path all point to B. No secret/proposal/publish token, cargo environment, cache shared with untrusted compiler output or privilege escalation is needed. Job permissions are explicit `contents: read`, plus `pull-requests: read` only where API inspection needs it. No `secrets: inherit`.

**Bootstrap:** before the approved classifier is on protected main, its absence/version mismatch gives explicit full mode. Thus COD-108's own PR necessarily runs full. The repository-policy job may execute PR-head release-gate and classifier unit tests as ordinary untrusted, secret-free full-PR verification; those runs cannot supply lightweight admission or override the selector. After its actual merge and successful full main CI, a generated metadata-only release PR can exercise the trusted lightweight path; retain A04 rollout evidence before merging the final release PR. Do not borrow the head helper to make its own PR lightweight or call tests a hosted scheduling result.

Known GitHub constraint: normal `pull_request` workflows are PR-head-editable. Protected-base helper provenance protects the admission algorithm **when the governed workflow invokes it**; it does not independently make arbitrary workflow YAML tamper-proof or replace independent review/ruleset enforcement. A PR editing CI is not semantically eligible and must use full, and policy/review rejects weakened wiring. Stronger platform-enforced immutable required-workflow policy would be a separate owner-controlled design, not an implied claim or settings edit here.

Live read-only settings observed: public/forkable repo; Actions enabled with `allowed_actions: all`, SHA pinning not server-required; fork contributor approval `first_time_contributors`; default workflow token read; can-approve-PR-reviews true. Therefore author/fork approval is **not trust**, and settings do not prove a restricted action allowlist. Keep existing pinned actions, explicit read permissions and secret-free CI. Do not change these settings or infer a bypass/non-bypass actor from them.

### 8.4 Scheduling and stable required

Add `classify` and an always-mandatory small `repository-policy` job. Move the existing `release-gate` command from the heavy `publish-dry-run` job to that mandatory job; do not duplicate two drifting repository-policy sources. Its ordinary gate includes SECURITY/source/Cargo/release sync/config/workflow/legacy checks; production confirmations remain publication-only. Run the bounded classifier/aggregate regression tests there (`cargo test -p xtask --locked`, subject to actual measured radius), not the product/native suites. This is lightweight relative to the product build and adds no Python dependency.

Both modes require successful **classify, fmt, supply-chain, repository-policy**. Existing advisory scan and parsing-feature guard remain intact. `fmt` remains `cargo fmt --check`. Lightweight positively proves metadata/version/changelog consistency in classify; do not add a package build disguised as metadata validation.

Heavy `clippy`, `publish-dry-run` (root/wiki package checks and size gate) and the two-target `native` reusable workflow run only for explicit valid `full` mode. Native artifacts remain retained only for main push, not PRs. Existing job IDs/native topology remain; no platform removal or empty-matrix trick.

`required` keeps its exact name and `if: ${{ always() }}`; needs every mode-selector/mandatory/heavy job. Its actual executable guard checks:

1. Selector completed success with schema1 and known mode and expected run/event/B/H identity. Missing, unknown, cancelled, failed or malformed selector result fails.
2. Common mandatory jobs each exactly `success`.
3. Full: all heavy jobs exactly `success`. Lightweight: every heavy job exactly `skipped` **and** valid positive classification for the trusted current PR; unexpected success/failure/cancelled/missing states do not get waved through.
4. Re-read current PR/head/base/main immediately before success and require the same identity/B/H/eligibility used for lightweight. For full PRs, require the current B/H still match the tested event. Current API/freshness inability fails required; it is not stale success. Main pushes have no lightweight condition and require full results.

Aggregate result checking can remain a short jq/Bash guard in the workflow, with tests executing the actual body; avoid a second Rust implementation of that state table. Put data in environment/files and pass `--arg`, never interpolate PR text into scripts. Record mode, immutable SHAs, reason and intentional skips in the run summary. A final read is not a transaction with later merge: main strict freshness through an authorized **non-bypass** actor is still mandatory; stack results are staging only. No claim of server freshness for the existing always-bypass admin.

At the eligibility/classification stage, API/diff acquisition failure or unsupported metadata → explicit valid **full** with bounded reason, using the immutable event inputs; no failed proof grants lightweight. At final aggregation, both full and lightweight PRs still require a successful current identity/base/head read matching their tested event. If that final API/freshness check is unavailable or mismatched, required fails even after a valid full fallback; full CI does not excuse stale or unverified identity. Invalid/missing selector likewise fails required until rerun on current input. Never silently default absent mode to lightweight or treat skipped selector as success. Main-push CI must run full even if its diff is exactly generated release metadata, preserving the release's exact-run native artifacts and three-heavy-run optimization rather than two-heavy-run or PR-artifact promotion.

## 9. Verification matrix and delivery handoff

All entries are **planned**, not executed evidence.

| Acceptance | Minimum meaningful execution |
|---|---|
| A01 | Validator output matrix plus real packaged graph MCP smoke on both supported native artifacts. |
| A02 | Actual resolver/uploader Bash with closed API fixture, positive ancestry and negative/error/mutation-race paths; no upload call on rejection; automatic/resume regressions retained. |
| External A09 | Actual Windows11 desktop under both shell families, native candidate checksums/exits, DryRun and real install, destination hashes and preserved failures. |
| A03 | Both gate modes with isolated docs/outside/root/workflow fixtures; portable path and supported symlink tests; actual native/crate inventories; no Python CI/runtime dependency or archived-helper restoration. |
| A04 | Complete real generated metadata positive; every forbidden field/file/format/identity/diff failure full; external lock version/checksum/source/dependency-edge change full; mixed source/features/workflow/install changes full; no head-helper execution. After rollout, actual hosted trusted release PR has heavy jobs unscheduled and mixed/untrusted PR has full CI. |
| A05 | Execute actual aggregation table for each job failure/cancelled/skipped/missing selector/unknown mode, stale base/head/label/identity and API failure. Hosted first true stack and later retarget CI demonstrate stable required; no main-push lightweight, no PR retention, no privileged event. Strict non-bypass main integration capability independently established. |
| A06 | Each serial actual main merge's full CI at exact SHA, exact candidate review, authorized reconciled full delta, production confirmations, exact-run native provenance and independent publication/tag/crate/assets/finalization verification. |

Classifier unit tests should mutate parsed field contexts/blobs and assert decisions; trust fixtures should execute actual workflow API guard bodies and inspect call logs. Workflow policy tests should mutate YAML semantics and execute critical guard bodies, not merely assert magic snippets are present. Include stale-base comparison B→H versus merge-base-only regression, duplicate workspace package/version keys and tables, decoy version lines inside comments/literal/triple-quoted strings, dependency-edge/source/checksum changes with otherwise permitted version bumps, renamed/symlink/submodule paths, malicious filenames/notes, partial API data and helper-absent bootstrap. The real generated six-file layout must remain a positive fixture; unsupported alternate TOML layouts must choose full, not grow a parser to make a test pass. Existing native target tests stay unchanged except necessary matching CI policy assertions.

Before freeze, Requirements/Architect/Planner/Production should cross-check these proposed constants: exact five stack branch names/four added bases, normal PR activity set, current observed proposal bot ID, complete six-file generated-layout admission, retained common checks and skip table. These are concrete **proposals for combined approval**, not unrecorded new user decisions. User's docs boundary, role override, PR158 inclusion, Windows/shell support and retained bypass are already settled and must not be asked again.

The fresh Proposal Reviewer must review the full combined committed snapshot and these known GitHub/bootstrap limitations before implementation. Record any genuine claim deltas only at the approved existing ownership; no claim writes were made here. Final review/evidence must bind exact resulting source. Planner should keep the queue's one active implementation issue and one writer/file while permitting only the explicitly approved ready stack staging exception.

Operational gates remain unestablished: **authorized non-bypass merge actor** and **actual Windows11 desktop PS5.1/supported7 execution**. No account/token/runner/settings changes are authorized by this design. Their absence does not block proposal work but blocks claiming complete acceptance/release. COD-103/platform/COD-51/COD-61 aggregate closure stays with the existing owner.

Reusable-knowledge assessment: no memory recording warranted for unapproved design or transient live settings. Existing immutable contracts and eventual executed evidence are the durable sources.

## Canonical rendering provenance

Production preserved the Software Designer source artifact SHA-256 `5fa6cbd4fc226de259a83444b87e1fb117582aaa67193670276ed9c45688cf12`. Only canonical document references were normalized and the already recorded draft Scryer IDs substituted for pending-ID wording. Design contracts and author conclusions are unchanged. Input artifact hashes identify the author inputs, not this rendered file or an approval. Combined Git snapshot and user approval remain pending.
