# COD-137 — Preserve native C4 contracts on the restored baseline

Revision 0.1, 18 November 2026 (Australia/Darwin). Work `CBG-RESTORE-01/C4-P0`, [COD-137](https://linear.app/codebasegraph/issue/COD-137), parent [COD-136](https://linear.app/codebasegraph/issue/COD-136). This is a bounded preservation brief under existing approval; original CBG-C4-01 identities, requirements, approvals and closure owners remain authoritative.

## Authority, baseline and endpoint

The maintainer approved [CBG-RESTORE-01 revision 0.1 at d809fe0f8289bbf7d167a891bd7c31160f27f0e1](https://github.com/rabii-chaarani/codebaseGraph/blob/d809fe0f8289bbf7d167a891bd7c31160f27f0e1/docs/specs/CBG-RESTORE-01/spec.md), with [actual decision 663b2fc8d2790956975587dcab6b248bd40b5958](https://github.com/rabii-chaarani/codebaseGraph/blob/663b2fc8d2790956975587dcab6b248bd40b5958/docs/specs/CBG-RESTORE-01/approval.md). REST-R06/R07/R08/R09/R10/R11/R12 authorize separate companion preservation in the fixed order. This brief reuses that exact scope; it changes no contract or acceptance criterion.

Source baseline: **6115439262005d1de984c5837e88f057c7317205**, tree **b2a56029eacc507a5b8f0a8a79207f73fe99e9eb**, [PR #200](https://github.com/rabii-chaarani/codebaseGraph/pull/200). Current connected GitHub readback confirms this open/unmerged head and CI845/run44114765768 success, 25/25 jobs. Its merge-test commit 14cf814f631f488f7876e194bfb745c8b6b975be and prior independent evidence are retained by the restoration handoff. This prerequisite is a verified PR handoff, not a merge or parent closure.

Original document source: **ba5c7c543bd711b663e1dfc92d4bd58497ffb557**, [COD-19 activation](https://github.com/rabii-chaarani/codebaseGraph/blob/ba5c7c543bd711b663e1dfc92d4bd58497ffb557/docs/specs/CBG-C4-01/activation/cod19-activation.md), original PR187. The four approval-subject files at **66dff2564e76ad06cdf172332e2da318d150fc73** remain byte-identical at that source. Canonical specification 0.3 SHA-256 is 6dda3d4795ac726b4ee76a62851f89f706e9292a715d5e0306c30d6c9d07b98f; transition SHA-256 is 3ee40b53eb52bcad84fd5a61cbadb6d53866cf6d00907948efe6df3c0c8445fe. The actual A/B decision remains approvals/cod19-approval.md with its original question references.

New branch: `codex/restore-c4-contracts`; isolated worktree `/private/tmp/codebasegraph-restore-c4-contracts-20261118`. PR target is **dev**, approved frozen base **2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9**. The dev comparison necessarily inherits the qualified restoration delta; disclose it and distinguish this issue's document additions. Preserve PR200 and all old branches/PR subjects. No merge, release, installation, service/client/settings change, live graph operation, native YAML acceptance or new authority activation is permitted.

## Ready outcome and constraints

COD-16, COD-17, COD-18 and COD-19 are Done in the existing CodebaseGraph Linear account. Search of canonical Git and Linear found no existing companion preservation record. The restored source has no docs/specs/CBG-C4-01 subtree. The next unmet outcome is making those unchanged contracts, decisions and verified scoped activation available on the restored companion baseline. Their original P0 outcomes are not reopened or renumbered.

Use the actual CBG-C4-01 scoped Git-bootstrap authority. Current effective canonical AGENTS and configured wiki authority wording resolve that selector; unrelated work stays Scryer-governed. No Scryer claim, ledger, sign-off, fold or runtime dependency is needed for this native document preservation. Do not reinstall historical policy or overwrite curated wiki content. Historical pending/proposed language in immutable documents retains its historical meaning alongside later actual decision/activation records.

This issue makes no product change and reads no product source. CLI/MCP/library contracts, placement, dependency direction, failure/recovery behavior, supported platforms and current native backend inputs remain those of the qualified source. The CodebaseGraph MCP tools are unavailable; a no-refresh CLI search at startup returned generation_gate_binding_changed. Context discovery is attempted only in the isolated unconfigured worktree; no live repair, guard bypass or rebuild is authorized. Product source discovery remains a prerequisite for the later technical issue, not evidence supplied by copying documents.

The original checksummed bundle remains 440e5ccfbac9f023c5e2e2a660d356eb5e62e44cb659a212e7d187af3c7eab99; the qualified bundle remains dd9a02f9056b30d75367367635c3a7add9927a75a37a15e19f2c23025f4eeb6a. Startup preservation records cover the 11 original extant worktrees, logical indices, statuses, local authority/configuration and dirty wiki/memory files. One historical Codex turn-diff checkpoint ref was already absent before any Git mutation; its tree f1ab536136cb4eb3ba1cacffc0e08f2d06da1697 and exact original ref are present in the archive. All other 353 historical refs match. Preserve this observed difference and original archive; do not claim all 354 are still live or reconstruct uncaptured bytes. The 75 previously absent worktrees remain a limitation.

## Workflow and exact file ownership

Workflow: **delegated**, one complete technical owner, default/gpt-6.1-terra/medium/fork_turns:none; no nested delegation. Root owns this brief, Git, Linear and final summary/handoff. The technical owner alone may add the following 22 files, copied byte for byte from the exact original source above:

```text
docs/specs/CBG-C4-01/spec.md
docs/specs/CBG-C4-01/proposal-review.md
docs/specs/CBG-C4-01/cod16-contracts.md
docs/specs/CBG-C4-01/cod16-decisions.md
docs/specs/CBG-C4-01/cod16-fixtures.md
docs/specs/CBG-C4-01/cod16-review.md
docs/specs/CBG-C4-01/cod17-baseline.md
docs/specs/CBG-C4-01/cod17-decisions.md
docs/specs/CBG-C4-01/cod17-review.md
docs/specs/CBG-C4-01/cod18-contracts.md
docs/specs/CBG-C4-01/cod18-decisions.md
docs/specs/CBG-C4-01/cod18-fixtures.md
docs/specs/CBG-C4-01/cod18-review.md
docs/specs/CBG-C4-01/cod19-review.md
docs/specs/CBG-C4-01/approval-scope.md
docs/specs/CBG-C4-01/authority-transition.md
docs/specs/CBG-C4-01/approvals/cod19-approval.md
docs/specs/CBG-C4-01/activation/cod19-activation.json
docs/specs/CBG-C4-01/activation/cod19-activation.md
docs/specs/CBG-C4-01/activation/cod19-installation.json
docs/specs/CBG-C4-01/activation/cod19-ledger-closure.json
docs/specs/CBG-C4-01/activation/cod19-preflight.json
```

The same owner writes `docs/specs/CBG-C4-01/execution/cod137-preservation-map.json`: original full SHA/path, Git blob, SHA-256, byte length, mode and unchanged destination path for each file; four approved-subject content bindings; original PR/issue/status mapping. Record destination equality without embedding a containing commit's own SHA. These 23 paths are the complete technical write grant. Root alone may add `docs/specs/CBG-C4-01/execution/cod137-preservation-evidence.md` after execution. No other file, manifest, lockfile, product source, original worktree, archive, generated graph/wiki state or local policy is writable. Workers share the workspace and must preserve all other edits; report conflicts or required ownership/contract changes before proceeding. No Git or tracking mutation by workers.

Commit this brief before the technical owner starts copying. A brief is not a new human approval; every preserved file must stay within the already approved unchanged-content outcome.

## Acceptance and verification

| Criterion | Approved requirement / acceptance | Observable check |
| --- | --- | --- |
| C4P-A01 | REST-R06/R08/R09, REST-A07/A08 | All 22 original documents exist at their original paths with exact Git blobs, bytes and modes; complete original-to-destination map. Four approval-subject files equal their immutable 66dff25 versions. |
| C4P-A02 | REST-R06/R08, REST-A07/A11 | Resolve actual A/B human decisions and verified scoped Git-bootstrap activation, C16/C18/C1/Q1 references, original completed P0 statuses and all remaining native trust/platform/aggregate gates. Porting grants no activation/qualification. |
| C4P-A03 | REST-R07/R08/R10, REST-A08/A09 | Before/after preservation checks show original current refs, 11 worktree HEADs/logical indices/statuses and protected policy/wiki/config/authority bytes unchanged, with the preexisting historical checkpoint-ref difference disclosed; both bundles remain checksum-exact. |
| C4P-A04 | REST-R09/R11/R12, REST-A11/A12 | Only the 22 preserved documents, map, brief and concise evidence summary differ from source6115439. JSON parses, local document links and Git diff whitespace checks pass or accurately retain unchanged historical limitations. No new product test result is claimed. |
| C4P-A05 | REST-R11/R12, REST-A12 | Fresh independent default/Terra/medium/fork-none execution and fresh prompt-only read-only default/Sol/xhigh/fork-none adversarial-review-engineer final review pass on the exact snapshot. Fix all confirmed P0-P3 and recheck affected paths. |
| C4P-A06 | REST-R12, REST-A12 | Published branch/tree/parent/PR and exact document bytes independently match; target dev/inherited restoration contents and current required checks are disclosed. Verified PR handoff only. |

Independent verifier may read all relevant immutable records and execute bounded read-only checks; no product/graph build or original writes. Final Reviewer must use the required adversarial skill and explicitly assess placement, dependencies, shared contracts, callers, replacement cleanup and failure behavior proportionally to a document-only preservation change. Reuse valid source6115439 product evidence only for identical inputs and distinguish new document checks from native execution. No local Rust target is needed. Disk check observed 7 GiB available; cargo-reclaim remains unchanged.

Raw checker/log/session evidence stays in this coordinator's durable artifact directory under `c4-contract-preservation`, outside the PR. Retain failed readiness attempts and their correction. Historical Windows10054 resets and pure-filter LEAK causes remain unexplained; current passing CI does not establish a cause or behavior fix. No minimum-desktop/release qualification follows.

## Failure, recovery and next handoff

Missing/mismatched required document or approval binding stops the affected copy. A changed baseline, semantic contract conflict or required file outside ownership returns to the coordinator and, if material, the maintainer. Delivery failures preserve the new branch and evidence; reconcile remote state before retrying. Rollback means abandoning only this owned candidate, with no live rollback or old-ref mutation.

After acceptance, independent review and publication gates pass, reconcile only COD-137 and its links; COD-136 retains its existing status/closure owner and COD-16–19 stay completed. Assess wiki/memory reuse explicitly; immutable decisions and this preservation map may be sufficient without a curated write. Start a **fresh coordinator** for COD-20 types only, with its own prerequisite checks, source discovery and exact execution brief. COD-21/22/23 and recovered COD-25 each require later fresh coordinators. COD-25 stays WIP before COD-24; no missing provider/Q1/accepted-state work is authorized here.
