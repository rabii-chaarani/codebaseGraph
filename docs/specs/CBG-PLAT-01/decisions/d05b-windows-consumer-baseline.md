# D05b — Windows consumer baseline decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D05b`; S02/COD-53, S05/COD-56 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.7 at full Git commit `4c63ddff0c3722604c082409765e92aa5b4c5568`, **Windows consumer target/qualification policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/4c63ddff0c3722604c082409765e92aa5b4c5568/docs/specs/CBG-PLAT-01/spec.md#d05b--windows-consumer-baseline-open
- Decision reference: this conversation, immediately after D05b offered A (Microsoft-supported Windows 11 x86_64 releases) versus B (Windows 10 and11 x86_64). The user answered **“A”**.
- Status: **consumer target/qualification policy approved**. Revision0.8 records the selection; this is not full-spec approval or implementation authorization.

## Selected option A

Target Microsoft-supported **Windows 11 x86_64 client releases**. Qualify actual desktop installation and operation of the final installed archive under the approved bundled-runtime policy. Record exact native edition/release/build, source/binary/archive identities, executed methods/results and limitations. Do not infer passing support from a selected policy or a PE subsystem version.

Windows Server CI remains native build/test evidence, not a substitute for desktop consumer qualification. This choice excludes Windows 10 and does not add Windows ARM64 or Windows Server consumer support. No particular Windows 11 edition, patch build or automated desktop test mechanism was selected by the user; the recorded execution must identify what was actually tested and any coverage limits.

## Remaining limits

D05c PowerShell/installer hosts and D05d Cargo source-install recipe remain open. Earlier D01–D04 and D05a decisions remain approved independently. D05b does not approve implementation, claim desktop tests passed, change source/constants, authorize merge/deployment/service restart or remove COD-51 gates. A desktop environment unavailable for testing leaves qualification pending; do not substitute Server-only CI or silently broaden support.

COD-51, COD-53, COD-56 and COD-61 remain Backlog. Later exact-slice implementation approval can reuse this settled Windows consumer policy without reopening it.
