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
3. The Windows COPY test separately observed 53 nodes against 56 expected at `tests/copy_phase_process.rs:417`. Its root cause remains unresolved. The connected graph does not index that PR file; direct source-read permission was requested, not assumed. No count reduction or speculative product patch is authorized.
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

No native Windows pass or complete CI repair is claimed. The user explicitly authorized a branch-local full graph rebuild so COPY discovery can proceed without using another checkout's graph.

## Independent review

Fresh proposal review: GPT-6.1 Sol / High, using `adversarial-proposal-review-engineer`; proceed with the bounded Windows test-only repair. Fresh integrated review: GPT-6.1 Sol / Max, using `adversarial-review-engineer`; no confirmed P0–P3 findings in the three-file repair against baseline `84cdb27386ea4d86166135d82736a62deb2bd1ba`. Exact reviewed source hashes are retained in [source-hashes.json](pr164-ci-evidence/source-hashes.json); [final review](pr164-ci-evidence/final-review.md). COPY diagnosis and native execution remain separate completion blockers.

## Qualification and closure

COD-55 and aggregate closure remain open. Native Windows Server CI is distinct from Windows 11 desktop qualification. No merge, release publication, deployment or restart is authorized or performed. Existing user edits in the original and previous PR worktrees remain untouched.
