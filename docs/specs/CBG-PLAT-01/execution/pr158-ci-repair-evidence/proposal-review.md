# PR158 / COD-53 proposal review — 2026-10-29

## Verdict

**Proceed with specified conditions for the bounded CI repair. No decision-blocking finding.** This permits implementation, not merge, release, desktop qualification, or COD-53/milestone closure.

Reviewed proposal v1 at `proposal.md`, baseline `5f68b41cebc327072cf71be770c9e5a9d1c0dad2`, plus the coordinator's explicit revisions received during review:

1. Select `crates/k-wiki/Cargo.toml` lower bound `0.13.5` and resolve `Cargo.lock` precisely to `0.13.5`.
2. **Replace v1 paragraph B's index edit:** preserve `evidence-index.json` byte-for-byte; add `archived-inputs.json` mapping the original proxy input to the archive. Preserve all historical reports/logs/commands unchanged. A short archival note belongs in the surrounding qualification summary, not the raw evidence.

Review mode: fresh independent read-only proposal review, requested GPT-6.1 Sol / High, no inherited transcript. Used `adversarial-proposal-review-engineer` and `codebasegraph-engineer`. No product/Git edits, builds, tests, installs, or settings changes were made by this reviewer. This review report is the only written artifact.

## Decision and strongest case

The requested commitment is a small reversible repair of two demonstrated CI blockers, not a platform redesign. The audit rejects an advisory-affected Markdown parser; the unchanged repository gate rejects a loose historical Python harness. A compatible patch and a byte-preserving historical archive address those causes without changing native assets, defaults, ABI, or policy. Status quo leaves both CI jobs failing. Disabling audit, adding ignores, exempting evidence directories, or renaming executable Python as text would evade the controls and is not supported.

The coordinator reports current explicit user authorization to debug/fix PR158 CI. I treat that bounded authorization as covering a compatible incidental Markdown security patch, not as expanding the older S02 approval. The supplied canonical approval expressly excludes Ladybug/extension ABI/dependency upgrades. If actual user authorization differs from the coordinator's description, resolve that before implementation. No Ladybug upgrade is proposed.

## Supported observations

- `baseline-audit-job.log` and `baseline-head-audit.log` demonstrate `pulldown-cmark 0.13.4`, RUSTSEC-2026-0414, patched requirement `>=0.13.5`. Existing allowed cxx/tokio warnings remain separate; do not describe them as repaired.
- `baseline-package-job.log` demonstrates the actual CI package gate failed on `reject-proxy.py`. The head-only gate log also lists four older helpers. `main-prior-archival-fix.diff`, commit `34eac4f8644adc046093babd84ee1fdbffdae853`, records their separate archival repair. Validating the reconstructed PR merge rather than importing unrelated main changes is proportionate.
- I independently discovered `check_rust_only_files` via graph search and explicitly requested semantic change-impact context before source inspection. Its implementation rejects loose `.py` files without an evidence-directory exception. Preserving that behavior and running negative controls avoids a policy bypass. Graph orientation is from the configured repository, not independently verified as a graph built for this detached worktree; the proposal documents this limitation.
- The staged `reject-proxy-baseline.tar.gz` contains exactly one regular file, under the original full repository-relative path. Its member is 1,311 bytes, SHA-256 `ca7ada29f99cb0a6da0807d6f468c22ad81d1bce59abe5c9f743c1b03b83588f`. Archive SHA-256: `d37b3c86d978018d137375e7a30316b3ce779b2787df5037237032bdec892752`. Parent must still perform the planned independent second archive generation and final staged-byte verification.
- All **65** baseline `evidence-index.json` file records match their current bytes and hashes. The index has 330 CRLF line endings. Leaving it unchanged and using a sidecar is safer than rewriting historical index entries, and avoids text-edit newline normalization.

## Selected dependency option

**Use lower bound `"0.13.5"` with unchanged `default-features = false` / `features = ["html"]`, plus a lockfile resolution precisely at `0.13.5`.** Do not use an exact `=0.13.5` manifest pin.

Lock-only is sufficient for this locked CI scan but continues to permit known-affected versions in fresh/ignored-lock resolution. Raising the lower bound excludes them without adding features, changing a public signature, or preventing later compatible patches. It is a narrow security-maintenance choice inside the described CI-fix authorization.

Primary sources retrieved on 2026-10-29:

- [RustSec advisory source](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/pulldown-cmark/RUSTSEC-2026-0414.md): affected parsing requires `ENABLE_WIKILINKS` (including `Options::all()`); it is disabled by default; patch is `>=0.13.5`. Absence of that option could reduce direct exploitability, but does not remove this demonstrated audit blocker or justify an ignore.
- [Upstream v0.13.4 manifest](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/pulldown-cmark/Cargo.toml) and [v0.13.5 manifest](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.5/pulldown-cmark/Cargo.toml): both declare Rust **1.71.1**, below the local wiki manifest's Rust 1.82. Their declared dependencies/features are unchanged; package version changes. Thus there is no declared MSRV/features mismatch.
- [Upstream version comparison](https://api.github.com/repos/pulldown-cmark/pulldown-cmark/compare/v0.13.4...v0.13.5): this is not only a version-string/advisory metadata change; parser implementation and build/test generation change. Version compatibility alone is not executed project compatibility evidence.

The crates.io version API returned HTTP403; I did not rely on it. Upstream tagged manifests establish the cited declaration, not the published crate checksum. The parent must obtain/verify the registry-selected lock checksum through the project's normal Cargo interface.

## Material conditional risk

### PRP-01 — Parser patch compatibility is not yet executed evidence

- **Type / priority:** Conditional risk / Material before delivery, not a blocker to the bounded implementation step.
- **Locator:** Proposal lines 7, 14, 21 and 26; upstream v0.13.4…v0.13.5 comparison linked above.
- **Premise:** The security patch remains compatible with existing wiki parser/rendering behavior and the supported build environment.
- **Mechanism:** The upstream patch changes parser logic, so an affected rendering case or API/build integration could fail despite unchanged manifest features/MSRV. This is a credible unresolved premise, not an observed local regression.
- **Resolution:** Preserve the existing feature set; inspect the exact lock/manifest diff for unrelated dependency churn; run the proposed full wiki regression tests, workspace strict checks, relevant configuration checks, and final native CI at the corrected head/merge. If unexpected behavior/API changes appear, stop the affected work and resolve scope/approval rather than changing product behavior to force green checks.

## Required conditions and acceptance boundary

1. Keep original index path/bytes/hash semantics intact. The new sidecar must separately identify original input, source commit, archive locator, original full member path, member byte length/hash, archive byte length/hash, and reconstruction instructions outside the current checkout. An archive-aware audit must resolve the original record through that mapping and verify every original record; a missing loose file alone must not be mislabeled corruption or silently skipped.
2. Verify two independent `git archive` invocations are byte-identical, with exactly one regular member; validate final archive/member bytes before removing the loose proxy. Preserve raw reports, logs, historical command paths, hashes, tested-source identity, and old index bytes. Do not assert the archive supplies the historical environment or newly qualifies Windows.
3. Keep Rust-only policy, CI controls, existing ignores, native SDK/assets, and product defaults unchanged. The patch does not grant ABI/license/runtime upgrade authority or fix existing cxx/tokio warnings.
4. Reproduce and pass the unchanged policy/package checks on the new synthetic merge against pinned main `9f69a0bd4b468eaa8652d933d518fafb8c596b6a`; distinguish head-only preexisting helper failures. Record exact tested revision/environment and limitations. A rerun of immutable old source cannot prove the repair.
5. Obtain the final audit, compatibility checks, package/size/wiki-source checks and fresh required final review before claiming delivery complete. Pending native SDK/platform evidence must remain explicitly pending, not counted as passed. Final Windows CI is not Windows 11 desktop qualification.

These conditions substantially preserve the proposal's existing gates. No new claims/topology or invented product tests are needed for archival metadata. Existing checks plus direct archive/index audits are appropriate.

## Review limits and reusable knowledge assessment

This is a proposal decision, not an integrated-diff review. I inspected supplied canonical approval/spec text, baseline logs, prior archival diff, archive/index bytes, graph-discovered policy implementation, dependency metadata, live advisory/upstream sources, wiki memory, and applicable Scryer rules. I did not independently inspect the user's original chat or re-run parent CI/build evidence. Native compatibility and final CI remain unexecuted here. Live wiki memory supports prompt patching and auditable dependency metadata, but does not override these source checks.

No additional durable memory is warranted from this review alone: the important facts are version-bound or task-local and belong in the repair evidence.
