# PR #162 release-please generator audit

Read-only audit, 2026-10-27. Live main: `34eac4f8644adc046093babd84ee1fdbffdae853`; PR #162 head: `95e3830c6997190d2eaaf4147dc6c7352c1880f8`. No repository files or remote state changed.

## Cause: missing baseline, then historical version override

- Main's `.release-please-manifest.json` is `4.0.0`. PR #156 merged that version at `7038e66ff8271ffddc511f879cf8d28353a09b2d`, but there is no `v4.0.0` tag or GitHub Release. Latest real tag/release is `v3.0.4`, `ca115690b226384164cf364458a0e3255b8908d3`.
- [Release run 40469954130](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40469954130) used release-please **17.6.0**. Its logs explicitly show `looking for tagName: v4.0.0`, zero matching releases, `Splitting 150 commits by path`, a manifest-only previous version of `4.0.0`, and 497 parsed conventional commits.
- release-please matches releases/tags against the **exact manifest version**, not simply the latest published version. With no matching tag and no configured cutoff, it collects the whole available history. It then uses manifest `4.0.0` with an empty release SHA as the synthetic previous release, explaining the misleading `v4.0.0...v1.2.2` comparison.
- Historical commit [`6fc7bbf3b3dad629ab2ccb4bcb92d9e23a641865`](https://github.com/rabii-chaarani/codebaseGraph/commit/6fc7bbf3b3dad629ab2ccb4bcb92d9e23a641865) contains `Release-As: 1.2.2`. Version selection checks such a trailer **before** normal semantic bumping and does not reject a downgrade. That old instruction was replayed only because the history boundary was lost.
- PR #156 currently has no labels. The local proposal guard and upstream outstanding-release detection both require `autorelease: pending`, so neither blocked this proposal. This observation is not permission to relabel it or fabricate publication.

## Smallest recommended generator repair

Add only this top-level setting to `release-please-config.json`:

```json
"bootstrap-sha": "7038e66ff8271ffddc511f879cf8d28353a09b2d"
```

Repair PR #162's release metadata to **4.0.1** consistently across the manifest, root/wiki/xtask packages, wiki dependency, and Cargo.lock. Do **not** add permanent `release-as` or `last-release-sha` settings, create a `v4.0.0` tag, or change publication authorization.

Why this cutoff: it excludes the failed 4.0.0 proposal and older trailers from next-version calculation while retaining post-merge fixes. It is only a changelog commit cutoff; it does not claim 4.0.0 was published. In 17.6.0, bootstrap is consulted whenever the manifest's expected release is missing, even after earlier release PRs have merged. Once a matching 4.0.1 tag/release exists, normal lookup wins and bootstrap is ignored. `last-release-sha` instead stays an explicit cutoff and carries a removal/update obligation.

A `v3.0.4` bootstrap cutoff is **not** equivalent: the unreleased storage breaking change would bump manifest 4.0.0 again to **5.0.0**, unless a separate one-shot override were introduced. The failed-4.0.0 cutoff needs no version override.

## Actual generator verification (not a simulated version calculation)

Installed `release-please@17.6.0` in an isolated `/tmp` npm project, then called `Manifest.fromManifest`, `buildPullRequests`, and `GitHub.buildChangeSet` against live main. A fetch guard blocked all REST writes and GraphQL mutations. Authentication was piped to Node stdin, never saved. No create/update/tag/release method was called.

| Bootstrap | Manifest | Release-As | Generated version | Notes |
|---|---|---|---|---|
| failed 4.0.0 merge `7038e66...` | 4.0.0 | none | **4.0.1** | only post-merge fix #159 |
| last published 3.0.4 `ca115690...` | 4.0.0 | none | **5.0.0** | storage breaking change + #159 |

For the first scenario, parsed generated contents agree on **4.0.1** in all eight version locations (manifest; root/wiki/xtask package versions; wiki dependency; three lockfile packages).

Evidence files:
- `/tmp/cbg-pr162-generator-node/bootstrap-failed-4-merge.json` — full generated body/updates and read-only request trace.
- `/tmp/cbg-pr162-generator-node/bootstrap-last-published.json` — comparison case.
- `/tmp/cbg-pr162-generator-node/evidence-summary.json` — parsed version agreement.
- `/tmp/cbg-pr162-generator-node/read-only-generation.cjs` — reproduction, write-blocking guard.

## Head configuration and post-publication follow-up

Read the parent's repaired config/manifest/release-file metadata from `/private/tmp/codebasegraph-cod100-release-401` without changing it. With those exact file contents overlaid in memory on live main history, the full 17.6.0 generator proposes **4.0.2**, not a downgrade: the head manifest already records 4.0.1 as current, and #159 remains a patch after the bootstrap cutoff. This is expected. Do not use that run to regenerate the manually repaired current 4.0.1 proposal.

An explicitly synthetic in-memory future test then supplied a matching 4.0.1 release/source commit, without creating a remote tag/release. With no newer commits the generator produced zero candidates; adding one new fix produced 4.0.2 notes containing only that fix. Matching 4.0.1 release discovery therefore supersedes the old bootstrap cutoff. Full output and request trace: `/tmp/cbg-pr162-generator-node/head-and-future-results.json`; reproduction: `head-and-future-verification.cjs` in the same directory. This verifies selection behavior, not actual future publication.

## Truthful 4.0.1 release notes

The automatically generated 4.0.1 body contains only #159 and compares from nonexistent `v4.0.0`. Curate the new 4.0.1 section/body to:

- Say it supersedes **unpublished** 4.0.0 / PR #156.
- Include the storage feature and `GuardedDatabase` breaking-change text from #153/#156, plus #159 and the artifact-staging correction the parent is investigating.
- Use actual published `v3.0.4` as the comparison base for the public 4.0.1 notes (`v3.0.4...v4.0.1`). Keep the existing historical 4.0.0 section; do not describe it as published.

Generated config on an unmerged PR is not read by the bot: the action loads config/manifest from **main**. Repairing #162 manually is therefore necessary while main remains unchanged.

## No supported hold label for the repaired open PR

17.6.0 has no `autorelease: hold` mechanism. Open PRs are selected by pending/snapshot labels and matching head branch; draft status or extra hold/snooze labels do not prevent updates. `autorelease: snooze` handles **closed** PRs and updates/reopens one when the generated body changes. A fixture using the actual Manifest with a stub write sink verified both extra labels still attempt to overwrite repaired #162 with the old generated 1.2.2 proposal: `/tmp/cbg-pr162-generator-node/no-hold-label.json`.

Do not remove pending as a substitute: it also breaks trusted release recognition and does not safely prevent same-branch creation/update. While main's config is old, avoid new proposal-triggering main CI or obtain explicit permission for an operational pause. Merely editing/pushing PR #162 triggers PR CI, not this main-push publication path. Report the remaining overwrite risk; do not disable workflows or merge main without authorization.

## Gates and scope

Keep release-please `skip-github-release: true`, exact-release-merge successful CI binding, immutable-source version/notes agreement, production policy gate, complete artifact/provenance validation, protected publication environment, crate gate, finalization rules, and non-cancelling concurrency unchanged. The bootstrap fix neither authorizes old 4.0.0 publication nor qualifies new 4.0.1 publication. Parent owns production/artifact corrections and final PR evidence.

Graph searches found `docs/release.md` and xtask evidence but not release-please config/workflow File nodes; that coverage gap was reported before reading live metadata with `gh api`. Consulted repository wiki memory, `architecture/release-verification`, Scryer Release Verifier responsibilities, and governing rules. Wiki's four-target description is older than live main's two-target docs/workflow; live mutable facts were used. No Linear connection was verified (connection inventory reports pending). Reusable knowledge is warranted (missing manifest-matching tags can replay old Release-As trailers), but memory writes were deferred to the implementation owner because this audit is read-only.

## Pinned upstream evidence

release-please 17.6.0 resolves to [`712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1`](https://github.com/googleapis/release-please/commit/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1):

- [`src/manifest.ts`, L537–727](https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/src/manifest.ts#L537-L727): exact-version baseline; conditional bootstrap; manifest fallback.
- [`src/strategies/base.ts`, L543–570](https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/src/strategies/base.ts#L543-L570): config/trailer precedence before semantic bump.
- [`docs/manifest-releaser.md`, L129–171](https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/docs/manifest-releaser.md#L129-L171): cutoffs and persistent release-as warning; implementation above determines bootstrap's exact condition.
- [`src/manifest.ts`, L970–1122](https://github.com/googleapis/release-please/blob/712fcf01effd08d7b0e7b1fd3861f2cb388bc8d1/src/manifest.ts#L970-L1122): open/snoozed PR update behavior.

Assumptions: user-authorized 4.0.1 supersedes unpublished 4.0.0; no old tag should be fabricated; parent will manually retain unreleased storage notes and validate the complete updated PR. Constants changed: none in repository/remote; isolated test overrides used only the two bootstrap SHAs listed above.
