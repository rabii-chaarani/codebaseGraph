# PR #153 — hosted testcase verification

- Run: [40426019260](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260); completed `success`; 22 / 22 jobs succeeded (from `hosted-ci-completed.json`).
- Run head: `d6dd34493e9da6f80847240bafcc92dbcaed2c38`.
- Actual tested checkout: synthetic PR merge `af71103436086bff1189e67e0f5f03464e977bd4`, not the bare head. Every downloaded job reports that full `source-sha`, checkout ref, and `git log -1` output. Checkout message: `Merge d6dd34493e9da6f80847240bafcc92dbcaed2c38 into ca115690b226384164cf364458a0e3255b8908d3`.
- Read-only retrieval: fresh completed logs fetched with `gh run view 40426019260 --repo rabii-chaarani/codebaseGraph --job <id> --log`. No old run logs, source edits, CI reruns, or remote writes were used.

## Actual testcase outcomes

Each item has an explicit Rust harness `test <name> ... ok` record, the relevant executable, a nonzero passed-test summary, and successful enclosing command. These are not inferred from green job status or filter/list output.

| Platform / lane | Exact testcase | Result UTC | Job / raw-log line |
|---|---|---|---|
| macOS arm64 / unit | `adapters::cli::tests::dispatch_materialize::watch_help_documents_parallel_default_and_opt_out` | **PASS** `2026-10-27T00:29:50.6256800Z` | [121931860962](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860962), L699 |
| Windows x86_64 / unit, shard 4 | `adapters::cli::tests::dispatch_materialize::watch_help_documents_parallel_default_and_opt_out` | **PASS** `2026-10-27T00:36:59.3961399Z` | [121931860936](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860936), L875 |
| Windows x86_64 / unit, shard 1 | `db_writer::phase::tests::copy_trace_metadata_reports_generated_file_bytes` | **PASS** `2026-10-27T00:54:33.5239111Z` | [121931861177](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861177), L3022 |
| Windows x86_64 / integration, shard 4 | `isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth` | **PASS** `2026-10-27T00:50:24.7300571Z` | [121931861293](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861293), L1000 |
| Windows x86_64 / integration, shard 2 | `fixture::process::tests::covered_epoch_observer_retries_actual_contention_until_release` | **PASS** `2026-10-27T00:57:06.6173135Z` | [121931860919](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931860919), L993 |
| Windows x86_64 / integration, shard 1 | `fixture::process::tests::covered_epoch_observer_propagates_missing_serial_lock` | **PASS** `2026-10-27T00:53:26.5228474Z` | [121931861193](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40426019260/job/121931861193), L1012 |

**NOT_EXECUTED: none among the requested platform/test combinations.** The new Windows contention test executed on integration shard 2; missing-serial-lock executed on integration shard 1; the real isolated-worker test executed on integration shard 4.

### Exact result records

The following are verbatim timestamp-and-message suffixes from the saved logs; unchanged complete lines with job prefixes and original line numbers are in the per-case extracts.

```text
2026-10-27T00:29:50.6256800Z test adapters::cli::tests::dispatch_materialize::watch_help_documents_parallel_default_and_opt_out ... ok
2026-10-27T00:36:59.3961399Z test adapters::cli::tests::dispatch_materialize::watch_help_documents_parallel_default_and_opt_out ... ok
2026-10-27T00:54:33.5239111Z test db_writer::phase::tests::copy_trace_metadata_reports_generated_file_bytes ... ok
2026-10-27T00:50:24.7300571Z test isolated_worker_first_creation_and_followup_keep_same_covered_publication_birth ... ok
2026-10-27T00:57:06.6173135Z test fixture::process::tests::covered_epoch_observer_retries_actual_contention_until_release ... ok
2026-10-27T00:53:26.5228474Z test fixture::process::tests::covered_epoch_observer_propagates_missing_serial_lock ... ok
```

## Command and binary provenance

