# COD-103 — Production-gate corrections and bounded policy refinements

**Proposed requirements revision:** 0.2. **Status:** preparation; not implementation-approved.
**Canonical destination for Production to freeze:** `docs/specs/COD-103/spec.md`.
**Owner:** repository maintainer/user; Engineer coordinates delivery. Detailed design, execution briefs, file ownership and claim changes belong in Scryer, not this requirements document.


## Combined proposal references (pending approval)

This canonical proposed revision is accompanied by [software design](design.md), [architecture rationale](architecture.md), [verification plan](verification.md) and the compact [delivery/queue index](delivery.md). These references settle proposal preparation, not user implementation approval. Detailed execution briefs remain only in Scryer: COD-105 `chg-ahwwmf`, COD-106 `chg-9bqhd2`, COD-56 `chg-2z3ngs`, COD-107 `chg-xd2jtt`, COD-108 `chg-cw9n3h`. Preparation ledger: `chg-fdm24r`. Proposed COD-108 changed intent: `chg-9xdkd4`, existing `node-1`, draft claims `resp-1rnbs9` and `resp-tddbkj`; sign-off is pending.

The fixed literal stack and bounded staging exception are proposed in delivery.md; actual main integrations remain serial with successful full main CI at each merge SHA. All acceptance checks remain unexecuted. Separate approval must identify this proposal's immutable remote full Git SHA and exact scope before dependent implementation. This file does not embed its own containing commit SHA.

Revision 0.2 clarifies only the first hosted stack-proof gate in response to [PR-01](reviews/proposal-r01.md); see [resolution record](reviews/resolution-r01.md). Revision 0.1 at `b4af6ac79f3603c9e277a2c98bee3c4f75f2639c` and its review are preserved. Focused rereview and combined user approval remain pending.

## 1. Identity, decisions and baseline

