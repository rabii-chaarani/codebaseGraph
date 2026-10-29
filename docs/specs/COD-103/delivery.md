# COD-103 — proposed delivery index

**Status:** proposed revision 0.2; independent proposal review and exact-scope user approval pending. No implementation, qualification, merge or release approval is recorded by this index.

Requirements are in [spec.md](spec.md), contracts in [design.md](design.md), architectural intent in [architecture.md](architecture.md), and planned evidence in [verification.md](verification.md). Detailed execution briefs, exact writable files, prerequisites and tests belong only in Scryer. Preparation ledger: `chg-fdm24r`. Repository source base: `5889ed09954a4a07efd2590dd1bd380ee77be06a`.

## Fixed ordered queue and Scryer briefs

One issue is active for implementation at a time. No other backlog is included.

| Order | Issue / acceptance | Scryer execution brief | Literal staged PR head → initial base |
|---|---|---|---|
| 1 | COD-105 / C103-A01, first-PR A05 prerequisite | `chg-ahwwmf` | `cod103-stack/cod-105` → `main` |
| 2 | COD-106 / C103-A02 | `chg-9bqhd2` | `cod103-stack/cod-106` → `cod103-stack/cod-105` |
| 3 | COD-56 / unchanged external A09 | `chg-2z3ngs` | `cod103-stack/cod-56` → `cod103-stack/cod-106` |
| 4 | COD-107 / C103-A03 | `chg-xd2jtt` | `cod103-stack/cod-107` → `cod103-stack/cod-56` |
| 5 | COD-108 / C103-A04/A05, delivery A06 | `chg-cw9n3h` | `cod103-stack/cod-108` → `cod103-stack/cod-107` |

COD-108 changed intent is separate: draft architecture `chg-9xdkd4`, existing `node-1`, claims `resp-1rnbs9` and `resp-tddbkj`. No sign-off or implementation verdict exists. COD-104 stays Done and admin bypass remains configured but cannot bypass delivery gates. COD-99 archival history is retained.

COD-56 keeps [CBG-PLAT-01 revision 0.11 S05/R05/A09](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer), its original milestone, COD-51 exact-scope approval prerequisite, COD-61 dependency and maintainer closure owner. No platform spec is copied or migrated.

## Explicit stack-staging exception proposed for approval

Normally actual merge/main-CI gates precede advancing a release queue. This bounded exception permits the next ordered issue to be staged as a **real PR on its reviewed and verified predecessor branch** after that predecessor's applicable handoff (COD-105 has the staging-only handoff below), without merging it first. It does not permit simultaneous active issues, bypassing acceptance blockers, intermediate-branch merges or out-of-order main integration.

The first COD-105 PR establishes the bounded stack-base trigger/policy/tests/docs prerequisite and the staging-only handoff below before COD-106 opens. All stack staging remains full CI; no classifier is pulled forward. Only the five literal bases in [design.md](design.md) and its ordinary PR activities are admitted. Push/release stay main-only, with no privileged trigger, publication permission expansion or PR artifact retention. Base/head changes invalidate prior qualification and require a fresh event/current pair.

### First-child staging clarification (PR-01)

1. **COD-105 staging-only handoff:** the foundation is implemented; applicable local and real-artifact checks, including C103-A01, full first main-base PR CI, independent final review and confirmed-fix verification pass at the exact staging snapshot. Engineer records this limited readiness and the still-pending C103-A05 hosted non-main-base proof. This is not Done, full stack qualification or integration approval.
2. **Permission to open the first child:** COD-106 becomes the sole active implementation issue. Production may open its real PR against `cod103-stack/cod-105` to obtain the deferred proof. COD-105 is no longer an active implementation writer.
3. **Deferred hosted proof:** Production obtains that first child's actual fresh non-main-base event. Evidence Verifier records its real full-CI/stable-`required` result, base/head/run/attempt identities and limits against C103-A05; Engineer links it to both COD-105 `chg-ahwwmf` and COD-106 `chg-9bqhd2`. If child CI reveals a foundation defect, pause COD-106 writes, return the fix to COD-105's owner, independently review it and rerun the child's fresh base/head pair. This first-child proof supplies the existing COD-105 brief's **later** hosted stack evidence; it does not require all later stack/retarget events before the first merge. Existing per-PR freshness and retarget checks remain mandatory at their actual stage.
4. **Completion barrier:** missing, failed or stale deferred proof blocks full COD-105/stack qualification, COD-106 handoff to COD-56, **every actual main integration**, Done and release claims. Opening COD-106 under the limited handoff does not imply that proof passed or was waived. All original tests, acceptance IDs and subsequent serial-main/full-CI gates remain.

This canonical qualification-stage rule interprets the immutable existing Scryer briefs' forward reference without rewriting or duplicating them. No later PR is needed before permission to open that PR. See [review resolution](reviews/resolution-r01.md); independent focused rereview and combined user approval remain pending.

Actual integration stays **serial to main**. Retarget/transplant only the next accepted slice to current main, review that exact resulting diff and run fresh full PR CI. Use an independently verified authorized non-bypass merge actor under strict freshness. Require successful full main CI at each actual merge SHA before the next merge. COD-108's own PR remains full until its classifier is on protected main. Hosted lightweight rollout evidence follows merge/full main CI and must exist before the final release-PR merge.

## Unpassed starting and delivery gates

- Freeze this combined proposal remotely, obtain fresh Proposal Reviewer Sol/high review, resolve material findings, then obtain exact-scope combined user approval before any product edit, including foundations. Record actual user sign-off for the two draft claims only after that approval.
- Combined approval covers proposed docs-helper restrictions, metadata/trust/constants/check modes, literal queue/staging exception, unchanged S05 scope and reconciled release contents/baseline. Preserve settled docs boundary, Sol/xhigh Designer override, PR158 inclusion, retained bypass and platform policies. COD-51 exact S05 sign-off still blocks COD-56 coding.
- Authorized non-bypass merger and real Windows11 x86_64 desktop PowerShell5.1/supported7 capability are unestablished. No account/secret/runner/settings change is authorized to bypass them.
- All behavioral checks remain planned. Focused genuine JUnit/report ingestion supplements, not replaces, unchanged full native tests/features/cohorts/captured CLI provenance. The proposed bounded PowerShell System.Xml actual-result route must report real A09 cases; setup errors and missing/unrun cases are not passes.
- Fresh Final Reviewer Sol/max reviews integrated snapshots/evidence and confirmed fixes. Production alone serializes Git and actual main integration. No warnings or acceptance failures may be relabeled as passed.
- Reconcile the current fully published baseline and full approved delta before release. PR167 was externally merged and pending at the source observation; it may publish normally. This proposal authorizes no retry/dispatch/relabel of that existing publication. User-approved PR158 inclusion is preserved; unknown later product delta requires a user decision. Do not force the next version.
- Release only the complete accepted queue and authorized delta, after exact-candidate review and all publication gates. A release-PR merge commits to unattended publication; independent publication verification follows. COD-103/COD-51/COD-61/platform aggregate closure remains owner-controlled.

## Provenance and evidence status

Compact index derived from Planner source SHA-256 `d75b0f94d64c1f9d2e0586d9df060ba427e35c8aa45d9185fa373bfdb0f37519`; Planner actual profile Sol/high. Detailed briefs were verified present through Scryer read-back; no brief contents are duplicated here. Source artifact hashes are preparation provenance, not approval. No behavioral tests/builds or qualification checks have executed for this proposal. Documentation reference/whitespace checks do not establish product acceptance.
