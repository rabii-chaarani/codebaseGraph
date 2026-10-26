# D02 — macOS prebuilt OpenSSL dylib bundling decision

## Approved scope and source proposal

- Work: `CBG-PLAT-01/D02`; applies to S03/COD-54 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.2 at full Git commit `18d4f1e2e1e59f8c8ac05f90e9c8982b0af6c00c`, **D02 policy only**, not the whole specification or implementation.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/18d4f1e2e1e59f8c8ac05f90e9c8982b0af6c00c/docs/specs/CBG-PLAT-01/spec.md#d02--macos-openssl-delivery
- Decision reference: this conversation, immediately after the D02 question offered A (bundle OpenSSL dylibs), B (static OpenSSL), or C (require Homebrew OpenSSL). The user answered **“A”**.
- Decision status: **policy approved**. Revision0.3 records this selected option; the user did not approve the entire revised document.

## Selected option A

macOS **ARM64 prebuilt releases** bundle compatible redistributable OpenSSL dylibs. Users do not need Homebrew or a separately installed OpenSSL. Package the libraries with relative install names/runtime paths, license notices and checksums/integrity verification. The stated tradeoff is maintaining those libraries and their packaging while retaining the existing dynamic-linking design rather than a static-linking redesign.

The exact supported library versions, archive layout, relative install-name/rpath strategy and tooling must be validated during authorized implementation. The final release archive, not only a debug executable, must be inspected and tested on a clean approved consumer baseline. Missing/corrupt/incompatible bundled libraries must fail clearly. If licensing, compatibility or redistribution prevents the selected policy, report a blocker rather than silently require Homebrew or switch to static linkage.

## What this does not approve

- Implementation start/completion for COD-54/R03/A05–A06, source changes, a model implementation claim or passing qualification.
- D03 Windows discovery, D04 lock mode, or D05 OS/PowerShell floors and Cargo source-install policy.
- Intel macOS/Linux support expansion, a dependency/extension ABI upgrade, global environment mutation, automatic downloads, merge, deployment or service restart.

D01 remains approved independently. COD-51, COD-54 and COD-61 stay Backlog; no blocker or implementation gate is removed by this policy record alone. Reuse D02 in later exact-slice implementation approval without reopening the settled choice.
