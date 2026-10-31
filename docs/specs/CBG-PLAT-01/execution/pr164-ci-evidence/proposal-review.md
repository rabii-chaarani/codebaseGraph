# PR164 CI repair proposal v1 — bounded adversarial proposal review

## Verdict

**Proceed with the specified Windows test-only repair; no blocking finding for that bounded implementation.** This verdict includes the coordinator's reviewed clarification: put the exact standalone fixture source in `tests/support/mcp_client_fixture.rs`; consume it with `include_str!` in the Windows process tests and `include!` inside a private `#[cfg(test)]` module in `src/client_process.rs`, retaining its `main` with `const _: fn() = main`; emit the fixed JSON responses through `println!("{response}")`; compare the project dry-run target with the canonicalized existing repository directory plus the exact `.mcp.json` leaf.

This is not permission to close PR164, declare native Windows passing, repair COPY without root-cause evidence, merge, or release. Coordinator authorization remains required before edits.

## Decision and authority

Reviewed `/tmp/pr164-ci-debug/proposal-v1.md`, baseline `84cdb27386ea4d86166135d82736a62deb2bd1ba`, and the coordinator's concrete design clarification. Decision: allow a recoverable, narrowly bounded test repair under existing S04 approval, not change product contracts. The authoritative scope is `docs/specs/CBG-PLAT-01/spec.md` rev0.11 S04/R04/A07–A08, particularly lines 168–181, and `approvals/cod55-windows-client-discovery.md`: approved full SHA `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`; no aggregate Windows desktop qualification or merge/release approval.

Read the requested adversarial-proposal-review skill and relevant engineering/integration/governance lenses. Recalled repository memory; consulted the wiki command-context guide and Scryer rules/model. Scryer confirms lifecycle `node-136`, `resp-25`, child `resp-30/31`; identifiers in the proposal omit their hyphens. Graph semantic search and explicit change-impact context discovered `knowledge/architecture/windows-mcp-command-context.md`, whose source anchors identify the two client helpers and Windows process tests. Additional graph lookup identified lifecycle and stdio verification source. No COPY source was read.

## Verified causes and repair constraints

### PR164-P01 — Fixture is invalid Rust, not evidence of a client failure

**Established issue; material, resolved by the proposed repair.** The five response statements in `tests/windows_client_discovery.rs:406–410` pass raw JSON as a Rust format string. Literal braces remain format syntax even in a raw string. CI `/tmp/pr164-ci-debug/failed.log` reports this compiler error at fixture construction for all five fixture-dependent Windows cases, before those tests exercise their real behavior. The supplied independent reproduction `/tmp/pr164-ci-debug/baseline-fixture-compile.log` reports all five failures at fixture lines 8–12.

Move the exact reusable standalone source, not a second copy, into the test-support file. The private host-test include must compile the same bytes that `rustc` receives on Windows. The proposed response-array and `println!("{response}")` form fixes format parsing without changing response payloads. `#[cfg(test)]` prevents a shipped production module/API; referencing `main` prevents dead-code warnings without running the fixture in the host unit-test process. This is proportional regression coverage for the observed blind spot. Do not remove native process assertions or change the dedicated batch invocation/security policy.

### PR164-P02 — Project dry-run assertion confuses path spelling with target identity

**Established issue; material, resolved by the proposed repair.** CI `failed.log:957–961` / `excerpts.log` records actual `\\?\D:\a\_temp\...\repo space\.mcp.json` versus expected ordinary drive spelling for the same repository and leaf, at test line 195. The process test's preceding profile, explicit-path, and override checks ran before this assertion. `client_config::repository_config_path` selects `repo_root.join(".mcp.json")` for generic project scope; lifecycle resolves the repository root before path selection. The approved requirement is to preserve the selected project target, not require the caller's nonverbatim root spelling.

Canonicalize only the already-created `repo space` directory and append `.mcp.json` for this expectation. Do not canonicalize the nonexistent dry-run leaf, normalize away arbitrary path differences, or relax unrelated profile/override/explicit-path assertions. Keep the exact selected repository and leaf equality. This repair changes the test premise, not product behavior.

### PR164-P03 — COPY discrepancy remains an independent closure blocker

**Decision-critical evidence gap; decision-blocking for a complete CI-repair claim, not for the Windows test-only edit.** CI `failed.log:1999–2004` establishes `bounded_production_chunks_reopen_with_primary_keys_connectors_and_search` observed 53 against expected 56 at `tests/copy_phase_process.rs:417`. It does not establish whether the fixture, parsing, node persistence, or expectation is wrong. Source access/discovery is explicitly pending; I did not read that test. Do not lower the count or infer a harmless platform difference. Obtain authorized graph discovery/source access and an exact missing-node/root-cause explanation before choosing a repair. A product-semantic change needs the applicable contract/scope decision first.

## Conditions and validation

- Host fixture compile coverage must use the exact standalone fixture source; no duplicate source string and no new public production API.
- Preserve response payloads and all native/batch target, argument, visibility, unsafe-input, fallback, and unrelated-config assertions.
- Run targeted host tests, formatting, and strict requested Clippy after edits. These establish compilation/static quality, not Windows process behavior.
- Native Windows CI on the final pushed head must execute the repaired tests and the remaining failed/cancelled coverage. The fixture compiler error masked later assertions, so a static repair cannot promise those assertions will pass.
- Keep the native-test profile/bundled-extension policy unchanged. Report cancelled jobs as unexecuted/incomplete rather than observed product failures. Native Server CI is distinct from Windows 11 desktop qualification.
- Keep COPY and any newly exposed runtime failures open until independently explained and verified; do not weaken gates or add blind retries/timeouts.

## Strengths and limits

The proposal correctly distinguishes fixture compilation, target spelling, independent COPY counts, and cancellations; preserves security/product assertions; and stops on new semantics. That supports a bounded test repair rather than a speculative product patch. This was a read-only assessment: no project edits, Git writes, external mutations, test runs, or platform probes. Primary evidence was the approved local spec/approval, supplied execution logs/reproduction, graph-discovered source, wiki rationale, and Scryer intent. No external research was necessary for the demonstrated compiler and path-equality failures. No reusable knowledge record was added: findings are task-local until the repair is executed and verified.
