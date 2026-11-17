# COD-137 preservation evidence

18 November 2026 (Australia/Darwin), `CBG-RESTORE-01/C4-P0` / [COD-137](https://linear.app/codebasegraph/issue/COD-137). Read the [committed brief](cod137-preservation.md) and [exact preservation map](cod137-preservation-map.json). This record reports document preservation under existing approval. Original native C4 P0 closures and later implementation/qualification gates retain their owners.

## Exact inputs and implemented outcome

The [restoration specification 0.1 at d809fe0f8289bbf7d167a891bd7c31160f27f0e1](https://github.com/rabii-chaarani/codebaseGraph/blob/d809fe0f8289bbf7d167a891bd7c31160f27f0e1/docs/specs/CBG-RESTORE-01/spec.md) and [actual decision 663b2fc8d2790956975587dcab6b248bd40b5958](https://github.com/rabii-chaarani/codebaseGraph/blob/663b2fc8d2790956975587dcab6b248bd40b5958/docs/specs/CBG-RESTORE-01/approval.md) authorize this separate companion outcome. No approved contract bytes were revised.

| Item | Exact identity / result |
| --- | --- |
| Qualified source baseline | 6115439262005d1de984c5837e88f057c7317205, tree b2a56029eacc507a5b8f0a8a79207f73fe99e9eb, open/unmerged [PR200](https://github.com/rabii-chaarani/codebaseGraph/pull/200) |
| Original documents | ba5c7c543bd711b663e1dfc92d4bd58497ffb557, original open/unmerged [PR187](https://github.com/rabii-chaarani/codebaseGraph/pull/187) |
| Preserved native approval subject | 66dff2564e76ad06cdf172332e2da318d150fc73, specification 0.3 and original A/B scopes/actual decisions |
| Brief committed before copying | 1908a325f31947c0505fed8ea1e99bab3a697d0b |
| Independently executed preservation snapshot | 115003368e6780da658b77ba2e6247dcd9a87848, tree b7036773a4b80086dd21ee4adc9c52c8ca1db2cb |
| Authored delta at that snapshot | 24 added files: 22 unchanged original documents, map and brief; no product, test, build, workflow, policy or wiki edits |
| Map SHA-256 | 66c07d1af5cb98e828e39b23ae61b32757211ae3d2d80efc52dcbb42ae6d1e6b |

All original paths, Git blobs, file modes, bytes and approval references are preserved. The selected C16/C18/C1/Q1 contracts and verified scoped Git-bootstrap activation remain the same immutable records. Historical proposed/pending statements retain their original checkpoint meaning; later actual approval and activation records identify the approved state. Copying those records neither reinstalls policy nor accepts a native YAML model.

## Executed acceptance evidence

One complete technical owner `/root/cod137_preserve`, default/gpt-6.1-terra/medium/fork-none, copied only its 23 assigned paths and returned without Git/tracking/model writes. Coordinator inspection confirmed the exact allowed file set and each original-content equality before committing. Independent execution was performed by fresh `/root/cod137_verify`, default/gpt-6.1-terra/medium/fork-none, against snapshot1150033 above.

| Criterion | Actual independent method and result |
| --- | --- |
| C4P-A01 | Python/hashlib with Git show, ls-tree and rev-parse compared all 22 unique map entries: original/destination path, bytes, blob, SHA-256, length and mode. Zero errors. All four approval-subject files equal immutable66dff25; connected GitHub fetched equal remote content at66dff25 andba5. PASS. |
| C4P-A02 | Read actual REST and native decisions, verified activation, current effective AGENTS selector and retained trust/Q1/platform/release/aggregate gates. Connected CodebaseGraph Linear independently returned COD-16/17/18/19 Done; connected GitHub returned PR187ba5 and PR200611 open/unmerged, both targetingdev. PASS. |
| C4P-A03 | Inspected and ran `GIT_OPTIONAL_LOCKS=0 python3 -B preservation_check.py verify` against the recorded startup baseline. All 11 original worktree HEADs/logical indices/statuses, 26 protected files and both pinned archive checksums matched; the owned new branch was the only new ref. The historical checkpoint-ref difference below remained unchanged. PASS within that disclosed scope. |
| C4P-A04 | Git diff name/status exactly matched 24 additions; no product/build/test input changed. Git diff whitespace check passed. All five JSON documents parsed. Relative-file-link scan resolved59 repository-relative links. The brief commit precedes the preservation commit. PASS with link-check limits below. |
| C4P-A05 | Independent execution above passed. Fresh adversarial final review is a separate required gate; its outcome is recorded below when actually executed. |
| C4P-A06 | Publication readback and current required CI remain separate delivery gates. Their exact head/tree/PR/results belong in linked PR/Linear and the final coordinator handoff; this pre-publication execution checkpoint asserts no future result. |

This evidence summary is an additional coordinator-owned document after1150033; it does not alter the 22 preserved inputs or map. Final review must include this summary and bind any later metadata-only rebind to its actual snapshot.

## Preservation and limitations

The original bundle remains SHA-256440e5ccfbac9f023c5e2e2a660d356eb5e62e44cb659a212e7d187af3c7eab99 and the qualified-source bundle remains dd9a02f9056b30d75367367635c3a7add9927a75a37a15e19f2c23025f4eeb6a. At this coordinator's startup, 353 of354 historical refs matched live refs. One old Codex turn-diff checkpoint ref was already absent before any Git mutation; its tree f1ab536136cb4eb3ba1cacffc0e08f2d06da1697 and exact original ref remain in the archive. The task preserved that startup state and did not rewrite the old ref. The original archive still retains all354 captured refs. Seventy-five old worktrees were already missing; no uncaptured former bytes are reconstructed.

Failed readiness attempts are retained: unavailable Python `hashlib.file_digest`; strict historical-ref comparison detecting the preexisting absent checkpoint; and an incorrect assertion that this checkpoint object was a commit when it was a tree. The corrected checker streams SHA-256, verifies actual object type/archive ref, records the difference and compares later state against the disclosed startup snapshot. Failed attempts are not passing evidence.

The Markdown scan is a regex file-target check, not a complete parser. Five fragments, 83 remote URLs and six unchanged historical absolute skill links were outside that check. Remote approval-subject equality and PR head/state were verified separately as described above.

Graph MCP tools were unavailable. Startup no-refresh CLI search returned generation_gate_binding_changed; explicit change-impact context in the owned unconfigured worktree returned missing config. No product source was read or changed, and no live graph repair, bypass, rebuild or successful graph query supplied this document-only result. Later code work still needs its own graph/source discovery.

No local Rust build, product test or native qualification ran. No Rust target or cleaner was created. The baseline source's CI845/run44114765768 passed25/25jobs and its merge-test tree equals its source tree; that existing evidence applies only to its identical inputs. Current companion checks must be reported for their actual head. Historical Windows10054 resets and pure-filter LEAK causes remain unexplained; later passing CI is not causal proof or a behavior fix. Minimum-desktop, release, first-model trust, Q1/provider and hosted accepted-state qualification remain separate.

## Review and handoff state

At this execution checkpoint, independent execution is complete and fresh integrated adversarial final review/publication are pending. No confirmed defect or approval is inferred from a commit or issue state. The required final reviewer is fresh, prompt-only, read-only default/gpt-6.1-sol/xhigh/fork-none using adversarial-review-engineer; repository settings take precedence over its generic Max default. All confirmed P0-P3 must be fixed and affected paths reverified before PR handoff.

Raw receipts/checkers and failed attempts are retained in this coordinator's durable `c4-contract-preservation` artifact directory. The concise criterion trace here is the Git-tracked evidence; external evidence identities are not relocated or rewritten.

Wiki/memory assessment: this task changes document availability, not approved intent, terminology or a reusable runbook. Existing canonical decisions/activation and the exact preservation map already retain the durable facts. No curated wiki or memory write is warranted. Original dirty wiki/memory work stays in its owning checkout and archive.

Endpoint is a verified PR todev with inherited restoration contents disclosed, without merge/release/live activation. COD-137 alone records this destination preservation; COD-16–19 remain Done and COD-136 retains its existing owner/status. After handoff, a fresh coordinator assesses only COD-20 types under its own original prerequisites and bounded brief. COD-21/22/23/recovered25 remain later issues; COD-25 stays WIP before COD-24.
