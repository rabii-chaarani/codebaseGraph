# COD-110 FINAL local Windows verification

**Result:** assigned final-source CRLF and no-refresh checks **PASS**. Full Windows xtask suite: **95 registered tests passed, exit0**. Five direct checks passed. Current production CLI passed LF/all-sixCRLF/mixed clean stale-stat cases with **full raw index bytes unchanged**, and rejected dirty content with expected exit1.

## Exact source and environment

HEAD `862bb3b3c40c36b21cdcd388f596d52ad482530b`, tree `02cd3ebed5a1861bef4506f678449d836e7e2bf8`, **plus only two dirty overlays**:

| File | SHA256 | Bytes |
| --- | --- | ---: |
| `crates/xtask/src/ci.rs` | `9cce67411b43c86a6797d35f5538bf2cc72227713c4dfa0baae5a02eb258e2fd` | 95794 |
| `crates/xtask/src/release.rs` | `06b674185df7f7ac4e7cf3cd2642c2d75b4eb62169ee922c3c2f9724fd760594` | 33789 |

HEAD alone does not contain this fix. Other2694 tracked files match HEAD. Archive SHA256 `99e5ae374235e318315d17f67d1a51c53fa60ea101da2483cc3051342449fdf3`; all2696 source hashes verified before testing. Final audit confirms2690 exact snapshot files plus only six authorized CRLF variants. Both source overlays remain exact on guest and host.

Local Parallels Windows11Pro ARM64; existing x64 Rust1.99.1/MSVC under emulation and Git2.56.0 nativeARM64. `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc`, `CARGO_NET_OFFLINE=true`, `GIT_OPTIONAL_LOCKS=0`. No install, VM/settings/privilege or product config change.

## Actual execution

| Criterion | Method | Actual result |
| --- | --- | --- |
| Final-source full xtask regression inventory | `cargo test --offline --locked -p xtask --bin xtask --profile native-tests -- --test-threads=1 --nocapture` | Exit0;95pass,0fail/ignored/filtered; build40.62s, tests53.11s. Includes new stale-stat raw-index regression, dirty/flagged/staged/deleted/preflight/mode/classifier/ordinary-helper controls. Inner symlink exclusions below remain explicit. |
| Actual all-sixCRLF original real-root regression | Same compiled test PE; `--exact release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout --nocapture` | Exit0;1pass/94filtered. Original baseline101 remains separate. |
| Positive mixed/CRLF and unchanged negative boundaries | Direct exact `candidate_metadata_accepts_crlf_and_mixed_six_file_layouts_without_writing`, `candidate_metadata_keeps_rejecting_bare_cr_ambiguous_and_mismatched_inputs`, `classifier_keeps_raw_crlf_metadata_ineligible` | Each exit0;1pass/94filtered. |
| New no-refresh regression | Direct exact `ci::tests::verify_checkout_accepts_stale_stat_without_mutating_index --nocapture` | Exit0;1pass/94filtered. |
| Current normal CLI | One `cargo build --offline --locked -p xtask --bin xtask --profile native-tests` | Exit0;10.89s. No old binary substitution/rebuild retry. |
| Production clean LF, all-sixCRLF, mixed stale-stat cases | `xtask.exe release-metadata --source-dir <new-owned-six-file-fixture> --source-sha 96d44cd9203a725f7c8fb1bec0af18b22488b567` | All3 exit0; exact four-key schema, source SHA/version4.0.5/tagv4.0.5; notes equal exact committed release section. Metadata unchanged. Allsix mtimes moved−10s after setup. Full raw index byte equality, modes and flags verified before/after each invocation. |
| Dirty tracked content is not hidden by normalization/no-refresh | Append explicit comment to private Cargo.toml; same production CLI with stale mtime | Expected exit1: `source checkout has tracked changes in worktree: Cargo.toml`. Full raw index bytes/modes/flags unchanged. Dirty fixture preserved. |

Private CLI fixture alone uses local `core.autocrlf=true`. Every clean representation LF-roundtrips to committed six-file bytes. All-sixCRLF CR=LF=CRLF counts:80/2069/34/13/3/424; bareCR/bareLF0. Mixed counts and all stale mtimes/input hashes are retained. Raw index before/after files are genuine full snapshots, not just staged blob hashes; host independently compared bytes and modes/flags. All4 cases preserve index SHA256 `03a836107cc7d7d7e6832b2057a73216d303d512671ec9a709bbd3c90a641879`.

Test PE SHA256 `550b95fb5cfdb6eddfd20c02c772225b5d35d4bfdb32880a913b66d42c10f183` (2716160B); normal CLI `56b0a7499d80ad3437aca4e64e5c39e72ebcc9f9d4b5377b1c6cbe7742003b22` (1940992B). Both saved with source-bound build provenance.

## Limits and closure

- Windows inner symlink cases **NOT EXECUTED**: privilege unavailable/os1314. Unix cfg-gated cases **EXCLUDED**, not PASS. No privilege workaround or retry. No distinct cancellation-named test appears in the current inventory; no separate cancellation failure-path claim is made.
- No compiler warnings. Git fixture LF→CRLF warnings retained separately. Cargo generated no JUnit; actual text/JSON only, no conversion.
- This is local Windows x64 emulation, not native x64 or whole-issue/all-platform acceptance.
- `COD110-FR-001` remains for original reviewer resolution. Evidence proves production stale-stat raw-index preservation here; no assertion that CRLF caused the original defect.
- Baseline88 manifest `393c0cd702a785ee5ac4a4378447f5ce6bb3cfd6cc8fa8fae9e19986cbee4ced` and prior fixed75 `19570ec28a14ab9214e344cae77d7bd01b98638fb5fd8351bc49067cca42aa76` unchanged. Their roots/sources/PEs were not altered.
- Final guest process inventory clear; all owned Bash complete/0running. Guest staging lease **RELEASED**. All guest/read/private/Cargo actions **STOPPED**; source/target/control fixtures preserved unchanged after closure.
- EvidenceVerifier, accepted `openai-codex/gpt-6.1-sol`/xhigh; self-metadata unexposed. No nested dispatch/sourcefix/GitAPI/model/CI/tracking writes. Same-scope authority supplied by Engineer316be0e6008/Design9/wzt/49612/compat754c82/user2917.
- No source constants/product settings changed; only authorized private EOL/stale-mtime/dirty-negative fixtures. Reusable knowledge assessed; no new memory mutation warranted or authorized.

**Next owner:** Engineer combines same-source independent Mac proof and original SolMAX focused review before any PR update.

**Preserved paths:** guest `C:\Users\rabii\AppData\Local\Temp\c110-final-b5aa2377`; cache `source\target`; separate CLI fixture `source\cli-six-metadata-fixture`; host `/Users/rabii/Documents/codebasegraph-cod110-final-b5aa2377`. No further reuse without new grant.
