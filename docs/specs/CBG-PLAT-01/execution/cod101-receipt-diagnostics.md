# COD-101 — canonical receipt diagnostic checkpoint

The failure in [CI 40608776432, job 122554842902](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40608776432/job/122554842902) is **not yet reproduced or causally fixed**. The durable-detach case failed during `CanonicalFixture::new`, before its interruption boundary: the normal producer returned `Written`, but the fixture did not observe a complete receipt. The referenced diagnostic JSON was absent from the uploaded artifact.

This checkpoint preserves optional tracking and every existing receipt, ownership, interruption, recovery and payload assertion. It logs ignored producer-stage errors only in test builds and emits the fixture's diagnostic JSON into the retained test log, adding payload length/hash without printing payload bytes. It does not add retries, relax the fixture or change production publication behavior.

## Authority and trace

- [COD-101](https://linear.app/codebasegraph/issue/COD-101), related to [COD-52](https://linear.app/codebasegraph/issue/COD-52). User decision: the explicit 2026-10-28 request to debug/fix the linked failed job on PR157's branch.
- Existing canonical wiki `knowledge/architecture/graph-disk-cleanup-contract.md`, DC-S5 revision 0.1 R5/R6/R8 and A2/A3; unchanged DC-S2/S3 ownership contracts. No new specification or architecture contract is introduced.
- COD-52 R01/A01–A02 retains [specification 0.10 at approved SHA 1a9db7b558b60248888408daa6268a3b45f2f0df](https://github.com/rabii-chaarani/codebaseGraph/blob/1a9db7b558b60248888408daa6268a3b45f2f0df/docs/specs/CBG-PLAT-01/spec.md) and [its original approval](../approvals/cod52-macos-nested-scans.md).
- Scryer proportional ledger `chg-nw1t96`, existing Graph Store `node-88` / `resp-232,233` context. The connected model belongs to the older root checkout; the target has no model. No model copy, invented claims, root test verdict ingestion or unrelated drift reconciliation occurred. Target graph discovery was performed explicitly.
- Source baseline `2a392a1b49f25923291ad90944148aa72a5caace`; failed synthetic merge `4a9bef13247bf2faa1f311e4de0328ebd1dc67d3` has an identical Git tree. Changed implementation/fixture anchors and hashes are in [the reviewed patch](cod101-evidence/diagnostics.patch) and [snapshot](cod101-evidence/snapshot.json).

## Executed evidence and limits

- Current checkpoint: **27/27 selected existing tests pass**, zero failures/errors; **819 not executed**. This includes actual durable detach, payload unlink, genuine producer collection, origin semantics and rooted traversal. [JUnit](cod101-evidence/targeted-tests.junit.xml), [log](cod101-evidence/targeted-tests.log), [environment/commands](cod101-evidence/provenance.json).
- Strict locked workspace/all-target/all-feature Clippy with warnings denied passes; formatting and whitespace checks pass. [Clippy log](cod101-evidence/clippy.log).
- Investigation before this checkpoint: unchanged exact case 12/12 passes; temporary diagnostic exact case 20/20 passes; one optimized predecessor-plus-failing-case pair passes; one temporary real producer experiment completes 1,000 tracked publications. These observations do not establish a cause. Repeat records and frozen diagnostic identities are retained in [the evidence directory](cod101-evidence/).
- Additional native-pair repetition was deliberately interrupted. A sandbox full run was interrupted after coordinator IPC denial. A subsequent unsandboxed frozen diagnostic baseline run is still in progress. Neither full run is counted as passing, and neither tests this later checkpoint.
- Current macOS 26.6.2 ARM64 / Rust 1.96 execution is not Rust 1.82, minimum-macOS, Windows, whole-matrix or consumer qualification. Hosted execution of this checkpoint remains pending.

Fresh read-only GPT-6.1 Sol/High proposal review cleared the bounded diagnostic experiment. Fresh read-only GPT-6.1 Sol/Max [checkpoint review](cod101-evidence/review.md) found no confirmed P0–P3 defect. Its readiness verdict applies to diagnostic preservation only. The underlying fix and COD-101 acceptance remain open; aggregate/milestone closure stays with the maintainer. No merge, deployment or service restart occurred. No new reusable-memory claim is warranted by an unreproduced cause.
