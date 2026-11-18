# COD-17: API and resolution-scope decisions

## Decision record

- Work: `CBG-C4-01/F01/S03` / [COD-17](https://linear.app/codebasegraph/issue/COD-17/cbg-c4-01f01s03-supported-pilot-baseline-governed-scope-and-resolution).
- Date: 6 November 2026, Australia/Darwin.
- Authority: the user's explicit answers in chat `01a1af8a-cec1-7d00-8a1d-7b84cf45d10c`, question call `call_39f88c66a9754562b3c34464c8d56e81`, items 0 and 1.
- Reviewed proposal: [COD-17 baseline at e84fdd91b3d9a0b50d2c4e081f0de4b0879e3925](https://github.com/rabii-chaarani/codebaseGraph/blob/e84fdd91b3d9a0b50d2c4e081f0de4b0879e3925/docs/specs/CBG-C4-01/cod17-baseline.md), SHA-256 `9fc7ab3c651dfb43168b741559cda8757988b96b0112883dc5008aa8e4af4177`.
- Source context: pinned 4.2.1 baseline `90e2ab6fae2767bdb158e19f0072d43568e80dbc`.

| Decision | User's selected answer | Consequence |
| --- | --- | --- |
| API direction | **C1 — Compatible redesign:** preserve existing Rust callers and add a separate architecture request family through shared internal dispatch. | Keep existing public request variants, field shapes, trait obligations and paths. Design the additive family and common private dispatch/catalog in COD-16. C2 is retained as a considered alternative, not selected work. |
| Initial resolution scope | **Q1 — Qualified pilot subset:** scoped imports and direct references; prove pilot sufficiency before calibration. | Qualify the bounded observation domain described in the reviewed proposal. Unsupported evidence remains `UNKNOWN` and blocks dependent mandatory checks. Q2/Q3 are alternatives requiring a new scope decision if Q1 is insufficient. |

The questions explicitly distinguished proposal selection from implementation authorisation. These answers select C1 and Q1; they do not approve product implementation, a new numeric budget, complete dependency coverage, an accepted native model or hosted enforcement.

## Preserved gates and handoff

The canonical CBG-C4-01 revision 0.2 proposal and historical review remain unchanged. This record resolves the two choices requested by the COD-17 supplement; it does not rewrite the historical proposal or silently advance an approved revision.

COD-16 must settle concrete shared contracts, signatures, validation, errors, bounds, compatibility tests and applicable Scryer intent before coding. COD-18 must establish reviewer/verifier trust and the protected integration-path contract. COD-19 must publish/pin the final contracts and record exact-scope human implementation approval. COD-25/F04 must demonstrate Q1 endpoint and coverage sufficiency before projection acceptance or numerical calibration; failure returns to the user with evidence rather than broadening scope automatically.

No architecture responsibilities, links or implementation claims are folded by these choices. There is no Scryer runtime dependency in the product and no implicit approval of the repository-authority transition. Maintainer aggregate closure, supported-platform qualification, merge and release gates remain separate.

The baseline and its independent review remain immutable historical inputs. Read this decision record alongside them: their previously open C1/C2 and Q1/Q2/Q3 choices are now resolved to C1 and Q1. The next sub-issue must start with a fresh coordinator session after COD-17's review/handoff gates are satisfied.
