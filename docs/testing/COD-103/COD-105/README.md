# COD-105 — executed local evidence and reviewed source

## Identity and exact source

- Work: [COD-105](https://linear.app/codebasegraph/issue/COD-105), parent [COD-103](https://linear.app/codebasegraph/issue/COD-103); brief `chg-ahwwmf`, existing claims `resp-16`/`resp-17`/`resp-256`/`resp-15`.
- Approved [spec revision0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/7e79f7e924536f29d3212e83899b01c38afd39de/docs/specs/COD-103/spec.md), [exact approval](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md); approved contract SHA is not advanced by this evidence.
- Tested/reviewed snapshot: initial HEAD `c8b8e7395c0a83bb8a2fe53a5add97df0d9cf617` plus exactly the three hashes below; product baseline `5889ed09954a4a07efd2590dd1bd380ee77be06a`. Technical commit **`d24245c0e364d55964473a91edb078b46a3c0a07`** contains identical Git blob bytes, independently checked by Production before/after commit. Tests did **not** run at a later evidence-only commit; same-byte proof binds the reviewed implementation honestly.

| File | Reviewed/tested/committed SHA-256 |
|---|---|
| `crates/xtask/src/main.rs` | `652dbeb10cfbeabef04b61fa2429356be7155f70be90d05822ff0d278df985d4` |
| `.github/workflows/ci.yml` | `98f659e98ad2060f503785390bc810b453a3eb23518b372e85122d2090f1fcda` |
| `docs/release.md` | `88efe80f119558fdc3e913211c4ebdde11a131a9d5b349248cb1b5a4b8e0c5a3` |

[Committed byte proof and artifact hashes](committed-byte-proof.json) binds the exact technical commit, reports and raw files. Only the three technical files changed; no manifest/dependency/native pipeline/release workflow change.

## Actual results and remaining gates

| Criterion | Method / actual result | Scope limit |
|---|---|---|
| C103-R01/A01 local portion | Genuine focused nextest15/15, full xtask51/51; stdout/status parser and actual YAML-policy regressions; [independent report](independent-evidence.md), [machine record](independent-evidence.json), [JUnit](raw/cod105-focused.xml) | Actual macos-arm64 and windows-x86_64 packaged MCP builds/smoke still pending hosted CI; unit fixtures are not artifact qualification. |
| C103-R05/A05 first-PR foundation | Exact literal bases/activities and no privileged event, full-main-only push policy; executed named tests and `check-workflows` passed | Fresh full first main-base PR CI/stable required still pending. First actual COD-106 non-main-base proof remains deferred, not waived. |
| Local checks | Formatting, strict xtask Clippy (no warnings), ordinary release-gate passed; full commands/times in [command records](raw/command-records.json) | Not full workspace lint, production confirmations, supported Windows desktop qualification or main-push CI. |
| Independent final local review | [Final review](final-review.md): no confirmed P0–P3; exact three-file source and genuine results inspected | Does not establish staging-only handoff, full qualification, Done, merge or release. Post-hosted evidence/focused review still required. |
| Scryer evidence reconciliation | [Binding finding](scryer-evidence-binding.md) preserved | Connected model/fingerprint uses old root; no official worktree override. No copied/relinked model, source copied to old root, old-root attachments/ingestion or fabricated verdict. Applicable Scryer gate remains pending. |

## Sessions and historical corrections

- Fresh COD-105 Backend: `sub-1ab0ef16`, `implement-cod105`, actual Engineer-confirmed `openai-codex/gpt-6-luna/xhigh`. No reuse from another issue. [Implementation handoff](implementation-handoff.md) is preserved unchanged.
- Independent Verifier: `01a186e2-7e46-7149-81c0-32c3feb8d5b3`, `openai-codex/gpt-6.1-sol/max`, independently exposed persisted settings.
- Fresh Final Reviewer: `sub-59591f89`, session `01a18718-9487-7152-8fa3-259b68adcfe0`, independently exposed Sol/max.
- Correct count is **7 actual new named tests**, not the implementer handoff's8. The grouped MCP negative test contains8 assertion subcases, not8 named tests. Raw author reports remain historical evidence; this index and independent evidence explain the count rather than silently altering them.
- Both initial and repeat raw logs/XML are retained. Initial wrapper durations were lost across kernel restoration and are explicitly unrecorded; repeated small runs used the same unchanged hashes with durable command records. No invented timing or JUnit. Final XML SHA-256 `580f693dafbc418d493990064b0b29da9e59564cc0168a580b2b1e61395f856b` has15 passed named tests.

## Artifact integrity and limits

All raw files under [raw/](raw/) are small actual logs/XML/command records/config/diff/inventory, copied byte-for-byte; no build outputs or `.py` source added. Historical reports may retain declared absolute execution paths; canonical links here point to actual durable files. A bounded token/private-key pattern check found no secret matches; no redaction was needed. Raw reports are not edited to pretend later gates passed.

Production committed reviewed bytes and durable evidence only. Non-bypass merge capability, Windows11/PowerShell5.1+supported7 qualification, full first-PR hosted/native checks, deferred first-child proof and complete-queue release gates remain pending as applicable. No merge, auto-merge, release, aggregate closure or COD-106 work is authorized by this evidence.
