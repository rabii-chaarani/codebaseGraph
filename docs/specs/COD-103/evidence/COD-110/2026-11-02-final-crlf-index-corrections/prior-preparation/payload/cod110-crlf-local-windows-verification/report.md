# COD-110 corrected-source local Windows evidence

**Result:** assigned CRLF behavior **PASS**. The original named regression now exits **0** with all six actual metadata files CRLF. The unchanged baseline failed the same test with exit101; its closed evidence remains untouched.

## Snapshot and environment

- HEAD `862bb3b3c40c36b21cdcd388f596d52ad482530b`, HEAD tree `02cd3ebed5a1861bef4506f678449d836e7e2bf8`, **plus exactly two dirty source overlays**:
  - `crates/xtask/src/ci.rs`: SHA256 `3c52c2c8e01262cf530c1e2c286cd77c8b7c50aace8fbf532cdc605c48178570`, 94601 bytes.
  - `crates/xtask/src/release.rs`: SHA256 `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594`, 33789 bytes.
- HEAD alone does not contain this fix. Archive SHA256 `8263b676c39bd90127487e76fefe2521062a92f1bb5d4426ad594fb759e0ecbb`; all2696 extracted source hashes verified. Other2694 tracked files match HEAD.
- Local Parallels Windows11Pro ARM64; existing Rust1.99.1 `stable-x86_64-pc-windows-msvc` under x64 emulation, existing MSVC/Git2.56.0. This is **not native x64 qualification**. Offline/locked; no downloads, installs or settings changes.

## Executed evidence

| Assigned behavior | Actual method | Result |
| --- | --- | --- |
| Full corrected xtask Windows inventory; candidate, classifier, checkout/preflight and ordinary-helper regression coverage | `cargo test --offline --locked -p xtask --bin xtask --profile native-tests -- --test-threads=1 --nocapture` | Exit0; **94 registered tests passed**, 0failed/ignored/filtered. Build39.78s; tests50.28s. See inventory and genuine stdout/stderr. Inner symlink exclusions below are not claimed as covered. |
| Original real-root regression with actual six CRLF files | Existing corrected test PE, `--exact release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout --nocapture` | Exit0; 1pass/93filtered. Baseline exit101 is fixed at this exact snapshot. |
| CRLF/mixed positive layouts without writes | `--exact release::tests::candidate_metadata_accepts_crlf_and_mixed_six_file_layouts_without_writing --nocapture` | Exit0; 1pass/93filtered. |
| BareCR, ambiguous and mismatched inputs remain rejected | `--exact release::tests::candidate_metadata_keeps_rejecting_bare_cr_ambiguous_and_mismatched_inputs --nocapture` | Exit0; 1pass/93filtered. |
| Raw CRLF immutable classifier remains ineligible | `--exact ci::tests::classifier_keeps_raw_crlf_metadata_ineligible --nocapture` | Exit0; 1pass/93filtered. |
| Actual production CLI on Git-normalized CRLF checkout | One `cargo build --offline --locked -p xtask --bin xtask --profile native-tests`; then new `xtask.exe release-metadata --source-dir <owned-source> --source-sha 10ca608411fdc0ef788a2fa233856a6179757bd0` | Build0 (10.50s), CLI0. Version4.0.5/tagv4.0.5; returned source SHA matches private metadata fixture HEAD. Notes equal the exact committed changelog section. Git normalized clean check0; allsix `i/lf w/crlf`; metadata bytes/index unchanged. |

Direct checks use the same corrected-source test PE SHA256 `94991ca788efb718e433900d766c36e96977581436835f219ed26a107cd7ccda` (2711040B). Current normal CLI SHA256 `bb9334930414c8580d9955a309ab8642e94d1fb486cb03a0ecaa07e447443711` (1940480B). Both PEs and provenance are preserved. No old baseline binary substitution or second test build occurred.

## Controlled representations and post-state

Only private six metadata files changed LF→CRLF. Their CR=LF=CRLF counts are rootCargo80, lock2069, k-wikiCargo34, xtaskCargo13, manifest3, CHANGELOG424; bareCR/bareLF0. Every CRLF→LF roundtrip SHA matches committed metadata. Raw LF/CRLF input hashes/counts are retained. Final full2696 audit:2690 exact corrected-snapshot files plus only six authorized CRLF variants. Corrected source overlays stay exact. Private Git index stays `76b8ab081ea5e6829dc015b5db1cc7b80bb180331d4624b829d40506c0bcb48d`.

Final guest inventory has no Cargo/rustc/test/CLI/Git or previous verification wrapper. All owned host Bash handles completed;0running. Guest staging lease **released**; isolated source/target/CRLF files preserved. All further reads/private/VM/Cargo actions stopped.

## Limits, warnings and next owner

- Actual stderr: Windows symlink privilege unavailable, OS error1314; inner symlink gate cases **not executed**, even though their registered parent tests report success. Unix cfg-gated cases are excluded, **not PASS**. No privilege/settings workaround or retry.
- No compiler warnings in either build. Git's temporary-fixture LF→CRLF warning is preserved separately from compiler warnings.
- Cargo generated no JUnit. This evidence uses genuine text/JSON, not converted/fabricated JUnit.
- Bounded CRLF acceptance evidence only; no blanket all-platform/whole-issue closure. Independent same-source localMac evidence and required focused final review remain Engineer gates before any PR update.
- Baseline88-payload root remains immutable; manifest SHA256 `393c0cd702a785ee5ac4a4378447f5ce6bb3cfd6cc8fa8fae9e19986cbee4ced` unchanged. Its LF-positive/missing-Cargo-exit, wrapper255 cleanup, two staging failures and CRLF101 remain distinct historical evidence.
- Role: Evidence Verifier. Parent accepted `openai-codex/gpt-6.1-sol`/xhigh; runtime self-metadata unexposed. No nested dispatch. Parent supplied matching routine approval316be0e6008/Design9/wzt49612/compatibility754c82. No claims invented; no source/GitAPI/model/wiki/tracking/CI writes.
- No source constants or policies changed. Version4.0.5 is observed fixture content, not a changed constant. Reusable knowledge assessed; no new durable claim or memory write warranted/authorized.

**Next owner:** Engineer combines matching independent Mac results and required focused review; platform limitations remain explicit.

**Preserved paths:** guest source `C:\Users\rabii\AppData\Local\Temp\c110-fix-b5aa2377`; build cache source`\target`; host staging `/Users/rabii/Documents/codebasegraph-cod110-crlf-fixed-b5aa2377`. No further reuse without a new grant.
