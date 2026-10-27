---
description: Approved prebuilt-first and WinGet direction, current installer gaps, release-contract constraints, and proposed consumer qualification gates.
resource: repository-installation-study
tags:
- windows
- installation
- release
- proposal
- winget
timestamp: 2026-10-20
title: Windows Prebuilt Installation Study
type: proposal
---
# Windows prebuilt installation study

**Status:** study complete; product direction approved; implementation and platform qualification pending. Reviewed on 2026-10-20.

**Recommendation:** make official prebuilt binaries the normal Windows installation path. First harden and qualify the existing archive installer. Then add WinGet as the normal managed channel, retaining the official release archive as fallback. Keep Cargo available as an explicitly documented source-build route. Both channels must consume the existing verified release binaries.

## Identity, authority and approval

This concept is the installation study identity. No existing installation-delivery milestone or slice identity was identified, and none is renumbered here. The canonical delivery context remains [Native Release Verification](./release-verification.md), with [Architecture Invariants](./invariants.md) unchanged.

Scryer remains the sole architectural authority. Relevant existing nodes are `container-release-verifier` and `node-1` (Release Gate); existing claims include `resp-255` (native archives), `resp-256` (archive provenance and smoke), `resp-16` (packaged command capabilities), and `resp-17` (MCP negotiation). Study ledger: `chg-j65vgg`. No new claims, execution briefs or implementation approvals have been created. Any future changed responsibility, derived-artifact contract or installer failure contract must be planned in Scryer and signed off before dependent code changes.

The user selected two product directions in this chat on 2026-10-20:

1. Ordinary Windows installation should use prebuilt binaries; Cargo may require build tools.
2. WinGet should be the later managed channel, with the release archive as fallback.

These decisions approve the study direction. They do not approve implementation details, external package submission, a Windows support floor, persistent PATH changes or runtime deployment policy.

The studied release is **v3.0.3**, commit `4af5db4e9dc959431a467a076a881f67d584cc37`. The working checkout is the older v2.0.0 at `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`; release claims below use the pinned release source. The archive installer is identical between those snapshots. Unrelated working-tree changes and Scryer drift were preserved.

## Problem and observable outcome

A Windows user attempting Cargo installation could not launch CMake. The dependency uses CMake when its native prebuilt-library attempt falls back to source compilation. Installing one missing tool treats the immediate symptom but leaves ordinary users exposed to the complete native build environment.

The desired outcome is that an ordinary Windows x64 user can install and run both `codebase-graph.exe` and `k-wiki.exe` from a single official release without Rust, CMake, Ninja, a C/C++ compiler SDK or Git Bash. This outcome still requires qualification on a named consumer Windows/PowerShell baseline. It does not establish that the existing executable has no redistributable runtime dependencies.

Fixing Ladybug's downloader alone would not remove all native compilation from Cargo installation: [the root build script](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/build.rs#L38) unconditionally compiles the vendored WebAssembly text grammar with `cc::Build`. Source installation remains a legitimate developer route.

## Existing foundation and findings

The release pipeline already creates both executables, internal binary checksums, an archive checksum, packaged installers and commit/version/target provenance. Preserve the build-once/promote contract and the Windows bundled-extension feature. GitHub v3.0.3 already has a Windows x64 release archive. No replacement database or new compiler-installation service is needed to meet the approved direction.

