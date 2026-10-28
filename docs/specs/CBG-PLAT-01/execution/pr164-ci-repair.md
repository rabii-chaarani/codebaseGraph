# PR #164 CI repair — COD-55

## Identity and scope

- Work: [COD-55](https://linear.app/codebasegraph/issue/COD-55), CBG-PLAT-01/S04, R04 / A07–A08.
- Canonical approval: [specification rev0.11](https://github.com/rabii-chaarani/codebaseGraph/blob/8a410c7a22f5291f06a1cece5cd9e7c0be255e72/docs/specs/CBG-PLAT-01/spec.md#s04--use-explicit-windows-user-directory-and-executable-discovery-policy), [exact approval](https://github.com/rabii-chaarani/codebaseGraph/blob/949db168fcf128cc1fe6e73cb70a14db832d50b5/docs/specs/CBG-PLAT-01/approvals/cod55-windows-client-discovery.md).
- Repair baseline: `84cdb27386ea4d86166135d82736a62deb2bd1ba`; [failed/cancelled CI40616153340](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40616153340).
- User request: “debug and fix ci jobs of this PR https://github.com/rabii-chaarani/codebaseGraph/pull/164”. Existing approval and this repair request cover behavior-restoring tests; no new scope, architecture, release gate or closure ownership.
- Scryer proportional repair ledger: `chg-hpmj66`, signed off with no changed claims. Lifecycle `node-136`, `resp-25/30/31`; verification `resp-32` remain unchanged. The MCP binding points to the older original checkout, not this worktree; branch fingerprints must not be inferred from that binding.

## Verified failures

1. Windows fixture compilation fails on five raw JSON `println!` format strings. Reproduced independently on macOS with the exact embedded fixture source and `rustc --edition=2021`; exit 1, five invalid-format errors. This prevented the fixture-dependent native process assertions from running.
2. Windows generic project dry-run returns the selected repository's canonical verbatim path. The test expected its nonverbatim spelling. The selected `.mcp.json` leaf is unchanged; the repair compares the canonical existing repository parent plus that exact leaf. It does not canonicalize a nonexistent dry-run file or weaken target checks.
3. The Windows COPY test separately observed 53 unique numeric PIDs across 56 COPY phase records at `tests/copy_phase_process.rs:417`. Earlier log-only diagnosis could not identify the counted quantity; branch-graph discovery established that this is a historical PID uniqueness assertion, not a graph node count. Sequential processes may reuse terminated numeric PIDs. Existing separately approved [COD-98](https://linear.app/codebasegraph/issue/COD-98) fixes precisely this test/instrumentation oracle; its source patch applies to byte-identical parent files without changing loading or data contracts. Reuse is undergoing fresh proposal review and final-head verification. No count reduction or weakened isolation/data check is authorized.
4. Windows unit shard 2 and macOS integration were cancelled before final results. They are incomplete evidence, not established product failures. Required/native artifact jobs cannot pass until their dependencies finish.

## Repair and verification

The two Windows test repairs share one standalone fixture source. The integration test reads it with `include_str!`; a private `cfg(test)` module compiles those exact bytes with `include!` in every host unit build. The five JSON response payloads are unchanged. Production behavior, public APIs, argument policy, constants and resource limits are unchanged.

Executed on macOS ARM64, rustc 1.96.0:

- Baseline resolver unit suite: 10/10 pass before edits.
- Standalone repaired fixture: Rust compilation and process smoke pass; five JSON responses parse with IDs 1–5.
- Standalone exact fixture Windows x86_64 MSVC metadata compile with `-D warnings`: pass; compile-only, not Windows execution.
- Affected slice suite: 58/58 pass, zero selected failures/errors/skipped; 795 other cases were not selected and are not claimed passing. Separate target directory rebuilt the root package from this worktree. `cargo nextest run --locked --lib --profile pr164 --config-file /tmp/pr164-ci-debug/nextest.toml -E <exact 58-case selection>`; serial execution. [JUnit](pr164-ci-evidence/affected.junit.xml), [log](pr164-ci-evidence/affected-tests.log).
- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: pass; zero warnings. [Log](pr164-ci-evidence/clippy.log).

No native Windows pass or complete CI repair is claimed. The user explicitly authorized a branch-local full graph rebuild. It completed with 420 parsed artifacts, 1,860,077 node rows and 4,079,344 edge rows; branch-local semantic search and explicit change-impact context then discovered the COPY test and phase trace source. [Rebuild summary and discovery](pr164-ci-evidence/branch-graph-rebuild.json). The original checkout's graph was not used as evidence for missing branch source. Rebuild cleanup was reported pending; no complete reclamation is claimed.

## Independent review

Fresh proposal review: GPT-6.1 Sol / High, using `adversarial-proposal-review-engineer`; proceed with the bounded Windows test-only repair. Fresh integrated review: GPT-6.1 Sol / Max, using `adversarial-review-engineer`; no confirmed P0–P3 findings in the three-file repair against baseline `84cdb27386ea4d86166135d82736a62deb2bd1ba`. Exact reviewed source hashes are retained in [source-hashes.json](pr164-ci-evidence/source-hashes.json); [final review](pr164-ci-evidence/final-review.md). COPY diagnosis and native execution remain separate completion blockers.


## COPY process-identity integration repair (COD-98 reuse)

The reconstructed branch graph discovered the exact assertion. `copies.iter().map(pid).collect::<BTreeSet<_>>().len() == copies.len()` requires all historical sequential children to retain globally unique numeric PIDs. Its failure does not prove graph data loss. The contract requires separate process instances, not nonreused historical numeric identifiers.

The existing [COD-98 exact-scope approval](https://github.com/rabii-chaarani/codebaseGraph/blob/931c3ddaeccba9c3f94f75e3ae58be9e3016a4ef/docs/specs/CBG-COPY-01/approvals/process-identity-test.md) authorizes the debug/test instrumentation repair under CBG-COPY-01 rev0.2 A3/A9. Approved implementation `5e3e42f48a55581ef75b25ba21732913089f664d` changes only `src/db_writer/phase.rs` and `tests/copy_phase_process.rs`. Both current files are byte-identical to that patch's parent; source-only applicability check passes. No unrelated COD-52 source, old execution evidence or canonical spec is copied. Existing contracts/approval locations are linked, not duplicated.

Proportional Scryer `chg-nwmt6n` is signed off for this reuse under the current PR CI-fix request and existing same-scope approval. No new claims/topology; `run_isolated_phase`/`resp-281` release-resources behavior is unchanged. COD-98 remains Done for its historical delivery; this branch integration/qualification is tracked on COD-55.

The reused oracle uses one process-lifetime identity, stable across calls/threads, with numeric PID retained as validated diagnostic evidence. Deterministic coverage accepts reused PID with distinct instances, rejects repeated instance even with changed PID, and rejects absent/malformed evidence. All three historical PID comparisons use the same predicate. No per-event token, retry, timeout, skipped case or production loading/containment/protocol/resource/dependency change.

Current baseline nearby phase units: 12/12 pass. Fresh GPT-6.1 Sol / High proposal review passed with no blockers. Exact source-only transplant completed; both files are byte-identical to approved implementation `5e3e42f48a55581ef75b25ba21732913089f664d`. [Source hashes](pr164-ci-evidence/copy-source-hashes.json), [transplant report](pr164-ci-evidence/copy-transplant-report.md), [proposal review](pr164-ci-evidence/copy-proposal-review.md). Executed current-source local macOS verification: **13/13 phase unit tests and 5/5 native COPY suite cases pass, zero selected skips**. The real chunked/reopen/PK/connector/search/control case passed in 120.220s; jumbo row in 28.961s; Managed+Direct acknowledgement-failure/no-replay/old-bundle preservation/recovery in 148.412s; both deterministic identity cases pass. The captured rebuilt CLI hash is `0b3b9bca048662fd6f839b4d9047cf2611b044b4b989460bbcb91928e30ded93`. `cargo fmt --all --check` and strict workspace/all-target/all-feature Clippy pass. The full sequential command chain completed exit 0. [Native JUnit](pr164-ci-evidence/copy-native.junit.xml), [phase JUnit](pr164-ci-evidence/copy-phase.junit.xml), [native log](pr164-ci-evidence/copy-native-tests.log), [provenance/commands](pr164-ci-evidence/copy-provenance.json).

Fresh GPT-6.1 Sol / Max integrated source review and focused completed-execution addendum found no confirmed P0–P3. Reviewer independently checked the exact source/CLI hashes, phase 13/13 and native 5/5 JUnit, final strict checks and completed exit-0 command chain. [Final review and addendum](pr164-ci-evidence/copy-final-review.md). Native final-head macOS/Windows CI remains pending, distinct from local Mac success and Windows consumer qualification.

## Qualification and closure

COD-55 and aggregate closure remain open. Native Windows Server CI is distinct from Windows 11 desktop qualification. No merge, release publication, deployment or restart is authorized or performed. Existing user edits in the original and previous PR worktrees remain untouched.
