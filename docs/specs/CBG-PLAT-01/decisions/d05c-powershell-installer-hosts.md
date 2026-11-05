# D05c — PowerShell installer-host compatibility decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D05c`; S05/COD-56 and qualification S10/COD-61; relevant to the Windows prebuilt installation path in S02/COD-53.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.8 at full Git commit `5917e450816abf7761f4adfa1cedd4072bd7c1d6`, **installer shell compatibility policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/5917e450816abf7761f4adfa1cedd4072bd7c1d6/docs/specs/CBG-PLAT-01/spec.md#d05c--powershell-baseline-open
- Decision reference: this conversation, immediately after D05c offered A (built-in Windows PowerShell 5.1 plus supported PowerShell 7 releases) versus B (PowerShell 7 only). The user answered **“A”**.
- Status: **installer compatibility policy approved**. Revision0.9 records the choice; this is not full-spec approval or implementation authorization.

## Selected option A

Support the installer under **built-in Windows PowerShell 5.1 and Microsoft-supported PowerShell 7 releases** on the approved Windows 11 x86_64 desktop scope. Users need not install a second shell before running the installer. The stated tradeoff is compatibility and actual tests under both shell families.

Verify native exit codes, arguments, checksum checks and destination preservation consistently under both. A correctly hashed executable that launches but returns nonzero must fail DryRun and real preflight before existing destination binaries change. Include successful controls and missing/corrupt-file failures, recording exact PowerShell versions, native OS/build, binary/archive identities, commands/results and limits. The policy choice is not passing execution evidence.

## Separate contracts and limits

D03b independently approves native .exe plus .cmd/.bat client shims through a dedicated safe Windows invocation adapter. **D05c does not select that adapter's implementation mechanism, require PowerShell for client-shim execution, or authorize generic shell-command evaluation.** It selects installer compatibility only.

D05d Cargo source-install recipe remains open. Earlier decisions remain approved independently. No implementation start, source/constants change, passing qualification, merge, deployment or service restart is authorized. Keep COD-51, COD-56 and COD-61 Backlog until their actual remaining gates are met; later exact-slice implementation approval can reuse this settled shell policy.
