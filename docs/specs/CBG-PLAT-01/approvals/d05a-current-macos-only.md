# D05a current-macOS-only amendment approval

- Canonical path: `docs/specs/CBG-PLAT-01/spec.md`, revision **0.11**.
- Approved full Git SHA: **`8a410c7a22f5291f06a1cece5cd9e7c0be255e72`**.
- Immutable revision: https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md
- Exact scope: **D05a current stable macOS-only Apple Silicon qualification**, S03/COD-54 R03/A05 and corresponding S10/COD-61 R10/A15 macOS references; preserve A06 runtime integrity/failure checks and all unrelated requirements.
- User decision reference:2026-10-27, this Prime Agent conversation. After the assistant explained that current hosted CI does not execute13.3 and proposed a separate minimum-version runner, the user said **“well then just drop it and keep only current”**. This explicit user-directed edit is recorded against its committed revision here.
- Scryer amendment ledger: `chg-p0s0ar`; existing responsibilities unchanged, no new topology/claims.

## Approved effect and limits

Drop macOS13.3 execution and older-version support qualification. Keep only current stable macOS on Apple Silicon, with actual tested OS/build/source/final-installed-archive identities and existing runtime/isolation/failure evidence. Current-release hosted CI may supply that evidence when its actual OS and no-external-Homebrew/OpenSSL runtime conditions are demonstrated. The moving runner label or configured tests alone are not passing execution.

Retain the existing13.3 deployment target and Mach-O metadata checks as implementation details; no product constants, library versions, Rust/workflow code or artifact bytes are changed by this amendment. Prior spec revisions, the original D05a decision and S03 implementation approval remain historical immutable records. The reviewed/tested product source remains `855c7e282a27de4afbad2d4252d7692de4b42ecb`; this approval changes qualification scope, not that execution identity.

No new Windows/shell/source-install policy, other slice authorization, S09 disposition, signing/notarization decision, passing qualification assertion, issue/milestone closure, merge, publication, deployment or restart. Maintainer closure ownership is unchanged.
