# COD-103 independent proposal review

## Verdict

**Revise before implementation approval.** One material execution-gate ambiguity must be resolved before the fixed queue can be authorized as ready. The technical contracts otherwise support the proposed bounded implementation; this is not a finding that the product changes are defective or that delivery capabilities must already be available for proposal approval.

- Reviewed combined snapshot: **`b4af6ac79f3603c9e277a2c98bee3c4f75f2639c`**.
- Parent/source base: **`5889ed09954a4a07efd2590dd1bd380ee77be06a`**.
- Canonical scope: `docs/specs/COD-103/{spec,design,architecture,verification,delivery}.md`, proposed revision 0.1; governing unchanged CBG-PLAT-01 rev0.11 S05/R05/A09 at `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`; scoped Scryer intent and all five execution briefs.
- Independence: fresh reviewer; no authorship of the reviewed proposal, no other role dispatch.
- Required profile: GPT-6.1 Sol / high. Engineer explicitly confirmed actual spawn `openai-codex/gpt-6.1-sol`, `thinking=high`. Runtime model/thinking metadata is independently **unexposed to this child**; no substituted profile or settings change.
- This report is review, not user approval, Scryer sign-off, implementation evidence, or release authorization.

## Finding PR-01 — First hosted stack proof has no non-circular handoff gate

**Priority:** Material; resolve before combined implementation approval of this queue.  
**Evidence type:** Established execution-gate ambiguity, with a concrete deadlock under the literal completion interpretation. It is not an observed implementation failure.

### Evidence