Work: [COD-103](https://linear.app/codebasegraph/issue/COD-103), CodebaseGraph workspace/team COD, Maintenance P-COD-2. Remaining children: COD-105, COD-106, COD-56, COD-107, COD-108. COD-104 remains Done; its intentionally retained admin bypass is unchanged. COD-99 remains Done; its historical archival decision/evidence is preserved.

The user requested: **“start resolving the sub-issues. they all should be in a stacked PRs. you are responsible for everything end-to-end including releasing the changes.”** This authorizes the endpoint of the complete accepted queue, not an unreviewed implementation proposal or partial release.

On 2026-10-29 the user added: **“1. use gpt-6.1 xhigh; 2. allow only for docs/. 3. include in final release”**. Decision record: [COD-103 comment 17758bee-a742-4ba8-bc84-bb2c82a47f2c](https://linear.app/codebasegraph/issue/COD-103#comment-17758bee-a742-4ba8-bc84-bb2c82a47f2c); docs boundary is also recorded on COD-107 comment `a0f43f4a-3bb2-491d-bdfc-285419fa9d91`.

Settled by those decisions:

- Software Design may use GPT-6.1 Sol / Extra High for this task instead of unavailable GPT-6.1 Terra. This task override does not rewrite the role map or other role settings.
- Python documentation/evidence helpers may be allowed **only within `docs/`**. This is not permission for Python product implementation moved into docs.
- Include the existing **PR #158 / COD-53 subset at `c4e9767d8f3ec92c40dd2f809b8421b8f9c78742`** in the final complete-queue release. This does not close COD-53 or assert its remaining consumer/runtime qualification passed.

Latest remote main independently refreshed by Production is **`5889ed09954a4a07efd2590dd1bd380ee77be06a`**, release PR **#167** merge. Production reports the full delta from fully published **4.0.2 / `9f69a0bd4b468eaa8652d933d518fafb8c596b6a`** is exactly approved PR158 plus PR167 version/changelog metadata, with no unknown product change. At refresh, PR167 was `autorelease: pending`, its own main CI **40837906797 / attempt 1** was queued, and no 4.0.3 tag/release/crate existed. These are time-bound observations, not a failure or a new frozen publication baseline. Let normal live publication proceed; do not dispatch/retry the current release or change its labels. Record a new published baseline only after independent verification. The final candidate must reconcile any subsequently published baseline/full delta without silently authorizing new scope.

Before dependent implementation, freeze the current proposal/design/plan at a full Git SHA with remote immutable links and a separate exact-scope approval record. Do not embed this document's own containing SHA. Existing approvals are never silently advanced.

### Existing platform scope is referenced, not duplicated

COD-56 remains **CBG-PLAT-01/S05, R05 → A09**, at the existing canonical [platform spec rev0.11, full SHA `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer). D05b Windows 11 x86_64 desktop and D05c built-in PowerShell 5.1 plus supported PowerShell 7 are settled policies. **COD-51 still blocks S05 exact-scope implementation approval.** Earlier approvals for other slices do not clear it.

Retain COD-56's original milestone, COD-61 blocking relationship, qualification evidence and maintainer closure owner. The platform spec was absent from the earlier `c4e9767…` main tree but remains published at its existing full SHA. If reconciliation is needed, Production must preserve that path/revision/history and bring the exact existing document through the authorized history; no parallel COD-56 spec, new platform work ID or rewritten approval.

## 2. Problem, outcome and scope

Packaged MCP smoke can falsely accept an error/failing process. Legacy existing-tag recovery omits the current-main ancestry guard. PowerShell installer probes can accept native nonzero exits before replacement. The Rust-only gate rejects all docs Python, contrary to the selected bounded documentation policy. Release-metadata PRs currently repeat heavy CI even when their actual semantic changes need only lightweight validation before a fully qualified main merge.

Outcome: restore the three existing failure barriers; permit the selected docs-only helper boundary; reduce heavy release-PR CI only for positively verified metadata-only changes; deliver all accepted children as a real ordered PR stack, integrate serially, and release the complete authorized source and artifact set.

**In scope:** COD-105/106/56/107/108, necessary stack qualification staging, existing approved PR158 subset, exact-source release evidence and delivery.

**Excluded:** reopening COD-104/COD-99; removing admin bypass; product/dependency license changes or inferred relicensing of helpers/evidence; new supported platforms; a two-heavy-run alternative; new publication permissions/privileged PR triggers; PR-artifact promotion; weakening exact-SHA CI/provenance/security gates; two-binary rollback redesign; unrequested helper restoration; automatic COD-53/platform milestone/parent aggregate closure; unknown later main delta without reconciliation and approval.

Only current approved native targets **macOS ARM64 and Windows x86_64** apply. Stale four-target skill/wiki wording must be reconciled as procedure, not used to expand platform support.

## 3. Requirements

The `C103-` IDs below belong only to this combined delivery specification. They do not replace child identities or platform R05/A09.

### C103-R01 — Successful packaged MCP smoke (COD-105)

A packaged graph MCP smoke passes only when the child completes successfully and valid JSON-RPC responses for the expected initialize and tools/list request IDs demonstrate successful initialization and an actual tools entry named `graph_search`.

Nonzero status, response errors, missing required response, malformed protocol data and a missing tool fail artifact acceptance. Error-message substrings cannot establish success. Report useful status/protocol diagnostics. This restores the current release contract; it does not define a new product protocol.

### C103-R02 — Main-history source in legacy recovery (COD-106)

Every publication source, including existing-tag native-assets-only recovery, must remain in current main history. Sources identical to or ancestors of current main are valid; removed/diverged sources are not. Establish ancestry before authorizing recovery and recheck before the legacy asset mutation.

Preserve strict tags, exact-source successful main-push CI, complete artifact/provenance checks, dry-run, historical tags without release-PR linkage, native-assets-only legacy scope and original source identity. Do not require current main-tip equality or add crate publication to this recovery mode.

### External requirement — CBG-PLAT-01/S05 R05/A09 (COD-56)

Implement and qualify the existing platform requirement **in place**. Every native probe's nonzero exit must fail before either destination binary is replaced; both binaries must pass existing checksum/probe validation first. Reuse all A09 success/failure/DryRun/destination-preservation cases and both approved PowerShell shell families on the approved Windows desktop scope. Do not restate or renumber the platform acceptance contract here.

### C103-R03 — Bounded docs helpers (COD-107)

The selected path boundary is `docs/` only. Proposed remaining constraints for combined approval:

- Allowed content is documentation, evidence, qualification, audit or reproduction support, not product runtime/build/install implementation.
- Helpers may be explicitly invoked by developers as **optional Python 3 standard-library-only local commands**. This slice adds no Python dependency, package/tool manifest, setup or invocation in CI, Release, product build, runtime or installation. Existing workflow Python prohibitions remain.
- Helpers are absent from shipped native archives and publishable `.crate` payloads. Ordinary Git/tag source snapshots may contain documentation; this is not an executable distribution dependency.
- Existing prohibited Python surfaces outside docs and prohibited root tooling remain rejected. No docs-path allowance exempts other repository/security/workflow checks.
- No automatic extraction/restoration of COD-99 archived helpers. Preserve archive bytes, reconstruction notes and historical decisions.

These constraints are recommendations pending combined approval; the user explicitly settled only `docs/`, not every invocation/packaging choice above.

### C103-R04 — Positively verified metadata-only PR mode (COD-108)

Only a current trusted repository-owned release PR with a positively verified **actual base/head semantic diff** may use lightweight CI. Branch, label, title or small diff alone is insufficient. Every ordinary PR and **every push to main**, including a release-PR merge, keeps full CI and exact-source native artifacts.

Proposed exact allowlist for combined approval:

- `.release-please-manifest.json`: root `.` release version only.
- `Cargo.toml`: root package.version only.
- `crates/k-wiki/Cargo.toml`: package.version and local codebase-graph dependency.version only.
- `crates/xtask/Cargo.toml`: package.version only.
- `Cargo.lock`: only corresponding codebase-graph/k-wiki/xtask workspace package-version identities; no external package resolution, checksum, source or dependency-edge changes.
- `CHANGELOG.md`: consistent new version-specific release section; no rewriting historical sections.

All fields/versions must agree. All other changes run full CI. Real generated release PR #165 includes xtask and workspace-lock version bumps; a blanket lockfile exclusion would reject the intended positive case.

Trust must verify current same-repository head/base and expected configured release identity; actual semantic diff proof is still mandatory. Do not presume a bot username without verifying the configured proposal-token identity.

Proposed mandatory lightweight checks: positive classifier, repository/workflow policy, metadata/version/changelog consistency, formatting, and the existing advisory scan. Heavy native/test/artifact/package/lint jobs may be skipped only for this positively established mode. Report skips as skips. Code, workflows, dependencies, build, packaging, features, installers or uncertain acquisition/classification use **full CI**; no silent lightweight fallback.

### C103-R05 — Stable, fresh required result and stack qualification

Keep exactly one stable `required` result for each qualified PR. The result must be based on the selected valid mode and that mode's mandatory successful checks. Failed/cancelled checks, missing/invalid classification or unexpected job states cannot make it green. Intentional heavy skips are accepted only with positive fresh lightweight evidence.

A changed head or base must invalidate previous classification/qualification. Preserve strict-freshness enforcement and the intentionally retained admin bypass; do not claim a bypass-capable actor provides server enforcement.

Proposed fixed true-PR-stack order: **COD-105 → COD-106 → COD-56 → COD-107 → COD-108**. The stack CI/staging prerequisite may be delivered as part of first-PR staging under the approved combined scope. The technical proposal must specify how non-main-base stack PRs gain valid qualification under currently main-only PR triggers. It may not silently discard stacking, introduce an unapproved privileged trigger or accept missing/greenless required checks. Any approved staging exception to default merge-before-next-implementation must be explicit in the fixed queue record. The [delivery index](delivery.md#first-child-staging-clarification-pr-01) distinguishes COD-105 staging-only handoff from full qualification: its own applicable local/artifact/full-main-base-PR checks and independent final review/fix verification permit opening COD-106 solely to obtain the deferred first-child non-main-base full-CI proof. Production obtains it, Evidence Verifier records it against C103-A05, and Engineer links both existing ledgers. Missing/failed/stale proof blocks full COD-105 qualification, COD-106 handoff to COD-56, every actual main integration, Done and release. Later per-PR retarget proof remains due at its actual stage, not as a prerequisite to opening its own PR. No evidence is waived or presumed passed.

Integrate actual main merges serially. Require successful full main CI for each actual merge before the next integration step. Shared ownership and implementation details remain in Scryer.

### C103-R06 — Complete authorized release and retained ownership

Release only when all five selected children satisfy their exact acceptance/review/verification gates, the approved full unreleased delta is reconciled, and the exact release candidate passes applicable gates. Include PR158's approved subset; do not infer unknown later changes are approved or COD-53 is closed.

Publication remains bound to the release merge's own successful main-push CI, exact native artifacts, protected production confirmations, complete provenance/validation and existing recovery/finalization rules. Keep the current release process and supported targets. Final candidate review and independent publication verification are required before reporting release complete.

No partial queue release or silent qualification deferral. Parent COD-103, COD-51, COD-61 and platform milestone aggregate closure stay with their existing maintainer/user owner; a release or completed child does not automatically close them.

## 4. Failure, recovery, permissions and privacy

- Smoke/probe failure stops acceptance or replacement, preserves existing installed bytes where A09 requires, and retains useful failure evidence. A source read, child status or malformed-response failure cannot become a pass.
- Failed/unavailable ancestry comparison blocks publication. If main changes invalidate source before mutation, stop; retry only after re-establishing valid source. Never move a tag or clear pending labels to bypass identity or publication gates.
- Diff/API ambiguity, failed/partial acquisition, unsupported file/field, rename/delete or stale base/head cannot select lightweight mode. Use full CI; if the mode result itself is invalid/missing, `required` must not succeed. Retries must use current identity/diff, not stale success.
- Cancellation/failed jobs cannot authorize merge or publication. Preserve failed attempts and exact rerun identity. Partial external publication uses the existing exact-target recovery process; no substitute SHA/run, mixed artifact source or premature finalization.
- No automatic account/token/settings changes or new secret permissions. Use existing read-only PR qualification posture. Do not paste tokens into chat or commit credentials/private runtime state in evidence. Retain only relevant source/run IDs, hashes, OS/shell versions and necessary redacted diagnostics.
- The authenticated admin always-bypass identity is not the required enforceable merge path. An **authorized non-bypass merge actor** remains a capability gate before integration/release; preserve the user's bypass choice and do not replace server enforcement with a weaker local-only merge check.
- **Actual Windows 11 desktop PowerShell 5.1 and supported 7 execution** remains a capability/evidence gate for COD-56 A09. Windows Server CI, macOS PowerShell, configured jobs and source traces are not substitutes. If unavailable, keep acceptance/release pending; do not reduce the approved support contract.

## 5. Acceptance and executed-evidence plan

All checks below are **planned**, not passed. Record exact tested/reviewed full SHA, method/command, environment, result, named cases, durable report/JUnit where required and limitations. Existing Scryer claims are applicable only after current-intent verification; no new claims/topology are invented for restoring fixes.

| Criterion | Requirement / issue | Given / When / Then | Evidence / applicable intent |
|---|---|---|---|
| C103-A01 | R01 / COD-105 | Given valid initialize/list responses and zero exit, smoke passes; when nonzero+error, zero+error, missing init, malformed data or missing graph_search occurs, smoke fails. | Affected xtask regressions and corrected real packaged native smoke; `resp-16`, `resp-17`, `resp-256`. |
| C103-A02 | R02 / COD-106 | Given otherwise-valid existing-tag CI/artifacts, identical/ancestor sources authorize; removed/diverged or comparison errors fail before upload, including a mutation-time ancestry change. | Executed actual guard-body fixtures; automatic/resume main-advance and removed-history regressions; unchanged dry-run/assets-only behavior; `resp-259`. |
| **Existing A09** | **CBG-PLAT-01 R05 / COD-56** | **Use the existing linked A09 unmodified**, with both shell families on actual Windows 11 desktop, success/nonzero candidates, DryRun/real install and byte-identical existing destinations on failure. | Exact native OS edition/release/build, shell versions, source/archive hashes, probe exits and before/after bytes; `resp-16`, `resp-256`. Do not claim platform aggregate qualification from this slice. |
| C103-A03 | R03 / COD-107 | Given a qualifying docs helper, ordinary and confirmed-production gates accept it; neighboring non-doc Python, prohibited root tooling and workflow Python remain rejected. | Positive/negative boundary checks; actual native/crate inventories exclude helpers; optional invocation matches policy; diagnostics/docs/Scryer intent agree; `resp-14`, applicable `resp-15`. |
| C103-A04 | R04 / COD-108 | Given trusted allowed semantic metadata only, lightweight succeeds and heavy jobs are unscheduled; mixed/untrusted/uncertain changes select full CI, never an unvalidated exemption. | Classifier negatives plus hosted allowed and mixed PR event/mode/job evidence; no external lock resolution change allowed; `resp-15`, `resp-254`–`resp-256`. |
| C103-A05 | R05 / stack | Given valid fresh mode and successful required jobs, stable required succeeds; failure/cancellation/missing classification/unexpected states fail; changed base/head requires fresh qualification. True stack receives the approved qualification before serial integration. | Executed aggregation/freshness/event tests and hosted stack/main evidence; authorized non-bypass merge identity; same required context/admin policy retained. |
| C103-A06 | R06 / delivery | Given all accepted children and authorized reconciled delta, actual serial main merges pass full CI; exact release merge builds/smokes retained artifacts and publishes only their source; independent verifier confirms publication. | Per-merge main CI, immutable candidate review, release run/artifact/checksum/crate/finalization evidence; `resp-259`, `resp-276`; no premature parent/platform closure. |

## 6. Open decisions and readiness

**Settled; do not ask again:** docs path `docs/`; Software Design Sol/xhigh task override; PR158 subset inclusion; fixed supported Windows/shell policies; retained admin bypass and COD-99 historical archival scope; stacked delivery through complete-queue release endpoint.

**One combined exact-scope approval after technical preparation/review must cover:** this committed revision and linked unchanged S05/R05/A09; proposed optional stdlib/local-only docs invocation and package/archive constraints; precise metadata/trust/lightweight/full-fallback contract; fixed stack staging/CI strategy and ordered queue; exact reconciled full release contents/baseline. Recommendations are not approval. Material changes require a new pinned revision/impact review and approval, not a silently edited link.

Concrete preparation dispositions and remaining gates:

1. The narrow literal-stack qualification strategy and classifier contracts are now proposed in [design.md](design.md) and [delivery.md](delivery.md); combined review and user approval remain pending.
2. Production has reconciled `5889ed0…` as approved PR158 plus PR167 version/changelog metadata. It must independently verify any normal live 4.0.3 publication before advancing the published baseline, then freeze the final authorized candidate contents/version. Unknown subsequent scope still requires owner disposition; no release retry or label mutation is authorized by preparation.
3. Engineer/owner must provide the authorized non-bypass merge path and real Windows desktop/shell qualification capability. Production found zero repository self-hosted runners and only the owner/admin bypass collaborator; capability remains unestablished. These are operational gates, not new support-policy questions or permission to weaken acceptance.
4. Record explicit S05 exact-scope sign-off through COD-51 without claiming the other platform slices or milestone are closed.

## 7. Preparation evidence and handoff limits

Requirements role requires GPT-6.1 Sol/high. Coordinator previously confirmed actual spawn `openai-codex/gpt-6.1-sol`, `thinking=high`; child runtime model/reasoning metadata is independently unexposed. Software Design's newly authorized xhigh override does not change this role.

Read requirements skill/role map, latest full relevant Linear records and comments, prior canonical platform contract and intake, configured wiki active memory. Current decisions were verified against live comments above. Initial local lookup could not resolve the latest SHA; that was an earlier object-availability observation, not a claim about current remote state. Production subsequently independently verified main `5889ed0…`, its already-merged/not-yet-published PR167 release metadata and the exact approved PR158-plus-generated-PR167 delta, and prepared a clean isolated proposal worktree at that SHA. Requirements made no newer-source behavior inference from the old checkout. Original dirty checkout is untouched.

The Requirements author produced a preparation artifact; Production rendered it into this canonical Git proposal with relative companion links, actual Scryer references and current proposal dispositions. Requirements input SHA-256: `7695dad95775d674e04a4151bc90f5034c30e10dae4e2aff4962073139e5d6ac`. No product changes, behavioral tests, qualification passes or changed constants are claimed. No new reusable memory is warranted for unapproved/transient preparation. This proposal freeze preserves old approvals and precedes fresh proposal review and the combined immutable user approval request; it is not implementation approval.
