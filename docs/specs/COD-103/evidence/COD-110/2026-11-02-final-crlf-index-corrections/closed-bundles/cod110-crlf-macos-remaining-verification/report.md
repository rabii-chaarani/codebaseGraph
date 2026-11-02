# COD110 — remaining local macOS verification (partial FAIL)

**Status:** Index non-mutation criterion **FAIL**. Dependent LF/CRLF/mixed/dirty acceptance stopped. No source changes. No live owned Bash/Cargo/CLI processes at closure.

## Scope and source

- Root: `/private/tmp/codebasegraph-cod110-20261101`.
- HEAD: `862bb3b3c40c36b21cdcd388f596d52ad482530b`; tree: `02cd3ebed5a1861bef4506f678449d836e7e2bf8`.
- Dirty `ci.rs`: SHA-256 `3c52c2c8e01262cf530c1e2c286cd77c8b7c50aace8fbf532cdc605c48178570` / 94601 B.
- Dirty `release.rs`: SHA-256 `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594` / 33789 B.
- All 2696 tracked inventory contents/modes match the original verifier's pre-inventory before and after this work; the other 2694 files are unchanged. Dirty patch SHA-256 `1ef031e9df426d7a2847c1e1bdda46f6e76fbe95350100725c2f6e6b9dbe789d` / 6085 B matches original.
- Product HEAD/index/config and Cargo manifests/lock/config match original protected identities. No source/config/index restore was performed.
- Authority reused from Engineer's explicit remaining-verification assignment and preserved original authority/context. Parent-provided approval/design references (`316be0e6008`, Design `9wzt/49612`, compatibility `754c82`) are references, not newly asserted full-SHA approval records. No new behavior or architecture decision was made. Canonical graph orientation is the existing same-project registration, not proof of the fixed source snapshot. No model delta was assessed or adopted.

## Results

| Observable criterion / check | Executed evidence | Result |
|---|---|---|
| Formatting | Original `cargo fmt --all --check` | PASS reused, not rerun |
| Full affected xtask native tests | Original `cargo nextest run --offline --locked -p xtask --bin xtask --cargo-profile native-tests ... --no-fail-fast --retries 0 --test-threads 1` | PASS 157/157, 0 skipped, genuine JUnit |
| Strict workspace lint | Original `cargo clippy --offline --locked --workspace --all-targets --all-features -- -D warnings` | PASS reused, no warnings, not rerun |
| Workflow policy | `cargo run --offline --locked -p xtask --bin xtask --profile native-tests -- check-workflows` | PASS exit 0; compiled current CLI |
| Ordinary release gate | Same Cargo prefix, `release-gate` without `--source-sha` on dirty lease | PASS exit 0 |
| LF metadata CLI output | Actual current `target/native-tests/xtask release-metadata --source-sha <private HEAD> --source-dir <private LF fixture>` | PASS exit 0; source SHA, 4.0.5, v4.0.5, release notes correct |
| LF fixture index remains byte-identical | Full private-file inventory around CLI; diagnostic per-Git snapshots | **FAIL**, real `.git/index` stat-cache write |
| All-six CRLF / mixed CLI equivalence | Fixtures prepared; no CLI invocation after LF failure | NOT RUN |
| Dirty tracked negative CLI | Fixture prepared; no CLI invocation after LF failure | NOT RUN |

The new non-test CLI is SHA-256 `61b70d9f13598c1effd22e4b986614bc6a05333ff73fc260b4b78ab8bd4aa7da` / 2194688 B. It was compiled from this dirty fixed source by `04-check-workflows`; it is not the old baseline executable.

## Failure and bounded actor isolation

Initial LF CLI changed only private `.git/index` (669 B), from `37c8559a81e1c5f3ea1fc36a29a9fec612486c1eba5db2c1715c059f2687cda1` to `3b9bd82719cbb526db763e3da5dcf0167e2b1304d53d7b228735fdfaf81515b8`. All six metadata file bytes/modes and all other private files were unchanged. No post-CLI Git/status/diff wrapper command ran. Initial raw before-index bytes were not captured; only its identity is available. No missing bytes were inferred or restored.

Engineer then authorized **one new LF diagnostic only**, not acceptance retry. A private PATH proxy forwarded exact arguments/results/environment to the captured actual Git, logging every invocation and copying raw metadata/index before and after each subprocess. Outer-shell snapshots bracketed the single CLI call at normal product cwd. Original LF fixture remained untouched.

