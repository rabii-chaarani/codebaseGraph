# COD110 final local macOS verification — PASS

**Assigned-scope result:** All current checks pass, including full raw Git index non-mutation with stale stat cache. Source was not edited. This report supports, but does not itself close, Final Review or both-platform/aggregate gates.

## Snapshot and authority

- Repository `/private/tmp/codebasegraph-cod110-20261101`; HEAD `862bb3b3c40c36b21cdcd388f596d52ad482530b`; tree `02cd3ebed5a1861bef4506f678449d836e7e2bf8`.
- Dirty `crates/xtask/src/ci.rs`: `9cce67411b43c86a6797d35f5538bf2cc72227713c4dfa0baae5a02eb258e2fd`, 95794 B.
- Dirty `crates/xtask/src/release.rs`: `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594`, 33789 B.
- All 2696 tracked inventory entries, contents and modes are identical before/after; the other 2694 source entries are unchanged from earlier verification. Product protected HEAD/index/config/Cargo files are unchanged. Preserved dirty patch: `3438843aea02a1de52173b61203a498f34da993c2738233463e9e008cc5cb1e5`, 8097 B.
- Engineer's bounded assignment reuses COD110 approval/design/compatibility context (`316be0e6008`, `9wzt/49612`, `754c82`, decision `2917`) and known rules/memory/graph discovery. These are supplied references, not newly asserted full-SHA approval records. No new planning, approval, architecture, Scryer or model operation was performed.

## Actual current execution

| Check | Result | Raw evidence |
|---|---|---|
| `cargo fmt --all --check` | PASS exit0 | `03-fmt.*` |
| Current native xtask inventory | 158 registered | `04-xtask-inventory.*` |
| Full native-tests nextest, no-fail-fast/retries0/thread1 | **158/158 PASS**, no errors/failures/skips | `05-xtask-full.*`, genuine `xtask-junit.xml`, `06-junit-audit.json` |
| Strict workspace/all-targets/all-features Clippy `-D warnings` | PASS exit0, no warnings | `07-clippy.*` |
| Actual `check-workflows` | PASS exit0, compiled current non-test CLI | `08-check-workflows.*` |
| Actual ordinary `release-gate` | PASS exit0; no source SHA on dirty product lease | `09-release-gate.*` |
| Actual CLI LF, all-six CRLF, mixed | Each PASS exit0; equivalent version/tag/notes; source SHA matches own fixture HEAD | `13-metadata-*`, `12-case-*/` |
| Actual dirty tracked negative | PASS expected exit1; actual error identifies `Cargo.toml` worktree change | `13-metadata-dirty-tracked.*` |

Genuine JUnit UUID `04f09f6e-edf5-47f4-85ea-d1c9a0e744e7`; SHA-256 `714ac93274e22e22b39c164b697688f72e0fffd5581e51bcfbed9a87aaa19f34`, 77242 B; runner time 71.499 s. Its exact 158 names match the actual inventory. The new `ci::tests::verify_checkout_accepts_stale_stat_without_mutating_index` passed; current flag/mode/staged/dirty failure tests, raw-CRLF classifier rejection, and ordinary permissive reader compatibility tests also passed. Evidence is newly executed after the final source change, not the old157 report. No JUnit synthesis, refresh, ingestion or fold occurred.

Current actual non-test CLI SHA-256 `aa514794c5303514964bd80077aca9688b6f641ae16845d4e1818440c220d42c`, 2194688 B. `10-binary-binding.json` and exact Cargo depfile tie it to current source paths/identities and `08-check-workflows` compilation, not the old baseline binary. Exact commands, cwd, selected options, UTC starts, actual exits/durations and relevant native environment are in each start/result record. 15 top-level command programs were executed; one full nextest run, no retries.

## CLI and index contract

Four new private six-file Git fixtures were created. Only their `.git/config` has `core.autocrlf=true`; identity setup uses command-local user values. No global/product Git configuration, product index or old fixture was changed. Git commits hold LF metadata blobs. Physical LF/all-six CRLF/mixed bytes are documented by per-file newline counts and LF-roundtrip SHA-256; every positive roundtrip equals actual source bytes. The all-CRLF case has nonzero CRLF and zero bare-LF counts in **every one of six files**.

After commit/setup, `Cargo.toml` mtime was moved back exactly 10 seconds in each fixture. All three positives retain identical metadata bytes and have demonstrably stale index stat cache. Before and after each single actual CLI call, raw full index bytes, parsed entries (OIDs/modes/flags/stat fields), six raw files and full private file byte/mode inventories were captured. **Every CLI case preserved full raw index bytes and all private file bytes/modes**, including the dirty tracked rejection. No proxy, index refresh/restore, status command or acceptance retry was used.

The current checkout worktree diff explicitly supplies `-c diff.autoRefreshIndex=false` (`ci.rs:585`). `18-acceptance-anchors.json` and `result.json` trace the affected criteria, including **COD110-FR-001**, to actual current tests/CLI evidence. The previous closed FAIL-index bundle and its actor trace are immutable and remain historical failure evidence; this report does not rewrite or relabel them.

## Environment, limits and handoff

Local macOS 26.6.2 / Darwin arm64; Cargo/rustc 1.96.0, nextest 0.9.140, Git 2.52.0. Normal original native environment reapplied before commands/resume: Ladybug0.19.0 from native019, OpenSSL3.6.2 library path, deployment13.3, jobs2, `GIT_OPTIONAL_LOCKS=0`. Default target; offline/locked Cargo; no RUSTFLAGS, feature/profile/dependency installation changes. Fixture setup emitted expected Git LF→CRLF conversion warnings; no Rust compiler warnings. Accepted parent dispatch: explicit `openai-codex/gpt-6.1-sol` / XHIGH / default / fresh non-fork; role map matches Sol/xhigh; runtime model self-inspection unexposed, no known mismatch, no nested dispatch.

No source, Scryer/model/Linear/CI/GitHub/PR writes, package publication, graph rebind/build, VM setting changes or root application/native-engine full suite. Separate Windows qualification remains with its verifier/Engineer. Unrelated model delta was not assessed; no blanket model invariance, attribution or restore is claimed. Constants changed in product: none; the assigned -10s private fixture timestamp is a trigger, not an acceptance threshold. Reusable lesson assessed; no wiki/memory writes authorized here.

**Next owner:** Engineer and original Final Reviewer reconcile COD110-FR-001 at this exact snapshot and both local platform results before any PR update. All assigned checks are closed; read/private/Cargo leases **STOPPED**, source lease **NONE**, zero live owned Bash/Cargo/CLI processes. `files.json` is the single canonical payload manifest, written last, with actual per-file types, sizes and SHA-256; it excludes itself. Source inventory entries are source identities, not extra artifact payload paths.