| ID | Evidence status and consequence | Required response |
| --- | --- | --- |
| WIN-01 | **Established installer defect.** [PowerShell line 41](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/release/install/install.ps1#L41) invokes a native probe without checking its exit code. Lines 72–77 can report validation success after an unsuccessful native exit. | Reject nonzero native probe exits explicitly. Verify failure before replacing either installed executable. |
| WIN-02 | **Established gap against a proposed recovery guarantee.** [Lines 81–82](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/release/install/install.ps1#L81) replace graph and then wiki; failure on the second operation can leave different releases. Per-file temporary copies are already present and should be preserved. Existing documentation does not prove a pair transaction. | Define matching-pair recovery and verify second-replacement and busy-file failures before promising it. Decide interruption/power-loss scope separately. |
| WIN-03 | **Decision-critical qualification gap.** [Archive smoke](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/crates/xtask/src/main.rs#L382) dry-runs the installer and exercises extracted executables. [The native workflow](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/.github/workflows/native.yml#L43) prepares native dependencies before smoke. | Perform a real installation and exercise installed command paths on a clean consumer image; a prepared build runner or a restricted PATH alone is insufficient evidence. |
| WIN-04 | **Conditional channel risk.** Binary package managers extract/install files themselves and do not inherit packaged PowerShell installer behavior. | Verify the selected WinGet install, upgrade, command aliases, failure and removal paths independently. Do not claim archive-installer tests qualify WinGet. |
| WIN-05 | **Established packaging constraint.** [The current archive inventory](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/crates/xtask/src/main.rs#L512) is exactly five files; complete-artifact validation accepts one canonical archive, sidecar and provenance per target and rejects extras. | Plan an explicit derived-ZIP contract, hash identity and promotion validation. Do not add a ZIP or DLLs to distribution as unchecked extras. |

The release runbook describes the probes as validation. This study records the source/document mismatch and recommends restoring that gate; it does not redefine Scryer responsibilities or infer pair atomicity from the existing wording.

WIN-01 is established by source tracing plus Microsoft's documented [native exit-code semantics](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_error_handling?view=powershell-5.1#external-program-errors). It was not reproduced under PowerShell in this study.

No missing DLL or CRT failure has been demonstrated. [Windows native setup](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/.github/actions/setup-native/action.yml#L26) deliberately uses static OpenSSL libraries, so the absence of OpenSSL sidecars is not evidence of a defect. Inventory the actual executables' dependencies and exercise dynamically loaded extensions on the consumer baseline. Microsoft distinguishes [runtime redistribution](https://learn.microsoft.com/en-us/cpp/windows/determining-which-dlls-to-redistribute?view=msvc-170) from compiler installation; static native-library linkage alone does not establish static CRT linkage.

## Options and selection

| Option | Fit and maintenance implications | Study disposition |
| --- | --- | --- |
| Cargo-first with better prerequisite documentation | Helps developers diagnose source builds, but ordinary users still need native tools when source compilation occurs. | Retain as the developer route; insufficient as the primary solution. |
| Official archive and packaged installer | Reuses existing builds, checksums and release identity. Users still obtain/extract the archive, and command-discovery and upgrade behavior need validation. | First delivery stage and permanent fallback. |
| WinGet portable package | Offers a familiar Windows lifecycle and supports multiple portable executable aliases. Requires a validated ZIP, manifest maintenance and channel-specific tests. | User-selected managed channel after the foundation is qualified. |
| Project-owned PowerShell downloader | Could automate version/target selection but adds acquisition, retry and installer maintenance overlapping WinGet. | Not selected; not needed for the current direction. |
| cargo-binstall | Can map existing archives with metadata, but has Rust-target naming and crate-inventory constraints and can fall back to compilation. | Optional future developer convenience; outside required scope. |

[WinGet's manifest contract](https://github.com/microsoft/winget-pkgs/blob/master/doc/manifest/schema/1.28.0/installer.md) supports portable ZIPs containing multiple binaries, command aliases, hashes and user/machine scope. The proposed package must list both executables explicitly and use an exact official version URL and digest. A package identity has not been reserved or submitted.

A Windows ZIP should be derived from the same verified staged binary bytes as the canonical archive, rather than recompiling. Preserve existing target identifiers and archive URLs; update the typed packaging/completeness/provenance contract before adding this format.

The current root crate declares only the graph executable; wiki is separate. The [cargo-binstall configuration contract](https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/SUPPORT.md) infers binaries from crate declarations, uses Rust target triples, and permits fallback strategies unless restricted. An archive containing wiki would not automatically make root-package binstall install it. Adding binstall is not necessary to solve this user's problem.

## Proposed requirements and acceptance gates

The labels below identify this study's proposed acceptance gates, not approved new Scryer claims or passing execution. Existing claims cover the noted release responsibilities; new installation promises require the Scryer plan before implementation.

| ID | Observable acceptance and verification | Intent/evidence mapping |
| --- | --- | --- |
| WPI-01 | On the chosen clean Windows x64 baseline, a normal user installs both executables from one official version without the developer toolchain, a checkout, Ladybug caches or build-environment overrides. Run real graph/database, wiki and MCP operations through installed paths. Transfer the package first, then verify applicable runtime smoke without network access. | Approved product outcome; existing `resp-16`, `resp-17`, `resp-256` provide related packaged checks. New consumer qualification is **not executed**. |
| WPI-02 | Unsupported target, unavailable asset, download failure and digest mismatch produce an explicit failure before replacement, with no Cargo fallback or automatic SDK installation. Exact release selection is consistent for archive and checksum. | Existing `resp-255`/`resp-256` support artifact identity; acquisition/channel behavior still needs planned intent. Proposed tests use controlled failures, not live production probes. |
| WPI-03 | A correctly hashed probe fixture returning a nonzero native exit fails validation and install, leaving the previous installation untouched. | Restores the documented validation gate; covers WIN-01. PowerShell execution **not performed**. |
| WPI-04 | Fresh install, reinstall, upgrade, second-file failure and a running/locked executable never report successful installation of a mismatched pair. The specified recovery path is demonstrated. | Proposed recovery contract covering WIN-02. Existing per-file replacement is not sufficient evidence. Cancellation/power-loss guarantees remain a separate decision. |
| WPI-05 | Shell command discovery and existing MCP registrations resolve the intended installed release. An older Cargo binary earlier on PATH is detected/reported rather than mistaken for the new installation. Existing registrations are verified and clients restarted through the documented runbook. | Preserve [release upgrade guidance](https://github.com/rabii-chaarani/codebaseGraph/blob/4af5db4e9dc959431a467a076a881f67d584cc37/docs/release.md#L152); no automatic process termination or deletion of a Cargo installation is proposed. |
| WPI-06 | The WinGet ZIP contains the verified binary bytes, has its own validated digest/release binding, and the manifest exposes both commands. Real WinGet install/upgrade/removal and lock/failure scenarios satisfy the promises made for that channel. | `resp-255`/`resp-256` require an explicit derived-artifact plan. No ZIP, manifest, submission or WinGet execution exists from this study. |
| WPI-07 | Cargo documentation distinguishes build prerequisites from runtime installation, names the Windows source-fallback requirements, and retains the chosen source version/features and appropriate locked dependency guidance. | Documentation within the approved source-build option; no claim that every Cargo installation invokes CMake. |

Import inspection and clean execution are complementary: DLLs loaded at runtime may escape a static import list, while build-host smoke may obtain runtime libraries from the prepared host. If a redistributable is required, choose and qualify its supported deployment policy rather than promising that no runtime component is needed.

## Delivery sequence and boundaries

1. **Archive foundation:** restore the native exit gate, settle failure/recovery and command-discovery behavior, preserve both-binary validation, and make prebuilt installation the main documentation route. Retain explicit Cargo prerequisites. This stage changes no database architecture or platform identity.
2. **Consumer qualification:** extend verification to installed paths and the named clean Windows/PowerShell baseline, including graph, wiki, bundled extensions and MCP. Resolve actual native-runtime findings if any; do not infer them solely from package file counts.
3. **WinGet:** plan and implement the derived ZIP and channel-specific verification through the existing typed release pipeline. Reserve/submit the external package only with explicit authorization. Retain the archive route when WinGet is unavailable.
4. **Optional later work:** evaluate cargo-binstall only if users need it; do not make multiple new acquisition mechanisms a prerequisite for completion.

Detailed execution briefs, changed claims, file ownership, dependencies and closure assignments belong in Scryer. This study does not allocate new slices or duplicate those briefs. Existing release gates, four supported target identities and aggregate closure ownership are unchanged.

## Open decisions before dependent implementation

- **Windows/PowerShell support floor:** name the client OS versions and built-in/newer PowerShell versions to qualify. The current platform label alone does not establish this.
- **PATH and migration behavior:** retain explicit user directory/command paths and decide whether any persistent PATH mutation is offered. WinGet's aliases and existing Cargo installations need a coexistence plan.
- **Upgrade recovery scope:** choose the supported behavior for normal I/O errors, cancellation, interruption and power loss; do not describe independent file moves as an atomic pair transaction.
- **Runtime policy:** after import and consumer evidence, decide how any required redistributable is supplied. No missing-runtime premise is established today.
- **Channel operations:** select the package identity and publication maintenance owner. External submission has not been authorized by selecting WinGet for this study.

The decision owner is the user. None of these open details blocks this study's recommendation; they must be resolved before implementation that depends on them.

## Evidence, review and handoff

Executed in this study: read-only source comparison against v3.0.3; `cargo metadata --offline --locked --format-version 1 --no-deps` on the working checkout; graph discovery/context/schema and bounded queries; Scryer orientation/read; wiki intent and related decisions; primary documentation checks. The preceding investigation also ran the Windows-target locked dependency tree successfully on macOS. That was dependency inspection, not Windows execution.

A read-only reviewer using **GPT-6.1 Sol / Max** challenged proposal v0.1 against the pinned release source. The recommendation was narrowed from multiple possible acquisition mechanisms to the archive foundation followed by one selected managed channel. Findings WIN-01 through WIN-05 and the conditional runtime/channel limits are retained above. No implementation changes were made, so the findings are not represented as fixed.

Limitations: no Windows or PowerShell runtime execution, actual consumer install, PE dependency inventory, WinGet test, new archive build or successful user retry was performed. Graph File-node coverage omitted distribution scripts/manifests; graph-indexed packaging anchors identified the relevant references, and pinned source/Cargo metadata supplied the bounded missing detail. Unrelated Scryer drift was not reconciled.

See also the [CMake diagnostic procedure](../memory/procedural/windows-cargo-cmake-source-fallback-diagnosis-2026-10-20.md). Implementation readiness requires the applicable owner decisions, Scryer plan/sign-off and executed acceptance evidence; this document establishes neither platform qualification nor release closure.
