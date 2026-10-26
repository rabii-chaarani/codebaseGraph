# D03b — Windows client-command invocation decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D03b`; S04/COD-55 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.4 at full Git commit `950b09618b33b103ef766b308bb9413010dd4133`, **Windows executable/client-shim policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/950b09618b33b103ef766b308bb9413010dd4133/docs/specs/CBG-PLAT-01/spec.md#d03b--executable-suffix-and-batch-command-handling-open
- Decision reference: this conversation, immediately after the D03b question offered A (native executables only) or B (native executables plus .cmd/.bat shims). The user first answered “A”, then immediately corrected it with **“no i meant B”**. The effective decision is **B**; it supersedes A. No native-only policy was committed.
- Status: **command policy approved**. Revision0.5 records the corrected choice; this is not approval of the entire revised document or implementation start.

## Selected option B

Support native `.exe` clients and `.cmd`/`.bat` client shims used by ordinary Windows client installations. Use a dedicated Windows invocation adapter with separate, tested argument handling for native executables and batch scripts. Discovery, actual invocation and verification must agree on supported command kinds.

Preserve spaced paths, arguments and explicit overrides. Do not introduce generic shell-command evaluation or blind shell fallback. Unsupported or unsafe inputs must fail clearly rather than execute ambiguously. This permits safe invocation of the supported client shim, not evaluation of arbitrary caller-provided command strings. Additional implementation/testing work was an explicit tradeoff.

The exact adapter APIs, command-line encoding and compatibility behavior must be verified during authorized implementation. Native Windows tests must capture exact arguments through controlled `.exe`, `.cmd` and `.bat` fixtures and cover quoting/metacharacter boundaries and rejection paths. This choice does not approve a particular dependency or implementation.

## Remaining limits

D03a user-directory precedence remains approved independently; it is not changed by this correction. D01/D02 distribution choices also remain approved. D04 Direct lock-mode policy and D05 supported consumer/OS/PowerShell/Cargo-install recipe remain open.

COD-51, COD-55 and COD-61 remain Backlog. This record does not remove implementation gates, authorize code changes, claim tests passed, approve merge/deployment/service restart, or select a generic shell framework. Later exact-slice implementation approval can reuse D03b without reopening the settled native-plus-shim choice.
