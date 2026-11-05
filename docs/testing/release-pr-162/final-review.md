# COD-100 / GH-162 final read-only review

## Outcome

**No confirmed P0–P3 findings in the reviewed repair.** Proceed with delivery of this bounded correction, subject to the remaining remote-state and hosted-CI checks below. This is not merge approval or publication qualification.

## Reviewer and exact snapshot

- Fresh independent reviewer; no implementation work performed. Required adversarial-review-engineer skill and applicable requirements, correctness, release operations, trust-boundary, verification, and maintenance lenses were used.
- Actual spawn profile confirmed by the parent: `model="openai-codex/gpt-6.1-sol"`, `thinking="max"`; returned model matches. Fresh session `01a17c92-d8f4-731a-8cb2-becef5f1d851`. RLM exposes no separate `agent_type`/`fork_turns` parameters; this session received the bounded task, not a forked conversation.
- Worktree: `/private/tmp/codebasegraph-cod100-release-401`.
- Baseline main: `34eac4f8644adc046093babd84ee1fdbffdae853`.
- Existing malformed PR HEAD: `95e3830c6997190d2eaaf4147dc6c7352c1880f8`. **HEAD alone is not the reviewed correction.** Reviewed the full working-tree diff against baseline main and the source/test SHA-256 map in `docs/testing/release-pr-162/verification.json`.
- All ten enumerated source/test hashes matched before and after checks. Diff is 10 files, 78 insertions, 9 deletions. Evidence files are additional documentation, not source changes.
- User scope: “make it 4.0.1 and resolve the issues first”; existing PR #162, COD-100, Scryer `chg-9sernm`. No merge, tag, release, registry publish, source rewrite, remote write, or workflow-settings change was performed by this reviewer.

## Contracts examined

1. **Versions and notes:** root/wiki/xtask package versions, wiki dependency, lockfile packages and release manifest agree on 4.0.1. Independent locked Cargo metadata confirmed all three workspace versions. `CHANGELOG.md:3–18` compares from published v3.0.4, includes the unreleased storage breaking change/feature, #159 and this repair, and truthfully describes 4.0.0 as unpublished. Historical 4.0.0 notes remain intact.
2. **No persistent version pin:** `release-please-config.json:2` adds only the failed merge's bootstrap cutoff (`7038e66ff8271ffddc511f879cf8d28353a09b2d`), not `release-as` or `last-release-sha`. Verified pinned release-please 17.6.0 implementation: bootstrap applies only while no matching manifest release/tag is found; matching release discovery restores normal commit boundaries. The cutoff excludes the old 1.2.2 override and is not publication authorization.
3. **Generator evidence:** examined the actual read-only 17.6.0 generation report and request trace: main manifest 4.0.0 plus the cutoff proposes 4.0.1 with agreeing updates, with no REST writes or GraphQL mutations. Examined the future fixture and its script: synthetic matching 4.0.1 release yields no proposal without newer commits and 4.0.2 with only the added fix. That future check is explicitly synthetic, not proof of future hosted publication. Overlaying the repaired 4.0.1 manifest on pre-publication history generates 4.0.2 as documented; it must not be used to regenerate the manually repaired current proposal.
4. **Complete native assets:** current typed targets are macos-arm64 and windows-x86_64. `.github/workflows/release.yml:378` changes 8 to 4 public files. The unchanged validator at workflow line 369 runs first and requires both unique target provenance documents, exact SHA/version, correct archive/digest/sidecar contents, archive contracts, and no unexpected or missing files (`crates/xtask/src/main.rs:681–778`). Thus reducing the staging count does not admit an arbitrary four-file artifact set. The new actual-Bash test at `workflow_tests.rs:259–290` accepts the complete supported set and rejects a missing checksum.
5. **Safety boundaries:** full diff changes no CI binding, trusted branch/label recognition, production confirmations, artifact source/provenance, publishing permissions/environment, tag conflict handling, crate validation, finalization or concurrency gate. The only workflow change is the public-file count. Scryer existing release/metadata/note responsibilities remain satisfied without new architecture claims.

## Executed reviewer checks

All passed in this exact worktree, with an isolated Cargo target directory:

