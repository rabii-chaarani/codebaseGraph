# CBG-COPY-01 — PR #153 baseline-adaptation review

## GREEN — proceed with approved Scope A

**No new decision-blocking premise or scope blocker was found in the actual PR #153 baseline.** Scope A can be implemented on `5839055d9279f4475f893a54abeaa542344c1b02` without replacing its newer maintenance/containment design or reopening the settled PR-head choice. This is a proposal/baseline verdict, not an implementation, test or release approval.

### Reviewed identity and boundary

- Fresh read-only review under the mandated GPT-6.1 Sol / Max, fresh/default assignment; used `adversarial-proposal-review-engineer/SKILL.md` and the governing original-root `AGENTS.md`.
- Actual worktree: `/private/tmp/codebasegraph-copy153-scope-a`. Inspected HEAD `92f3c14a7f61cf01c8e598aea8e6c3c73ccdf274`: approval-record commit above spec commit `c5b62ba77d2f94b71714b5319f3b3fa215983f9f`, above the selected PR baseline.
- Canonical specification SHA-256 verified: `1a0ea5f860ce2764e1666bd6345faf9bb24929f35795255772c04380839b4dd2`. Read `approval.md`; the exact user decision approves Scope A and PR #153. Its explicit baseline decision supersedes the draft's provisional main recommendation.
- Read the prior focused verification. APR-001–003 remain resolved at specification level; no broad reinvestigation or Scope B design was performed.
- Source discovery used the registered original-root graph first, then explicit semantic `graph_context(run_isolated_phase, runtime)`. Graph health identified `/Users/rabii/Projects/Repositories/codebaseGraph`; its older indexed line ranges are **not** evidence of current PR source. All current-source conclusions below come from direct worktree reads and comparison. A read-only Git diff confirmed the inspected writer/phase/run/accumulator/writer/xtask files have no changes relative to the selected PR baseline.

### Focused premise checks

| Approved requirement | Actual PR-baseline evidence | Assessment |
| --- | --- | --- |
| R1/R2: one write attempt, configured request pool, no native-error suppression | `src/db_writer/write.rs:25–36,218–249,379–396` retains the known fallback retry, isolated pool loop and suppression. Its only delta from the registered original writer is the database-use lease at lines 28–30. `src/db_writer/phase.rs:497–568,711–746` retains the corresponding native query/checkpoint/suppression boundaries. | The approved fix targets the actual exposure. Removing retry/suppression does not require a new recovery owner or a changed pool ceiling. Preserve the newer lease across the whole invocation. |
| R4: preserve first-chunk/index ordering | `write.rs:119–159,264–291` already completes the first successful repeated-table COPY before its hash index and processes connectors only after node indexes. | No new sequencing premise or missing detector. Keep this ordering; stop on failure rather than replay or repair. |
| R3: reuse equal existing spill/final limits | `src/execution/run.rs:60–75` still passes `usize::MAX` through `COPY_FILE_CHUNK_BYTES`. `src/staging_writer/accumulator.rs:739–829,869–979` already rotates sequential JSON/CSV files and accounts for complete JSON framing / connector headers when checking a single row. | Passing the existing spill bound to both limits fits the approved design. No native list/glob support, new setting or table-wide buffering is needed. |
| R5/R6: preserve newer ownership, containment and retention while bounding diagnostics | `phase.rs:121–188` holds a database-use lease, resolves absolute DB paths and chooses inherited versus standalone guarded execution. Full stderr reads remain at lines 247 and 318. Guarded teardown checks are at 287–340; registered-root cleanup restrictions and ownership are at 344–369,578–708. `run.rs:25–47` holds artifact/database use leases. The accumulator keeps the legacy staging guard through its result (`26–40,65–91,420–433`), uses owned output/finalization (`803–826,942–975`) and defers registered spool disposal (`1002–1037`). | Both diagnostic routes require the approved bounded-reader/assembled-context change; they must not be collapsed into the old path. Existing cleanup authorization, typed RSS failure, ownership/provenance wrappers and guard lifetime remain intact. The spec already requires this; no new architectural decision is needed. |
| Unchanged supported-platform gates | `crates/xtask/src/main.rs:21–35,52–56` defines exactly `MacosArm64` and `WindowsX86_64` and rejects other target names. | Qualification remains macOS arm64 and Windows x64 only. Older wiki/memory statements mentioning Linux or macOS Intel are historical, not authority to broaden this delivery. |

The attempted falsifier was that the newer maintenance-owned phase/output paths make the approved local retry/diagnostic/chunk changes depend on a different cleanup or retry architecture. The source does not support that concern: Scope A changes the load policy and bounded I/O at existing boundaries while retaining the current owners. No revised scope or user decision is required before implementation.

### Limits and remaining gates

- The existing normal-profile RSS, control-list scaling, actual isolated mutation/reopen, PK/endpoint/search, preservation/cleanup, bounded-reader and supported-platform acceptance gates remain mandatory. None is claimed to pass here.
- Scryer orientation/rules and existing storage/recovery wiki contracts were read through their registered original-root services. Those services expose original-checkout state, not a fresh PR-head model reconciliation; implementation reconciliation remains with the coordinator. Linear is verification-pending; no tracking synchronization is claimed.
- No builds, tests, native probes, dependency changes, Git writes, repository edits, model/tracking writes or daemon actions ran. Only this report was written. **No constants changed.** The only baseline assumption used is the explicit approved PR-head target, independently checked against the inspected source files.
- Reusable knowledge assessed: this is a narrow source-adaptation verdict, not a new verified runtime fact. The existing uncertain-COPY-outcome memory already covers the durable lesson; no memory write is warranted.

Independent fresh final code review remains a separate gate after implementation and executed evidence.

### Inspected source SHA-256

- `src/db_writer/write.rs`: `d916186b25d4c0b074b476eaba3225f535987f8590c0019f6453d509e77afb8d`
- `src/db_writer/phase.rs`: `a0afc7c8be36c10ae7dcc839ccb9abd821dd3a2966b93564434e0d6473902b97`
- `src/execution/run.rs`: `899acb0f8bf2cdeb55a1f232694ae6d5d4ef4aee61db33235eebfcd1d5dc3470`
- `src/staging_writer/accumulator.rs`: `359385201e8af617957ee854be2069092b1cdebfc96c5b738962c874ce445481`
- `src/staging_writer/writer.rs`: `10d0980acecb45a9965d7fe2ee03ad123088eed25ec732d3e78662e7659990f0`
