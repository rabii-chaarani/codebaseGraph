# COD-105 — independent executed local evidence

**Result:** all authorized local checks passed. **Not full A01/stack qualification:** real two-target packaged smoke, hosted PR CI, deferred child proof and final review remain pending.

## Identity and scope

- Verifier actual exposed profile: **openai-codex/gpt-6.1-sol / max**, read from persisted session events. Independent of implementer.
- Approved `docs/specs/COD-103/spec.md` revision0.2 at **`7e79f7e924536f29d3212e83899b01c38afd39de`**; same-SHA contracts and approval record commit **`383a481bd5a71c58dab7dd796ace92a365760da5`** read. Active issue COD-105, brief **`chg-ahwwmf`**, C103-R01/A01 and first-PR R05/A05 prerequisite. Existing claims resp-16/17/256/15, no new claims.
- Worktree `/private/tmp/codebasegraph-cod103-cod105-20261029`. **Tested code is uncommitted**: initial HEAD **`c8b8e7395c0a83bb8a2fe53a5add97df0d9cf617`** plus exact file hashes below; do not call HEAD alone the tested implementation. Product baseline5889ed09954a4a07efd2590dd1bd380ee77be06a. HEAD remains unchanged after checks.
- Handoff `/tmp/cod105-implementation-handoff.md` SHA-256 **`1cfe23e93d62627da0d1f60516a079e5f8a138a01e4c990da5af607c62cf0154`** independently matched. Read actual approved docs, source, tests and diff, not worker results as proof. Semantic graph search/context from preparation supplied discovery only; older-root graph/model is not exact-source proof.

| File | Before SHA-256 (matched handoff) | After |
|---|---|---|
| `crates/xtask/src/main.rs` | `652dbeb10cfbeabef04b61fa2429356be7155f70be90d05822ff0d278df985d4` | same |
| `.github/workflows/ci.yml` | `98f659e98ad2060f503785390bc810b453a3eb23518b372e85122d2090f1fcda` | same |
| `docs/release.md` | `88efe80f119558fdc3e913211c4ebdde11a131a9d5b349248cb1b5a4b8e0c5a3` | same |

Read-only Python parsed all858 index-v2 tracked entries and compared actual Git blob digests: only those3 files differed before execution. No Git command/write. Actual diff saved to `/private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/implementation.diff`. Manifests/lock, `native_tests.rs`/shards, native/release workflows, verified-release action and Cargo config are unchanged; final hashes match product baseline, listed in JSON.

## Environment and command results

macOS26.6.2 ARM64; Rust/cargo1.96.0 (`aarch64-apple-darwin`), nextest0.9.140. Commands ran serially from this exact worktree with normal project Cargo config: forced worktree `target/test-home`, RUST_TEST_THREADS1. No environment override/install/dependency/config-settings change. Explicit temporary outside-repo nextest config uses reporting profile `cod103`, retries0, user config none; focused compilation uses **Cargo native-tests** with `--locked`. Ordinary native pipeline is unchanged. BashHandle captured complete combined stdout/stderr; logs are unfiltered, not falsely presented as separate streams.

External config `/private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/nextest.toml`; exact commands, times, PIDs, log hashes and tool details are in JSON/`command-records.json`.

| Actual command (filter abbreviated only here) | Exit | Duration | Full output log |
|---|---:|---:|---|
| `cargo --version` | 0 | 0.064s | `cargo-version-final.log` |
| `rustc -Vv` | 0 | 0.021s | `rustc-version-final.log` |
| `cargo nextest --version` | 0 | 0.034s | `nextest-version-final.log` |
| `cargo nextest list --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/nextest.toml --user-config-file none --profile cod103 -E <exact15-name-union>` | 0 | 0.226s | `nextest-inventory-final.log` |
| `cargo nextest run --locked -p xtask --bin xtask --cargo-profile native-tests --config-file /private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/nextest.toml --user-config-file none --profile cod103 --no-tests fail -E <exact15-name-union>` | 0 | 0.152s | `nextest-focused-final.log` |
| `cargo test --locked -p xtask` | 0 | 5.342s | `full-xtask-final.log` |
| `cargo fmt --all --check` | 0 | 0.949s | `fmt-all-check.log` |
| `cargo clippy --locked -p xtask --all-targets -- -D warnings` | 0 | 0.097s | `strict-xtask-clippy.log` |
| `cargo run --locked -p xtask -- check-workflows` | 0 | 0.524s | `check-workflows.log` |
| `cargo run --locked -p xtask -- release-gate` | 0 | 0.323s | `release-gate.log` |

