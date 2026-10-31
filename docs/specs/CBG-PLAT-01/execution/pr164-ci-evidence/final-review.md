# PR164 bounded Windows CI repair — final independent review

## Result

**No confirmed P0–P3 findings. Proceed with the bounded three-file patch push.** This is not whole-PR CI approval, native Windows qualification, aggregate closure, merge, or release approval. COPY 53/56 and failed/cancelled coverage remain separate blockers.

Actual reviewer: fresh independent **GPT-6.1 Sol, Max effort** (`openai-codex/gpt-6.1-sol`, `thinking=max`, coordinator-confirmed spawn settings). Used `adversarial-review-engineer/SKILL.md` and the applicable engineering lenses. No implementation authorship.

## Reviewed snapshot and authority

- Worktree: `/private/tmp/codebasegraph-pr164-ci-clean`.
- Verified HEAD/baseline: `84cdb27386ea4d86166135d82736a62deb2bd1ba`. Reviewed the working-tree diff plus the new untracked fixture; not the original PR's complete implementation.
- Scope: `src/client_process.rs`, `tests/windows_client_discovery.rs`, `tests/support/mcp_client_fixture.rs` only. Read-only caller/context inspection was limited to the client path policy, lifecycle handoff and stdio verification.
- Governing contract: `docs/specs/CBG-PLAT-01/spec.md` rev0.11, S04/R04/A07–A08, approved at `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`; current spec bytes match that commit. Approval: `docs/specs/CBG-PLAT-01/approvals/cod55-windows-client-discovery.md`; COD-55. Proportional Scryer ledger: `chg-hpmj66`.
- Recalled repository memory; consulted the wiki command-context guide and Scryer rules/lifecycle intent. Explicit semantic graph search and change-impact context found `knowledge/architecture/windows-mcp-command-context.md`, whose source anchors identify the client helpers and native tests. Initial graph calls were temporarily busy; later context and stdio caller discovery succeeded. The connected graph is not evidence that the new test-support file was indexed. No COPY source was read.

SHA256 of the exact reviewed source files (rechecked unchanged before report):

| File | SHA256 |
| --- | --- |
| `src/client_process.rs` | `e9a460c783287409f151765bbf0db704bc3e77dfd3978dd1c11a17dd76a5ea05` |
| `tests/windows_client_discovery.rs` | `c8ef90536f8f0bfef9ae216e43fad17c0f3c099d9429140044d5b1b483b7e43d` |
| `tests/support/mcp_client_fixture.rs` | `944a044a2f81ff4b97d76f044fdcea82e5fa95d190d11e60cc451267f36de148` |

The new fixture also matches `/tmp/pr164-ci-debug/reviewed-fixture.rs` byte-for-byte, and all hashes match the coordinator's `source-hashes.json`.

## Claims challenged and verified

1. **Host compile check uses the actual standalone fixture, without changing production behavior.** `src/client_process.rs:626–631` adds only a private `#[cfg(test)]` module. The preceding file is byte-identical to baseline. Its `include!` resolves to the same file consumed by `include_str!` at `tests/windows_client_discovery.rs:71`. The latter writes that source directly to the file passed to `rustc --edition=2021`. `const _: fn() = main` references the entry point without executing it in the unit-test process. No public API or non-test execution path is added.
2. **No response drift or new fixture deadlock.** The five response literals at `tests/support/mcp_client_fixture.rs:15–19` are exactly equal to baseline, ordered IDs 1–5, and each parses as JSON. Only format-safe emission changed: array iteration with `println!("{response}")`. Normalized baseline comparison shows the complete non-server add/list/logging/failure path is unchanged. The stdin-to-EOF step is unchanged. Its actual caller, `src/adapters/cli/install/verify.rs:216–233`, drops the taken stdin after writing before `wait_with_output`; the repair adds no wait/read interaction. This is a static lifecycle trace, not a native Windows execution result.
3. **Project target identity remains exact.** `tests/windows_client_discovery.rs:195–198` canonicalizes the existing `repo space` directory created at lines 24–25, then appends the exact `.mcp.json` leaf. It does not canonicalize a nonexistent dry-run file or compare only a suffix. The path-policy helper still selects that repository plus that leaf. Profile, override, explicit-path, target visibility, native/batch, failure/fallback, unrelated-config and unsafe-input assertions remain intact. Exact baseline reconstruction verifies the Windows test differs only by fixture extraction and this approved expectation replacement.

No conditional code defect survived those checks. The extraction is proportional to the demonstrated five invalid-format-string failures; no production or security policy was loosened.

## Completed verification evidence

Evidence below distinguishes supplied execution from this review's read-only checks. I ran no Cargo tests, fixture process, native tests or CI; I inspected the artifacts and independently compared source/payloads, parsed the JUnit report, and used `rustfmt --emit stdout` on the baseline fixture without changing it.

- Worker report `/tmp/pr164-ci-debug/windows-repair-report.md`: standalone host wrapper compile/run passed (0 tests, intentional compile check); standalone fixture compile passed; `server start` smoke exited 0 with five parsed JSON responses, IDs 1–5. These are worker-reported executions, not rerun here.
- Coordinator host affected suite: `cargo nextest run --locked --lib --profile pr164 --config-file /tmp/pr164-ci-debug/nextest.toml -E <affected-expression.txt>`, with `CARGO_TARGET_DIR=/private/tmp/codebasegraph-pr164-target`. `/tmp/pr164-ci-debug/affected-tests.log` and parsed `affected.junit.xml` confirm **58/58 pass, 0 failures/errors/selected skips**; 795 tests were not selected. The library compilation includes the shared fixture; this does not execute the Windows-only process tests.
- Coordinator formatting: `cargo fmt --all --check` passed. `fmt.log` is empty; success is established by the coordinator's completed shell chain exit 0, not absence of log text.
- Coordinator strict static check: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passed. `clippy.log` ends with `Finished` and no warning. The coordinator confirmed the nextest → fmt → Clippy chain completed exit 0 (pid 70475), so all three commands succeeded.
- Coordinator also reports the exact shared fixture passed Windows MSVC metadata compilation with `-D warnings`. This is compile evidence only; I did not inspect or rerun that command's artifact.

## Limits and remaining gates

Native Windows execution has **not** run on this repaired snapshot. The former compile failure masked later process assertions, so passing host compilation and unchanged payloads do not establish that all those assertions pass. Re-run the native Windows job on the pushed final head and retain its native-test profile and bundled-extension policy. Windows Server CI remains distinct from Windows 11 consumer qualification.

COPY 53 versus 56 remains unresolved and outside this review. Cancelled jobs are incomplete, not passing evidence. No waiting/polling for CI was performed. No source/repository edits, Git writes, shared-service mutations or memory writes were made. Existing dirty/untracked work was preserved. Reusable-knowledge assessment: the review adds no durable knowledge beyond the existing command-context guide; task-local evidence belongs in this report and the canonical execution handoff.
