# D05d — Windows Cargo default extension-bundling decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D05d`; source-install part of S02/COD-53 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.9 at full Git commit `d9541d4ef2302d19f36ee4fe222eaf55a88e583a`, **Windows Cargo source-install/default extension-byte policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/d9541d4ef2302d19f36ee4fe222eaf55a88e583a/docs/specs/CBG-PLAT-01/spec.md#d05d--cargo-source-install-recipe-open
- Decision reference: this conversation, immediately after D05d offered A (bundle Windows extensions by default for plain Cargo install) versus B (keep the bundled-windows-extensions feature opt-in). The user answered **“A”**.
- Status: **source-install policy approved**. Revision0.10 records the choice. This is not approval of the full revised specification, all source-install implementation details, or implementation start.

## Selected option A

Plain Windows `cargo install codebase-graph` includes the required JSON/FTS extension bytes by default, without a separate feature flag or runtime extension download for normal operation. The stated tradeoff is a larger binary and consistent default extension availability.

Cargo still compiles from source. The source-install guide and verification must state the required compiler/native build tools, native libraries and runtime DLL prerequisites. Default extension bytes do not satisfy their imported DLLs automatically, make source installs self-contained, or promise byte/provenance equivalence with a prebuilt release. Recommend prebuilt archives for users without a native build environment; preserve D01's stronger bundled-runtime prebuilt policy rather than weakening it.

During authorized implementation, choose the smallest compatible additive Cargo feature/target-conditional mechanism and test the real default source-install command on Windows 11 x86_64 with documented prerequisites. Use an isolated empty extension cache and disabled runtime extension downloads to establish that normal graph/JSON/FTS/reopen/search behavior uses bundled bytes. Record exact source/package/features/toolchain and prerequisite identities and clear missing-prerequisite failure evidence. No passing execution is claimed by this decision.

## Remaining limits

All listed D01–D05 policy choices are now selected, including corrected D03b **B**. This does not authorize implementation, an ABI/dependency upgrade, implicit Cargo publication, a new macOS source-build contract, merge, deployment, service restart or aggregate closure. Explicit implementation scope/approval, applicable architectural reconciliation, actual native/consumer/shell/source-install evidence and S09 inclusion/deferral are still required.

COD-51, COD-53 and COD-61 remain Backlog. Preserve exact historical decisions and implementation gates; do not reopen settled policy choices or claim all gates complete from this last policy answer alone.
