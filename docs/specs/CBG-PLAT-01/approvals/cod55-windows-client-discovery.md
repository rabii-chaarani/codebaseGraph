# COD-55 — exact S04 implementation approval

- Work: CBG-PLAT-01/S04, [COD-55](https://linear.app/codebasegraph/issue/COD-55), prerequisite [COD-51](https://linear.app/codebasegraph/issue/COD-51).
- Canonical path: `docs/specs/CBG-PLAT-01/spec.md`, revision **0.11**, approved full Git SHA **8a410c7a22f5291f06a1cece5cd9e7c0be255e72**.
- [Immutable approved specification](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s04--use-explicit-windows-user-directory-and-executable-discovery-policy).
- Exact scope: **S04/COD-55 only, R04, A07–A08**. Reuse settled D03a directory precedence and corrected D03b option B native `.exe` plus safe `.cmd`/`.bat` client invocation. Implement and verify the specified preservation and failure behavior.
- Decision reference: this conversation, 2026-10-28, immediately after the recommendation to approve COD-55 only under the immutable rev0.11 specification. User: **“i approve this implementation, but you need to use the most recent branch from the most recent main branch”**.
- Baseline condition satisfied: fetched `origin/main` immediately before creating fresh branch `fix/cod-55-windows-client-discovery` at **9557b3806ce46cb1acadd95cfbf712c07ea8556d** (4.0.1). Isolated worktree preserves the existing dirty checkout. This source baseline is distinct from the approved specification revision.
- Scryer: preparation ledger `chg-9y3363`, signed off against the explicit decision; proportional execution brief records detailed ownership and prerequisites. Existing lifecycle and verification claims are reused rather than inventing topology.

Approval covers this implementation and its repository verification/delivery workflow. Other slices, release/milestone inclusion of S09, aggregate qualification/closure, merge, package publication, deployment and service restarts retain their existing gates. Approval supplies no passing test or platform qualification evidence. The maintainer retains aggregate closure ownership.
