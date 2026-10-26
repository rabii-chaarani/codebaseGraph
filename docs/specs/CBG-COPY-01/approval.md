# CBG-COPY-01 — Scope A approval

- Canonical specification: [spec.md](spec.md), logical revision 0.2.
- Approved specification full Git SHA: `c5b62ba77d2f94b71714b5319f3b3fa215983f9f`.
- Approved file SHA-256: `1a0ea5f860ce2764e1666bd6345faf9bb24929f35795255772c04380839b4dd2`.
- User decision, this conversation: **“i approve the implementation of scope A. push the fix to PR #153”** (2026-10-26).
- Approved scope: Scope A only, including all-mutating-phase fail-closed behavior, removal of native-error substring suppression and write retry, initial configured request pool, equal existing spill/final-file limits, bounded diagnostic ingestion/reporting and the specified verification. Scope B automatic candidate reconstruction and Ladybug upgrade remain excluded.
- Target: existing [PR #153](https://github.com/rabii-chaarani/codebaseGraph/pull/153), branch `codex/automatic-graph-disk-cleanup`, baseline `5839055d9279f4475f893a54abeaa542344c1b02`. The user's explicit PR target supersedes the draft's provisional clean-main recommendation; the worktree starts clean from that PR head and preserves its lifecycle/maintenance and supported-platform policy.
- Independent proposal review and focused verification: [resolution](review-resolution.md), [final verification](proposal-review-verification.md). Reviewer verification is not the user decision above.
- Closure owner remains the repository maintainer. Approval, implementation, local verification, supported-platform qualification and rollout remain separate states.
- Linear connection remains pending verification; no delivery issue or successful synchronization is claimed.

The specification snapshot is retained unchanged. This record attaches the explicit same-scope user approval to its committed bytes; no additional scope choice is inferred.
