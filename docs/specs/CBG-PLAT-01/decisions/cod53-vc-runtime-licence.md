# COD-53 — Visual C++ runtime redistribution licence route

## Decision and scope

- Work: **CBG-PLAT-01/S02**, [COD-53](https://linear.app/codebasegraph/issue/COD-53), R02 / A03, A04, A16. Canonical [specification revision0.10](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md), full approved SHA `1a9db7b558b60248888408daa6268a3b45f2f0df`; [exact S02 approval](https://github.com/rabii-chaarani/codebaseGraph/blob/eaa430c849094078bb4c228ea03136dde5587e91/docs/specs/CBG-PLAT-01/approvals/cod53-windows-native-runtimes.md).
- **Selected applicable licence route: Microsoft Visual Studio Community 2022**, subject to its eligibility, acquisition/use and redistribution terms. A paid Professional/Enterprise licence is not required if the distributor qualifies for Community and complies with its terms.
- **User decision reference:** 2026-10-27, Prime Agent session `01a17b9c-1a65-77b0-bcc9-caa7c08f4b98`. After the assistant explained the Community2022 route and linked official eligibility/licence/REDIST terms, the user replied: **“Yes then, record the applicable licence for VC”**. This authorizes recording and using that route within the existing D01 bundling policy. It is not a statement that the agent acquired/installed Visual Studio or accepted terms on the user's behalf.
- Existing D01 app-local bundling, compatibility, checksums/licences and controlled-loading requirements remain unchanged. No manual-runtime prerequisite substitute, ABI/dependency upgrade, release/publication, merge/deployment/restart or aggregate closure is approved by this record.

## Governing Microsoft terms

Primary sources inspected directly2026-10-27:

1. [Visual Studio Community2022 licence terms](https://visualstudio.microsoft.com/license-terms/vs2022-ga-community/) — official embedded [English licence document](https://visualstudio.microsoft.com/wp-content/uploads/2021/11/Visual-Studio-2022-Community-License-EN.docx). Section1 permits an individual working on their own applications and permits organizational development/testing of applications released under OSI-approved open-source licences, subject to the complete terms. Other organizational scenarios have separate limits; no assumption of eligibility outside the applicable route is made.
2. Section4, **Distributable Code**, permits distribution of object code listed in the referenced Distributable List within applications developed under the licence and permits application distributors to distribute it as part of those applications. Conditions include significant primary application functionality, terms protecting Distributable Code at least as much as Microsoft's agreement, and the listed restrictions. Do not present the Microsoft DLLs as licensed under the project's MIT licence or remove their notices.
3. [Visual Studio2022 REDIST list](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution) explicitly names Community2022 alongside Professional/Enterprise, conditions redistribution on a validly licensed copy and the governing terms, and lists eligible **unmodified** files under `[VisualStudioFolder]\VC\redist`, with stated exclusions. Preview/pre-release/beta and debug/nonredistributable files are not supplied by this authorization.
4. [Microsoft redistribution guidance](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170) applies the licence requirement to packages, merge modules and individual runtime binaries. Merely finding DLLs on a GitHub-hosted VS Enterprise runner is not evidence of distributor entitlement or an exact permitted file source.

The official Community2022 English DOCX was fetched successfully; SHA256 **`41a207b10c8ab91d0d2f10a854715f73dca54509581692d2fe179aa3ffcb8540`**. This is an evidence identifier for the inspected terms, not a Microsoft DLL licence file or proof of acquisition. Read the complete applicable terms; this record is an engineering compliance handoff, not a substitute agreement or legal opinion.

## Packaging evidence still required

Before any VC DLLs are distributed:

- The distribution owner must have an eligible, validly licensed Community2022 copy and comply with its applicable terms. No actual installation/acquisition/acceptance has been observed or performed by the agent. This decision selects the licence route; it does not invent those facts.
- Identify the exact eligible unmodified release x64 runtime files, their official source/toolset version, names, hashes and applicable notices/terms. Current extension import tables require `MSVCP140.dll`, `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll`; actual final transitive import/export closure can require more. Names alone are not byte/provenance or licence proof.
- Preserve Microsoft component notices and licence restrictions separately from project MIT/OpenSSL Apache terms. Check exact selected files against the REDIST list and applicable distribution conditions; no copying arbitrary developer/debug DLLs.
- Execute the specified archive integrity, safe-loader and real installed/native success/failure checks. This licence selection is not passing Windows11 consumer/source-install qualification or proof of compatible DLLs.

The publisher/user retains responsibility for eligibility and compliance. Implementation within the selected route may proceed through the approved technical review gates; copying/shipping depends on the actual applicable entitlement and permitted-byte evidence above. No credentials, activation keys or purchase records are requested or stored.
