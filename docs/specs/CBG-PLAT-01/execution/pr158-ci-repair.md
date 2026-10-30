# PR158 — repair failed CI policy and audit checks

## Scope and authority

[COD-53](https://linear.app/codebasegraph/issue/COD-53), CBG-PLAT-01/S02.
User request on 2026-10-29: “at the moment debug and the fix the failing ci job of PR 158”.
The existing [approved spec rev0.10](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md)
and [S02 approval](../approvals/cod53-windows-native-runtimes.md) remain unchanged.
Scryer `chg-fq8t0y` and execution brief `chg-h07kt8` record the bounded repair.
No new architectural claims or public contract changes were needed.

Baseline PR head: `5f68b41cebc327072cf71be770c9e5a9d1c0dad2`.
[Failed CI40798473142](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40798473142)
checked synthetic merge `f49fbb5afe9e6c24ac410a65b61a646d72078aa2`
against main `9f69a0bd4b468eaa8652d933d518fafb8c596b6a`.

## Causes and repair

1. The package job's unchanged Rust-only repository gate rejected a loose historical
   Windows harness input, `reject-proxy.py`. Preserve its exact 1,311 bytes in
   `cod53-windows-qualification-evidence/historical-reject-proxy.tar.gz`, under its
   original repository-relative member path. `archived-inputs.json` maps the
   unchanged historical index to this member. All 65 original index records,
   including their original hashes and CRLF bytes, still verify. No raw report,
   command or historical verdict was rewritten. This follows the established
   byte-preserving archival procedure from [PR159](https://github.com/rabii-chaarani/codebaseGraph/pull/159),
   rather than adding a policy exception.
2. The audit rejected `pulldown-cmark 0.13.4` for
   [RUSTSEC-2026-0414](https://rustsec.org/advisories/RUSTSEC-2026-0414).
   Raise the wiki manifest lower bound to `0.13.5` and resolve only that lockfile
   package to `0.13.5`. Features and other dependency versions are unchanged.
   The downloaded crate checksum matches the new lock entry. Its declared Rust
   minimum remains 1.71.1; this is not an executed full-workspace Rust1.82 claim.
   The existing consumers do not enable the vulnerable wikilinks option, but
   that does not justify bypassing the audit. Existing cxx/tokio audit informational
   warnings and the guarded time exception are unchanged, not repaired here.

No product numeric constants, Ladybug/extension ABI or assets, Windows defaults,
loader/installer behavior, CI controls or advisory ignores changed.

## Executed verification

Exact commands, working-file hashes, environment, results and limits:
[verification.json](pr158-ci-repair-evidence/verification.json).

- Audit0.22.2: reproduced baseline failure; patched scan exits0.
- Unchanged release gate on the actual combined-tree contents plus the repair:
  pass. A deliberate temporary loose Python file is rejected; removing it
  restores the pass. The PR head alone still contains four older helper inputs
  already archived separately on main by PR159; they are absent from CI's merge.
- Full wiki and xtask suite: **174/174 pass, 0 skipped**, with
  [JUnit](pr158-ci-repair-evidence/wiki-xtask.junit.xml).
- Combined-tree xtask suite: **44/44 pass, 0 skipped**; the release gate also
  passes after adding canonical evidence. [Checks](pr158-ci-repair-evidence/merged-checks.json).
- Formatting and strict workspace/all-target/all-feature Clippy: pass.
- Normal Cargo publish dry-run: pass, including extracted source-package compile
  with the unchanged pinned Ladybug0.19 SDK. Nothing was uploaded.
- Generated root source archive: **8,458,182 bytes ≤ 10,485,760**. All four bundled
  XZ payloads match source. Wiki archive creation passes with the CI path patch;
  its manifest contains the patched parser lower bound. This is not an isolated
  wiki archive compile.
- Windows-filtered default and explicit root opt-out feature resolution: pass;
  not native execution.
- Two independent Git archives are byte-identical. Archive/member hashes and
  all65 historical records verify; original index and other evidence bytes
  remain unchanged.

Local host: macOS ARM64, Cargo1.96.0. The first baseline compilation used an
incorrect include directory and failed; the corrected environment compiled
without a source change. Both logs are preserved. The registered graph/Scryer
binding describes the original older checkout, not the isolated PR branch;
exact source comparisons and local snapshot hashes are retained, and no
unrelated drift was cleared or branch-specific passing fingerprint invented.

## Post-push main synchronization

The reviewed repair was pushed as `359d946e91c6cb3ee7e170b73c6e9bab577b0b04`.
GitHub could not create its PR merge or start CI: the parser declaration was
adjacent to main's package-version update. A normal pinned-main integration
retains main `9f69a0bd4b468eaa8652d933d518fafb8c596b6a` and the repair history.
The only manual conflict resolution keeps `codebase-graph 4.0.2` and
`pulldown-cmark 0.13.5` in the wiki manifest. No PR merge, rebase, release,
new scope or changed approval is implied.

The resolved critical file hashes match the combined contents already used for
local policy/package verification. The actual integrated tree passes **176/176
wiki/xtask tests, 0 skipped**, fmt, strict workspace Clippy, unchanged release
gate/audit and the normal extracted-package publish dry-run. Its generated root
archive is **8,458,173 bytes**; the earlier package measurement remains a separate
historical result. [Integration checks](pr158-ci-repair-evidence/integration-verification.json)
and [Sol Max focused integration review](pr158-ci-repair-evidence/merge-final-review.md)
are retained separately; no confirmed P0–P3 defects were found. Older results are
not labeled new-head CI.
Historical raw evidence imported unchanged from main and the original PR has
preexisting whitespace/CRLF diagnostics. It was not rewritten to satisfy a
blanket diff check; the new manual Cargo resolution passes its scoped check.

## Reviews and remaining gates

Fresh independent **GPT-6.1 Sol / High** proposal review cleared the bounded
implementation with compatibility checks required:
[proposal review](pr158-ci-repair-evidence/proposal-review.md).
Fresh independent **GPT-6.1 Sol / Max** integrated source/evidence review found
**no confirmed P0–P3 defects** and cleared the bounded repair for commit/push:
[final review](pr158-ci-repair-evidence/final-review.md). It checked staged tree
`649f78da13f2e044309ebf78953730aa57b1b25c` against the baseline, including the
tracked archive, hashes, original index and executed checks. The review-link
update receives focused verification; future native CI is still a separate gate.

Final pushed-head native macOS/Windows CI is pending. Prior Windows11 execution
remains bound to its original source and prepared environment; this repair adds
no desktop/clean-consumer qualification. Full COD-53 S02/A03/A04/fullA16 and
maintainer aggregate closure remain open. No merge, release publication,
deployment or service restart is authorized.

Reusable knowledge assessed: no new memory is needed. Existing archival practice
and exact-revision release lessons were applied; version-bound results belong here.
