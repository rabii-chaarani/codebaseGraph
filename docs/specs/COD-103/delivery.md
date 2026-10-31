# COD-103 — delivery index

**Revision0.4: COD-56 qualification-method supplement only.**

**Current authority (2026-10-31):** revision0.3 and all five contracts are approved at full SHA **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`**, [separate approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md), actual user confirmation **“i already approved revision 0.3”** ([decision](https://linear.app/codebasegraph/issue/COD-103#comment-c8690556-e836-428d-bc81-079c701d2395)). The full six-child queue and selected release contents remain approved. Historical revision0.3 preparation/approval-pending and active-issue statements below are retained provenance, not current gates. COD-56 is the sole active issue; autonomous ordered progression needs no further routine go-ahead.

Revision0.4 changes **only COD-56's qualification environment and test invocation method**, as explicitly selected/approved in the [dated decision](decisions/2026-10-31-cod56-emulated-qualification.md). No requirement/acceptance IDs, product support, other contracts, queue, release contents or closure owners change. Production must commit this reviewable method amendment and persist its separate exact-path/full-SHA approval linkage to those actual user decisions before dependent test implementation; this is recording already-settled approval, not asking for another approval. This document does not contain its own containing commit SHA.

Requirements: [spec.md](spec.md); contracts: [design.md](design.md); intent: [architecture.md](architecture.md); evidence: [verification.md](verification.md). Detailed file ownership, prerequisites, checks and execution briefs remain **only in Scryer**. Amendment preparation: `chg-pxt0x2`, existing `node-1` sections **COD-110 DESIGN** and **COD-110 EXECUTION**. The execution section is proposed, not an implementation approval or new claim. COD-105 remains the sole active implementation issue; unchanged approved work may continue independently. No optional Proposal Reviewer gate is added.

## Fixed ordered queue and canonical briefs

One active implementation issue; one fresh implementation session per child; no other backlog. Production owns Git and integration.

| Order | Issue / acceptance | Existing canonical brief | Literal staged head → initial base |
| --- | --- | --- | --- |
| 1 | COD-105 / C103-A01, first-PR A05 prerequisite | `chg-ahwwmf` | `cod103-stack/cod-105` → `main` |
| 2 | COD-106 / C103-A02 | `chg-9bqhd2` | `cod103-stack/cod-106` → `cod103-stack/cod-105` |
| 3 | COD-56 / external A09 + [method supplement](decisions/2026-10-31-cod56-emulated-qualification.md) | `chg-2z3ngs` | `cod103-stack/cod-56` → `cod103-stack/cod-106` |
| 4 | COD-107 / C103-A03 | `chg-xd2jtt` | `cod103-stack/cod-107` → `cod103-stack/cod-56` |
| 5 | COD-108 / C103-A04/A05, delivery A06 | `chg-cw9n3h` | `cod103-stack/cod-108` → `cod103-stack/cod-107` |
| 6, proposed | COD-110 / C103-A07/A08, retained A04–A06 | `chg-pxt0x2`, `node-1` proposed EXECUTION section | `cod103-stack/cod-110` → `cod103-stack/cod-108` |

COD-108 intended claims remain `chg-9xdkd4`, `node-1`, `resp-1rnbs9` / `resp-tddbkj`; original approval and later sign-off/implementation/evidence/fold remain separate. No COD-110 topology/claim delta is invented. COD-104/admin bypass and COD-99 archive history remain unchanged.

COD-56 retains [CBG-PLAT-01 revision0.11 S05/R05/A09](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer), original milestone, COD-51 exact-slice approval/sign-off linkage, COD-61 blocker and maintainer closure owner. No parallel platform spec or aggregate approval is created.

## Approved stack-staging exception

The existing exception permits the next ordered issue to stage as a real PR on its reviewed/verified predecessor before that predecessor merges. It permits neither simultaneous active issues nor intermediate-branch merges. All staged stack PRs remain full CI. Freshness/retarget proof is due at each actual stage, not before the PR that produces it can open.

### First-child staging clarification (PR-01)

1. **COD-105 staging-only handoff:** implemented foundation, applicable local/real-artifact checks including C103-A01, full first main-base PR CI, independent final review and confirmed-fix verification pass at the exact staging snapshot. Engineer records the still-pending C103-A05 first-child proof. This is not Done/full qualification/integration approval.
2. **First child opens:** COD-106 becomes sole active implementation. Production opens its real PR against `cod103-stack/cod-105` to obtain the deferred non-main-base event.
3. **Actual proof:** Production obtains fresh full-CI/stable-`required` base/head/run/attempt evidence; Evidence Verifier records C103-A05 and Engineer links both `chg-ahwwmf` / `chg-9bqhd2`. A foundation defect pauses child writes, returns to its owner, receives review/fix verification and fresh child qualification.
4. **Barrier:** missing/failed/stale proof blocks full COD-105 qualification, COD-106→COD-56 handoff, every actual main integration, Done and release. No later unborn PR or retarget evidence is required before permission to open its own PR. Existing per-PR proofs remain mandatory when due.

### Proposed sixth-child foundation

After exact amendment approval, deliver the **sole new literal base `cod103-stack/cod-108` during future COD-108 foundation work**, with matching exact-set policy/tests/docs and serialized ownership in Scryer. Leave active COD-105's existing literal foundation unchanged. No wildcard, privileged trigger, push/release-main expansion or new permission is implied. COD-108's checked foundation/applicable full bootstrap checks and fresh integrated review permit opening COD-110; the real COD-110→COD-108 full-CI proof follows that opening. Missing/failed/stale proof blocks its handoff/completion/integration, not permission to create the event. Detailed ownership and evidence stay in the proposed Scryer execution section.

## Integration, rollout and release stages

- Integrate actual main merges **serially**. Retarget/transplant only the next accepted slice to current main, review the exact resulting diff and run fresh full PR CI. Require authorized **non-bypass** merger/strict server freshness; retain configured admin bypass without using it to replace enforcement. Each actual merge SHA needs successful **full own-main CI before the next integration**.
- COD-108's own PR remains full/bootstrap; its actual main merge remains full. Its post-protected-main lightweight rollout evidence stays required. After COD-110 changes shared core/wiring, repeat affected hosted trusted-release lightweight/common-preflight/heavy-skipped and mixed/untrusted-full evidence on the **final corrected shared state** before final release-PR merge. Ordinary PRs and every main push remain full; required/freshness negatives remain fail-closed.
- **Before final release-PR merge:** complete all accepted children and approved delta reconciliation, candidate review and actual tested-checkout/version preflight with fresh head/base/required evidence. No future release-merge SHA/main CI/artifacts are presumed to exist. This retains candidate review; it adds no separate Final Reviewer release-candidate gate.
- **After that merge, before any public mutation:** require its actual immutable merge source's own successful full main CI/run/attempt and exact two-target native artifacts/provenance, current ancestry/metadata/tag/version/authorization/conflict guards, and same-source/version nonmutating crate verification plus exact compressed-size success. Only then may tag/release/native mutations proceed. Preserve draft → native upload → complete size/byte readback → native publication **before crate upload**, registry-aware retries/recovery and finalization after both succeed. Reuse verification results; normal Cargo repackaging of unchanged source/version is allowed. PR artifacts/candidate checks never replace these final gates. Independent publication verification precedes release-complete reporting.

## Qualification route and release contents

- Fresh mapped Final Reviewer Sol/max reviews each integrated implemented snapshot read-only before PR handoff, including focused verification of confirmed P0–P3 fixes; Evidence Verifier independently establishes acceptance. Attached/configured checks are not executed passes. No unresolved warnings/acceptance failure may be relabelled successful.
- The [2026-10-31 COD-56 method decision](decisions/2026-10-31-cod56-emulated-qualification.md) supplements the [historical post-push route](https://linear.app/codebasegraph/issue/COD-56#comment-62b8347d-ebc5-41e7-b639-5fbe7dac415d): Parallels Windows11 OSArm64 with actual x64 PE emulation is sufficient, using built-in WinPS5.1 ARM64 and supported official PS7 X64-emulated. Only a disclosed architecture-predicate test-copy delta and policy-preserving fresh-child memory invocation are permitted; shipped installer bytes and support are unchanged. All28 actual matrix outcomes require raw/assertion-derived evidence and independent acceptance. Once that checkout qualification and other applicable staging gates pass, COD-56→COD-107 proceeds autonomously. Final publishable archive identity/provenance qualification remains at its later release stage, not a new PR-staging dependency on unavailable uploads. Main/non-bypass/own-merge-CI/release/closure gates remain; no account/token/runner/settings change is authorized.
- Independently verified fully published baseline: **v4.0.4 / `7125900c740297be5c60821dbeb9811f32a5894a`**, [accepted report](../../testing/COD-103/release-baseline-v4.0.4/report.md), source CI40997892360/attempt1 and publisher41010094891/attempt1. This is baseline evidence only, not future queue/platform qualification.
- Reconcile final delta from that baseline: selected already-merged **COD-54 / PR163 `c60a00c1d747b73789cd6edc7ee20601182a35ca`**, exact authorized future queue and necessary generated metadata. Prior PR158/COD-53 selection at `c4e9767d8f3ec92c40dd2f809b8421b8f9c78742` is preserved only where still unreleased; do not republish baseline. Reconcile later main delta before release; unknown changes need approval and any baseline advancement needs independent publication audit. No version is forced.
- Existing approval releases only its complete five-item queue/contents; the separately approved amendment requires the complete **six-item** queue/contents. No partial release. Native targets remain only macOS ARM64 / Windows x86_64. Selection/publication does not close COD-53/COD-54 or COD-103/COD-51/COD-61/platform aggregates; recorded maintainer/user owners retain closure.

## Evidence status

This index is proposal preparation, not a passing acceptance or implementation approval. Existing baseline audit is linked unchanged; all new behavioral checks/reports/hosted rollout remain planned. Scryer holds exact ownership, readiness prerequisites and planned execution, with real source/test anchors to be added only after implementation. Production persists verifier evidence and freezes contracts; Engineer obtains exact-path/full-SHA amendment approval and alone advances queue/tracking. No tests/builds/CI/Git/publication/settings or credentials were changed by Planner.
