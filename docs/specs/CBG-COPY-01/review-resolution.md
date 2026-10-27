# CBG-COPY-01 — Proposal review resolution

- Canonical scope: [spec.md](spec.md), proposed revision 0.2.
- Implementation authority: none. The user requested planning and explicitly withheld implementation sign-off.
- Initial independent reviewer: fresh/default GPT-6.1 Sol at Max effort, required proposal-review skill; [full initial report](proposal-review.md).
- Reviewed initial snapshot: revision 0.1, SHA-256 `54d51d60096d1038485f3d335fc25911bac4d092afbfaee034fff3e2697c64a2`.
- Current final candidate: revision 0.2, SHA-256 `1a0ea5f860ce2764e1666bd6345faf9bb24929f35795255772c04380839b4dd2`.
- Focused verification: [independent final-candidate readback](proposal-review-verification.md), SHA-256 `81e472f12bfde54b5a0a6bf964d137750ae3e68b803250e7a85f21a0ed4fb7f5`. APR-001–003 are resolved at the specification level; no unresolved material finding remains. Reviewer recommendation does not replace user approval.

| Finding | Proposed resolution in revision 0.2 | Verification |
| --- | --- | --- |
| APR-001 — Existing-object substring suppression contradicts the all-mutation failure rule | Scope A.1, R1, implementation step 3 and A1 remove blanket native `Err` suppression in both paths; actual successful idempotent SQL remains valid. Query-result-level tests supplement phase-runner tests. | Resolved for plan approval; independently verified against the final candidate. |
| APR-002 — Trimming only after full stderr ingestion is not bounded allocation | Scope A.4, R6, implementation step 3 and A8 require bounded file-tail/window ingestion before allocation and a capped assembled phase diagnostic, on both selected-main native routes including successful exits. Existing 64 KiB budget/retention/containment remain unchanged. | Resolved for plan approval; independently verified against the final candidate. |
| APR-003 — COPY statement/control state grows with chunks | R3 links existing Graph Writer intent; risks disclose retained/cloned strings; A7 records statement/file count and control-state cost under unchanged limits. No streaming-plan framework is added. | Resolved for plan approval; independently verified against the final candidate. |

Additional clarified gates: search already enumerates contiguous chunk files and needs cross-chunk search parity, not a reader rewrite; A7 separately requires a representative normal successful/default profile and a deliberate budget-abort case. Low-level request pools are not falsely constrained by the materialization calculator. [Prior research evidence](investigation-evidence.md) is kept distinct from unexecuted fix acceptance.

Planning checks found all current local spec links and cited line ranges valid and all six requirements/ten acceptance IDs present. Inspected source hashes remain unchanged. These are document checks, not compilation, native tests, runtime qualification or user sign-off. Linear remains pending verification; existing Scryer drift is not reconciled or folded. No code, dependency, configured constant, database or daemon changed.
