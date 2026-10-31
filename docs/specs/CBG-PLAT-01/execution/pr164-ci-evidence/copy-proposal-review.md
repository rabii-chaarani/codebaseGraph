# PR164 COPY process-identity repair — proposal review

## Recommendation

**Proceed with the exact two-file COD98 transplant. No decision-blocking or material proposal findings.** This permits implementation, not closure, merge, release, or supported-platform qualification.

- Requested reviewer: fresh GPT-6.1 Sol / High, read-only.
- Skill used: `/Users/rabii/.codex/skills/adversarial-proposal-review-engineer/SKILL.md`, including applicable proposal lenses.
- Reviewed baseline: `563ba4a308715fc09d62892c6e71c5dce5b5b496` in `/private/tmp/codebasegraph-pr164-ci-clean`.
- Proposal: `/tmp/pr164-ci-debug/copy-repair-proposal.md` (v1 plus settled reuse section), SHA256 `643a91b8cef2cae81c144d90415551a1c70654639e2e60cd7e2ac6a292d6646a`.
- Source patch: `/tmp/pr164-ci-debug/approved-cod98-source.patch`, SHA256 `a86248348434b968a8bddd979d35ab0dee6f903edac7064add881bd24f2f846a`; exact Git source diff of `5e3e42f48a55581ef75b25ba21732913089f664d`.
- Approval: `931c3ddaeccba9c3f94f75e3ae58be9e3016a4ef:docs/specs/CBG-COPY-01/approvals/process-identity-test.md`; scratch copy verified identical, SHA256 `40f3931378f657851aef0d202ab8ace69f90233a53f404436cf1f0f4fd356d6f`.
- Existing contract: CBG-COPY-01 rev0.2 at `75a99c840aa53e75a3f14df8dab41979c9efa19c`, A3/A9, with existing Scope A/resource amendments unchanged.
- Delivery: PR164/COD55 integration; historical COD98 delivery is not reopened. Proportional Scryer ledger `chg-nwmt6n` has no changed claims.

## Evidence and challenges resolved

1. **Is this a data-count fix or a process oracle fix?** Baseline `tests/copy_phase_process.rs:417–424` counts distinct diagnostic PIDs against COPY trace records. The data checks are later (`439–550`). Baseline `phase.rs:199–228` waits for each child; `231–298` waits for guarded phase completion. A numeric PID therefore is not a lifetime identity across the sequential history. Exact reuse in the failed CI run is plausible, not established by its retained trace; the proposal states that limit and does not claim data loss.
2. **Could each event mint a new identity and mask same-process reuse?** No. Proposed `test_process_instance` caches one `Result<String, String>` in a function-local static `OnceLock`; repeated calls/threads use the same identity. The 32 random bytes are generated once per executable process. Execution uses `Command::new`/spawn of the executable (`329–351`, `190`), not reuse of a preinitialized forked worker. The proposed unit checks repeat calls and a second thread. This is practical randomized identity evidence, not a mathematical no-collision guarantee; a collision fails the oracle rather than accepting same-process reuse.
3. **Does missing/malformed evidence become success?** No. The predicate requires a positive u32-range diagnostic PID and exactly64 lowercase hexadecimal identity bytes. It rejects missing/invalid evidence, fewer than two records, and duplicate identities. Deterministic cases accept the same PID with distinct identities and reject the same identity with a different PID. Generation errors propagate instead of emitting successful trace evidence.
4. **Are isolation or correctness assertions weakened?** All three historical PID-only identity assertions become the same process-instance predicate. The patch preserves actual launch/wait/containment paths; successful phases from a reused executable process remain detectable because their identity repeats. No source protocol, native dependency, resource, loading, retry, timeout, or public-contract changes occur. Exact reopen IDs, PK lookups/operator, connector endpoints, chunk/index order, single-thread, search/control parity, no-replay, Managed/Direct preservation and independent recovery checks remain in place.
5. **Is the transplant compatible with this branch?** Independently executed `git diff 5e3e42f^ HEAD -- src/db_writer/phase.rs tests/copy_phase_process.rs`: exit0, empty diff. Independently executed `git apply --check /tmp/pr164-ci-debug/approved-cod98-source.patch`: exit0. Existing `OnceLock` import is present at baseline phase.rs:13. No manifests/dependencies are changed by the exact patch. Canonical approval and patch were separately read using Git and compared byte-for-byte with their supplied copies.

## Orientation and limits

The branch-local graph CLI found `tests/copy_phase_process.rs::bounded_production_chunks_reopen_with_primary_keys_connectors_and_search` L338–552 and `src/db_writer/phase.rs` L1–1221. Explicit change-impact context and a phase-trace callgraph lookup were run before conclusions. Connected repository graph MCP is unavailable for this branch; the configured MCP/model points to the older checkout, so branch-local CLI was used for implementation discovery. Scryer rules/orientation, scoped empty ledger and Materialization Worker resp-281 were read; existing claims remain unchanged. k-wiki bundles, memory recall/search and materialization-pipeline context were consulted. No new architecture/model intent is required. Existing approval is reused only for its exact narrow debug/test oracle scope.

No Cargo/native suite was run by this reviewer, preserving coordinator ownership of Cargo targets. Baseline12/12 phase-unit success was reported by the coordinator, not independently executed here. Historical22/22 or previous native CI results are design evidence only, not qualification of the new PR164 head. The specific failing CI log was supplied through the proposal/parent; this review independently verified the matching source assertion, not a raw phases.jsonl from that run. No external records, Git state or repository source were changed by this review.

## Conditions before completion

After transplant, require actual executed evidence for the five-case native COPY suite, phase13-unit suite, fmt and strict Clippy. Require fresh Sol/Max final integrated review, then final-head native macOS/Windows CI with exact source/binary/native provenance. Preserve failures and keep platform qualification separate from local success. No skips, weakened checks, retries, larger timeouts/resources, or unrelated COD52/contract/evidence transplant are authorized by this verdict.

Reusable knowledge assessed: existing atomic temporary-path/process-scope and COPY uncertainty memories cover adjacent durable lessons; this bounded proposal review establishes no new reusable runtime result, so no repository memory was recorded.
