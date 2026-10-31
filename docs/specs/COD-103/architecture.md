# COD-103 — architecture rationale and Scryer references

**Status:** proposed revision 0.3 amendment, 2026-10-30. Existing revision0.2 remains approved at full SHA `7e79f7e924536f29d3212e83899b01c38afd39de`; [separate approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md). COD-110 amendment preparation is authorized by [the 2026-10-30 decision](https://linear.app/codebasegraph/issue/COD-103#comment-1f26d6dd-6044-4349-bb02-e44779e4c8cc); exact amended-SHA implementation approval remains pending. No new sign-off, implementation or verification is claimed here.

Governing scope is [spec.md](spec.md), existing approved C103-R01–R06 / A01–A06 plus proposed C103-R07/R08 / A07/A08, and unchanged [CBG-PLAT-01 revision 0.11 S05/R05/A09](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer). Concrete software contracts are in [design.md](design.md). This is an architecture rationale/reference, not a duplicate execution brief or platform specification.

## Unchanged architecture

Retain Release Verifier (`container-release-verifier`) and Release Gate (`node-1`). No new nodes, containers, links, runtimes or data stores are proposed. Existing packaged smoke, legacy ancestry and installer corrections restore existing claims. The selected docs-only source-policy change remains at `node-4/resp-14`. Static workflow validation stays at `node-5/resp-15`; it does not acquire GitHub orchestration ownership.

Retain packaged command/MCP claims `resp-16`/`resp-17`, native selection/artifact/provenance claims `resp-254`–`resp-258`, exact-source publication `resp-259`, and ordinary-main proposal maintenance `resp-276`. Supported native targets remain only `macos-arm64` and `windows-x86_64`; stale four-target skill/wiki wording does not authorize expansion.

## COD-110 amendment: verification placement, unchanged intent

Proposed C103-R07/A07 retains COD-108's one mandatory secret-free repository-policy lane and shares candidate metadata validation between the actual tested release-PR checkout and final selected main source. This refines existing policy/metadata accountability under `resp-9`, `resp-10`, `resp-12`, `resp-15`, `resp-trgvnp` and `resp-ew32yv`. `resp-18` remains actual tag/package consistency. Ordinary non-release PRs retain existing repository-policy/release-sync obligations without invented future versions, tags, notes or candidate-only transitions; lightweight history immutability remains distinct from actual-version section validation. Candidate correctness grants neither lightweight eligibility nor publication authority; protected-base admission, full fallback, bootstrap and final head/base freshness remain unchanged.

Proposed C103-R08/A08 moves the final same-immutable-source crate dry-run and exact compressed-size check before tag creation, public Release creation/publication or native upload. This refines verification ordering under existing `resp-9`, `resp-10`, `resp-15` and `resp-259`, not publication ownership. Reuse the successful source/version-bound verification result, not necessarily identical preflight `.crate` bytes: normal Cargo repackaging of unchanged source/version remains allowed, while native archive build-once/promote identity stays exact. An authoritatively visible exact registry version retains success/no-upload recovery; compatible nonmutating source-package verification and size validation still precede any pending public mutation. Preserve native publication before actual crate **upload**, including the current draft → asset upload → complete size/byte readback → publication flow. Preserve matching immutable-release verification-only recovery, registry-aware retries, partial-outcome reconciliation and finalization. Existing-tag recovery stays native-assets-only.

No COD-110 node, link, responsibility or directive delta is proposed. The changed premerge/public-mutation failure contracts require exact amended-revision approval even though existing architectural accountabilities cover them. Retain final own-release-merge full main CI/run/attempt, current main ancestry, selected metadata/tag/version, exact-run two-target artifacts/provenance and current authorization/conflict barriers. Do not substitute PR artifacts or another source/run. No conda distribution or new privilege is selected.

Amendment preparation ledger `chg-pxt0x2` is separate from existing COD-108 intent `chg-9xdkd4`; keep `resp-1rnbs9` / `resp-tddbkj`, their original approval and later implementation/evidence/fold gates unchanged. Detailed COD-110 design/execution documentation uses the Engineer-designated existing `node-1` carrier with serialized ownership; no topology is created merely to hold a brief. Architect made no model/ledger/directive write. Appending COD-110 after COD-108 and selecting already-merged COD-54 contents does not close platform/parent work or authorize unknown later main changes, a partial release or weakened gates.

## Existing COD-108 changed intent in Scryer

Existing change **`chg-9xdkd4`**, on component **`node-1`**, contains exactly these two pending responsibilities for COD-108 / C103-R04/R05 / A04/A05, covered by the preserved revision0.2 approval:

- **`resp-1rnbs9`** — **When** a release pull request meets approved metadata-only eligibility, **permit** its lightweight verification.
- **`resp-tddbkj`** — **If** a change is not positively eligible for lightweight pull-request verification, **require** full verification.

Production read back the model and pending change on 2026-10-29: both server-assigned IDs are additions under `chg-9xdkd4` at `node-1`; existing `resp-10`/`resp-11`, `resp-15` and descendant claims are retained. These remain pending model intent, not implemented evidence. The separate revision0.2 approval records the user decision; Scryer sign-off, implementation, verification and fold are distinct gates and are not claimed completed here. Preparation change `chg-fdm24r` is separate. Do not alter unrelated pending changes/drift.

## Boundaries and acceptance trace

Lightweight admission is verification privilege, never publication authority. Protected-base helper provenance, positive semantic eligibility, explicit full fallback and final base/head freshness remain governed by the spec/design. Classification uncertainty may yield valid full mode; unknown/mismatched final base/head fails the aggregate in both modes. PR-head policy/tests cannot authorize lightweight admission before the protected-base helper exists; COD-108's own PR stays full. Real PR165/167 positive-fixture execution remains planned, not evidence.

All main pushes remain full. Literal bounded stack-base PR support falls under existing workflow policy; main push/release triggers and exact-source publication ownership stay unchanged. Non-bypass strict-freshness merge capability and Windows11 desktop PowerShell5.1/supported7 execution are delivery gates, not reasons to change topology or reopen retained bypass/platform decisions.

| Acceptance | Applicable Scryer references |
|---|---|
| C103-A01 | `resp-16`, `resp-17`, `resp-256` |
| C103-A02 | `resp-259` |
| Existing platform A09 | `resp-16`, `resp-256` |
| C103-A03 | `resp-14`, applicable `resp-15` |
| C103-A04/A05 | existing pending `resp-1rnbs9`, `resp-tddbkj`; existing `resp-15`, `resp-254`–`resp-256` |
| C103-A06 | `resp-259`, `resp-276`, exact-source publication gates |
| Proposed C103-A07 / R07 | Existing `resp-9`, `resp-10`, `resp-12`, `resp-15`, `resp-trgvnp`, `resp-ew32yv`; COD-108 admission/fallback remains separate. |
| Proposed C103-A08 / R08 | Existing `resp-9`, `resp-10`, `resp-15`, `resp-259`; retained `resp-254`–`resp-258` / `resp-276` govern unchanged artifact/proposal paths. |

Planner's detailed ownership, prerequisites, contracts and execution briefs belong only in Scryer: COD-105 `chg-ahwwmf`, COD-106 `chg-9bqhd2`, COD-56 `chg-2z3ngs`, COD-107 `chg-xd2jtt`, COD-108 `chg-cw9n3h`; see the compact [delivery index](delivery.md). No implementation anchors, attached tests, executed evidence or verdicts are claimed here. Existing revision0.2 approval remains intact; aligned revision0.3 freeze and exact amended-SHA approval remain pending before dependent COD-110 implementation. No optional Proposal Reviewer gate is added.

## Provenance

Condensed without changing architectural intent from Architect's source proposal artifact, SHA-256 `2b05d8ccf3c3d18a451cd8dbd8f57d190c3f0ea10d77fe81042c472eb96c3d2f`; profile `openai-codex/gpt-6.1-sol/xhigh`. Architect source base: `5889ed09954a4a07efd2590dd1bd380ee77be06a`. Requirements input SHA-256 `7695dad95775d674e04a4151bc90f5034c30e10dae4e2aff4962073139e5d6ac`; final Design input SHA-256 `5fa6cbd4fc226de259a83444b87e1fb117582aaa67193670276ed9c45688cf12`. These are artifact provenance, not approval or this file's containing commit SHA. No tests/builds were executed for this proposal.

Revision0.3 intent-impact assessment reread Requirements-authored C103-R07/R08/A07/A08, inspected spec SHA-256 `35e154564f476a523adf22adf205e52c939563acc134c7c6051f17d18ef1d9c3`; this is assessment-input provenance, not approval. Current Scryer rules and explicit-project plan/committed Release Verifier reads support the no-delta result. Live COD-108/COD-110 and the preparation decision were read. Graph semantic search and explicit change-impact/dependency context supplied discovery only: the registered root is old source, not current implementation proof. After Production marked the isolated proposal workspace ready at PR168 head `e06d7404955abeb7a002b54ad740399dfe10c931`, Architect read graph-discovered current `main.rs` and `docs/release.md`; the latter records the newer immutable draft/assets/readback flow. Direct graph matches for newer release orchestration files were unavailable, so this assessment does not claim to have verified their current implementation. No tests, builds, hosted CI or publication checks ran. Current-source implementation discovery, actual workflow-body ordering/failure tests and hosted/native/publication evidence remain with their assigned owners. Architect changed this rationale only; no Scryer/Git/Linear/settings/source writes or constants changed.