- `CARGO_TARGET_DIR=/private/tmp/cbg-cod100-review-target cargo test -p xtask --locked`: **44 passed**, zero failed/ignored/filtered; includes actual-Bash complete/incomplete staging and release authorization/publisher regressions. Output: `/tmp/cbg-cod100-review-xtask.log`.
- `cargo run -p xtask --locked -- check-workflows`.
- `cargo run -p xtask --locked -- release-gate`.
- `cargo run -p xtask --locked -- release-gate --production --confirm release-environment --confirm private-vulnerability-reporting`.
- `cargo run -p xtask --locked -- verify-release-version v4.0.1`.
- `cargo metadata --no-deps --locked --format-version 1`.
- `git diff --check 34eac4f8644adc046093babd84ee1fdbffdae853`.

Also inspected supplied before-fix failure, 44-pass nextest/JUnit report, local strict-Clippy/workflow/gate evidence and generator audit. The before-fix staging test failed because the valid two-target set was rejected, establishing that the new test detects the repaired defect. No full workspace tests, native builds or publish dry-run were started by this reviewer.

## Conditional risks and remaining delivery checks (not confirmed defects)

- **Open-PR overwrite before merge:** current main still has the old generator config. Another ordinary-main successful CI event can overwrite the repaired open PR. This is an existing operational window, explicitly documented at `docs/release.md:177–179`, not a reason to fabricate a v4.0.0 tag, remove trusted release labels, invent a hold label or weaken workflow gates. Recheck/reapply the exact correction if the bot changes it.
- **Remote metadata/supersession not yet applied at review time:** parent reported no GitHub mutations yet. Verify pushed branch content equals this reviewed source snapshot; verify PR title/body consistently say 4.0.1 and retain the curated notes. Add truthful supersession communication for failed/unpublished #156 without describing it as tagged/published. These are pending delivery steps, not source findings. Existing empty #156 labels must not be misrepresented as a task change.
- **Hosted qualification remains required:** PR CI and the eventual release merge's own complete main-push CI, artifacts, production checks and publish gates must pass. Local production confirmation flags exercise configured policy logic; they do not independently verify owner-controlled environment settings. No merge/publication is authorized by these results.
- **Repository integration scope:** used configured wiki memory and release architecture plus Scryer governing rules/orientation/claims. Graph returned docs/xtask orientation but does not index the hidden config/workflow and split/new release file; parent-provided graph discovery and Scryer file anchors supplied the bounded scope. Configured graph/Scryer are against the unrelated original worktree, so no fingerprint reconciliation or memory/model activation was attempted from this isolated snapshot. Wiki's older four-target description is pre-existing and contradicted by current typed targets/docs/workflow; no architecture change is introduced here. Linear connection inventory is pending; no updates were claimed.

## Assumptions and constants

Assume the already approved recovery is 4.0.1 superseding an unpublished 4.0.0, not repair/publication of the old immutable source. Reviewed changed constants: versions to 4.0.1; public file count 8 → 4; bootstrap SHA to the failed 4.0.0 merge. Reviewer changed none. Reusable knowledge is warranted about missing manifest-matching tags replaying historical Release-As trailers; candidate/activation work is deferred to the implementation owner because this review is read-only.

## Reviewed source SHA-256

- `.github/workflows/release.yml`: `56b4530d355685f13da6b690ca28c401f9e3211d7db76d387c63281524fcc2e2`
- `.release-please-manifest.json`: `f87e6c288df6c2b7ae134915c0a4e9f0828551c9c8d200f64a32743015bdd050`
- `CHANGELOG.md`: `c83a38e8e2f9a91324ec47784809eeca94f016d51f122f81cfe61a4dc0971978`
- `Cargo.lock`: `03b13a29495eb408054cb381fd3dee3f029c28cce07ad73998c38ba1667463f1`
- `Cargo.toml`: `e9c384f697b8c0af11c2e73c269c2cd1711f1be4445a7d0909629b931a2335cc`
- `crates/k-wiki/Cargo.toml`: `a819729c99aa5927f0b9e796a77031860fe3f3eb5d796d9263e26dadc0cd72b3`
- `crates/xtask/Cargo.toml`: `b5eb92df487c6ecfe6c17d01b8b1f812ac2ca6bb11e5fb7267763be01ae0e30c`
- `crates/xtask/src/release/workflow_tests.rs`: `ce8e04c3afd53f9095d5e549a111ab0cb294b32105aac511fa7ba6bfd9b06c2a`
- `docs/release.md`: `0e871d395f34cbbfb4a1d9095bdbe781b503a7981fca5c0f9f9af5c64e5303b0`
- `release-please-config.json`: `aee0cc12299ec52e26f56bf7621a7e48a50445f0105041da9fed5eea178f5497`