**Focused JUnit:**15 named tests passed,0 failures/errors;8 existing baseline+**7 actual new tests**, not handoff's8.36 other suite tests were intentionally filtered from this focused run, not omitted from full xtask below. Inventory exactly matched the selected15. Genuine nextest XML: **`/private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/cod105-focused.xml`**, SHA-256 **`580f693dafbc418d493990064b0b29da9e59564cc0168a580b2b1e61395f856b`**, run `21e6c57d-70ac-419c-add4-449a1e5153fd`. Runner-reported suite duration0.019s.

**Full xtask:**51 passed,0 failed,0 ignored,0 filtered; ordinary Cargo test profile. Formatting passed. Strict xtask Clippy passed with **no warnings observed**. `check-workflows` printed `workflow policy passed`; ordinary `release-gate` printed `release gate passed`. These are not production external confirmations or hosted CI.

Initial independent focused/full runs also passed. After a kernel restore lost transient wrapper timing records, the small selected/full runs were repeated with immediate durable records before parent advised repetition was unnecessary. Original logs and `cod105-focused-initial.xml` remain; initial durations are explicitly unrecorded, not fabricated. Both attempts were on unchanged hashes. No further repeat was performed for timing.

## Actual cases and what they prove

New named cases (7):
- `tests::mcp_smoke_accepts_valid_initialize_and_tools`
- `tests::mcp_smoke_rejects_nonzero_child_with_success_payload`
- `tests::mcp_smoke_rejects_errors_missing_malformed_duplicate_and_decoy_responses`
- `tests::workflow_policy_accepts_exact_cod103_stack_bases`
- `tests::workflow_policy_rejects_unknown_stack_base_and_push_namespace`
- `tests::workflow_policy_rejects_privileged_pr_event`
- `tests::workflow_policy_rejects_unapproved_pull_request_activity`

MCP negative grouped test contains **8 assertion subcases**, not8 additional named tests: zero-status JSON-RPC error; missing initialize; missing tools response; malformed JSON; nonobject protocol data; wrong response ID; duplicate response ID; description-only graph_search decoy. The separate nonzero case uses status7 despite convincing valid success responses plus error text. Positive uses initialize id1/tool-list id2 actual graph_search entry.

Policy namespace grouped test checks4 actual mutations: PR base `cod103-stack/cod-108`, PR wildcard, push `cod103-stack/cod-105`, push wildcard. Separate tests cover exact approved base/activity set, `pull_request_target`, and unapproved labeled activity. All8 baseline filters were actually selected and passed; exact names appear in JSON/inventory. Actual YAML diff changes only approved PR types/bases; full job graph/required/read-only permissions/main-only push/release/PR artifact non-retention remain unchanged.

Source confirms `smoke_mcp_stdio` uses the same validator, retains request/protocol values, closes stdin, drains stdout/stderr with `wait_with_output`, fails write errors after kill/reap, and rejects nonzero first. It validates expected response IDs/results/tool names instead of substrings. Executed fixtures prove the named cases only; invalid UTF-8, notification permutations and actual spawn/write/wait failures were source-inspected paths, not separately executed cases here. No real packaged MCP process/artifact pass is inferred.

## Pending gates and handoff

- **A01:** actual macos-arm64 and windows-x86_64 packaged build/smoke remains due in full hosted first main-base PR CI. No local product/native artifact build, package, full workspace test/lint or hosted dispatch ran.
- **A05:** actual first `cod103-stack/cod-105 -> main` full fresh CI/stable required and independent final review/confirmed-fix verification remain. Only afterward may Engineer permit approved **staging-only** COD-106. Actual fresh `cod103-stack/cod-106 -> cod103-stack/cod-105` proof remains deferred/pending, not waived/Done. Missing/failed/stale proof blocks full qualification, further handoff, every main integration and release.
- Fresh Final Reviewer must review this exact three-file snapshot/diff and executed evidence. Later edits invalidate affected checks. Production alone commits/persists evidence; no source fixes or Git operations here.
- No Scryer anchor/test/JUnit ingestion/verdict writes authorized. Genuine report is ready for the authorized owner to reconcile resp-16/17/256/15 tests proportionately and ingest once/report, inspecting unmatched/ambiguous/unmentioned results; absent attachment is not pass. No force-fold/new claims.
- Windows11 A09/non-bypass actor capabilities remain later owner gates; this local run does not qualify them or change support/release policies. No PR/main merge/publication/aggregate closure claimed.

Machine report: **`/tmp/cod105-independent-evidence.json`**. Complete raw artifacts: **`/private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb`**. No constants, source, canonical spec, model, Linear/wiki/settings or dependencies changed. Only external evidence/config/logs and build outputs from authorized xtask checks were produced. No new reusable memory warranted for per-snapshot status.
