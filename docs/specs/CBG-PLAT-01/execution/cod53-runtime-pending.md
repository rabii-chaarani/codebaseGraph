# COD-53 remaining runtime work — blocked supply handoff

## Identity/status

CBG-PLAT-01/S02 [COD-53](https://linear.app/codebasegraph/issue/COD-53), Maintenance P-COD-2. Canonical approved [spec rev0.10](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md), R02/A03/A04/A16; [exact approval](https://github.com/rabii-chaarani/codebaseGraph/blob/eaa430c849094078bb4c228ea03136dde5587e91/docs/specs/CBG-PLAT-01/approvals/cod53-windows-native-runtimes.md). Existing Scryer `chg-56s373` remains open; detailed execution/ownership remains there, not this evidence handoff.

The independent D05d default-extension subset is implemented/reviewed/pushed at `6d5de0c57d9311e3a5bf86f30a7767641d6b84bc`, [draft PR158](https://github.com/rabii-chaarani/codebaseGraph/pull/158); [evidence](cod53-default-extensions.md). **Full S02 is not implemented or qualified.** No native DLL/loader/installer/NativeTarget/setup changes were made. No merge/publication/deployment/restart or milestone closure.

## Licence decision preserved

The user selected the Visual Studio Community2022 route and directed recording it. [Decision record](https://github.com/rabii-chaarani/codebaseGraph/blob/389f86f58a03f71d81b0a2aeb44e7cc49a58f4ce/docs/specs/CBG-PLAT-01/decisions/cod53-vc-runtime-licence.md) at `389f86f58a03f71d81b0a2aeb44e7cc49a58f4ce`. A paid Enterprise licence is not required for eligible Community use. Do not reopen that route decision or substitute consumer runtime installation for D01.

Selection does not invent acquisition/installation/acceptance or exact permitted source bytes. Publisher eligibility/compliance remains the owner's responsibility. No Microsoft DLLs were copied into the repository or distributed; external downloads were analysis inputs only.

## Concrete supply blocker

Before dependent full-runtime implementation/packaging, provide or establish an exact permitted unmodified release-x64 **Visual Studio Community2022 `VC\Redist` input**, with source/toolset version, required file identities/hashes and applicable notices/terms. A pinned authorized Community component/layout is suitable in principle; arbitrary DLLs from the unversioned GitHub-hosted Enterprise installation, third-party NuGet packages or an unevidenced extraction route are not substituted.

- [Runtime source follow-up](cod53-runtime-proposal-evidence/runtime-source-followup.md) identifies Microsoft's documented installed REDIST source and the remaining provenance fact. It also inspects published **historical v3.0.4**, not the unpublished4.0.0 baseline: both EXEs directly import `MSVCP140.dll`, `VCRUNTIME140.dll`, `VCRUNTIME140_1.dll`. Exact final images still require inspection. No Windows binary was executed.
- An official central redistributable download was identified by pinned Microsoft URL/hash ([download identity](cod53-runtime-proposal-evidence/vc-provider-download-identity.json)); that is **not** evidence of an authorized app-local payload extraction method or source entitlement. Do not silently convert it into the installed REDIST input.
- A direct official Community17 release-channel/catalog check failed its declared SHA256. [Integrity record](cod53-runtime-proposal-evidence/community-catalog-integrity-check.json): expected catalog `5bd3b0b2ae509af2e9559b852f78545df0bf2c081dd850caed76f1dc134c511e`, actual downloaded `87791000b1d6357e174f039ce9c2f03d150d4e90b45e8e52468d797dac62022c`. Basic BOM/line-ending checks did not resolve it. The catalog was rejected as a pinned source; no derived payload was trusted/downloaded for packaging. This observation does not establish a cause or malicious content. A valid verified source must replace it.

The next action is a verified permitted source, not another choice between Community/Enterprise. No new licence purchase, account connection, VS installation or term acceptance was performed by the agent.

## Reviewed technical direction — not implementation approval

Read-only research supports complete verified non-system DLL preloading without process-global DLL search policy or an upstream/ABI upgrade; **full concrete proposal gate remains open**.

- [Supply/API/PE research](cod53-runtime-proposal-evidence/runtime-supply.md): official source-built OpenSSL3.5.9 is a compatible candidate, not final Windows DLL/export/recursive-import qualification. Exact0.19.0 extensions are unchanged.
- [Loader investigation](cod53-runtime-proposal-evidence/loader-design.md), [independent preload delta](cod53-runtime-proposal-evidence/loader-proposal-delta.md): verify complete normal/delay/runtime import closure, origin and paths before any code load; preload dependency-first via exact absolute `LoadLibraryExW` with restricted dependency lookup; check existing/returned modules and retain accepted handles. Initialize before `Database::new` because WAL recovery can replay extension loads; validate actual main/optional cached loader inputs too. Plain Cargo source mode remains separate from strict packaged mode. No global PATH/default DLL policy change or arbitrary fallback.
- [Install-layout investigation](cod53-runtime-proposal-evidence/installer-layout-followup.md), [supply/install review](cod53-runtime-proposal-evidence/supply-install-proposal-delta.md): keep existing executable paths and place initial-import DLLs beside EXEs. No launcher, path relocation or new rollback design. Validate candidate sidecars and real JSON/FTS probe wholly in staging before destination mutation; check new probe's native status explicitly without broad COD-56 repair. Settle conservative compatible-forward/reuse-or-refuse upgrade/intermediate-set policy against actual old/new validators before coding.
- [Failure-contract clarification](cod53-runtime-proposal-evidence/installer-failure-contract-delta.md): no extra owner decision is currently required merely because sequential sidecar updates can fail. Approved invalid candidate inputs fail before mutation. Arbitrary mid-copy/power-loss whole-install atomicity is not promised. Do not weaken identity or silently add a transaction framework; surface an actual contract conflict if evidence reveals one.

These are evidence-backed feasibility/constraints, not passing native behavior or a frozen full execution brief. Once valid source supply is established, settle package/source mode, manifest generation order, cache-integrity scope, upgrade/probe contract and file ownership in Scryer, then close the independent full proposal gate **before** dependent source edits.

## Remaining execution/closure

[Windows default-byte execution evidence](cod53-windows-qualification.md) now records a native no-extra-feature build/test, exact-head default Cargo path install with declared shared native prerequisites, real JSON COPY/native FTS/reopen and installed graph/reopen/search. The report preserves failed trials, download-barrier scope and missing-runtime diagnostic limits; it does not claim full A16 acceptance. Complete app/helper/library trusted runtime loading, final PE/DLL/archive identities, legal supply, negative installer cases and clean-consumer Windows11 archive/shell qualification remain pending. Windows Server CI on PR158 is useful native evidence only, not A03/A04/fullA16. Track its exact head/merge/commands/jobs separately; queued/running/skipped jobs are not passes. COD-53 remains In Progress with full runtime work blocked. Maintainer retains aggregate closure ownership.

Reusable-knowledge assessment: no memory recorded. Licence choice is in the canonical decision; runtime/installer work is unimplemented and not a verified runbook. No curated wiki change is required by this partial/blocker handoff.
