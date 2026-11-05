# COD-56 — revision0.4 qualification-method approval linkage

## Frozen content and actual authority

This separate record binds the user's **already-approved described COD-56 qualification method** to its faithful frozen Git content. It does not claim that the user reviewed an unseen containing commit, invent an earlier full-SHA approval, ask for the same approval again, or assert executed/passing results.

**Logical revision:** COD-103 revision **0.4**, COD-56 / external CBG-PLAT-01 S05, R05 → A09 environment and test-invocation method **only**. **Full method-freeze Git SHA:** **`653177127041caed55b6628ff3a596a013ef0cd4`**. The four approved-scope frozen paths and SHA256 values are:

| Frozen path | SHA256 |
| --- | --- |
| [`docs/specs/COD-103/spec.md`](https://github.com/rabii-chaarani/codebaseGraph/blob/653177127041caed55b6628ff3a596a013ef0cd4/docs/specs/COD-103/spec.md) | `99a081d33c5240332d803d46dfe9ea7f074dd38a7568a820b30b862cde0a1597` |
| [`docs/specs/COD-103/verification.md`](https://github.com/rabii-chaarani/codebaseGraph/blob/653177127041caed55b6628ff3a596a013ef0cd4/docs/specs/COD-103/verification.md) | `1da3e51dc3974442dd8c11f17b8fba684852f8022cea68b8ea857d06d17c02fa` |
| [`docs/specs/COD-103/delivery.md`](https://github.com/rabii-chaarani/codebaseGraph/blob/653177127041caed55b6628ff3a596a013ef0cd4/docs/specs/COD-103/delivery.md) | `982f8045ef0325628e78f2109b374b67c8fd1711f462dfca84d09dedf23c8d2f` |
| [`docs/specs/COD-103/decisions/2026-10-31-cod56-emulated-qualification.md`](https://github.com/rabii-chaarani/codebaseGraph/blob/653177127041caed55b6628ff3a596a013ef0cd4/docs/specs/COD-103/decisions/2026-10-31-cod56-emulated-qualification.md) | `ed9940badc1e8e290fe7a551af93d7af4f75a3b8edc6c659c87e945d3d57f8f4` |

Freeze tree: `1dfe9c1aacd83ea7efd22dc6da21397db5d48049`. These exact bytes were supplied by Requirements and accepted by Engineer as the faithful narrow method record. The full SHA above identifies the reviewed method documents, **not this separate approval record's containing commit**.

## Actual user decisions

1. **Environment sufficiency:** user **“do the native windows 11 x64 test matrix using parallels pro using the emulated x64. consider those tests enough”**; [COD-56 decision `abf699e8-05cb-4db1-b88b-a404784334dd`](https://linear.app/codebasegraph/issue/COD-56#comment-abf699e8-05cb-4db1-b88b-a404784334dd).
2. **Test-only exception:** user **“i approve the test-only architecture-check exception”**; [COD-56 decision `f05d6f4f-4715-4c01-a8e8-736db517fb9f`](https://linear.app/codebasegraph/issue/COD-56#comment-f05d6f4f-4715-4c01-a8e8-736db517fb9f), cross-recorded in [COD-103 `7b9f1fef-6ffc-47bc-aed5-e6513503cdf5`](https://linear.app/codebasegraph/issue/COD-103#comment-7b9f1fef-6ffc-47bc-aed5-e6513503cdf5). The answered question explicitly included **test-copy-only OSArchitecture rejection**, preserving the Windows check and shipped script, plus **fresh-child memory invocation where unchanged file policy blocks `-File`**, with **no execution-policy change**. That described method was approved; no new approval question remains.

Production records these exact decisions relayed and accepted by Engineer. This is approval linkage, not user approval supplied by a commit, status, reviewer or test result.

## Exact scope and limits

- Sufficient qualification environment: actual Parallels Windows11 **OSArm64**, built-in Windows PowerShell5.1 **ARM64**, supported official PowerShell7 **X64-emulated** and real native **AMD64 PE** fixtures. Record actual OS/process/shell versions and architectures; do not call this native OSx64 or WinPS5.1x64, and do not expand product support to ARM64.
- Only negate/remove the **OSArchitecture rejection predicate in a disclosed derived test copy**. Retain Win32NT and all checksum, probe-argument/order, immediate-exit, both-validations-before-replacement, replacement and cleanup bytes/semantics. Measure exact original/derived/delta hashes; do not invent derived identities. Preserve original intact-guard negatives separately. The shipped corrected installer remains blob `9890b016699948634200624e85b868616c14617e`, LF SHA256 `830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb`; its guard is unchanged.
- Each case uses a **fresh child PowerShell process**. Where existing policy blocks File invocation, explicitly bound memory ScriptBlock invocation is accepted for this S05/A09 logic qualification. It is **not `-File` contract coverage or byte-exact-original execution**. No global policy/`-ExecutionPolicy Bypass`, preferences, VM/account/settings/credential or product-surface change is approved.
- All **seven scenarios × two shells × DryRun/real =28 actual cases** remain mandatory: real checksum-valid AMD64 fixtures, original graph `--help`/wiki `--version`, actual exits0/23/29, independently failing probes, missing/checksum-negative controls, mode-specific success and **both** old/new destination hashes. Setup/report/missing/unrun/skipped cases fail closed; genuine assertion-derived JUnit/raw evidence and independent acceptance required. Earlier20pass/8fail baseline and limited VM supplement remain unchanged, not relabelled28/28.
- Passing independently accepted **checkout/test-copy28 qualification plus all other applicable staging gates** permits COD56→COD107 autonomously under the approved ordered stack. Final source-bound **publishable archive identity/provenance and applicable runtime qualification remain at the later release stage**; unavailable PR native archive uploads are not a new staging gate. No passing matrix, Done, main integration, release or aggregate closure is asserted here.

## Preserved approvals, contracts and gates

Prior COD-103 **revision0.3 five contracts** remain approved at full SHA **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`**, with [separate original approval](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md), user “i already approved revision 0.3”. This record supplements only COD-56's described method and does not silently advance that earlier approval to unrelated later content.

External **CBG-PLAT-01 revision0.11 S05/R05/A09**, canonical [original path/full SHA `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer), remains absent from this tree and is not restored, copied, migrated, renumbered or replaced. Preserve its original approval and exact COD-51 S05 sign-off linkage, COD-61 blocker and original maintainer/user closure owners.

All other requirements/criteria, design/architecture contracts, supported targets, fixed **105→106→56→107→108→110** queue, selected complete-release contents, prior evidence, COD-99 history and COD-104/admin-bypass configuration remain unchanged. No partial release or scope expansion. Current-source SAME-model anchors/tests/fingerprint-bound reconciliation, independent verification/fresh implemented review of actual later harness/fixtures, applicable hosted checks, non-bypass strict-freshness serial main integration/full own-merge-SHA CI, complete accepted queue/release/publication/independent public verification and aggregate closure gates remain intact.

Related method, approval, later implementation and evidence stay on **PR173**. This record authorizes no source/harness edit or execution by Production and performs no tests/VM/model/Linear/sign-off/fold/lifecycle action. Engineer separately assigns ready technical ownership after the detailed design/Scryer brief is ready. No new approval or review stage is created by faithfully recording these settled decisions.
