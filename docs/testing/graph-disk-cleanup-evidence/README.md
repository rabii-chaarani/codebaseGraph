# Historical Python evidence helpers

[COD-99](https://linear.app/codebasegraph/issue/COD-99) archives the four Python
helpers from commit `7038e66ff8271ffddc511f879cf8d28353a09b2d`. The user approved
archiving them on 2026-10-27. They are retained historical evidence, not maintained
implementation or supported verification tooling. The Rust-only repository policy
is unchanged.

`historical-python-helpers.tar.gz` preserves each file's original bytes and repository-relative
path. [The manifest](./historical-python-helpers.json) records the source commit,
Git blob IDs, byte lengths, SHA-256 digests and exact archive creation command.
The archive SHA-256 is `032ac4ef9cfddf40dc0540a913f71c34de4901a4a7d61d035141cb96832e7519`. Two independent `git archive`
invocations produced identical bytes; each regular member matched its original
file byte-for-byte before removal.

Historical reports, source manifests, test outputs and their hashes are unchanged.
Their `.py` paths still identify their original inputs, now stored in this archive.
The archive does not turn earlier partial evidence into current qualification.
The canonical DC-S1–DC-S5 contract remains
`knowledge:architecture/graph-disk-cleanup-contract`.

## Reconstruct historical inputs

Use a separate temporary checkout of the revision named by the report you are
reproducing. Do not extract into the current checkout: loose Python source would
correctly fail the repository gate again. The archive contains only the four
helpers, not their historical environment or every report prerequisite.

From this checkout, inspect and verify the archive before extracting:

```bash
shasum -a 256 docs/testing/graph-disk-cleanup-evidence/historical-python-helpers.tar.gz
tar -tzf docs/testing/graph-disk-cleanup-evidence/historical-python-helpers.tar.gz
```

For an unused temporary directory, reconstruct the archive's source revision:

```bash
git worktree add --detach /private/tmp/codebasegraph-historical-7038e66 \
  7038e66ff8271ffddc511f879cf8d28353a09b2d
tar -xzf docs/testing/graph-disk-cleanup-evidence/historical-python-helpers.tar.gz \
  -C /private/tmp/codebasegraph-historical-7038e66
```

All file members are under `docs/testing/graph-disk-cleanup-evidence/`, so sibling
imports and original relative paths are preserved. Verify their hashes against
the manifest. Follow the selected historical report's commands and prerequisites;
this archival change does not claim that historical helper tests were rerun or
that all inputs can be reproduced from the archive alone.

## Release limitation

The original [failed job](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40453997235/job/122023180345)
checks immutable source `7038e66`. A fix on a later commit cannot make that source
pass by rerunning it. Corrected publication needs a trusted replacement release
PR and its own successful exact-SHA CI under [the release process](../../release.md).
Version selection, replacement of PR #156, tag/label changes and publication are
not approved by the archival decision.