1. [`delivery.md:25–27`](https://github.com/rabii-chaarani/codebaseGraph/blob/b4af6ac79f3603c9e277a2c98bee3c4f75f2639c/docs/specs/COD-103/delivery.md#L25-L27) permits opening the next real stacked PR only after its predecessor's reviewed and verified PR handoff, does not permit bypassing acceptance blockers, and requires COD-105's stack foundation before any dependent PR opens.
2. [`design.md:77–93`](https://github.com/rabii-chaarani/codebaseGraph/blob/b4af6ac79f3603c9e277a2c98bee3c4f75f2639c/docs/specs/COD-103/design.md#L77-L93) puts the foundation in COD-105, then makes COD-106 the first PR with a non-main base. No separate qualification PR or intermediate-branch merge is proposed.
3. Scryer **`chg-ahwwmf`**, COD-105 execution brief, **Completion/radius**, includes: “actual hosted first main-base PR and later non-main-base stack event/retarget show fresh stable required.” The same brief releases shared files and permits COD-106 staging only after COD-105's reviewed/verified PR handoff.
4. Scryer **`chg-9bqhd2`** makes COD-105's reviewed/verified handoff and usable stack policy prerequisites for COD-106. [`verification.md:74`](https://github.com/rabii-chaarani/codebaseGraph/blob/b4af6ac79f3603c9e277a2c98bee3c4f75f2639c/docs/specs/COD-103/verification.md#L74) requires real hosted qualification for the actual stack pair, not configuration-only evidence.

### Trigger and consequence

When closing COD-105's staging handoff, a verifier can reasonably treat every item in its Completion/radius as required evidence. The first real non-main-base event can then only come from COD-106, but COD-106 cannot open until that handoff passes. Calling the proof “later” signals an intended staging distinction, but does not define which handoff may proceed with it pending or who owns its later gate. The alternative is an undocumented relaxation of the no-bypassing-acceptance rule. Either interpretation leaves the ready-queue commitment ambiguous.

The technical stack solution is feasible; the problem is assignment and timing of its first hosted proof, not the exact branch list, normal PR trigger choice, or missing desktop/merge credentials.

### Proportionate correction and validation

Planner should explicitly distinguish **foundation/first-main-base-PR handoff** from **first hosted non-main-base stack qualification**. A narrow correction is to allow COD-105 staging handoff after its own applicable local, artifact, review and main-base PR checks, while assigning the first real non-main-base event/retarget proof to COD-106's qualification gate. Keep that proof pending until executed, require it before COD-106's accepted handoff/integration, and preserve all serial-main/full-CI gates. This is a recommendation, not an approved change or permission to skip evidence.

Update the affected Scryer briefs and the compact delivery gate distinction together. Validate by tracing the ordered sequence with no later PR required before its own permission to open. Later actual hosted runs must still prove fresh `required` for their exact head/base; a clarified plan is not a passing hosted test. Focused rereview of these changed gates is sufficient unless the correction changes other contracts.

## Supported conclusions; concerns checked and not retained as findings

- Metadata admission is a positive six-file semantic proof, not a filename/bot-label exception. Current B→H ancestry/diff, protected-B helper/build context, observed creator-ID pin, bootstrap-to-full, conservative generated-layout recognition, unchanged remainder, and explicit full fallback have clear boundaries (`design.md:99–141`). Read-only inspection of the real PR167 merge diff confirms that its generated six-file shape matches the proposed intended fixture. **The classifier has not been implemented or executed.**
- The common/heavy state table rejects missing, failed, cancelled, stale and unexpected results; final API/freshness failure remains failure even after full-mode fallback. Every main push remains full, with no PR artifact promotion (`design.md:145–162`; `verification.md:185–196`). Head-editable workflow limits and non-bypass strict-main enforcement are explicitly disclosed rather than overclaimed.
- COD-108 is not the same sequencing defect: its own PR explicitly runs full, and hosted lightweight rollout occurs only after its merge and successful full main CI, before final release-PR merge (`design.md:139`; `delivery.md:29`). Those staged gates should remain.
- MCP output/status validation, legacy selection and pre-upload ancestry/tag revalidation, and immediate PowerShell native-exit checking stay within existing responsibilities. The proposed actual-body fixtures and unchanged A09 controlled-native/desktop/shell matrix are meaningful checks, not mere string assertions (`design.md:29–57`).
- The docs exception uses a literal component/path boundary and rejects symlink-based exemption. The limited walker scope and content-policy enforcement by review are disclosed; no scanner framework, Python CI dependency, helper restoration or packaging exemption is implied (`design.md:59–73`).
- Scryer `node-1` contains exactly the two cited draft additions in `chg-9xdkd4`; static workflow validation remains `resp-15`. The delivery index resolves early artifact/hash/forward-reference wording to the final canonical documents and actual draft IDs. No new ledger/topology is needed for this review.
- Exact-source release, two supported targets, full authorized delta reconciliation, independent publication verification, preserved PR158 inclusion, and maintainer aggregate-closure ownership remain intact. Existing four-target wiki wording is stale orientation, not permission to expand scope (`spec.md:28–48,111–127`; `delivery.md:35–39`). No claim that 4.0.3 is fully published is made here.

## Existing gates, not new proposal findings

Exact combined user approval and S05/COD-51 sign-off remain prerequisites before dependent implementation. An authorized non-bypass merger remains an integration/release capability gate. Actual Windows 11 desktop PS5.1/supported-7 execution remains COD-56 A09 acceptance and complete-queue release evidence. Hosted classifier rollout, all acceptance executions, final exact-candidate review, publication gates and independent publication verification remain outstanding. These disclosed gates do not themselves prevent preparing or reviewing this proposal, and do not authorize provisioning, settings changes, weaker substitutes or partial release.

## Actually performed checks and limits

- Read both assigned reviewer skills, Engineer skill/role map and applicable adversarial-review lens guidance.
- Read freeze metadata, checked clean proposal-worktree status, exact parent, and changed-path inventory using read-only Git commands. The proposal changes only the five canonical documents.
- Recomputed SHA-256 for all five document byte contents; all match `/tmp/cod103-proposal-freeze.json`.
- Read configured wiki bundles, relevant memory and release architecture; read binding Scryer rules, scoped model/orientation, both actual draft claim entries and the five specified brief rationales. No unrelated open-change contents were used as task authority.
- Graph MCP was not exposed. Used semantic CLI search and explicit change-impact context with `--no-refresh` to discover xtask. Older-root graph/Scryer implementation orientation was not treated as current-source behavior. Inspected exact-source Git blobs for the discovered xtask and its named workflows/helper/install/manifests, and the immutable existing S05 contract.
- **No tests, builds, installs, hypothesis execution, hosted workflow runs, production probes, remote mutations, Git/source/spec/Linear/Scryer/wiki/settings edits, approvals or sign-offs were performed.** Read-only metadata/source inspection is not behavioral acceptance. Live PR identity/settings/publication and Windows/non-bypass capabilities were not independently re-probed; their dated supplied observations remain limits.
- Sole write: this report, `/tmp/cod103-proposal-review.md`. No constants changed. Assumptions: the supplied fixed queue/endpoint and selected decisions are authentic; exact combined approval remains pending; preparation inputs outside the freeze are not canonical.
- Reusable-knowledge assessment: no new memory warranted for an unapproved proposal and transient scheduling finding.

**Next owner:** Engineer routes PR-01 to Planner, Production freezes any material corrected canonical snapshot, and a focused independent rereview checks the affected gate bindings before the combined immutable approval request.
