# COD-106 — selected independent local evidence

**Independent local verification and Final Reviewer review accepted by Engineer for local commit and stacked PR opening.** This is not full readiness, PR acceptance, C103-A05 qualification, Done, main integration or release. These documents belong in the **same future COD-106 stacked PR**, not a parallel PR. Production copied the Verifier's selected artifacts without changing their substance or executing more checks.

## Authority and snapshot

- [Approved COD-103 spec revision 0.3](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/spec.md) and companion contracts at full SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8`.
- [Separate exact approval](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md), user confirmation “i already approved revision 0.3”. Scope C103-R02/A02; existing `resp-259`, retained `resp-276` and R05/R06 gates unchanged.
- [COD-106](https://linear.app/codebasegraph/issue/COD-106), existing Scryer `chg-9bqhd2`, project `/Users/rabii/Projects/Repositories/codebaseGraph`. Detailed execution remains canonical in `container-release-verifier`, section **COD-106 EXECUTION — current-source ownership reconciliation (2026-10-31)**; whole-description SHA256 `2c1a58fb8882594137c391a846ab4194dc11daa8203b6b10d23d96a15e75f486`, section SHA256 `0d4f49be1db82eaddb8612b9015f2cee64717ed2b07c2ae9ac7f79efe1134be8`. No detailed brief is duplicated here.

Verified local source is **HEAD `99a339c54df79cef0598905cde8c6ce4e5d62163` plus two dirty files**, not that commit alone. HEAD tree `8bc68d5e5018ca3b64f0a3ad4240431b9e5ac902` identifies the committed baseline, not the dirty tree:

| Dirty source | SHA256 |
| --- | --- |
| `.github/workflows/release.yml` | `e11c179705340bea6230dfac1a50c055d34fb7137d20f2c271e335c589653117` |
| `crates/xtask/src/release/workflow_tests.rs` | `66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450` |

Complete dirty diff SHA256: `c7b76fcb32d0943b426d1895194c1739d6e30a7bda1cd5f563996bb5d905996c`. The Verifier's before/after source inputs, binary identity and raw diff are retained. Evidence persistence did not change either source file.

Technical commit `5a556cb2ff06e7b7f8d42ca9762d52863947a734` (parent `99a339c54df79cef0598905cde8c6ce4e5d62163`, tree `6046a6ebce821b94a197a9b61ab9caea0435d2c3`) contains exactly those two tested/reviewed file bytes. [committed-byte-proof.json](committed-byte-proof.json) maps the tested HEAD-plus-dirty-file identity to actual committed blobs. This byte-equality check is **not new test execution at the commit**. The later evidence commit changes documentation only. Hosted non-main-base C103-A05 and exact-source Scryer/full-completion gates remain pending.

## Selected copies and provenance

Read [the unchanged independent final report](verification/final-report.md), [result](verification/result.json), [commands](verification/commands.json), [genuine JUnit](verification/xtask-junit.xml), and [case inventory](verification/case-inventory.json). [production-copy-manifest.json](production-copy-manifest.json) records **all 56 selected source-to-destination mappings, sizes and SHA256 values**. The copies contain **262,107 bytes**, all small UTF-8 text; none is 2 MiB or larger. Relative paths are preserved under `verification/`.

Original artifacts remain outside Git at `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-bee0f574/cod106-independent-verification`. The exact external selection manifest is `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-11eee5d7/cod106-evidence-selected-files.json`, SHA256 `cb250e0c73897037861d1d0d8d333555531804d8b9c124b3b42426471b7ad799`.

The original report, result, raw manifest, JUnit and failure logs are unchanged. [raw-manifest.json](verification/raw-manifest.json) can reference original external paths and unselected artifacts; those references were **not rewritten** and do not imply every referenced artifact is versioned. Selected native-tests dependency information is text, not a binary. No memory/wiki/model/discovery dumps, approved-spec copies, general help, baseline scratch sources, binaries or build targets were copied. The [unchanged Final Reviewer report](final-review.md) is included with SHA256 `1c92e22aeafe624b73a9d652d50c823d985ee159a2bbd25c6856d829386c0def`; it reports no confirmed P0–P3 defects. Host-accepted settings supplied by Engineer are Sol/max; independent runtime metadata is unexposed. Its disclosed graph-read-order procedural deviation is preserved, not removed.

## Results and limits

- The independent local run reports **76/76 xtask tests**, including **23/23 workflow tests and 5/5 new tests**. The latter are subsets, **not additive counts**. Grouped loop subcases are not extra JUnit cases. JUnit SHA256: `f7482e2d985ae6b794194000d9dbe4ac56f57efbaa5de442786caa87b396388f`.
- Exact commands, environment and outputs are preserved. Scoped xtask strict Clippy passed with zero observed compiler warnings in the executed outputs. This is **not full-workspace/native or Windows qualification**, hosted scheduling/publication, C103-A05 proof or a Scryer pass.
- Two observed author baseline failures (`unexpected authorization`, `unexpected upload`) were independently **audited, not re-executed**. Their original raw logs, initial failed formatting diff and failing negative-mutation assertion are retained byte-for-byte under `verification/author-evidence-audit/`. The scratch setup log is prose only: original failed setup stdout, exit and timing evidence was unavailable, not reconstructed.
- Fixture `Drop` removes per-case temporary state/logs. API order and preservation results are **assertion-derived from executed cases**, not separately durable API transcripts or successful-case state dumps. No transcript was invented.
- Graph/Scryer registration remains at the older root. No JUnit ingestion, claim attachment/fold, source-binding reconciliation or tracking write is represented by these copies. Genuine exact-source reconciliation remains pending.
- Actual first COD106→COD105 non-main-base full CI/stable-required proof remains due after the reviewed stacked PR opens. Missing/stale/failing proof blocks full COD105 qualification, COD106→COD56, any main integration, Done and release. Review, platform and aggregate closure ownership remain unchanged.

Production verified selected sizes/hashes and ran a bounded credential-pattern scan over these new files: **no findings**. This is a bounded scan, not a universal secret-absence guarantee. No credentials/settings were changed. No tests, Cargo, source edits, Git staging/commit/push, PR/CI actions or publication were performed for this copy task. No project constants changed; no new assumptions beyond the assigned exact selection and existing source identity were introduced.
