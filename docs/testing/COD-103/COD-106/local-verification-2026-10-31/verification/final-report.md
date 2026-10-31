# COD-106 — independent local verification

## Status and scope

**PASS for assigned local verification of C103-R02 / C103-A02, existing resp-259.** No implementation, platform/hosted qualification, PR acceptance, Done, integration or release is claimed. Cargo is **released; STOP**. No running check remains.

Evidence Verifier used current `inherit`/`inherit` mapping, with no explicit overrides as assigned. Actual model/effort metadata is not independently exposed. This verifier is independent of the two source authors. Writes were limited to this session artifact directory and normal project build outputs. No source/Git/CI/model/tracking/settings/credentials/memory writes occurred.

## Authority and exact snapshot

- Canonical `docs/specs/COD-103/spec.md` revision0.3 and all five companion contracts read at `316be4c4bb81d4f6e025016b12374836b4dcc1a8`. Separate approval `0e6008a1f46679a86aba85ac513beb05af94b0c0:docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md` pins that exact revision and user confirmation “i already approved revision 0.3”; COD103 decision `c8690556-e836-428d-bc81-079c701d2395`.
- Existing Scryer `chg-9bqhd2`, project `/Users/rabii/Projects/Repositories/codebaseGraph`, `container-release-verifier` whole description SHA256 `2c1a58fb8882594137c391a846ab4194dc11daa8203b6b10d23d96a15e75f486` was retrieved and matched. No new claims/topology. Engineer supplies accepted limited COD105 staging and current COD106 activation; historical pending prose is not a new approval gate.
- Actual cwd `/private/tmp/codebasegraph-cod106-20261031`, branch `cod103-stack/cod-106`, HEAD `99a339c54df79cef0598905cde8c6ce4e5d62163`; HEAD tree `8bc68d5e5018ca3b64f0a3ad4240431b9e5ac902` is the **committed baseline tree**, not the dirty tree.
- Exactly two tracked dirty files and no untracked files in the actual status; unchanged before/after:
  - `.github/workflows/release.yml`: `e11c179705340bea6230dfac1a50c055d34fb7137d20f2c271e335c589653117`
  - `crates/xtask/src/release/workflow_tests.rs`: `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450`
- Complete dirty diff SHA256 `c7b76fcb32d0943b426d1895194c1739d6e30a7bda1cd5f563996bb5d905996c`; `git-diff-before.log` and `git-diff-after.log` are byte-identical. Actual diff is 552 insertions/5 deletions in those two files. Production's supplied YAML diff matched the actual YAML diff exactly.
- All xtask source, manifests, lock and Cargo config hashes in `source-inputs-before.json` / `source-inputs-after.json` are unchanged. No shared other-checkout executable was used. Fresh inventory compilation identifies this exact path; dependency info includes actual workflow/action sources and CARGO_MANIFEST_DIR. Native-tests binary `xtask-eed5a74c0d8f01ef`, SHA256 `2151a0b5e0cce95c8e91bb2195e0c2dea11fc7586d2a416ae6776a2ead5c424b`, unchanged after checks. Fingerprint shows features `[]`, Rust flags `[]`, and unchanged declared dependency set. See `native-tests-fingerprints.json` and `native-tests-depinfo.d`.

## Actual environment and execution

Actual `sw_vers`: **macOS26.6.2, build25G83**. `uname`: Darwin25.6.0, arm64. Rust/Cargo1.96.0, nextest0.9.140, Bash3.2.57, jq1.7.1-apple. The historical macOS15.6 claim is not propagated. Raw version outputs are retained.

Existing Cargo `native-tests` profile inherits release and keeps debug assertions, overflow checks and unwind. Project Cargo config forces serial RUST_TEST_THREADS and project-local test HOME. No feature, default, profile, manifest, dependency, runner or installation changes. Temporary **outside-repository** nextest config adds only reporting profile `cod103`, retries0 and one thread. Exact commands, cwd, actual handle timings/exits and timestamps are in `commands.json`.

