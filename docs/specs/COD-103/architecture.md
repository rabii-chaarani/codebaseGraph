# COD-103 — architecture rationale and Scryer references

**Status:** proposed, revision 0.2; user approval, Scryer sign-off and implementation remain pending. Date: 2026-10-29.

Governing scope is [spec.md](spec.md), C103-R01–R06 / A01–A06, and unchanged [CBG-PLAT-01 revision 0.11 S05/R05/A09](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer). Concrete software contracts are in [design.md](design.md). This is an architecture rationale/reference, not a duplicate execution brief or platform specification.

## Unchanged architecture

Retain Release Verifier (`container-release-verifier`) and Release Gate (`node-1`). No new nodes, containers, links, runtimes or data stores are proposed. Existing packaged smoke, legacy ancestry and installer corrections restore existing claims. The selected docs-only source-policy change remains at `node-4/resp-14`. Static workflow validation stays at `node-5/resp-15`; it does not acquire GitHub orchestration ownership.

Retain packaged command/MCP claims `resp-16`/`resp-17`, native selection/artifact/provenance claims `resp-254`–`resp-258`, exact-source publication `resp-259`, and ordinary-main proposal maintenance `resp-276`. Supported native targets remain only `macos-arm64` and `windows-x86_64`; stale four-target skill/wiki wording does not authorize expansion.

## Draft changed intent in Scryer

Dedicated proposal-only change **`chg-9xdkd4`**, on existing component **`node-1`**, contains exactly these two new draft responsibilities for COD-108 / C103-R04/R05 / A04/A05:

- **`resp-1rnbs9`** — **When** a release pull request meets approved metadata-only eligibility, **permit** its lightweight verification.
- **`resp-tddbkj`** — **If** a change is not positively eligible for lightweight pull-request verification, **require** full verification.

Production read back the model and pending change on 2026-10-29: both server-assigned IDs are additions under `chg-9xdkd4` at `node-1`; existing `resp-10`/`resp-11`, `resp-15` and descendant claims are retained. These are planned intent, not approval or implementation. Draft claims precede combined review/user approval; record sign-off only after the actual exact-scope approval. Preparation change `chg-fdm24r` is separate. Do not alter unrelated pending changes/drift.

## Boundaries and acceptance trace

Lightweight admission is verification privilege, never publication authority. Protected-base helper provenance, positive semantic eligibility, explicit full fallback and final base/head freshness remain governed by the spec/design. Classification uncertainty may yield valid full mode; unknown/mismatched final base/head fails the aggregate in both modes. PR-head policy/tests cannot authorize lightweight admission before the protected-base helper exists; COD-108's own PR stays full. Real PR165/167 positive-fixture execution remains planned, not evidence.

All main pushes remain full. Literal bounded stack-base PR support falls under existing workflow policy; main push/release triggers and exact-source publication ownership stay unchanged. Non-bypass strict-freshness merge capability and Windows11 desktop PowerShell5.1/supported7 execution are delivery gates, not reasons to change topology or reopen retained bypass/platform decisions.

| Acceptance | Applicable Scryer references |
|---|---|
| C103-A01 | `resp-16`, `resp-17`, `resp-256` |
| C103-A02 | `resp-259` |
| Existing platform A09 | `resp-16`, `resp-256` |
| C103-A03 | `resp-14`, applicable `resp-15` |
| C103-A04/A05 | draft `resp-1rnbs9`, `resp-tddbkj`; existing `resp-15`, `resp-254`–`resp-256` |
| C103-A06 | `resp-259`, `resp-276`, exact-source publication gates |

Planner's detailed ownership, prerequisites, contracts and execution briefs belong only in Scryer: COD-105 `chg-ahwwmf`, COD-106 `chg-9bqhd2`, COD-56 `chg-2z3ngs`, COD-107 `chg-xd2jtt`, COD-108 `chg-cw9n3h`; see the compact [delivery index](delivery.md). No implementation anchors, attached tests, executed evidence or verdicts are claimed here. Combined immutable proposal review and user approval remain pending.

## Provenance

Condensed without changing architectural intent from Architect's source proposal artifact, SHA-256 `2b05d8ccf3c3d18a451cd8dbd8f57d190c3f0ea10d77fe81042c472eb96c3d2f`; profile `openai-codex/gpt-6.1-sol/xhigh`. Architect source base: `5889ed09954a4a07efd2590dd1bd380ee77be06a`. Requirements input SHA-256 `7695dad95775d674e04a4151bc90f5034c30e10dae4e2aff4962073139e5d6ac`; final Design input SHA-256 `5fa6cbd4fc226de259a83444b87e1fb117582aaa67193670276ed9c45688cf12`. These are artifact provenance, not approval or this file's containing commit SHA. No tests/builds were executed for this proposal.
