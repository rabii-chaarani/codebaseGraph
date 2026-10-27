# CBG-COPY-01 R1 — independent final code review

## Verdict: source GREEN; qualification remains pending

**No confirmed P0, P1, P2 or P3 production defect survived review.** The reviewed implementation follows the approved removal of enforcement/options without replacing the ceilings with sentinel limits. The corrected native-jumbo reopen test and actual MCP schema check now pass. This is not an unconditional acceptance/release pass: final affected/full-suite and platform checks remain separate gates. No merge/deployment approval is implied.

### Identity and authority

- Actual reviewer: `openai-codex/gpt-6.1-sol`, thinking `max`, default/fresh independent context, as recorded in `review-gates-and-model.json`; reviewer did not implement changes. Required repository `AGENTS.md`, adversarial-review skill and engineering lenses were read.
- Actual source: `/private/tmp/codebasegraph-copy153-scope-a`; baseline `8e47d2b3c74b72b56a0b6b212d1bf939c8088ca7`; documentation HEAD `82077d051efb3ac5c38f5aba0a0ec88f677c1e36` plus uncommitted integrated source/test changes.
- Governing amendment: `docs/specs/CBG-COPY-01/resource-ceilings-amendment.md` R1, approved commit `c8dd4ea9ca2a6f992756a2a41828396a27eabb6d`, verified file SHA-256 `40fa2007539bf26e13a88a3c894428e3cee9dc46230d647e4be6bc1c2ca9b696`, approval companion and historical Scope A. Live read-only Scryer rules/orientation and `chg-vhbvd3` four reworded claims were checked; wiki/memory recalled. No model/tracking synchronization is claimed.
- Reviewed 54-file source/test snapshot: `final-source-hashes-02.json`, with the following later **test-only** additions/final formatting inspected (current source hashes in `final-source-hashes-03.json`; its three wiki changes are outside this code-review boundary):
  - `src/adapters/cli/tests/mcp.rs`: `22cd4cef51368a0f3050641e7e437d7fbc4d798e6337b87c3bf1b446cf3081bc`
  - `src/staging_writer/spill.rs`: `a1870eac16d7299ef937178b9520e68ff7603a308b7a5f59ed1b63c248035efd`
  - `tests/copy_phase_process.rs`: `dbff1132733d5557a7108033f9bb41b8e5968357614c1b1bb8a4344fc046424b`
- Original-root graph discovery/context preceded actual-worktree source inspection; newer modules were followed through imports. The registered graph is not a fresh index of this worktree. Source and baseline diff, not stale graph coordinates, supply the conclusions below.

## Confirmed findings

**None (P0–P3).** The first native-jumbo run used an invalid full-text oracle; its corrected metadata oracle now passes without production changes. The failed attempt remains recorded, not counted as passing evidence.

## Challenged contracts and counterevidence

1. **Public removal and error contract (RM1/RA1).** Fields/defaults/validation/forwarding were deleted through public `MaterializationRequest`, native request, config, refresh overrides/status, CLI and MCP startup. Spill/concurrency/parallel controls remain. Config uses ordinary Serde unknown-field behavior; old CLI flags fail parsing. `src/api/core.rs:1281–1283` now maps ordinary materialization errors without the obsolete memory-specific branch; removal of its narrowly corresponding test does not remove unrelated error mapping. `MemoryBudgetExceeded` and pool-bearing constructors/exports are gone, consistently with the approved contract, not a compatibility shim. MCP tools derive from the registered descriptor schemas (`src/adapters/mcp/tools.rs:10–43`, `src/api/core.rs:139–261,584–644`); materialization is not an MCP tool and no registered schema carries these knobs. The new actual tools/list negative assertions are at `src/adapters/cli/tests/mcp.rs:208–212`.

2. **No replacement workload ceiling (RM2/RA2).** Source, partition and artifact estimates are now observations, not admission decisions (`src/execution/parallel.rs:216–318`, `src/artifact_store.rs:508–545`). Snapshot length/hash, artifact envelope/key/payload/raw-row validation remain. Native opens use `SystemConfig::default()` with only independent read-only/thread settings (`src/db_writer/access.rs:114–149`, `src/db_writer/phase.rs:552–555`), not an oversized application constant. Exact cached lbug 0.19 Rust SDK defaults pool size to zero; its native constructor resolves zero by physical-memory auto-sizing. Native/OS/address-space limits still exist and are not promises of unlimited capacity. No dependency change is needed for the approved behavior.

3. **Streaming and jumbo merge progress (RM3/RA3).** `SortedSpool::push` flushes existing rows before overshoot and a jumbo singleton immediately afterward (`src/staging_writer/spill.rs:153–194`). Merge levels group at most 32 runs and strictly reduce any count above 32 (`:197–236`); heap-budget rejection was removed, not replaced by whole-table buffering. JSON/CSV sinks rotate only nonempty chunks, avoiding endless empty rotation on a jumbo row (`src/staging_writer/accumulator.rs:715–759,826–875`). u32 spill framing remains. Search occurrence spill remains bounded-fan-in; chunk-derived document admission is gone, while the existing independent 1 MiB reader-format limit is now shared with the writer (`src/search_index/io.rs:57–75`, `model.rs:3`). This format restriction is approved RM5 scope, not a concealed chunk ceiling.