- macOS unit command 1 (L663): `cargo test --workspace --locked --profile native-tests --lib --bins --examples`; unfiltered. Test executable: `target/native-tests/deps/codebase_graph-bfdd253a54eaf67a`. Its summary: `840 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out`.
- Windows unit shard 4 command 1 (L839) and unit shard 1 command 2 (L2891): `cargo test --workspace --locked --profile native-tests --features codebase-graph/bundled-windows-extensions,k-wiki/bundled-windows-extensions --lib --bins --examples --no-fail-fast -- --exact <full selected testcase list>`. Each requested exact filter is present. Both use `target\native-tests\deps\codebase_graph-c56ba430bcacd3a9.exe`; each relevant harness reports `89 passed; 0 failed; 0 ignored; 0 measured; 738 filtered out`.
- Windows integration shards 1/2/4 command 1: `cargo test --locked --profile native-tests --features codebase-graph/bundled-windows-extensions --package codebase-graph --test agent_hooks_process --test auto_repo_root --test coordinator_process --test copy_phase_process --test http_admission_process --test http_daemon_process --test native_request_identity --test refresh_recovery_process --test storage_foundation --test storage_maintenance --no-fail-fast -- --exact <full selected testcase list>`. Every requested filter is present. All three requested integration cases execute in `target\native-tests\deps\storage_maintenance-3a0b83c19e697dd3.exe`; relevant summaries are respectively `2 passed / 0 failed / 0 ignored`, `4 passed / 0 failed / 0 ignored`, and `3 passed / 0 failed / 0 ignored`.
- The complete, unabridged command/filter identity, source checkout, project compilation, `Running` binary path, result and summary lines are saved in each bounded per-case extract below. The `<full selected testcase list>` notation above is a report abbreviation, not a modified command or independent test invocation.
- The jobs also log a separate compiler-produced CLI artifact and SHA-256. These identify the CLI used by native process tests; they are not hashes of the Rust harness executables or release artifacts. No cross-job byte identity is assumed.

| Job | Compiler CLI SHA-256 |
|---|---|
| 121931860962 | `5a82f6fbf69cea32c6b8341e3725bc9ef041033b0119e9c5ba3b1bfde22dfb1d` |
| 121931860936 | `fb7387eb7abedba3902e8a3f331e6f0d08baa656f9ab74a3a486663867c5246a` |
| 121931861177 | `d44206b167a44b6c0dfe5215ed895cbe9bdc5fa72e44772754f9f3d933d2adac` |
| 121931861293 | `c9839f2b76302c6d9bfbb66db6e276b4bd8e8bde3f6361700aa785eee5bc1d9d` |
| 121931860919 | `095d8e5fe195215e1250c876325af5903281a3b90bb3fe51706dcafe2e3a7400` |
| 121931861193 | `b7a51d019091e868e5796f1936b7e47705d2c365777449948a71355e0c363c4d` |

## Saved evidence

- Directory: `/tmp/codebasegraph-pr153-ci-only-evidence/hosted-completed-logs/`.
- `retrieval-manifest.json`: download commands, job URLs, exit codes, raw-log SHA-256, byte and line counts.
- Fresh raw logs: `job-121931860962.log`, `job-121931860936.log`, `job-121931861177.log`, and all four Windows integration shards (`job-121931861193.log`, `job-121931860919.log`, `job-121931861031.log`, `job-121931861293.log`).
- Combined bounded, exact, line-numbered evidence: `/tmp/codebasegraph-pr153-ci-only-evidence/hosted-case-excerpts.txt`.
- Bounded, exact, line-numbered per-case extracts:
  - `watch-help-macos-exact-evidence.txt` (3,335 bytes).
  - `watch-help-windows-exact-evidence.txt` (11,424 bytes).
  - `copy-trace-windows-exact-evidence.txt` (11,657 bytes).
  - `isolated-worker-windows-exact-evidence.txt` (11,199 bytes).
  - `actual-contention-windows-exact-evidence.txt` (11,365 bytes).
  - `missing-serial-lock-windows-exact-evidence.txt` (11,243 bytes).

## Scope and remaining uncertainty

- All requested actual execution checks are verified. No missing execution or failed requested testcase was found.
- Hosted CI proves the tests on the synthetic merge containing the exact approved head, not a standalone-head build. Provenance comes from hosted checkout/build/execution logs, not an independent binary attestation.
- Native artifact jobs `121950333792` (macOS arm64) and `121951853727` (Windows x86_64) are `success` in `hosted-ci-completed.json`. This report does not independently inspect artifact smoke logs; it makes no extra smoke-execution claim.
- Assumption: the supplied completed run JSON is the authoritative run/job metadata; downloaded logs are from that same explicitly selected run and job IDs. No constants were changed.
