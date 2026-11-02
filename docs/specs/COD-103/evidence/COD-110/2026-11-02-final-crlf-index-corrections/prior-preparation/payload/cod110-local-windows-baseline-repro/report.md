# COD-110 local Windows baseline regression

**Actual result:** controlled CRLF metadata fails the unchanged named test with exit **101**: `root Cargo.toml has unsupported or ambiguous generated metadata`. One test failed; 90 were filtered out.

- Snapshot: commit `862bb3b3c40c36b21cdcd388f596d52ad482530b`, tree `02cd3ebed5a1861bef4506f678449d836e7e2bf8`.
- Actual Windows: local Parallels Windows 11 ARM64; existing Rust 1.99.1 `stable-x86_64-pc-windows-msvc` under x64 emulation. No install or toolchain update.
- Original Cargo invocation: `cargo test --offline --locked -p xtask --bin xtask --profile native-tests release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout -- --exact --nocapture`. Compiled once in 52.18 seconds. Actual six metadata files were LF; stdout says 1 passed, 0 failed, 90 filtered. Cargo numeric exit was not captured because `Start-Process -Wait` remained stuck after Cargo exited. Authorized cleanup terminated only its owned PowerShell/conhost; wrapper exit255 is not Cargo exit.
- System `core.autocrlf=true` was recorded. Command-local `checkout-index --force --all` did not rewrite existing LF files. The misleading raw `six-metadata-controlled-crlf.json` filename is preserved unchanged; its actual bytes/counts establish LF.
- After explicit quiescence and separate parent authorization, .NET byte writes converted only six metadata files to CRLF. CR=LF=CRLF counts: root Cargo80, lock2069, k-wiki Cargo34, xtask Cargo13, release manifest3, CHANGELOG424. BareCR/bareLF=0. Every LF roundtrip SHA equals committed metadata. Private Git index unchanged.
- Existing compiled test PE was run exactly once: `xtask-5e2fcd04c8daf5d1.exe --exact release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout --nocapture`. It failed at `crates/xtask/src/release.rs:551:83`, exit101. Binary SHA-256 `0772b115b581c048707c1868d217f82a60a0ddabbdbe6d72ea68b8455f215823`; saved under `actual-guest-artifacts/final-closure/`.
- All2696 tracked file hashes matched the archive before testing. Final hash audit confirms2690 unchanged files and only the authorized six CRLF variants. Seven reviewed source files stay exact. No source fix or other content change.
- Both initial staging failures are preserved: shared mirror symlink extraction unsupported; guest-local .NET hash reader hit a long-path limit. The reader resumed with extended paths; no Cargo had run in either failed stage.
- Final guest process inventory is clear; all owned Bash handles completed. Guest staging lease released; isolated source/target preserved. No further reads, private writes, Cargo, VM actions, PR/CI/model updates are running.

**Scope/limits:** baseline reproduction, not corrected-source acceptance, hosted checkout proof, or platform qualification. Local macOS prior evidence belongs to the parent; this report does not replace its authority. Parser broadening/line-ending policy remains unapproved. No source constants or settings changed; only the expressly authorized disposable six-file CRLF representation.

**Settings:** Evidence Verifier; parent accepted `openai-codex/gpt-6.1-sol`, xhigh; actual self-metadata unexposed. No nested dispatch. No wiki/memory/Scryer/Linear/API calls under this assignment.

**Next owner:** Engineer/user for behavior decision and any separately approved fix. Reusable lesson assessed; no memory write authorized/performed.
