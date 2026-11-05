# D04 — Windows Direct-read exclusive-mode decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D04`; S09/COD-60 and its qualification/disposition in S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.5 at full Git commit `b62dd75fc490d81484e8d235a01a4bc55833f517`, **Windows Direct-read lock-mode policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/b62dd75fc490d81484e8d235a01a4bc55833f517/docs/specs/CBG-PLAT-01/spec.md#d04--direct-read-lock-mode-contract
- Decision reference: this conversation, immediately after D04 offered A (keep Exclusive and report it accurately) versus B (release/reacquire/revalidate to Shared). The user answered **“A”**.
- Status: **policy approved**. Revision0.6 records the choice; the user has not approved the entire revised document or implementation start.

## Selected option A

For Windows Direct reads, retain the existing Exclusive lock and report/verify its actual Exclusive mode. Do not add a redundant Shared lock layer, an unlock/relock transition or a new concurrent-reader guarantee. The stated benefit is preserving current exclusion without introducing a transition race; the stated tradeoff is no concurrent-read improvement.

Preserve publication/recovery safety, release behavior, and current macOS lock behavior. This choice does not approve applying Exclusive everywhere or changing the separate fresh-creation transition protocol. Exact code/API/name changes are implementation details, not preselected changes.

Native Windows verification must show that an admitted read reports/holds Exclusive, both Shared and Exclusive second-handle attempts contend until release, and release permits intended later acquisition. Rerun affected Direct read/replay/writer-exclusion cases on Windows and macOS. No passing implementation test is claimed here.

## Remaining limits

COD-60 remains preexisting Low/P3 debt, not a cause of the currently failing CI. Selecting its policy does not silently include its implementation in a release or remove its approval gate. D01/D02/D03a/D03b remain approved independently; D05 consumer/OS/PowerShell/Cargo-install policy remains open.

COD-51, COD-60 and COD-61 remain Backlog. No implementation start, source change, full-spec sign-off, qualification, merge, deployment or service restart is authorized. Later exact-slice approval may reuse this settled policy without reopening it.
