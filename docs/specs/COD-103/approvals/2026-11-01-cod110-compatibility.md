# COD-110 — compatibility decision approval record

## Actual approval and frozen decision identity

- **Decision addendum:** `docs/specs/COD-103/decisions/2026-11-01-cod110-compatibility.md`.
- **Full containing decision commit SHA:** **`49612ec66814ad6b883a0c4c3c624911bd5a8825`**.
- **Immutable decision file:** [approved compatibility scope](https://github.com/rabii-chaarani/codebaseGraph/blob/49612ec66814ad6b883a0c4c3c624911bd5a8825/docs/specs/COD-103/decisions/2026-11-01-cod110-compatibility.md).
- **Actual user approval:** on **2026-11-01**, in reply to Engineer's two numbered recommendations, the user stated **“i approve your recommendations”**.
- **Decision reference:** [COD-110 comment `2917ddb7-8746-407a-a37d-23b1e72396bf`](https://linear.app/codebasegraph/issue/COD-110#comment-2917ddb7-8746-407a-a37d-23b1e72396bf).

The user explicitly approved both behaviors **before versioning**. Production froze the faithful decision addendum first, then created this separate confirmation on 2026-11-01. This records the already-selected scope at its exact committed identity; it does **not** claim that the user reviewed an unborn SHA or that a commit itself supplies approval. This record does not contain its own containing commit SHA.

## Exact narrow scope

1. Strict release-candidate/tag/final validation rejects leading-zero versions such as `v01.2.3`, retaining the settled strict `X.Y.Z` grammar without prerelease-format expansion. Ordinary `check_release_version_sync`, `cargo_version`, `dependency_version`, and native `release_version` accepted behavior stays unchanged. No candidate-only restriction is imposed on ordinary compatibility callers.
2. Clean `assume-unchanged` / `skip-worktree` checkouts are allowed only after actual tracked bytes are verified. Reject staged or working tracked changes, index/worktree cancellation and flag-hidden changes. Do not mutate or refresh the real index or its flags to produce a clean result. Retain benign untracked build output, exact expected-SHA/HEAD binding, and six regular tracked required metadata files. No blanket flag ban, missing-metadata waiver, partial-source or new sparse-checkout support promise is approved.

These are only the addendum's C103-R07/A07 and C103-R08/A08 clarifications. Planned checks cover strict candidate negatives and ordinary compatibility, clean flagged/unflagged exact-source positives, dirty/cancelled/hidden tracked negatives without real-index mutation, and the untracked positive. Actual test identifiers and passing evidence remain with their assigned owners, not this approval record. Preserve the original failed F1 evidence (`013cf`), corrected functional evidence (`151b`) and environment limits; this is not retroactive approval of a blanket flag ban or of existing dirty implementation files.

## Preserved authority and gates

The five canonical contracts retain full approved SHA **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`** and their [separate approval record at `0e6008a1f46679a86aba85ac513beb05af94b0c0`](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md). This does not advance those approvals to a newer specification revision, replace frozen contract bytes, or duplicate a wiki specification. Design section9 and Scryer project `/Users/rabii/Projects/Repositories/codebaseGraph`, `node-1` COD-110 DESIGN / EXECUTION, remain authoritative for detailed internals and ownership; current execution ledger is **`chg-wzt5hk`**.

Retain R04–R06/A04–A06, the approved queue and endpoint, all earlier SHA approvals, evidence and failed runs, ordinary compatibility, two native targets, permissions, independent verification, integrated final review/fix checks, hosted proof, serial non-bypass integration/own-main CI, release/publication gates and original closure owners. No new claim, topology, dependency, setting, permission, target or scope is authorized. No Scryer carrier/history/sign-off/map/verdict is rewritten. Production's two documentation commits and normal branch push provide immutable remote decision availability only, not implementation acceptance, a PR handoff, merge or release authority. Engineer must separately assign dependent implementation; future Production workflow consumers still wait for the checked Rust contract.
