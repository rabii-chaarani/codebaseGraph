# PR164 — qualification evidence release-policy repair

## Cause and scope

[Job123869404257](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40996079962/job/123869404257), run40996079962 at documentation head `f2936a4c956c3cd90a45089dc7f7f041e7aef477`, failed **Check repository release policy**. Package verification/size checks were skipped; the job is not evidence of a Cargo packaging error. [Job metadata](pr164-windows11-ci-policy-evidence/job.json).

The exact local reproduction is:

```text
cargo run -p xtask --locked --offline -- release-gate
FAIL: python-file-present: docs\specs\CBG-PLAT-01\execution\pr164-windows11-evidence\assert-supplemental.py must not be maintained source.
```

Exit1; [before log](pr164-windows11-ci-policy-evidence/release-gate-before.log). Hosted log download was denied by the API (403); failure-stage attribution comes from public job metadata, and the specific diagnostic is reproduced locally against the exact failing commit. The existing `check_rust_only_files` scans all repository paths, including documentation, for `.py` files. The qualification documentation introduced the only `.py` path. That was a documentation-delivery error, not a Windows product regression.

## Minimal correction

- Rename the completed one-off external assertion source to `assert-supplemental-source.txt`, explicitly an immutable **evidence transcript**, not maintained/runnable repository tooling.
- Preserve every source byte and its SHA256. Update the report link and checksum **pathname**, not the captured script or completed execution results.
- Keep historical external command records at their original `.py` path; changing them would falsify what ran.
- No release-policy exception, workflow, product source, dependency, constant, test-selection, timeout or acceptance change. No new PR.

## Verification

- Same release-gate command after the rename: **exit0, `release gate passed`**. [After log](pr164-windows11-ci-policy-evidence/release-gate-after.log).
- The archived source SHA256 is unchanged: `796f0522664831dcdf79ab67e4e3f8e7c29470d6e4821283dac7c2461f33c17c`. All54 original evidence checksum entries still match after the pathname update. [Verification](pr164-windows11-ci-policy-evidence/verification.json).
- `cargo package --locked --offline --list`: exit0; qualification evidence is not included in the root crate. This is package inventory, **not** full `cargo publish --dry-run` execution. [List](pr164-windows11-ci-policy-evidence/package-list.log).
- Source, tests, xtask policy, workflows, manifests and dependencies remain unchanged. The original Windows21+6 tests and six supplemental outcomes are retained; they are not rerun or expanded by this archive-only change.
- [Independent repair review](pr164-windows11-ci-policy-evidence/repair-review.md) verifies the correction's policy/evidence boundary. Raw repair evidence has its own [SHA256SUMS](pr164-windows11-ci-policy-evidence/SHA256SUMS).

Full hosted-job success is distinct from the local policy check and must be verified on the new commit. No merge/release/closure or new PR is performed. Reusable-knowledge assessment: this is a revision-bound documentation-delivery mistake; the existing explicit repository policy is the durable authority, so no new repository memory candidate is needed.
