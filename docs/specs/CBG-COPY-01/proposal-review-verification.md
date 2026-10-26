# CBG-COPY-01 — Focused proposal revision verification

## Recommendation: proceed to the user's plan-approval decision

**APR-001, APR-002 and APR-003 are resolved at the specification level. No unresolved material finding remains from this focused review.** Revision 0.2 supports the recommended Scope A without adding automatic Scope B retries, a production full-graph integrity validator, or a streaming-plan subsystem.

This is a reviewer recommendation about **the plan only**, not user approval or permission to implement. Implementation behavior, tests, resource feasibility and platform qualification remain unexecuted gates.

## Reviewed snapshot and method

- Final frozen approval candidate: `spec-r0.2-final-candidate.md`, SHA-256 **`1a0ea5f860ce2764e1666bd6345faf9bb24929f35795255772c04380839b4dd2`**.
- Canonical `docs/specs/CBG-COPY-01/spec.md` was byte-for-byte equal to that final candidate and matched that exact hash during verification.
- Compared revision 0.2 with the frozen revision 0.1 and the original independent report. Canonical `docs/specs/CBG-COPY-01/proposal-review.md` preserves that original report unchanged.
- Verified the final candidate against the earlier revision 0.2 freeze (`dd0bc0dae859355529328b497dc613f1513bb891a11be0f5a3ca5b67945d7c22`). Only three changes occurred: review status now refers to the separate resolution record, the requested decision explicitly names error suppression and bounded diagnostics, and a stray space was removed. None changes the requirements or acceptance assessed below. This report's verdict is bound to the **final candidate** hash above.
- Reused the exact source evidence established in the original review. No new broad research or implementation verification was performed. Source locators below describe previously inspected code, not a claim that a fix now exists.
- Same independent reviewer context and mandated GPT-6.1 Sol / Max assignment; focused follow-up, not a substitute fresh final implementation review.

## Finding resolution

| Stable ID | Status | Revision evidence and assessment |
| --- | --- | --- |
| **APR-001** — native-error suppression | **Resolved for plan approval** | Scope A.1 (line 51), R1 (81), implementation step 3 (98) and A1 (108) explicitly remove the blanket native `Err` → success mapping for all three former substrings in both writer paths. Successful idempotent SQL/results remain successful. A1 now tests at the query-result boundary, so a runner-level error injection cannot conceal suppression inside a phase. The behavior change is exposed for sign-off (57). This directly covers the prior `write.rs:376–393` and `phase.rs:305–322` concern without inventing a general SQL parser or declaring every idempotent operation unsafe. |
| **APR-002** — bounded diagnostic ingestion | **Resolved for plan approval** | Scope A.4 (54), R6 (86), implementation step 3 (98) and A8 (115) bound ingestion before allocation and the assembled isolated-phase diagnostic under the **existing 64 KiB** budget. They explicitly cover selected-main guarded/inherited routes and avoid unnecessary successful-exit stderr reads. Metadata and truncation framing share the output budget. A8 checks bounded read requests/bytes or allocations plus an oversized file, not only returned string length. The scope correctly excludes later API envelopes, keeps typed resource errors and cleanup ordering, and adds no retention policy or increased constant. This addresses the prior full-file reads at old `phase.rs:135` and inspected main `phase.rs:210,281`. |
| **APR-003** — COPY-control-list scaling | **Resolved** | The risk table (127) states that retained/cloned statement lists grow with chunk count and that per-file bounds do not bound all control state independently of cardinality. A7 (114) records retained/control cost, statement/file count and high-water marks under unchanged limits. It distinguishes normal successful publication from a separate deliberate resource-abort test. R3 (83) reuses Graph Writer `node-81` / `resp-273`; no new architectural claim is invented. A streaming-plan subsystem remains conditional on measured necessity and separate approval. |

## Affected assumptions and counterevidence rechecked

- **Normal-build usability is a real gate.** A7 now requires the existing representative previously successful/default profile to publish correctly under unchanged limits. Safe termination of an intentionally over-budget fixture cannot pass that requirement. A failure returns to the owner rather than authorizing a higher ceiling or an implicit retry design.
- **Configured pool is not measured resident memory.** A7 explicitly avoids that inference and distinguishes the materialization calculator's admitted range from a plain low-level write request. The prior review's virtual-reservation and allocator-release counterevidence still defeats a claim that starting at the full configured pool is already proven infeasible. Actual RSS and workload results remain pending.
- **File bounds have precise semantics.** R3 now requires equal existing record/output limits and counts framing/header bytes, including the case where an unframed row fits but its complete file does not. This is compatible with the previously inspected sinks; it is not a new chunk-size setting or a native-memory-fit guarantee.
- **Search needs regression coverage, not a rewrite.** The new source-evidence row (37) acknowledges the existing contiguous-chunk iterator. A9 (116) places searchable terms in base and later files, reopens in a separate process, and checks genuine PK-index results as well as connectors/search. This matches the prior `search_index/build.rs:97–101,197–235` evidence and does not expand into sparse-staging repair.
- **No new correctness overclaim.** The revision retains uncertainty about the original incident and distinguishes preserved prior native observations from planning activity. It does not infer rollback from a native error, claim Ladybug 0.19 safe, or require routine host-disk-full experiments.

## Remaining gates, not unresolved review defects

1. The user selects Scope A and its availability/pool behavior, or requests a different plan. Removing hidden native-error suppression is explicitly part of the proposed behavior. **Scope B remains excluded** (59 onward).
2. Select and recheck the clean then-current-main baseline, preserving lifecycle ownership and containment safeguards. Unknown teardown still blocks destructive cleanup/new mutation.
3. Freeze/commit the agreed specification and record the actual user decision, exact scope/full SHA and required Scryer sign-off. Linear synchronization remains pending availability. Neither this verification nor the original review is approval.
4. After authorization, implement and execute the planned query-level, bounded-reader, actual-isolated-fault, reopen PK/connector/search, successful-resource-profile, deliberate budget-abort and normal platform/final-review gates. None has been executed by this review.

Only this report was written. No specification/repository/service state, constants, Git state, database, daemon, dependency, model or tracking record was changed; no builds, tests or probes ran. The existing uncertainty memory already covers the reusable lesson, so no new memory entry is warranted.
