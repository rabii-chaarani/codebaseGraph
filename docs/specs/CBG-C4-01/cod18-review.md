# COD-18 documentation review and evidence

## Identity and scope

- Work: `CBG-C4-01/F01/S04` / [COD-18](https://linear.app/codebasegraph/issue/COD-18).
- Date/environment: 6 November 2026, Australia/Darwin; Darwin arm64 documentation worktree `/private/tmp/codebasegraph-cod18-trust-contract-20261106`, branch `codex/cod-18-trust-contract`.
- Documentation diff baseline: `0c49902ba7e12e10a8a93ba8540c2be0527d7d47`, COD-16 PR #185 final handoff. Target dev observation: `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`. Product/source observation: unchanged main `90e2ab6fae2767bdb158e19f0072d43568e80dbc`.
- Documents: [contract revision 0.1](cod18-contracts.md), [decisions](cod18-decisions.md), [40 planned cases](cod18-fixtures.md), this review record. All nine inherited COD-16/17 documents are to remain byte-identical to the diff baseline.
- Scope: P0 documentation/contract proposal. No product implementation, live workflow/protection changes, hosted qualification, native-authority transition, merge/release, issue closure or next-issue advancement.

## Read-only handoff and platform inspection executed

The coordinator read repository AGENTS/RTK; searched advisory local memory (no relevant registry hit); recalled k-wiki memory and searched/read curated architecture context; read CodebaseGraph Linear COD-18 and the relevant COD-16/19/37/46/47 contracts; fetched Scryer governing rules/orientation and semantic graph search/context. Available graph/model describe the current product and do not index the stacked new worktree documents; immutable Git documents supply their actual content. No other project's tools/state were substituted.

Connected GitHub readback confirmed PR #185 open/unmerged at `0c49902ba7e12e10a8a93ba8540c2be0527d7d47` and PR #184 open/unmerged at `f67e1d95862484dee34c8e818c98b72959366c5b`, both targeting dev. All four COD-16 files and historical spec were fetched at immutable full revisions. Python SHA-256 and Git blob identity checks matched the handed-off values and local evidence worktree bytes. Canonical source and both earlier worktrees were clean at their handed-off heads; they were preserved.

GitHub repository/ruleset/branch/workflow/check APIs were inspected read-only. The connector could not access classic branch protection (403) or its unsupported effective-rules URL. After ordinary shell network failure, authorized read-only `gh api` calls inspected the same repository: classic `main`/`dev` protection GETs returned 404; `rules/branches/dev` returned active ruleset `26100127`. This distinguishes classic endpoint results from active ruleset protection. No setting was changed.

Observed ruleset: default/dev coverage, no bypass actors returned, zero approving reviews, stale-review dismissal/last-push approval disabled, strict `required` check bound to integration ID `15368`, PR/linear-history/code-quality rules, squash/rebase allowed. Current CI blob is identical on source main and target dev and excludes dev PRs. Exact COD-16 head returned zero check runs and zero PR-triggered workflow runs. These are configuration/readback observations, not green CI or hosted enforcement demonstrations.

Current primary GitHub documentation was read for self-review prohibition, review identity/state, manual workflow/rerun semantics, required-check event/skipping behavior, merge API head guard and merge-queue availability. References are beside their claims in the contract. No live acceptance or bypass probe ran.

## Documentation validation

Executed with Python 3 in the Darwin arm64 documentation worktree:

```text
python3 -B /private/tmp/codebasegraph-cod18-checks-20261106/validate_docs.py /private/tmp/codebasegraph-cod18-trust-contract-20261106
```

Validator SHA-256: `d8085f02489b1e61cd74725cf1e1171ae82f1807bfc567b65184897639227cfa`. The temporary script is retained for review; its availability is not product evidence. It reads Git baseline blobs and current files, confines changed/untracked paths to the four documents, runs per-file `git diff --no-index --check`, verifies local links/fences/table widths and trace identifiers, and calculates the two illustrative race/composition examples.

| Executed static check | Result and limit |
| --- | --- |
| Historical identity / diff scope | PASS: nine inherited files byte-identical; exactly four new COD-18 documents |
| Whitespace / Markdown structure | PASS: final newlines, no trailing whitespace, balanced fences and consistent table columns |
| Local links / trace identifiers | PASS: 13 local links; C18-01-10, C18-D1-D5, 40 C18-F01-F40 definitions and required acceptance/review identifiers |
| Synthetic arithmetic | PASS: improvement 10 to 8 makes candidate 9 deteriorate by 1; separately acyclic A-to-B/B-to-A changes form a combined cycle |

These checks validate document structure and illustrative arithmetic only. They do not execute an extension compiler, evaluator, approval protocol or protected path. Trace/fixture counts are not semantic proof; the independent review remains separate.

No Rust build or product test is needed for this documentation-only diff, and none is claimed. No Rust target was created or redirected. The 40 cases and inherited 48 C16 fixtures remain planned definitions.

## Independent persisted-diff review

Fresh read-only `/root/cod18_final_review`, native `collaboration.spawn_agent`, `agent_type: default`, `model: gpt-6.1-sol`, `reasoning_effort: xhigh`, `fork_turns: none`, applied [adversarial-review-engineer](/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md) and its engineering lenses. User-provided AGENTS settings override the skill's Max default. Frozen reviewed commit: `c9dca03063846a2eef5f5c2bce30e75df1c6b39a`; diff baseline: `0c49902ba7e12e10a8a93ba8540c2be0527d7d47`.

Review scope: decision fidelity and explicit D11 amendment; bootstrap/update isolation; exact subject and manual action authenticity; producer/control closure; state/composition/waiver/cancellation/replay; current platform limitations; C16/linked-issue impact; placement/dependency/public-contract/caller implications; and truthfulness of evidence/qualification limits.

**Returned verdict: no confirmed P0-P3 defects; no semantic correction requested.** User handoff and a documentation PR are supported within the stated planning boundary after recording this verdict and focused verification of evidence-only additions. This is not product implementation approval or hosted qualification.

| Frozen reviewed input | SHA-256 |
| --- | --- |
| cod18-contracts.md | `bf57465a7062880f1075201261f20551adf58c502a7c16b358d9f56e77c03823` |
| cod18-decisions.md | `0c4ab2414be0ce7ed84a42a87819febe61b87778332b3ee936873db45aedf8d8` |
| cod18-fixtures.md | `cd1eb5af7805a7dc6fd4ff278d9932117aa68cd3433b75e5d6d1d07a6a9c30ae` |
| cod18-review.md before this verdict | `32f10b4f8a0093a915c3f3e8266a6b02327de4616573ecf7826765fe9ad58d68` |

The reviewer independently inspected/reran the validator (PASS), matched the script and all four file hashes, confirmed all nine inherited blobs and historical canonical revision 0.2, ran `git diff --check` and verified the clean frozen snapshot. The reviewer read current primary GitHub documentation and relevant inherited contracts; no source/product/hosted execution or writes occurred.

The review retained three consequential conditional qualification risks: the actual producer-to-protected-path connection and authoritative final transition are unproved; authenticated account provenance alone does not establish the explicit human decision/credential boundary; and concrete approved executable/configuration pins plus withdrawal/expiry race enforcement remain prerequisites. The contract's stop gate correctly retains unsupported/UNKNOWN acceptance until COD-47 proof. These were not reported as demonstrated document contradictions.

Final evidence-only additions require focused verification against the unchanged semantic document hashes above. Exact publication head, PR and remote readback results are recorded in the COD-18 handoff; publication is not inferred from this local review.

## Reconciliation and retained gates

Scryer proportional documentation ledger `chg-ymqbv8` was closed empty with its rationale retained and no claim/link entries or folds. Current orientation also returns unrelated pending work, drift and stale anchors; this documentation work does not adopt, reconcile or declare them fixed. The repository's native-authority transition remains COD-19.

COD-18 tracking must distinguish selected planning decisions/documentation progress from product implementation approval and runtime trust activation. COD-19 must reconcile/publish the changed canonical revision and full-SHA exact-scope human implementation approval. COD-37/46/47 retain their implementation/hosted proof gates; COD-25/F04, COD-40 and D17 retain their own qualification/calibration work. Maintainers retain aggregate closure.

The coordinator also read the current scope/acceptance contracts of COD-33/36/41/42/43/44/45 to check the linked state, waiver, evidence, operation, transport and report impact. Those records were not changed or advanced. Their existing expected-state, expiry, recomputation, explicit-retry, shared-semantics and one-table obligations are retained by this proposal.

Reusable knowledge assessed: the policy decisions/rationale are retained in this proposal and decision record. No filler wiki/memory write is warranted. The independent reviewer reached the same assessment. The selected hosted path stays unqualified; no product build/test or live acceptance evidence is implied.