First mutation was invocation **#30**:

```text
/opt/homebrew/Cellar/git/2.52.0_1/bin/git --no-replace-objects diff --quiet 660ebad08bec4973555669ea67253dd357d70470 --
```

- `GIT_OPTIONAL_LOCKS=0`; `GIT_CONFIG_COUNT` absent (ci helper strips it).
- Actual Git SHA-256 `ec24d6af5c519b2f708ae5464f23f85dd3a3cc96e161c0b751f879a3a516ee74`.
- Prior 29 subprocess boundaries and outer pretool/shell boundaries did not change index bytes.
- Index 669 B: `0ed79b8adb9988f9412e62fe6a6112ccd03f8e0485cae43cb740fe41a85bf9b1` → `3d7a22e67d3a2f3af11d0ded720fa3a79d48fa247785b7084bf4dd77eb7e3e26`.
- Parsed change: six entries' ctime/mtime seconds/nanoseconds and resulting checksum. Blob OIDs, modes, flags and sizes stayed unchanged.
- All raw metadata snapshots throughout the diagnostic stayed unchanged. Diagnostic CLI exit 0.
- This is a real index non-mutation failure. It is **not labeled introduced by the CRLF change**. No baseline executable comparison was run.

Raw trace, proxy, one-shot shell, stdout/stderr, all per-subprocess index copies and metadata snapshots are retained in `actor-isolation-one-lf/`. `19-isolation-result.json` contains the first boundary and parsed deltas; `14-stopped-sequence-and-scope.json` preserves the initial exact command sequence.

## Reused proof and new execution are separate

Original immutable proof: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-58dfaa85/cod110-crlf-local-macos-verification`. Exact faithful copies of all 27 completed files are in `reused-original-completed-proof/`; per-file original/copy identities are in `23-reused-proof-provenance.json`. These copies are not new executions.

Original genuine JUnit: SHA-256 `91981d6262fdec1c3bf05b01022fc0bac82d34e55baeddd32905a5a1a5247883` / 76760 B; UUID `6635c9dd-ecad-42d0-870f-4a0330af7175`; 157 test cases, 0 failures/errors/skips, 69.019 s. Original raw inventory and test list remain preserved. No report or JUnit was fabricated or refreshed.

Original proof has five primary command start records (fmt, versions, inventory, nextest, Clippy). This continuation has nine primary start/result records: two policy gates, four private fixture setup programs, one initial LF CLI, one new diagnostic fixture setup, and one diagnostic CLI shell. Three ancillary Bash programs captured preflight, read-only Git config diagnosis, and final source/process state. Proxy trace logs 30 forwarded diagnostic Git invocations; these are nested subprocesses, not 30 test runs.

## Environment, limitations and closure

Local macOS 26.6.2 / Darwin arm64; native-tests/default target; normal original environment recorded in `03-environment.json` (Ladybug 0.19.0 native path, OpenSSL 3.6.2, deployment 13.3, build jobs 2, optional Git locks 0). No RUSTFLAGS/custom target/features/profile/dependency installation changes. Setup used command-local fixture user/autocrlf values only; bounded helper correctly strips command-local Git config environment. Fixture Git setup emits expected LF→CRLF conversion warnings; no Rust compiler warnings occurred in the new gate commands.

Accepted assignment settings: explicit `openai-codex/gpt-6.1-sol`, XHIGH, default, fresh non-fork. Role map matches `gpt-6.1-sol` / `xhigh`. Actual model self-inspection is unexposed; no known mismatch. No nested dispatch.

No source writes, product/global Git configuration or index writes, Scryer/model/Linear/CI/GitHub writes, package/publish operations, graph rebuild/rebind, full root-application/native-engine suite or Windows reruns. Separate Windows evidence remains parent-owned and is not newly claimed here. Scryer ingestion/fold is not assigned or executed. Aggregate qualification and PR handoff stay with Engineer.

**Next owner:** Engineer / Software Designer for the narrow restorative fix. Remaining affected acceptance must wait for the authorized final snapshot. All read/private/Cargo leases are STOPPED. Final process scan found no live Cargo/xtask/rustc process owned by this work. No more commands follow closure.

Reusable knowledge assessed: the Git diff index side effect is worth retaining, but no memory/wiki mutation is authorized in this read-only assignment. Constants changed: none. Fixture version/tag and baseline bytes came from the actual six metadata files; no oracle threshold was changed.
