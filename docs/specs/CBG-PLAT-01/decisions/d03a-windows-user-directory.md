# D03a — Windows user-directory selection decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D03a`; S04/COD-55 and qualification S10/COD-61.
- Proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.3 at full Git commit `f0b0cdefbfd4d655a776a6c4d0ca8d326d56a2cd`, **Windows user-directory policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/f0b0cdefbfd4d655a776a6c4d0ca8d326d56a2cd/docs/specs/CBG-PLAT-01/spec.md#d03--windows-discovery-precedence-and-command-kinds
- Decision reference: this conversation, immediately after the D03a question offered A (native Windows profile default) versus B (HOME before Windows profile). The user answered **“A”**. The question explicitly deferred executable/.cmd handling.
- Status: **directory policy approved**. Revision0.4 records the selection; the user did not approve the entire revised document or implementation start.

## Selected option A

Windows user-scoped MCP/client configuration follows this precedence:

1. Explicit configuration-path option.
2. Client-specific override, such as CODEX_HOME.
3. Native Windows user-profile directory.

Ignore Unix-style HOME for the default Windows location. If no valid directory can be found, fail clearly; never fall back to the current working directory or repository. Preserve deliberate client-specific overrides, explicit paths and project-scoped behavior. The selected tradeoff favors ordinary Windows profile behavior over Git Bash/HOME-first defaults; users retain explicit/client-specific overrides.

The exact native profile-resolution API/library and validation are implementation details to verify later. This does not approve a new dependency, a policy for invalid relative overrides, a change to macOS discovery, or an incidental change to Ladybug's native extension-cache home.

## Remaining limits

D03b executable suffix discovery, supported .cmd/.bat client shims, safe invocation/quoting and verification remain **unresolved**. They are not approved by the directory choice. D04 lock mode and D05 consumer/OS/PowerShell/Cargo recipe remain open. D01 and D02 remain approved independently.

COD-51, COD-55 and COD-61 remain Backlog; no implementation blocker is removed, and no implementation, merge, deployment, service restart or passing native qualification is claimed. Later exact-slice approval can reuse D03a without reopening the directory choice.
