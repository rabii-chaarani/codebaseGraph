# COD-110 — candidate grammar and tracked-byte compatibility

## Decision and authority

On 2026-11-01, the user explicitly approved both numbered recommendations with **“i approve your recommendations”**. Engineer recorded the actual decision in [COD-110 comment `2917ddb7-8746-407a-a37d-23b1e72396bf`](https://linear.app/codebasegraph/issue/COD-110#comment-2917ddb7-8746-407a-a37d-23b1e72396bf). The user approved these behaviors before this addendum was versioned; this document records that faithful scope, not a claim that the user reviewed an unborn Git SHA.

This narrow decision addendum supplements **C103-R07/A07 and C103-R08/A08**, retaining R04–R06/A04–A06. The five canonical contracts remain approved at full SHA **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`**: [spec](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/spec.md), [design](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/design.md), [architecture](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/architecture.md), [verification](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/verification.md), and [delivery](https://github.com/rabii-chaarani/codebaseGraph/blob/316be4c4bb81d4f6e025016b12374836b4dcc1a8/docs/specs/COD-103/delivery.md). Preserve their [separate revision0.3 approval](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md).

Detailed internals and ownership remain in Scryer project `/Users/rabii/Projects/Repositories/codebaseGraph`, `node-1` **COD-110 DESIGN / COD-110 EXECUTION**, and current execution ledger **`chg-wzt5hk`**, with Design section9 as the contract index. This addendum is not a parallel specification or copied design. All earlier approvals, failed runs and environment limitations remain intact.

## 1. Strict candidate and final version grammar

Strict release-candidate and final validation **reject leading-zero versions**, including `v01.2.3`. The shared candidate/tag/final core does not preserve the former permissive leading-zero acceptance. Retain the settled strict `X.Y.Z` grammar and the existing classifier's strict grammar; this decision adds no format or metadata-parser policy and no prerelease-format expansion.

Ordinary `check_release_version_sync`, `cargo_version`, `dependency_version`, and native `release_version` accepted behavior remains **unchanged**. Candidate-only restrictions are not imposed on those compatibility paths.

## 2. Verify actual tracked bytes without mutating the real index

A clean checkout with `assume-unchanged` or `skip-worktree` flags is allowed **only after actual tracked bytes are verified**. Flags are not proof of cleanliness. Reject every staged or working tracked change, including changes hidden by either flag and index/worktree changes that cancel each other when compared only against HEAD.

Keep exact expected-SHA/HEAD binding and the existing requirement for all six metadata files to be regular, tracked files. Benign untracked build output remains allowed. Verification must **not mutate or refresh the real index or its flags** to make a checkout appear clean. This is not permission for missing required metadata, partial source, or a new sparse-checkout support promise. A blanket ban on these flags is not the approved behavior.

Preserve the original failed F1 bypass evidence (`013cf`), the corrected functional result (`151b`), and its environment variance as historical evidence with their original limits. This approval is not retroactive approval of a blanket flag ban and does not relabel old failures as passes.

## Acceptance trace — planned, not executed

| Existing criteria | Observable checks required by this decision |
| --- | --- |
| C103-R07/A07; C103-R08/A08 | Strict candidate/tag/final leading-zero negatives; valid ordinary compatibility regression. |
| C103-R07/A07; C103-R08/A08 | Byte-clean flagged and unflagged checkouts at the exact expected SHA pass; benign untracked output remains allowed. |
| C103-R07/A07; C103-R08/A08 | Dirty index, dirty worktree, index/worktree cancellation and flag-hidden tracked changes fail, without changing the original real index bytes or flags. |

Actual test identifiers and executed evidence will be supplied by their assigned owners; none is invented or reported passing here. This addendum does not approve implementation evidence or source already present in a dirty worktree.

## Unchanged gates and scope

No new Scryer claims, topology, dependencies, settings, permissions, targets, queue items, endpoint or closure ownership are introduced. Preserve the approved ordered queue, one active issue, file ownership, existing ordinary compatibility, independent verification, integrated final review/fix checks, hosted proof and later integration/release/publication gates. Production persists only this decision and its separate approval record; dependent source work still requires Engineer's bounded assignment. The original five contracts, Scryer carrier/history/sign-off/maps/verdicts and prior evidence are not rewritten.
