# COD-56 independent Final Reviewer

## Outcome

**No confirmed P0–P3 defects. Ready, within this review's boundary, for the authorized local commit and real `cod103-stack/cod-56` → `cod103-stack/cod-106` PR push for subsequent Windows qualification.** This is not A09 acceptance, COD-56→COD-107 readiness, Done, main integration, release, or aggregate closure. The approved post-reviewed-push desktop sequence is retained; missing future desktop execution is not a new pre-push blocker.

## Scope and canonical inputs

Read-only review of the four-line change in `release/install/install.ps1`, not a general repository audit. Root `/private/tmp/codebasegraph-cod56-20261031`; current branch `cod103-stack/cod-56`. Existing work [COD-56](https://linear.app/codebasegraph/issue/COD-56), source ledger `chg-2z3ngs`; description-only execution carrier `container-release-verifier` under `chg-fdm24r`. Whole-description SHA256 `8cdad09eb03f129881f0accecd827fea4c230b1c2cc75a85e05356f2aa1f5ddc`; COD-56 EXECUTION section `d824b555c0954dc93b61947bd0898bd71082a514c72f87898d77143d6873cf2b`, both independently verified. Applicable unchanged claims `resp-16`/`resp-256`; no new claim/topology.

Oracle: `docs/specs/COD-103/{spec,design,delivery,verification,architecture}.md`, revision0.3, approved full SHA `316be4c4bb81d4f6e025016b12374836b4dcc1a8`; separate approval `0e6008a1f46679a86aba85ac513beb05af94b0c0:docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md`, actual user confirmation “i already approved revision 0.3”, decision `c8690556-e836-428d-bc81-079c701d2395`. Design §5 supplies the exact correction. External unchanged `docs/specs/CBG-PLAT-01/spec.md` revision0.11 S05/R05/A09 at `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`. Historical frozen pending headings do not reopen that recorded approval. Closure owners remain unchanged.

## Exact reviewed snapshot

| Identity | Verified value |
| --- | --- |
| HEAD/base | `2173d766c155583b1e8f979c303b560998cc7214` |
| HEAD tree | `baefd500ed61fb35bc0b0beb92184998cfb6bec9` |
| Dirty content | Only `release/install/install.ps1`, four insertions; staged diff empty |
| Baseline Git blob | `05950595ee08f9e277bea432faa90410925509fa` |
| Baseline LF SHA256 | `bdd6f0eace6fbd4ea25c9e09d2c6cb6c0b54a05955d024aa97d76443c58ae308` |
| Candidate Git blob | `9890b016699948634200624e85b868616c14617e` |
| Candidate LF SHA256 | `830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb` |
| Exact Git diff SHA256 | `d73427d60108ad176c3f3737c2f5f337ac5d9d9bd03e954ece5c580231971e6c` |

These values were computed from actual candidate/baseline bytes and read-only Git outputs, not copied as proof from the handoff. Removing exactly the insertion reconstructs the complete baseline byte-for-byte. Final reread after evidence inspection confirmed unchanged HEAD/tree/status/source/diff. Later source changes invalidate the affected review/evidence.

## Causal assessment

- Candidate lines46–50: the existing native invocation/pipeline is followed immediately by `$probeExitCode = $LASTEXITCODE`; no intervening native invocation can overwrite the captured exit. `Out-Null` is a PowerShell cmdlet, not another native program. Assignment/`if`/`throw` syntax is compatible with PowerShell5.1; this reviewer did not execute a parser.
- Nonzero graph `--help` reaches the throw before wiki or either replacement. Graph zero/wiki `--version` nonzero throws before replacement. The diagnostic includes the binary, probe argument and captured numeric exit; interpolation does not introduce the `$variable:` parsing ambiguity.
- Lines81–82 validate both binaries before DryRun84, directory creation89, or install90–91. Existing path/checksum/host guards defeat missing/corrupt/unsupported-input paths before destination mutation. The failure therefore leaves both destination files untouched by this installer. Both-zero success follows the original install path unchanged.
- The script sets `ErrorActionPreference=Stop` and StrictMode and has no catch/trap around these calls. Throw unwinds the failure path. If a supported shell already raises an invocation/native failure, it remains uncaught; no new native preference is required or changed. Packaged caller `crates/xtask/src/main.rs:707–735` uses `pwsh -File … -DryRun`; `run_command:914–922` rejects non-success process status.
- Baseline comparison confirms the missing native-exit rejection is restored without changing checksums, arguments, parameters/preferences, cleanup, or per-file replacement. Pairwise rollback, client shims, unsupported shells/architectures and other pre-existing behavior are outside this correction, not new findings.

Primary Microsoft documentation fetched HTTP200 during this review: [5.1 automatic variables](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-5.1#lastexitcode), [7.5 automatic variables](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-7.5#lastexitcode), [5.1 throw](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_throw?view=powershell-5.1), and [7.5 native-error preference](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_preference_variables?view=powershell-7.5#psnativecommanduseerroractionpreference). They support the static premises: LASTEXITCODE is the last native-program/script exit; `-File` exception termination is nonzero; throw stops the call chain under this script's Stop preference; enabled PS7 native-error handling may reject earlier. Documentation is not runtime qualification.

## Independent evidence actually inspected

Evidence Verifier report: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-c30ebc5e/cod56-prepush-verification/report.md`, SHA256 `1667c5fa6453bf0d2cec9723af18e5f3c48c42bc49c00c035474f68c476199fd`. Manifest SHA256 `800435aa180ee6d13ff6341ee49dd89c6bfb66d917a7797423068fb370af04d1`.

Inspected actual report, commands/exit records, environment, before/after source identities, raw check logs, reporting-only private nextest config and JUnit. All26 manifest entries matched their bytes/hashes. macOS26.6.2 build25G83 ARM64, Rust/Cargo1.96.0, nextest0.9.140; correct isolated source root shown in compiler logs. Accepted as available regression/policy evidence:

- `git diff --check`, before/after: exit0.
- `cargo fmt --all --check`: exit0.
- `cargo run --offline --locked -p xtask -- check-workflows`: exit0.
- `cargo run --offline --locked -p xtask -- release-gate`: exit0.
- `cargo clippy --offline --locked -p xtask --all-targets -- -D warnings`: exit0, no compiler warnings in supplied logs.
- `cargo nextest run --offline --locked -p xtask --cargo-profile native-tests --config-file <evidence-root>/nextest.toml --profile default --retries 0 --test-threads 1 --no-tests fail`: exit0. Run `dc166972-ec6b-4017-bae4-5b61e1cd06a3`; raw summary76/76, parsed genuine JUnit76 unique cases, zero failures/errors/skips/disabled. JUnit SHA256 `9048440d45f0eaa20623860a831185f6a3f41d85388e27ed5c4909898d8cd480`.

These tests do **not** execute the changed PowerShell branch. They establish available xtask/workflow/macOS controls only. No full product suite, workspace Clippy, Windows/A09, packaged archive or publication pass follows from them. The reviewer ran only read-only Git/byte/document/model inspections, not Cargo, tests, build, PowerShell or VM probes.

## Discovery, settings and limits

Read current repository/Engineer/FinalReviewer instructions, role map, common handoff and required adversarial method/lenses. This is a fresh reviewer, not an author; no nested dispatch or settings substitution. Map and coordinator-reported host-accepted pair: **gpt-6.1-sol / max**. `agent_type: default`, `fork_turns: none` are the required requested configuration. Actual runtime model/effort/agent/fork metadata are unexposed here; no independent host-metadata guarantee is invented and no known mismatch was observed.

Before source/patch reads: MCP semantic `graph_search(release/install/install.ps1, slim, context_limit1)` then explicit semantic `graph_context(..., change_impact, slim, context_limit2)`, discovering indexed `crates/xtask/src/main.rs`. Its actual `copy_file("release/install/install.ps1", …)` at line487 is the hop to the installer. Registered old-root graph is navigation only; candidate bytes/Git identity are authoritative. A graph-health call was busy; no rebuild/refresh was attempted. Wiki list→search/concepts and relevant memory recall performed. Scryer governing/scoped rules, explicit-project orient, source/carrier pending queues, Release Gate committed model and drift were read; no ledger/source-map/verdict writes. Existing source-root fingerprint reconciliation remains pending. Linear connection pending and scoped tool search returned no live issue tools, so no live tracking verification/update is claimed.

Host PowerShell is unavailable. Parent-reported Parallels Windows11 ARM64 and proposed emulated-x64 work have no raw supplement supplied to this review at finalization; no VM result is accepted or asserted here. ARM64/emulation mechanism or unsupported-host checks are supplemental, not native Windows11 x86_64 A09 acceptance; no host guard bypass/support expansion is approved by this review. Original private28 baseline files were not accessible or inspected.

## Remaining gates and next owner

No corrective owner action/recheck is required for a confirmed defect because none was found. Production may consume this result for the authorized reviewed commit/stacked-PR push and persist related evidence unchanged in that PR, subject to Engineer's delivery ownership. User/assigned Windows executor and independent Verifier still owe the exact corrected seven-scenario × two-shell × two-mode28-case matrix: real AMD64 PE fixture identity/argv/exits, actual rejection and BOTH old destination hashes retained on each failure, all controls, zero missing/duplicate/unrun/skipped cases, raw assertion-derived JUnit and independent acceptance. Final packaged-script/archive/provenance identity and required desktop execution remain separate pending evidence before COD56→107/A09/Done/release.

Current-source Scryer anchors/tests/fingerprint-bound verdicts, applicable full hosted stack CI, strict non-bypass current-main integration/own-merge CI, complete-queue release and aggregate-owner gates remain intact. No user approval is supplied by this review. No source, docs, tests, constants, preferences, settings, refs, tracking or model changed; only this private review report was authored. Assumptions are bounded to approved real native probes and supported invocation contracts; no unsupported-shell/architecture promise is inferred. Reusable-knowledge assessment: no new durable claim warranted beyond existing native-exit procedure; read-only scope prohibits memory writes.
