# Immutable release publication correction: scope and acceptance

## Authority and revision

- Existing canonical contract: [`docs/release.md`](https://github.com/rabii-chaarani/codebaseGraph/blob/5889ed09954a4a07efd2590dd1bd380ee77be06a/docs/release.md).
- Contract/base full Git SHA: `5889ed09954a4a07efd2590dd1bd380ee77be06a`.
- User decision, conversation `01a1876f-3b43-750e-a9c9-c326dc4e6e10`: **"try the required workflow correction"** (draft → upload and verify the complete native asset set → publish). Inspection exception: **"yes i approve the inspection"**, specifically `.github/workflows/release.yml` and `crates/xtask/src/release/workflow_tests.rs` despite missing graph indexing. These exact quoted decisions are supplied by the coordinating Engineer; the approval does not derive from a commit or review verdict.
- Same-scope fix restores existing Scryer responsibilities `resp-259` and `resp-276`; no new public contract, work ID or model claim. Scryer change `chg-4kq3vn`; detailed execution brief is in child ledger `chg-6qg638`, not duplicated here.
- Stable proposal SHA-256: `c552e03401ad3160b0472499df0a97f356e658f6affb12269a27c0526860eeb6`. Proposal file: `immutable-release-proposal.md` in this conversation's session artifacts. This record freezes scope/acceptance, not a new approval or a replacement canonical specification.
- Endpoint: tested, independently reviewed pull request. No merge or publication authority.
- **Frozen v4.0.3 is excluded from live action:** no rerun, recovery, publication, upload, tag movement, settings/secrets/immutability changes or other live release mutation.
- Linear is unavailable; synchronization remains pending. No substitute issue ID was created.

## Implementation boundary

Isolated worktree: `/private/tmp/codebasegraph-immutable-release-assets-20261029`. Branch: `fix/immutable-release-assets`. Base: full SHA above. Preserve all other worktrees and unrelated primary-checkout edits.

Writable scope: `.github/workflows/release.yml`, `crates/xtask/src/release/workflow_tests.rs`, narrow `docs/release.md` clarification, and evidence in this directory. Production owns the sole Git writer role. Detailed assignments remain in Scryer.

## Acceptance

1. A new automatic release stays draft until all four validated public assets for the two current native targets are uploaded and verified against retained staged bytes.
2. A matching existing draft can resume partial uploads, verify the complete set and publish only afterward. Upload/verification failure leaves it draft and blocks crate publication/finalization.
3. A published immutable release with matching complete expected assets succeeds through read-only verification. Frozen missing/mismatched assets fail before any mutation.
4. Preserve exact tag/SHA, CI/artifact/source/PR/metadata identity, latest-version ordering, dry-run behavior, native-before-crate and finalization gates. Legacy existing-tag mode remains assets-only and does not publish a draft or crate or change labels.
5. Behavioral regressions execute actual workflow Bash through the existing closed local mock-gh fixture, including authenticated paginated draft lookup, byte mismatch/failure paths, and zero-write immutable retries.

## Verification status at scope freeze

Executed: fresh origin/main fetch; authoritative exact-SHA and branch/worktree inspection; branch creation; read-only matching PR search (none existed); graph and Scryer orientation; permitted source/contract inspection.

**Not yet executed:** implementation regression tests, corrected workflow checks, final verification or final review. Source implementation is blocked on coordinator readiness after the fresh proposal assessment. Documentation-only freezing is not implementation acceptance.

Planned checks: `cargo test -p xtask --locked`; `cargo run -p xtask --locked -- check-workflows`; `cargo run -p xtask --locked -- release-gate`; `cargo fmt --all -- --check`; `cargo clippy -p xtask --all-targets --locked -- -D warnings`. Independent verifier will record actual environment, revision, results and limitations separately. No local fixture result claims hosted publication or supported-platform qualification.
