# COD-108 — local qualification and integrated review

This evidence accompanies the same COD-108 implementation PR. It records local implementation checks and the accepted integrated review, not full A04/A05 hosted qualification, issue completion, main integration, release or aggregate closure.

## Authority and exact reviewed source

Canonical five contracts remain approved at full Git SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8`, including Design§8.1–8.4 and C103-R04/R05→A04/A05. The separate approval record is [`docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md` at `0e6008a1f46679a86aba85ac513beb05af94b0c0`](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md), actual user confirmation “i already approved revision 0.3”, decision `c8690556-e836-428d-bc81-079c701d2395`. Later COD-56/COD-107 method supplements do not advance COD-108 approval. No canonical contracts or approval records are rewritten here.

Baseline HEAD is `90af76d3453a6f5cc0f032045ed48c4fd09c275c`, tree `cd05bfb5ebdb9652dfa8b6af90ce867c733667bb`. The exact four reviewed source SHA256 values are:

| Path | SHA256 |
| --- | --- |
| `crates/xtask/src/ci.rs` | `b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f` |
| `crates/xtask/src/main.rs` | `94e36cb3ceed27f45b552e42c9d6a52aa7cfef8dd70876f6e6ab4e562ec8891d` |
| `.github/workflows/ci.yml` | `b59d7028101120f6885870cec054a65d9788f55d364dc0f2eb2b53bd4fd9c339` |
| `docs/release.md` | `90d84f6614be6cd74b8340dd73b1a2c1000457af319475cf8939b1c316ebf486` |

All2014 other baseline tracked blob/mode pairs and the17 README.md package-selection paths remain unchanged. No workflow_tests.rs, manifests/lock, native/release workflows or native recipe changed.

## Executed local evidence and review

- [Independent Verifier report](verifier/report.md), [result](verifier/result.json), original [files manifest](verifier/files.json), [commands](verifier/commands.json) and [genuine JUnit](verifier/xtask-junit.xml):102 tests, no failures/errors/skips/retries; formatting/full strict workspace Clippy and actual policy checks passed with zero compiler warnings. JUnit SHA256 is `279e267e` prefix; the original manifest carries the full digest. Actual26 CLI cases and12 required-body supplements are separate executed counts, not added to the102 Rust tests. Runtime, compiler/binary provenance, raw streams, fixture definitions and oracle/source guard are retained in the selected evidence.
- [Fresh integrated Final Reviewer report](reviewer/report.md), [result](reviewer/result.json), original [files manifest](reviewer/files.json): PASS, no confirmed findings or open questions for those exact four source hashes. Requested settings were openai-codex/gpt-6.1-sol/MAX, default fresh non-forked native session; original settings/limitations are preserved.
- [Backend core report](backend-core/report.md), [result](backend-core/result.json), original [files manifest](backend-core/files.json): transcript-derived initial core evidence, not standalone raw logs/JUnit or independent verification. The original manifest identifies its exact transcript prefix; no logs are reconstructed here.
- [Backend final workflow report](backend-workflow/report.md), [result](backend-workflow/result.json), original [files manifest](backend-workflow/files.json): faithful report/result plus61 raw logs. Earlier failures/debug attempts remain historical, not final passes.
- [Production consumer report](production-consumer/report.md), [result](production-consumer/result.json), original [manifest](production-consumer/manifest.json): syntax/static checks only, not executable fixture or hosted evidence. Actual subsequent independent evidence above governs the integrated result.

## Selection and preservation

[copy-selection-map.json](copy-selection-map.json) lists every copied author's original path, relative destination, mode, byte count and SHA256, plus explicit exclusions. Original report/result/manifest bytes, whitespace, absolute private references and empty raw streams remain unchanged. The author manifests describe their full private closures, not a promise that every private file is copied here. Raw Git object stores/binary tree encodings, binaries, duplicated source/spec payloads, private Scryer/model/health/plan dumps and unrelated API context are excluded. Text fixture scripts are stored non-executable as evidence. No secret was redacted by silently rewriting an author artifact.

## Significant constants and pending gates

The core uses schema1 output and conservative2 MiB Git diff/blob read limits. Admission retains approved bot ID41898282 / github-actions[bot] / Bot, exact release branch/label and current same-repository B/H/main identity. The only new literal staging base is cod103-stack/cod-108; labeled/unlabeled activities are approved. COD-107 retention stays narrow, native retention90 days and diagnostics3 days unchanged. No dependency/profile/native recipe or account/settings/credential policy changed.

The two existing COD-108 claims `resp-1rnbs9` and `resp-tddbkj` under `chg-9xdkd4` remain PENDING, not folded. Correct-source model binding/reconciliation is separate; no evidence is ingested against the old canonical944 source. Execution ownership remains in Scryer (`chg-cw9n3h` and the current carrier appendix), not a second plan here.

Hosted Actions/full bootstrap PR checks, current-head/base qualification, actual Windows/hosted execution and actual trusted lightweight scheduling remain pending at their proper stages. COD-108's own non-main-base PR must run FULL. Lightweight rollout evidence requires protected-main deployment and its successful own full main CI, before the final release-PR merge. No PR artifact promotion or head-helper admission is authorized. All applicable CI must be green for the exact reviewed head/current base before any merge. Main integrations remain serial under non-bypass strict freshness; no merge/main retarget/release/COD-110/Done authority follows from these files. Existing aggregate closure owners are unchanged.