| Actual command | Result | Raw output |
| --- | --- | --- |
| `cargo fmt --all --check` | exit 0 | `fmt.log` |
| `cargo nextest list --offline --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-bee0f574/cod106-independent-verification/nextest.toml --user-config-file none --profile cod103 --message-format json` | exit 0 | `xtask-inventory.log` |
| `cargo nextest run --offline --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-bee0f574/cod106-independent-verification/nextest.toml --user-config-file none --profile cod103 --no-tests fail --test-threads 1` | exit 0 | `xtask-full-nextest.log` |
| `cargo clippy --offline --locked -p xtask --all-targets -- -D warnings` | exit 0 | `clippy.log` |
| `cargo run --offline --locked -p xtask -- check-workflows` | exit 0 | `check-workflows.log` |
| `cargo run --offline --locked -p xtask -- release-gate` | exit 0 | `release-gate.log` |
| `bash -n /Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-bee0f574/cod106-independent-verification/resolver.sh` | exit 0 | `resolver-bash-syntax.log` |
| `bash -n /Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-bee0f574/cod106-independent-verification/publisher.sh` | exit 0 | `publisher-bash-syntax.log` |
| `git diff --check` | exit 0 | `diff-check.log` |

**One genuine full run: 76/76 xtask tests passed, 0 skipped/failures/errors/retries.** This contains **23/23 workflow cases and all 5/5 new named cases**; these are subsets, not additive counts. Grouped matrix loop subcases are not extra JUnit names. No duplicate cargo-test run was needed solely for counts. Genuine runner JUnit: `xtask-junit.xml`, SHA256 `f7482e2d985ae6b794194000d9dbe4ac56f57efbaa5de442786caa87b396388f`, run `64ccee74-a225-4b2a-aa5a-e86f61a2b111`. `case-inventory.json` and `junit-summary.json` agree exactly on discovered/executed names. Observed Rust compiler warnings are **0 in the executed scoped xtask outputs**, including strict Clippy; this is not full-workspace/native qualification.

## Criterion trace and oracle assessment

| Criterion / requirement | Intent | Actual source and test anchors | Observed method/result and limit |
| --- | --- | --- | --- |
| C103-A02 / R02: historical source accepts identical/ancestor, not tip equality; no release-PR requirement | resp-259 unchanged | release.yml94–156, especially123–130; workflow_tests.rs803 `existing_tag_resolver_accepts_historical_lightweight_and_annotated_ancestors` | Actual YAML resolver executed with lightweight/annotated refs, identical/ahead and historical source; fixed full source output, automatic=false, no API writes. PASS. |
| Same: nonancestor/identity/acquisition failures before authorization | resp-259 | workflow_tests.rs853 `existing_tag_resolver_rejects_nonancestors_and_bad_identity_before_authorizing`, rejection helper728–740 | behind/diverged/empty/unknown comparison; missing/malformed ref/source/main/annotation; API errors and wrong repo/source endpoint fail. Empty authorization output, no summary, no mutation. Strict exact main/tag/annotation/compare endpoint and source…main direction; unknown requests exit97. PASS. |
| Same: freshly re-resolve selected tag and current main immediately before reachable upload | resp-259 | release.yml518–534; workflow_tests.rs1028 `legacy_publisher_rechecks_fresh_tag_and_main_before_upload_and_allows_advancement`; helper758–788 | Actual draft/complete and published-mutable/incomplete paths reach upload. Ordered assertions require initial tag < fresh tag < fresh main < fixed SOURCE…freshMAIN compare < upload. Annotated peel occurs twice; normal main advance succeeds. Legacy release stays unchanged/draft unchanged, no POST/PATCH. PASS. |
| Same: late ancestry/tag races block final asset mutation and preserve existing state | resp-259 | workflow_tests.rs1086 `legacy_publisher_rejects_late_history_and_tag_races_without_mutation`; helpers740–755 and790–800 | First actual resolver accepts. Later main divergence/lookup/compare/malformed result or changed/missing/error lightweight/annotated tag rejects on reachable legacy paths. Tests compare exact release bytes plus sorted asset names/bytes before/after and assert zero POST/PATCH/DELETE/upload. PASS. |
| Retained dry-run/assets-only | resp-259 | workflow_tests.rs957 `legacy_dry_run_resolves_without_scheduling_the_publisher`; recognizer925–954; release.yml402–408 | Actual resolver emits should-publish=true/dry-run=true/publish_assets=false. Actual YAML gate is checked by bounded four-conjunct recognizer; all clauses validated before evaluating false; missing/duplicate/bypass/unsupported-after-false/None fail. Publisher body not called. PASS for source-only gating, **not hosted scheduling**. |
| Retained automatic/resume/publication/recovery behavior | resp-259; resp-276 unchanged | existing workflow tests317–724 and actual publisher453–545 | All 23 workflow cases pass, retaining main advance/removed history/exact run/source/idempotence, draft→upload→readback→publish, immutable verification-only/missing/mismatch, mutable matching no-op/partial resume, lost create/publish responses, existing-tag assets-only, CI-run dry-run and proposal behavior. No actual public publication or registry check performed. |

