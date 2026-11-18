# CBG-RESTORE-01 / PR200 final qualification

Evidence-only handoff, 18 November 2026 UTC, for [COD-136](https://linear.app/codebasegraph/issue/COD-136) and [PR #200](https://github.com/rabii-chaarani/codebaseGraph/pull/200). This record adds no product changes or requirements. It is published on a separate qualification branch so the tested PR head remains unchanged.

## Authority and immutable subjects

The unchanged restoration revision 0.1 was approved at **d809fe0f8289bbf7d167a891bd7c31160f27f0e1**; see [actual approval](approval.md), [specification](spec.md), [ledger](preservation-ledger.md) and [execution assignment](implementation-go.md). The approved CI and security amendments and [DIAG-CI850-0.1 decision](windows-reset-correlation-approval.md) remain separate exact-scope approvals. No merge, release, installation, daemon/client configuration, live-store operation or native YAML activation is authorized here.

| Subject | Verified identity |
| --- | --- |
| Published and CI-tested PR head | ad38fe56f17f64ef104ce7556e20e9748e0df55e |
| Head tree | 111af2d8db5fa981a972176c7b33e11d898b585e |
| Head parent / last technical commit | de0b9d48c7a5e81178c25b2cce44ab1bace26fa0 |
| PR target dev | 2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9 |
| CI synthetic merge | d5feee36f66e77c6f675f9ca65ecb27db2e0aa5a |
| Synthetic merge tree | 111af2d8db5fa981a972176c7b33e11d898b585e |
| Final workflow | [CI856, run44238431967, attempt1](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/44238431967) |

The synthetic merge parents are the recorded dev and PR head. Independent publication readback checked the head, base, merge, tree and native workflow reference. The report-only ad38 addition preserves all code/test/build/workflow inputs of reviewed de0b. Historical passing runs retain their own revision subjects.

## Executed final gates

CI856 completed **25 of 25 jobs successfully**, including formatting, strict Clippy, audit, repository policy, crate publication dry-run, all macOS ARM64 and Windows x64 native test groups, both native artifact jobs, retention and the required aggregate. Root and fresh independent verification read the terminal job results; no test rerun was requested to replace a failure.

The final Windows logs explicitly show passing execution of:

- `direct_read_survives_candidate_open_without_share_delete` (integration shard3/job134580301784).
- `direct_read_reports_candidate_residue_and_preserves_shared_writer_exclusion` (integration shard2/job134580301745).
- `direct_read_selectors_report_candidate_residue_without_disposal` and `owner_controlled_close_and_client_early_close_are_single_submission` (unit shard3/job134580301706).
- `owner_writer_finishes_normally_before_client_reads` (unit shard4/job134580301704), permit-removal outcomes (unit shard1/job134580301650), and application-read progress buckets (unit shard2/job134580301680).
- The previously failing legacy-worker and missing-guardian recovery cases (integration shard1/job134580301843), plus manual-publication writer exclusion (integration shard4/job134580301663).

The macOS unit and integration logs independently show the corresponding applicable cases passing. Mac artifact job134596549967 and Windows artifact job134588956934 succeeded. Hosted execution now covers the socket/Direct/recovery paths that were not executable locally at de0b; the [local report and its restrictions](windows-response-observation-evidence.md) remain unchanged.

Artifact-job evidence is the successful terminal CI result and provider/run metadata. The independent verifier did not inspect those jobs' source/smoke logs, download final native artifact bytes or verify their manifests. No independent byte-level or installed-artifact validation is claimed.

The [Direct-read correction](pr200-direct-read-evidence.md) at 0837d7ff489bc6a17a7f2ba788fb1e05efd7e288 preserves the no-journal shared read lease and reports optional candidate residue through existing cleanup_pending on both Direct selectors. Required journal recovery, containment, writer exclusion and operation-owned cleanup retain their strict behavior. It adds no automatic collector or cleanup-only public interface. This addresses thread PRRT_kwDOSZ0Wus64bVWQ/comment4599530494.

Fresh independent verification used default/GPT-6.1 Terra/medium/fork-none. Fresh prompt-only adversarial final review used default/GPT-6.1 Sol/xhigh/fork-none and the required adversarial-review-engineer method. The final source/report rebind passed with no confirmed P0–P3 defects. Independent execution and compilation are distinguished in the linked reports. The owner full907 run was retained; no second full local suite was run. Subsequent focused64, atomic2 and non-network10 results remain bound to their stated snapshots.

## Preservation and proportional reconciliation

The durable archive remains under `/Users/rabii/.codex/visualizations/2026/11/17/01a1e723-ed0a-7b81-a2f8-23de6974c9c0/restore-v304-preservation`. Original branches, approvals, staged/untracked working data, COD-25 committed and recovered uncommitted inputs, earlier evidence and frozen binaries remain preserved. The original replay and independent audits are recorded in [execution.md](execution.md) and [implementation-evidence.md](implementation-evidence.md).

The final bounded audit confirms all11 original HEADs, logical indexes and statuses, plus11211 of11212 captured entry identities. The sole additional content delta is ignored AGENTS.md's assignment table: captured original and current versions are both archived and independently hashed, without attributing authorship. Its current version is not claimed byte-identical to the original. The original main and separate C4 checkpoints, live configuration and active generation remain unchanged.

An app-internal checkpoint ref name is absent; its exact original tree is preserved by an explicit preservation ref. A second later app checkpoint tree is pinned separately. Original user branch refs remain unchanged. No lost implementation was recreated from descriptions. The original75 already absent/prunable worktrees and excluded live/cache/generated data retain their documented recovery limits.

The current full bundle is88618773 bytes, SHA256 **3dae665a7c69884a3e3892c3c9ce7806cf7c2397cf23753cc5ef862720a81d59**. Recovery of ad38/tree111af was proven in a separate clean checkout from the independently recovered 8db full bundle plus the33840-byte incremental bundle, SHA256 **c48a31e59218d17e20df700f3f0479453d3badad18d6724092b38a97bed878f5**, with bundle verification, fsck and no alternates to the original repository. A separate attempted standalone full clone failed; its cause is unknown and it is not claimed as successful recovery.

All418 entries in the listed prior executable/dependency manifests and all12 payloads in the four C4 manifests rehashed successfully. This is a bounded manifest claim. Original frozen evidence stayed in place. Recorded cache retirement was limited to owned, inventoried compiler-only material; cargo-reclaim configuration/service remained unchanged.

Existing committed Graph Store/coordinator responsibilities remain applicable. Root officially closed only its two empty administrative ledgers chg-krba2h/chg-7m5d5h; get_pending then listed the same42 foreign open changes and neither owned ledger. Empty-ledger history uses administrative abandonment terminology; the restoration PR remains active. No foreign claim fold, source-fingerprint verdict or native authority activation is inferred.

Private receipts: `implementation/pr200-direct-read-followup/ci856-final-jobs-confirmed.json`, `diagnostic-verification/`, `preservation-final/post-ad38-report.json`, `preservation-final/post-ad38-instruction-supplement.json` and `response-ad38-chain-recovery.json`. They retain commands, actual results, hashes and qualifications rather than replacing original failed evidence. No new reusable memory claim is warranted beyond the existing frozen-binary/provenance procedures.

## Handoff limits and separate C4 work

This qualifies the stated restoration snapshot and supports resolving the identified Direct-read comment. Earlier Windows10054 connection-reset and historical LEAK causes remain unknown. Diagnostic progress/closing markers do not prove peer delivery, OS-close timing or a causal fix. CI856 success does not erase those observations or establish reliability beyond executed coverage.

PR #200 remains unmerged and targets dev. Native C4 remains separate: contracts cf9493edce3692054c4d6c17b53d2fdb562b632e / PR #201 and types6b6b273d0c37e43929c7d01aae11822783ad9bbe / PR #202 preserve their original approvals and evidence. The fresh companion coordinator receives this exact verified baseline for its own prerequisite reconciliation. Existing companion results are not relabelled as ad38 qualification; no force-push, automatic queue/issue/milestone closure, COD-25 completion or COD-24 implementation follows.
