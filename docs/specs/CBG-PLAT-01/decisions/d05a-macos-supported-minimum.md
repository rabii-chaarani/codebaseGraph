# D05a — macOS supported minimum and qualification decision

## Approved scope and proposal

- Work: `CBG-PLAT-01/D05a`; S03/COD-54 and qualification S10/COD-61.
- Canonical proposal: `docs/specs/CBG-PLAT-01/spec.md`, revision0.6 at full Git commit `9324a074953bdcea52d37050d39088ad5aca7ead`, **macOS minimum/qualification policy only**.
- Immutable proposal: https://github.com/rabii-chaarani/codebaseGraph/blob/9324a074953bdcea52d37050d39088ad5aca7ead/docs/specs/CBG-PLAT-01/spec.md#d05--supported-consumer-qualification-matrix-and-cargo-install-recipe
- Decision reference: this conversation, immediately after D05a offered A (target macOS 13.3+ Apple Silicon, with minimum/current release tests before claiming support) versus B (target macOS 14+). The user answered **“A”**.
- Status: **target/qualification policy approved**. Revision0.7 records this choice; the user has not approved the entire revised specification or implementation start.

## Selected option A

Target macOS **13.3 or newer on Apple Silicon (ARM64)**. Qualify the actual final installed archive on the declared minimum, macOS 13.3, and a current macOS release before claiming support. Name the exact current release/build tested. Keep the approved D02 bundled-OpenSSL/relative-runtime-path policy: no Homebrew/OpenSSL prerequisite on these clean consumer environments.

The current native extension deployment minimum is not an executed support result. Verify every shipped native component's minimum-OS compatibility, final archive imports/install names/rpaths, actual CLI/graph/JSON/FTS/reopen/wiki/MCP behavior, and applicable failure paths. Capture native OS/build, source/CLI/archive hashes, exact command/method/result and limits. This is a selected qualification requirement, not a claim those checks have passed.

If a minimum-version native environment is unavailable, keep minimum qualification explicitly pending. Do not silently raise the floor to14+, substitute only current-host CI, weaken required checks or call metadata passing execution.

## Remaining limits

D05b Windows consumer OS baseline, D05c PowerShell hosts, and D05d Cargo source-install recipe remain open. D01–D04 remain independently approved; D05a does not restore Intel/Linux targets or decide Windows support by inference.

COD-51, COD-54 and COD-61 remain Backlog. No implementation start, production constants/source changes, platform qualification, release approval, merge, deployment or service restart is authorized. Later exact-slice approval may reuse this settled target policy without reopening it.
