# COD-56 — approved emulated-x64 qualification method (2026-10-31)

## Identity and authority

This is a **COD-103 revision0.4 qualification-method amendment only**, supplementing **COD-56 / CBG-PLAT-01 S05, R05 → A09**. It creates no work, requirement, acceptance or architectural-claim ID. Original canonical [CBG-PLAT-01 spec rev0.11](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s05--reject-failing-native-probes-in-the-powershell-installer), full SHA **`8a410c7a22f5291f06a1cece5cd9e7c0be255e72`**, retains its path, identity and approval. It is absent from the current PR tree; this decision does not restore, copy, migrate or replace it.

COD-103 revision0.3 and all five contracts remain approved at **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`**, [separate approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md), user **“i already approved revision 0.3”**, [decision](https://linear.app/codebasegraph/issue/COD-103#comment-c8690556-e836-428d-bc81-079c701d2395). This supplement neither advances that approval link nor reopens unchanged contracts, the six-child queue or selected release contents.

## Actual user decisions — settled

1. **Environment:** “do the native windows 11 x64 test matrix using parallels pro using the emulated x64. consider those tests enough”. [Recorded decision](https://linear.app/codebasegraph/issue/COD-56#comment-abf699e8-05cb-4db1-b88b-a404784334dd). Parallels Windows11 **OSArm64** with actual x64 PE emulation is sufficient for this COD-56 matrix. Do not require a different native-x64 host or re-ask sufficiency.
2. **Test-only exception:** “i approve the test-only architecture-check exception”. [Recorded decision](https://linear.app/codebasegraph/issue/COD-56#comment-f05d6f4f-4715-4c01-a8e8-736db517fb9f). The answered question explicitly included a disclosed architecture-only derived test copy and fresh-child in-memory invocation where unchanged file policy blocks `-File`, without changing the shipped installer or execution policy. This settles both method choices; no further requirements question remains.

These decisions approve the described method, not unexecuted results. Production must commit the reviewable amended contracts and this decision, then persist a **separate** approval record identifying their paths/full committed SHA, narrow scope and actual decision references before dependent test implementation. Do not ask the user to approve the same settled scope again. No document embeds its own containing commit SHA.

## Exact scope and unchanged behavior

The unchanged installer rejects this guest at its OSArchitecture guard before probes/copying. Permit **only negating or removing the OSArchitecture rejection predicate in a derived test copy**. Retain the **Win32NT** check and all other installer bytes/semantics: checksums, immediate native-exit checks, original graph `--help` / wiki `--version` arguments and order, both validations before replacement, replacement and cleanup. No public parameter, native-command preference or product guard change; no atomic two-binary rollback redesign.

The shipped corrected script remains blob **`9890b016699948634200624e85b868616c14617e`**, LF SHA256 **`830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb`** at PR173 checkout head **`456483fbde6549f16dfb74759d0899763066bfee`**, tree **`6e030dae6bf614ef45e6799ddbb55852ad102473`**. Preserve baseline blob **`05950595ee08f9e277bea432faa90410925509fa`** at **`2173d766c155583b1e8f979c303b560998cc7214:release/install/install.ps1`**, SHA256 **`bdd6f0eace6fbd4ea25c9e09d2c6cb6c0b54a05955d024aa97d76443c58ae308`** (3086bytes; corrected original3248bytes). Derived hashes remain pending actual transformation and must not be guessed. Before executing each derived baseline/corrected copy, prove its exact isolated guard delta against the corresponding preserved original and record original/derived hashes. A derived copy is **not byte-identical original execution**. Preserve intact-original guard negatives separately; they neither qualify the probe/install matrix nor inflate its28 count.

Actual shell families are built-in Windows PowerShell5.1 **ARM64** and official supported PowerShell7 **X64-emulated**. Record exact versions, OS edition/release/build and OS/process architecture. Never label Windows PowerShell5.1 as x64 or this OS as nativeX64. This is test-method sufficiency for x64 fixture logic, **not ARM64 product support**.

Run each case in a **fresh child PowerShell process**. Where existing policy blocks `-File`, the child may read derived bytes into a memory ScriptBlock with explicit SourceDir/BinDir and actual DryRun mode. Capture the actual child outcome. The user accepts this invocation for S05/A09 logic qualification; it does **not** qualify the `-File` contract. No global execution-policy change, `-ExecutionPolicy Bypass`, VM/power/account/credential/settings change or new product surface is authorized.

## Acceptance and evidence — planned, not passed

The method changes; A09 observable behavior and identities do not. Execute the existing **7 scenarios × 2 shell families × DryRun/real = 28 actual cases**. Preserve all success, independently failing graph/wiki probe (including wiki failure after graph success), missing/corrupt/checksum cases and their mode-specific results. Use real native **AMD64 PE** controlled-exit fixtures, not mocked calls or `.cmd` substitutes, with recorded source/compiler/PE/hash identity, captured original argv and actual success/nonzero exits **23/29**. For checksum-negative controls, corrupt the expected digest/manifest, not the PE fixture.

- **Given** a valid-checksum candidate whose selected probe exits nonzero, **when** either installation mode runs, **then** the fresh installer child fails before replacement and **both** existing destination hashes remain unchanged.
- **Given** successful probes for both candidates, **when** the mode runs, **then** its intended DryRun or installation outcome occurs; record both destination hashes before/after, including successful replacements for real install.
- **Given** missing/corrupt/checksum-invalid input, **when** validation runs, **then** it fails before replacement with both existing destinations preserved.
- **Given** a setup/assertion/report failure, missing case or unrun/skipped row, **then** qualification fails closed; absence of evidence is not a pass.

Retain fresh child status/stdout/stderr, true native exits, both old/new destination hashes, source/derived/fixture identities and transferable raw evidence. JUnit must derive from actual case assertions, not hand-written passing cases or loose log strings. Independently verify complete28-case coverage/results and applicable source/test provenance. Preserve the earlier **20-pass/8-fail baseline**, policy failures and limited VM supplement as recorded; do not relabel them corrected28/28. No execution or passing result is asserted by this amendment.

## Stage and linked-issue impact

- **COD-56:** independent acceptance of the full checkout/test-copy matrix plus other applicable source/review/hosted staging gates permits **COD-56→COD-107** under the already approved stack-staging/autonomous queue. No extra user go-ahead is required. Source changes invalidate affected evidence as usual.
- **COD-103:** update only its external-A09 environment/method references and this decision. All other requirements/criteria, companion design/architecture, queue/order, selected full-release contents, COD-99 history and COD-104/admin-bypass policy remain unchanged.
- **COD-51:** retain exact-slice approval/sign-off linkage; this method supplement does not clear unrelated slices or rewrite original approvals.
- **COD-61/platform milestone:** retain the COD-56 blocker and original maintainer/user closure owner. Passing this slice does not establish whole-platform installed-archive/runtime/dependency/source-install qualification or aggregate closure.

Checkout/test-copy qualification is distinct from final source-bound **publishable archive identity/provenance and applicable runtime qualification**, which remain due at their later release stage. Do not add a staging prerequisite for future main-only archives that PR173 does not produce. Current-source Scryer reconciliation, non-bypass main integration, serial full own-merge-SHA main CI, full accepted queue, final publication verification and original aggregate closure gates remain unchanged. Related decision/evidence documentation stays with **PR173**, not a separate documentation PR.

No tests/builds/VM/Git/model/tracking/settings actions are performed by Requirements. Designer owns the bounded harness/fixture details; Evidence Verifier owns execution and independent acceptance; Production owns versioning and separate approval persistence; Engineer owns queue progression. No new approval or Proposal Reviewer gate is introduced. No reusable memory is warranted beyond these canonical decision references.
