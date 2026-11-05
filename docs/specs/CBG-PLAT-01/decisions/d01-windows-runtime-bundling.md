# D01 — Windows prebuilt runtime bundling decision

## Approved scope and source proposal

- Work: `CBG-PLAT-01/D01`; applies to S02/COD-53 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, proposed revision0.1 at full Git commit `33360340379f878e93324e0829a1bb6f33eafc9d`, **D01 only**, not the full specification or its implementation.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/33360340379f878e93324e0829a1bb6f33eafc9d/docs/specs/CBG-PLAT-01/spec.md#d01--windows-native-dependency-delivery
- Decision reference: this conversation, immediately after the user asked to resolve the pending decisions together. The assistant asked whether the Windows archive should work without manual dependency installation and presented A (bundle required runtimes) versus B (external prerequisites). The user answered **“A”**.
- Decision status: **policy approved**. Revision0.2 records that selection; it does not pretend the user approved the entire revised document.

## Selected option A

Windows **prebuilt releases** must bundle the compatible redistributable non-system runtime libraries required by the shipped programs and JSON/FTS extensions. Users should install/extract the archive and run it without separately installing OpenSSL or the required Visual C++ runtime.

The selected option includes maintaining compatible redistributable libraries, license notices, checksums/integrity verification and application-controlled DLL loading. Larger downloads and additional release-maintenance responsibility were stated tradeoffs. Do not widen global PATH or add arbitrary caller-writable library fallback. Determine exact compatible versions/layout and safe loading mechanisms during authorized implementation; no specific DLL placement or API design was selected by the user.

If licensing, ABI compatibility or redistribution prevents this option, report the blocker. Do not silently substitute the rejected manual-prerequisite experience.

## What this does not approve

- Implementation start or completion of COD-53/R02/A03–A04, source changes, a model implementation claim or passing platform qualification.
- D02 macOS distribution, D03 discovery, D04 lock mode or D05 OS/PowerShell floors and **Cargo source-install policy**.
- A general Windows self-contained/system-library guarantee beyond an explicit approved consumer baseline.
- Dependency/extension ABI upgrades, global environment changes, automatic downloads, merge, deployment or live service restart.

COD-51 stays Backlog as the broader exact-scope approval gate; COD-53/COD-61 remain unimplemented/unqualified. Existing issue blockers are not removed merely because one policy is now selected. Subsequent exact-slice implementation approval can reuse D01 without reopening it.
