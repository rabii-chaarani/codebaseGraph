# COD-54 — macOS bundled OpenSSL implementation and verification

## Current qualification amendment — 2026-10-27

The user explicitly said **“well then just drop it and keep only current”** after the minimum-runner/hosted-CI discussion. [Canonical spec revision0.11](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md) at full SHA`8a410c7a22f5291f06a1cece5cd9e7c0be255e72` and [exact amendment approval](https://github.com/rabii-chaarani/codebaseGraph/blob/5c2e9a56d0fa87779fb31312562f4e53f902f0ac/docs/specs/CBG-PLAT-01/approvals/d05a-current-macos-only.md) replace D05a with **current stable macOS on Apple Silicon only**. There is no13.3 execution/support-qualification requirement. Actual current-release final-archive runtime/failure evidence remains required; hosted CI may supply it when the actual OS/build and no-external-Homebrew/OpenSSL runtime conditions are recorded. A moving runner label is not execution proof.

The existing13.3 deployment target and the reviewed/tested product source`855c7e282a27de4afbad2d4252d7692de4b42ecb` remain unchanged. This is a qualification-policy/documentation change, not a new binary or a passing CI claim. Earlier approval/review/evidence files retain their original scope and are historical where they name the replaced minimum gate. D02, Windows, full native/hosted checks and maintainer aggregate closure are unchanged. Scryer amendment`chg-p0s0ar` records the impact; no architecture claim change.

## Identity and authority

- Work: **CBG-PLAT-01/S03 / COD-54**, R03 / A05–A06 only; [Linear](https://linear.app/codebasegraph/issue/COD-54), [draft PR163](https://github.com/rabii-chaarani/codebaseGraph/pull/163).
- Approved canonical spec: [`docs/specs/CBG-PLAT-01/spec.md`, rev0.10 at `1a9db7b558b60248888408daa6268a3b45f2f0df`](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md). [Exact user approval](../approvals/cod54-macos-openssl.md), committed at `71dc27e92857c4215bbbe583f104de7f341dc816`.
- Baseline main `34eac4f8644adc046093babd84ee1fdbffdae853`; implementation `a2005dbb37a9ab914ea97b2735825b5286af5b6e`; strict-lint-only follow-up **`855c7e282a27de4afbad2d4252d7692de4b42ecb`** is the final product-source candidate.
- Scryer `chg-h7rgh8` / `chg-metn5v`; existing Release Gate resp-16/17/255–258 and Graph Store resp-113. No new claims/topology. Detailed execution brief/file ownership stays in Scryer. Ancillary cfg-only baseline warning hygiene: `chg-8zcgg3`.
- Status: implementation delivered for review; final source-bound archive verification, hosted CI, clean-consumer qualification and maintainer closure are distinct. No package publication, merge, deployment, service restart or aggregate closure is authorized.

## Implementation and constants

The artifact-only macOS path downloads official OpenSSL3.6.2 source, verifies SHA-256 `aaf51a1fe064384f811daeaeb4ec4dce7340ec8bd893027eee676af31e83a04f` before extraction, and builds ARM64 shared libraries with target13.3. This matches the observed producer version; it does not change Cargo dependencies or the Ladybug/extension ABI. The producer Homebrew libraries required macOS26 and were not shipped. Source/license provenance and same-version source-build behavior were independently reviewed.

Both binaries link from the controlled library directory. Final binary dependencies use executable-relative version-qualified dylibs; SSL resolves crypto loader-relative. Packaging audits imports, IDs, no rpaths, ARM64 and minimum-OS metadata, then renews and strictly verifies ad-hoc signatures before hashing. This is not Developer ID signing/notarization.

The typed target inventory grows from two to **five checksummed macOS payload files** and from five to **eight regular archive entries**: both binaries, SSL, crypto, license, checksums and both installers. Windows remains two/five with provenance schema1. Installer preflight verifies all hashes and both probes before any destination write; real install places runtime/license first and uses per-file temp+rename. No all-file transaction, rollback or old-runtime cleanup is promised.

Smoke installs into a relocated path with spaces. Every macOS child receives a fresh fixture HOME, system-only PATH, and no inherited DYLD/OpenSSL overrides. Graph build/reopen/populated disk-BM25 search uses JSON. A separate legacy-manifest smoke executes actual LOAD fts with official hash-checked cache bytes; its absent native FTS indices intentionally return no results. It does **not** claim a populated native-FTS-index query. Missing/corrupt/incompatible runtime preflight runs in dry and real modes and compares all installed payload hashes.

No changes to bundled extension bytes, Cargo/lock/dependency versions, build.rs, workflows, Windows packaging or process-containment behavior. One `cfg(any(debug_assertions,test))` import attribute removes a pre-existing release-only warning; runtime bodies/constants are unchanged.

## Executed evidence to date

Environment: Apple Silicon ARM64, macOS26.6.2 build25G83; rustc1.96.0/cargo1.96.0. Ladybug0.19 official static macOS asset source SHA `6bee40babdf837bb6aa4d271047dfe5ed3dacf022f16af84952ca4c22d034d62`, static library SHA `d2e9a5c37617b0affb01f18f6ef48bb1531f5744f5ba06c849226842617ad142`.

| Check | Executed outcome | Evidence / limit |
|---|---|---|
| Baseline `cargo test -p xtask --locked` | 43 passed, zero skipped | [baseline](cod54/baseline-xtask.json) |
| Final-source nextest `-p xtask --locked`, serial/no retries | **55 passed, zero skipped** | [JUnit](cod54/final-xtask.junit.xml), [log](cod54/final-xtask.log), source855c7e2; earlier failed attempts retained |
| `cargo fmt --check` | passed | Final source candidate855c7e2 |
| Strict `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | **passed** | [log](cod54/clippy-final.log), [identity](cod54/clippy-final.json) |
| Actual native artifact provisional run | pipeline passed: source build, package/audit/sign/install/runtime positives and preflight negatives | [record](cod54/artifact-attempt-2.json), [inventory](cod54/artifact-attempt-2-inventory.json). Provisional metadata named approval SHA, so **excluded from final source-bound qualification** |
| Exact-source archive at855c7e2 | **full pipeline passed, exit0** | [record](cod54/source-bound-artifact.json), [inventory/provenance](cod54/source-bound-inventory.json), [compressed log](cod54/source-bound-artifact.log.gz). Archive SHA-256 `e5f32abbf8124584d5f6f2df403091c3323d16a1359a70f387fc99eeeb6e4282`; four Mach-O signatures/imports/ARM64/minOS13.3 independently checked by reviewer |
| Full local native suite, all cohorts | **passed, exit0; 1,356 passed / 0 failed / 4 declared ignored** | [summary](cod54/native-full-summary.json), [native execution/inventory report](cod54/native-full-report.json), [compressed full log](cod54/native-full.log.gz); source855c7e2, captured CLI SHA`de3930c3c3daa63b54b6fae59a8e287ce1f929b0e877a6c2bc24814a3157345a`. This is a native report/log, not JUnit; four ignored fixtures are not passing tests |
| Hosted macOS/Windows native CI | **failed [run40474966274](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40474966274)** at docs/policy head1f68596 | [failed-job details](cod54/ci-40474966274-summary.json), [compressed failed log](cod54/ci-40474966274-failed.log.gz). Mac unit quiescence-confirmation assertion and Windows integration numeric-PID assertion failed; both archive jobs skipped. No hosted qualification pass |

Earlier compiler/fixture/PAX failures are retained in linked evidence and are not passing results. The official source PAX global comment is validated and never extracted as a file; actual source entries remain confined. All product-source changes are isolated from the original dirty user checkout.

## Review

- Fresh independent proposal review: **GPT-6.1 Sol / High**, [GREEN revision2 and starting conditions](cod54/proposal-review.md), before implementation.
- Parallel bounded backend lanes: GPT-6 Luna / Extra High, one writer per file; coordinator serialized integration/Git/builds and later took over installer fixture cleanup after stopping the unresponsive worker.
- Fresh final integrated review: **GPT-6.1 Sol / Max**, [final report](cod54/final-review.md), **GREEN for bounded implementation/local native artifact handoff** at855c7e2. No unresolved introduced P0–P3 defect. The reviewer verified the final55/55 suite, strict lint, source-bound artifact pipeline, immutable archive/installed bytes, imports/IDs/minOS/signatures and clean-env probes. Full native/hosted CI and clean minimum/current consumer qualification remain separate pending gates. The transient older-read mutability observation was withdrawn, not left unresolved.

## Completed local native suite and hosted blockers — 2026-10-27

The source855c7e2 local all-cohort native run completed:1,356 passed,0 failed,4 declared ignored, exit0. These are executed source-bound native results, not clean consumer qualification. Four ignored cases remain explicitly listed in the summary; no generated JUnit is invented for the native runner.

Hosted CI40474966274 at docs/policy head1f68596 failed in two existing paths, so both native artifact jobs and required qualification remain blocked. Mac failure: `dc_s5_existing_old_current_managed_store_requires_one_confirmation_then_retires_old_generation` did not observe durable acceptance of the single quiescence confirmation; local same-source run passes, and no root cause is asserted. Windows failure: `bounded_production_chunks_reopen_with_primary_keys_connectors_and_search` asserts unique numeric PID count53against56; this is the separately tracked COD-98 process-identity issue, fixed and qualified in PR157 but not included in this branch's baseline. Both paths are unchanged by COD-54. This is not proof that the entire hosted failure is unrelated; no failure is suppressed or retried into success.

No storage/COPY fixes, cherry-picks, test weakening or new qualification job have been implemented from this read-only triage. Any independent consumer-qualification workflow discussed in chat remains a proposal, not approved/implemented scope. COD-54 stays In Review; actual current-release hosted/runtime qualification and full head-bound CI remain open. The13.3 execution requirement remains removed by the approved current-only amendment.

## Remaining qualification and handoff

**COD-54 is not Done.** Under the approved current-only amendment above, actual final installed archive execution on **current stable macOS on Apple Silicon without reliance on separately installed Homebrew/OpenSSL** remains required. Hosted CI may supply that execution when its actual OS/build and runtime conditions are evidenced. There is no13.3 execution gate. Deployment metadata, a moving runner label and developer-host success alone are not current-release qualification. Full native/hosted checks still need passing execution; the final bounded source/artifact review is GREEN. Aggregate closure stays with the maintainer; do not infer S10/COD-61 closure.

Configured wiki/Scryer/MCP graph are bound to the older original checkout, not the implementation SHA. Target-local graph discovery was completed before source inspection. Existing claims were checked proportionately; no source-map/current verdict is fabricated for the older root and no unrelated global drift reconciliation is performed.

Reusable knowledge assessment: no new memory candidate is warranted for transient pickup/progress. The existing single-artifact-recipe/provenance memory applies; canonical execution evidence is retained here instead of duplicating status into memory.
