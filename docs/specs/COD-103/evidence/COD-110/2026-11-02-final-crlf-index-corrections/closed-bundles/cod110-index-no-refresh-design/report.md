# COD-110: prevent diff’s real-index stat refresh

**Disposition:** routine restoration of approved compatibility §2 (`49612ec66814ad6b883a0c4c3c624911bd5a8825`, separate approval `754c82…`, user decision `2917ddb7-8746-407a-a37d-23b1e72396bf`). No new behavior approval, claim, planning/proposal-review or full-review stage. Existing `316be4…` Design§9 and Scryer node-1/wzt intent remain unchanged.

## Minimal proposed change

Backend owns **ci.rs only**, production invocation plus one checkout regression. At current `crates/xtask/src/ci.rs:581`, keep existing helper and error/exit handling; replace only its argument array:

- Old: `["diff", "--quiet", expected_sha, "--"]`
- Proposed: `["-c", "diff.autoRefreshIndex=false", "diff", "--quiet", expected_sha, "--"]`

Effective command: `git --no-replace-objects -c diff.autoRefreshIndex=false diff --quiet <expected_sha> --`, with existing `GIT_OPTIONAL_LOCKS=0`, environment cleanup, cwd and bounded output unchanged. `-c` applies to that process and overrides repository/user configuration; it persists no setting. Exit0 still means clean; exit1 still rejects differences; other exits still fail closed.

Do **not** change the earlier `diff --cached --quiet` at453–457. It compares index↔tree, not worktree; the actor trace proves no refresh there. Do not broaden all Git invocations, switch plumbing, clone/restore indexes, refresh then restore, change an index hash oracle, add fallback/dependency/framework, or alter settings/permissions. Lower-level diff-files is documented unaffected by autoRefreshIndex but would require a different proof/error path without benefit here.

Keep Git-normalized tracked-byte hashing, HEAD/index/flags/modes/symlink/required-six proof and staged/cancelling/hidden-dirt rejection. Preserve the CRLF candidate fix exactly: current ci.rs SHA256 `3c52c2c8e01262cf530c1e2c286cd77c8b7c50aace8fbf532cdc605c48178570`; release.rs `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594`. Exact `verify_checkout` and `git` helper bodies match pre-CRLF862bb3 source read earlier: this defect predates that fix, not CRLF causality.

## Primary documentation and bounded corroboration

Installed `git --version` is2.52.0. Its `git-config(1)`/`git-diff(1)` documents `diff.autoRefreshIndex=true` by default: porcelain diff silently runs `git update-index --refresh` for stat-only changes with identical content. `git(1)` documents command-local `-c` overrides file configuration. Exact local paths/excerpts: `git-2.52-primary-doc-excerpts.txt`.

One authorized disposable comparison, not product validation: private tracked bytes unchanged, mtime deliberately back10s; with `GIT_OPTIONAL_LOCKS=0`, command-local false diff exited0 and raw index remained SHA256 `d5313aa87dbe05d7961681a4a9e63a10298f83593d0544c000ac70539972e2cb`; same stale index with true control exited0 and changed to `0f90a9b7877cd0981187668c4f9137538d56dee92d3acfc45b158d418eb8c7b8`. Raw copies/results retained. No restore or product/old-fixture writes. This establishes installed macOS Git behavior, not Windows qualification or every Git version.

Confirmed product actor evidence remains immutable: parent-session `sub-c8e15d01/cod110-crlf-macos-remaining-verification/19-isolation-result.json` and raw per-call indexes. First mutation was final call30; earlier29, shell/pretool boundaries unchanged. CLI0 while six ctime/mtime cache fields changed violates the approved no-refresh requirement even with unchanged blobs/modes/flags. Preserve both historical failure scopes; do not relabel them passes.

## One meaningful regression

New test in existing ci.rs checkout fixtures, e.g. `verify_checkout_accepts_stale_stat_without_mutating_index`:
1. Use `checkout_fixture`; save Cargo.toml bytes and original modified time.
2. Set its mtime to original minus10s using the supported std file timestamp API; assert timestamp differs and bytes remain exact. No sleep, index refresh or pre-verification porcelain diff. This creates clean content with stale cached stat. Ten seconds is a deterministic test displacement, **not a production constant**.
3. Read `.git/index` as raw bytes immediately before `verify_checkout`.
4. Require `Ok(())`, exact original metadata bytes and **entire raw index byte equality** afterward—not merely `ls-files` output. Reuse existing tracked/flagged/mode/dirt/CRLF tests to ensure no rejection is weakened; strengthen their before/after index oracle where already assigned rather than inventing duplicate cases.

Implementation owner must confirm supported Rust std timestamp API against the project toolchain; no dependency is needed. Existing fixture has a normal private `.git/index`; no production copy-index mechanism is proposed.

## Existing next gates / stop

Backend implements only that invocation and regression. Evidence Verifier repeats exact-final-source affected behavior on **both local macOS and the local Windows VM before any PR update**, including stale-stat complete-index equality, real release-metadata LF/all-six-CRLF paths, and relevant clean/dirty/hidden/index-cancellation/mode/ordinary regressions. Run project format/lint/xtask checks with genuine evidence; no synthetic JUnit. Original Sol/MAX Final Reviewer verifies this focused correction; then Production obtains new own full CI. No former failed proof is waived and no new review stage is created.

Designer: mapped Sol/XHIGH; self-runtime metadata unexposed. No source/build/Cargo/VM/Scryer/API/tracker writes, model calls, nested dispatch or settings/dependency changes. Only the authorized new private bundle and disposable Git fixture were written. Reusable knowledge assessed: no memory write. Private lease closed; zero owned Bash handles. Next owner: Engineer→Backend.
