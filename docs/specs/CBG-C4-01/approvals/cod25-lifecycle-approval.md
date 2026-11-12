# COD-25 actual narrow lifecycle amendment approval

## Actual decision and exact subject

- Work: CBG-C4-01/F04/S01 / COD-25, R04/R12, A06.
- Actual human answer: **Approve narrow amendment**, received in this coordinator chat on 12 November 2026, Australia/Darwin.
- Decision locator: `request_user_input_async` call `call_b2f905aab56145fa9e8ec13b728bb075`, item 0.
- Approved full Git SHA: **`5b408ea994aabd537d9ae7f7c7c8f19c83c32435`**, tree `8fa4cd8a3a7e144abd48245eeb7951906f136c9d`, [draft PR195](https://github.com/rabii-chaarani/codebaseGraph/pull/195).

The question explicitly named this full SHA and the two paths below, adding only `src/process_lifecycle/mod.rs` to COD-25's permitted files for caller-controlled startup and typed retained recovery, while preserving the legacy API/defaults/protocol and every Q1 guarantee. It excluded other lifecycle/platform/storage/maintenance files, Q2/Q3 expansion, merge and release. The answer was actual human input, not an inferred default, reviewer result or issue status.

| Approved path | Immutable subject / exact bytes |
| --- | --- |
| `docs/specs/CBG-C4-01/cod25-lifecycle-contracts.md` | [lifecycle supplement0.1](https://github.com/rabii-chaarani/codebaseGraph/blob/5b408ea994aabd537d9ae7f7c7c8f19c83c32435/docs/specs/CBG-C4-01/cod25-lifecycle-contracts.md); 11,092 bytes; blob `bd019c8efebe248536a887dc4393552544eda0ee`; SHA-256 `8305174a0c898311010706a6bff4f5de40fa9e9930dad1d69627a0cf4df76e53` |
| `docs/specs/CBG-C4-01/execution/COD-25.md` | [brief3](https://github.com/rabii-chaarani/codebaseGraph/blob/5b408ea994aabd537d9ae7f7c7c8f19c83c32435/docs/specs/CBG-C4-01/execution/COD-25.md); 22,257 bytes; blob `8a6b0f1f06fb8fda65149048258738393aae5f2d`; SHA-256 `c5cd53f8640bd47d8c2363522257921e4221229e266bad617a42b677e8814185` |

This record does not embed its own containing commit SHA. Later progress/assignment records do not change these approved subject bytes. PR195 remains frozen at the approved head.

## Original approval and combined bounded scope

The original [C25 supplement0.1 and brief1 at f7](https://github.com/rabii-chaarani/codebaseGraph/blob/f7f10a21c45914b369fdfb9e8f0f364f10efc235/docs/specs/CBG-C4-01/cod25-contracts.md) and [actual original approval](https://github.com/rabii-chaarani/codebaseGraph/blob/e36e9e581441f9ead8e6ff5febec7234e5a6c5e8/docs/specs/CBG-C4-01/approvals/cod25-approval.md) remain valid and unchanged. This decision supplies the explicit one-file prerequisite exception; it does not replace the original scope or waive C25-A01–A08. C25-L01–04/C25-LA01–04 also govern that prerequisite.

Runtime execution retains the lifecycle owner. Add the neutral controlled preparation callback and opaque actual pending-recovery ownership inside the existing module; preserve existing prepare callers, defaults, fields and protocol. No Native Architecture types enter that lower module. Keep the cooperative/opaque-OS interruption limitation explicit; this approval is not proof that blocking OS calls can be interrupted or that product behavior passes.

Root must commit/publish this actual decision and versioned assignment brief, verify remote availability, recheck readiness and issue bounded GO before source changes. The sole complete technical assignment remains default/gpt-6.1-terra/medium/fork_turns:none for the approved source/file list. The former session was quiescent after read-only feasibility analysis and made no product edits/builds/tests; a fresh runtime session may resume that same assignment after its state is re-established. No concurrent writer or authority is introduced by the session reset.

Product baseline remains exact `7c81720f3cb32c392e34833033b0ccec6766e2ca`, distinct from the original pilot. Actual lifecycle behavior, original Q1 pilot sufficiency, compatibility/resource tests, independent execution, fresh final review and essential-only product PR/publication remain required. Other unassigned files or unresolved contracts still stop affected work for their owner.

No logs, execution/review evidence artifacts, checkers or unnecessary docs may be added to PRs. This actual approval and execution history stay on their separate canonical records branch and are linked from PR prose/Linear. No merge/release, weaker guarantee, extra language/provider registry, storage/maintenance takeover, authority migration or downstream/aggregate closure is granted.