These state/call-order outcomes are **assertion-derived from executed tests**, not separate durable API transcripts or state dumps. Fixture `Drop` removes per-case temporary files. Successful-case raw API logs/asset digests were not retained; no transcript was invented. YAML bodies were independently extracted, matched Production's bytes exactly, and passed Bash syntax. Resolver body SHA256 `c9833bda1387a3bbebd1661e959207f433740214ee418d5b77985fd83f726daa`; publisher `7d7877f9427283df1d05d98ad543df76d3f255a02c138a575d5299649f71bbd6`.

## Baseline and preserved failures

Independently read/audited, **not rerun**: Backend's two baseline scratch tests against exact99a339 release.yml fail with `unexpected authorization` and `unexpected upload`. Raw named failure logs are preserved under `author-evidence-audit/`; author handoff reports exit101 for each. The scratch YAML hash `aca94a20d46b2ecca85a9c112bb21daa7df1fb418e84f44d28bd35037c706bf3` matches immutable HEAD YAML exactly. The scratch harness predates **only the final dry-run recognizer/mutation assertion changes**; resolver/publisher race oracles are unchanged. Exact harness diff is `baseline-harness-to-final.diff`. The scratch has a reduced main/workspace, so this supports a focused baseline defect, not a full comparable baseline suite or independent re-execution.

Preserved author history includes initial formatting diff and the failing dry-run negative-mutation assertion before final correction, plus earlier/final pass logs. `baseline-locked-setup.log` is **prose-only**: it records a scratch lockfile/setup rejection and doubled module-path correction; original failed setup stdout/exit/timings were not retained. They cannot be reconstructed as measured evidence. This verifier's initial worktree AGENTS read was unavailable (file absent); canonical current AGENTS was read instead. No independent executed test/build failed. No blind rerun or synthetic XML/case/duration was produced.

## Binding, limitations and next owner

- Graph semantic search and explicit change-impact context preceded source reads; indexed main.rs literal/module hops discovered actual release/workflow/tests. Graph/Scryer registration is the older canonical root (Engineer identifies dirty6aa5), **not this exact worktree**; navigation only, not old-root behavior proof. No change bind, attachment, ingestion, fold, drift or other canonical mutation occurred. Genuine JUnit is retained for later legitimate exact-source binding; that gate remains pending.
- Linear connection is pending/unavailable. No synchronization or settings changes claimed. Wiki/memory read only. Historical four-target wording is not a current supported-platform result.
- Unix-only fixtures pass locally. No Windows pass, product native/cohort/full-workspace Clippy/archive build, hosted event/job scheduling, real publisher, CI rerun or registry/publication verification is claimed or needed for this bounded YAML/test-only local slice.
- Fresh independent Final Reviewer/focused confirmed-fix review remains with Engineer. Production may persist this evidence unchanged with the related PR under existing authority. First **actual COD106→COD105 non-main-base full CI/stable-required/C103-A05 proof** is due after the reviewed PR opens; missing/stale/failing evidence blocks full COD105 qualification, COD106→COD56, main integration, Done and release. Preserve order105→106→56→107→108→110 and all closure ownership.
- Assumption: reused exact approval and Engineer-confirmed predecessor acceptance/activation; no new authority inferred. **Project constants changed by verifier: none.** No new reusable-memory write warranted; existing recalled ancestry/immutable-release knowledge already covers this procedure, and memory writes were excluded.

**Next owner:** Engineer for exact-snapshot Final Reviewer and gate decisions; authorized Production for unchanged evidence persistence/PR; later Verifier for actual hosted proof and legitimate source-bound reconciliation. Cargo exclusive ownership is released now; no additional checks will run.