4. **Failure/ownership preservation (RM4/RM5).** The diff removes RSS termination predicates and budget-result plumbing but retains guardian cancellation, owner-loss, teardown proof, receipts and unknown-containment handling (`src/process_lifecycle/mod.rs:400–547`, `src/materialization_worker.rs:183–296`). RSS high-water observation remains. Database leases and phase ownership remain. COPY remains sequential with early hash indexes, no same-candidate replay or existing-object suppression; the scheduling/error tests and real Managed/Direct preservation test support this. Maintenance's own pass budgets/`ResourceLimit`, metadata/diagnostic framing and corruption checks were not removed.

5. **Private process protocol compatibility.** Removed fields do not invalidate old receipts/new readers: ordinary unknown fields are ignored and the baseline's removed receipt fields are optional. Bootstrap registers the current executable for worker/phase launches (`src/bootstrap.rs:12–19`); guard/phase paths launch that executable. Same-version intended operation is coherent. This does **not** establish mixed old/new executable support after replacement-in-place: unchanged v2 is not a cross-release negotiation mechanism. Keep the existing coordinated daemon/helper restart precondition; no new migration/framework is justified by the inspected scope.

## Executed evidence inspected; not executed by reviewer

- `check-04.log`: all-targets/features compile success before later minor core/test/lint edits.
- `focused-unit-02.log`: **134/134 passed**. Earlier focused01 had a CLI-test message mismatch; parent changed only the assertion. Focused02 predates the final dead-parser/import cleanup, which subsequent strict Clippy compiled.
- `clippy-02.log`: strict Clippy success; `normal-cli-build-final.log`: ordinary CLI build success without warnings. Final test-only additions postdate these runs and need affected rechecks.
- `public-settings-tests.log`: **42/42 passed**; `doctests.log`: **3/3 passed**.
- `native-copy-01.log`: **2/2 passed**, including small-target chunk/control parity, reopen/PK operator/connector/search and COPY-acknowledgement failure with Managed/Direct prior-bundle preservation and independent later rebuild.
- `legacy-budget-config-process.log`: **1/1 passed**; a 4 MiB source refresh succeeds with ignored stored worker/Rust values 2/1 MiB.
- `ownership-no-ceiling-process.log`: **1/1 passed**; fixture-owned 8 MiB loader allocation stays alive and recovers. This is not an actual exceedance of the old 512 MiB worker setting.
- `final-added-unit-cases.log`: **2/2 passed**, 1,027 jumbo runs through multiple levels and mixed small/jumbo sorted-value parity.
- `actual-mcp-schema-test.log`: **1/1 passed**, actual stdio tools/list schema has no retired memory fields.
- `native-jumbo-record-02.log`: **1/1 passed** after a test-only oracle correction. Real CLI spill target 1 MiB accepts a >1 MiB staged COPY, and a new read-only database open in the independent test process parses metadata JSON and confirms the full 2 MiB payload; search also succeeds. See `tests/copy_phase_process.rs:282–319`.
- `native-profile-compile.log`: optimized `native-tests` profile compiles the COPY and refresh-recovery integration binaries; compilation alone is not execution.

## Remaining conditions and limitations

- **RA3 failed-attempt resolution:** `native-jumbo-record.log` failed because it expected the complete comment in `SyntaxCapture.text`, which is a preview field. The parent diagnosed a 160-character preview and >2 MiB `metadata.canonical_key`; the corrected test reads `CAST(n.metadata AS STRING)`, parses the JSON and checks full padding. I inspected that corrected assertion and the passing `native-jumbo-record-02.log`. No production change was made or required. The existing native control case separately proves logical rows/IDs/endpoints/search parity; the new jumbo case proves full large-value persistence/reopen and search, but does not independently compare every jumbo graph row to a different spill-target control.
- Complete final affected compile/format/strict-Clippy checks against the final hashes, any remaining ownership/cancellation/owner-loss regressions, broader/full suites, optimized native-test execution and ordinary macOS ARM64/Windows x86_64 CI, or explicitly retain their pending qualification labels. Actual MCP schema, mixed-jumbo and native-jumbo checks have now passed. Pre-amendment binaries are not R1 qualification.
- In-place upgrades with old running daemons/helpers remain conditional on the existing coordinated restart rule, not verified mixed-version support. Increased native/OS memory use and real OOM/full-disk failure are the user's approved tradeoff, not a defect to reintroduce ceilings for.

**Side effects/assumptions:** only this report was written; no implementation, constants, Git/model/tracking/service state or dependencies changed by reviewer; no builds/tests/probes launched. Scope is the approved enforcement/options amendment, not an audit or redesign of all graph execution. No newly established durable lesson warrants a memory write under the read-only task.
