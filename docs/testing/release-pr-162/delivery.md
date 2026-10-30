# Release repair delivery

- Work: COD-100 / GH-162; Scryer `chg-9sernm`, proportional repair with no changed architecture claims.
- User approval: “make it 4.0.1 and resolve the issues first”, 2026-10-27. This authorizes the open-PR repair, not a merge or publication.
- Source repair commit: `654e1c6aabf7126b1732992bd17f86bb9ef37ff3`. Whitespace-only evidence follow-up: `39e817427bd9cd8fb65f6ff907599e0b7ff6ec6f`. Both retain the ten reviewed source/test hashes in `verification.json`.
- Existing PR: https://github.com/rabii-chaarani/codebaseGraph/pull/162; title now `chore(main): release 4.0.1`; body contains the curated 4.0.1 notes. Trusted pending label preserved.
- Failed unpublished 4.0.0 proposal: https://github.com/rabii-chaarani/codebaseGraph/pull/156#issuecomment-6437846462. Supersession is truthful commentary only; no tagged label or placeholder tag.
- Local checks: 44/44 nextest release-tool tests (JUnit), warnings-denied scoped Clippy, formatting, workflow policy, normal/production release gates, version validation and locked metadata passed. Actual pinned generator read-only checks demonstrate version-boundary recovery and future normal operation.
- Committed-source metadata helper run at `654e1c6aabf7126b1732992bd17f86bb9ef37ff3` returns that exact source SHA, version 4.0.1, tag v4.0.1, and the exact curated nonempty notes. This validates metadata, not main-history membership, CI or publication authority.
- Fresh independent review: GPT-6.1 Sol / Max (`openai-codex/gpt-6.1-sol`, `thinking=max`), exact ten-file hash snapshot, no confirmed P0–P3 findings. Independent 44-test rerun passed. See `final-review.md`.
- Observed hosted CI for `39e817427bd9cd8fb65f6ff907599e0b7ff6ec6f`: https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40478036814, queued at delivery. Later documentation/memory-only delivery commits have identical reviewed source hashes and require their own current PR check status. Do not interpret queued or configured jobs as passing evidence.
- Linear: https://linear.app/codebasegraph/issue/COD-100/correct-release-pr-162-to-401-and-reconcile-failed-400-proposal, In Review pending hosted gates and maintainer acceptance.
- Reusable knowledge: `memory/procedural/missing-manifest-release-replays-version-overrides-2026-10-27`, recorded/verified/activated through configured k-wiki. Recommended validation accepted with zero diagnostics; link check zero diagnostics; configured site build succeeded (328 routes, 2 assets). Existing architecture/wiki spec identities and approvals retained.

## Unchanged limitations

No merge, tag, release, registry publication, workflow dispatch, release-label mutation or integration-settings change was performed. The old source remains unpublished. Eventual release requires its own successful exact-source main-push CI and unchanged production/artifact/crate gates.

Until the corrected config reaches main, another ordinary main CI completion can overwrite the manually repaired open PR. Recheck/reapply the reviewed correction if that occurs; no supported hold label exists.

Scryer/graph are configured against the unrelated original checkout. JUnit and exact source hashes are retained; do not ingest them as passing fingerprints of different original-worktree source or advance whole-project drift. No new or changed claims were needed. The graph does not index hidden release config/workflow or the split release test file; Scryer file anchors and discovered xtask/docs scoped inspection.

Original unrelated source/wiki/spec edits were preserved. The new focused memory and generated wiki output are the only intentional additions in that original knowledge bundle; generated `.kwiki` state was built through k-wiki, never edited manually.
