# COD-56 hosted evidence — same-review follow-up

**Outcome: accept the current hosted regression/policy and executed packaged-smoke evidence within its stated limits. No confirmed P0–P3 defect or evidence blocker found.** This adds no source-review stage, approval, A09 acceptance or authority to advance COD-56→COD-107, Done, main integration or release.

## Exact snapshot and inputs

PR173 head `72ec5bc7481491ea7cd964b711d2b98866425241`, base `cod103-stack/cod-106` at `2173d766c155583b1e8f979c303b560998cc7214`; head tree `a403771d1b6dd10da4b28526bbafb8d053dec8a9`. [CI41192405055/attempt1](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/41192405055) actually tested synthetic merge `4ffd5caf7cc35c34d68793afd1280d568e6d0311`. Independent live GitHub readback confirms its parents2173+72ec, tree equal to head, current PR identities, completed successful run and all22 successful jobs. Checks attach to head72ec; native checkout logs/diagnostics bind actual execution to the merge.

Technical commit8359773→72ec changes only docs. Local read-only Git comparison confirms no xtask/workflow/manifest/lock change from the approved source baseline; installer blob remains `9890b016699948634200624e85b868616c14617e`. Independently inspected saved immutable head/merge installer bytes both equal the reviewed candidate, SHA256 `830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb`. Prior source review `56972838324112c8a481120509d4d03991f0d0818df652ea4c4387afac220662` and VM follow-up `c78aaa141744681ecde7995ebd83f12fd51d5654a0d7dcf02a55da6915b4ea94` remain unchanged.

Authority remains COD103 rev0.3 five contracts at `316be4c4bb81d4f6e025016b12374836b4dcc1a8`, approval `0e6008a1f46679a86aba85ac513beb05af94b0c0`, external CBG-PLAT-01 S05/R05/A09 at `8a410c7a22f5291f06a1cece5cd9e7c0be255e72`, source ledgerchg-2z3ngs, existing resp-16/resp-256. No reopened approval or new claim.

## Evidence actually assessed

Independent Verifier root: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-c30ebc5e/cod56-hosted-verification-72ec5bc`.

- `final-report.md` SHA256 `5ba4fd9347f5be78d5e0e0bc9358a7ae7cec484fddf1b15b11c5a3aabcf8245c`.
- `manifest-final.json` SHA256 `5d1fef6514dbd3369add445f02c4f43e56f9b90c0c073277903ed7047b5a38e8`.
- `result.json` SHA256 `5118ed9cc3deee7892066c746ad67a6d7ce128929bd89485aa511a0540bf9f7a`.

Read actual final report/commands, PR/run/check/merge receipts, all15 native report.json plus raw platform logs/named inventories,22 full job logs, artifact metadata, warnings/annotations and immutable source excerpts. All262 final-manifest entries match actual bytes/lengths/hashes. Live independent API results agree with the final report. Reviewer retrieval initially hit gh terminal-escape output refusal; successful read-only retrieval followed with `--allow-escape-sequences`. This was an observation limit, not a CI failure.

- **22/22 current jobs and22/22 head checks succeed**, including the stable required guard: four policy jobs,15 native cohort/shard test jobs, two artifact jobs and required. Native scope is three macOS ARM64 cohorts plus three Windows x86_64 cohorts × four shards. Ubuntu policy jobs do not imply Linux-native qualification.
- **15 native diagnostic reports** all identify tested merge4ffd5caf, captured source-bound CLI/compiler-artifact hashes, cli_unchanged=true, nonempty commands and error:null for every execution. Actual native-tests optimized profile and Windows bundled-extension features are retained. macOS image is macos-26-arm64; Windows is Server2022, not Windows11 desktop.
- Recomputed raw harness summaries total **2764 passed,0failed,8ignored per invocation**, not unique cases. Focused/nested runs may overlap. Eight ignored results remain visible with subprocess/public-producer fixture reasons and are not passes. Filtered-out names are not execution. No zero-test cohort was substituted. **No hosted JUnit XML exists**; local76-case nextest JUnit remains separate. No XML or verdict was fabricated.
- Both artifact jobs actually build optimized products and execute native-artifact against the tested merge. Raw Windows log records successful extracted `pwsh -File … -DryRun` validation and final archive path; Mac records final archive path after its installed-path/negative-runtime checks. The unchanged exact source call chain (`main.rs:464–535`,707–735,1827–1900,2369–2551) requires archive extraction/validation, installer DryRun, packaged graph/wiki smoke and strict successful native MCP initialize/tools-list validation before printing that path. Thus these are **executed bounded packaged smokes**, not merely configured jobs. Successful MCP transcripts are internally captured, not independently downloaded/printed evidence.
- PR native upload is disabled. Actual artifact API has15 diagnostics only, **no native archives/provenance/checksum bytes**. Exact copy logic and remote installer bytes do not prove independently retrieved packaged-script/archive identity or a public artifact. Server DryRun success does not exercise the controlled nonzero A09 matrix.

## Warnings and excluded history

No Rust compiler warnings were found in the22 actual raw logs; full workspace/all-target/all-feature Clippy succeeded with `-D warnings`. This is **not zero global warnings**. cxx1.0.138/RUSTSEC-2026-0202 and tokio1.53.1/RUSTSEC-2026-0386 remain visible audit-allowed baseline risks. Prior accepted COD106 report hash `8e6f2c01a3f5f8aa77822d44e9abd708af481769dc912f8c99fa8871f694b762` was independently checked. Unchanged lock blob `26a3c2b212c40fd20d8994379b52a062c6b36151` and CI blob `66e3cc1430c82d09dac6c25cd0a70deb8f7d9383` support the coordinator's unchanged-risk disposition, not a new security waiver/remediation. Dry-run existing-version/upload-aborted notices and nine runner UI notices are separate.

Cancelled predecessor runs are not passing current evidence. Watcher transport failures are not product/CI failures; the disclosed lost first-watcher raw output is not invented. None replaces the successful current run. No further watcher or execution was started by this reviewer.

## Limits, settings and next owner

This remains the same independent FinalReviewer, mapped/coordinator-reported host-accepted **gpt-6.1-sol/max**, required requested default agent/fork_turns none and fresh original review context. Actual runtime metadata remains unexposed; no independent host guarantee is invented. Prior valid graph-first discovery was reused and explicit semantic native_artifact dependencies context refreshed before bounded call-chain reads; indexed main.rs literal references discover the unchanged workflows. No nested dispatch, Cargo/test/build, PowerShell/VM command, source/docs/model/tracking/remote/Git write occurred. Only this separate private report was authored; no constants changed. No new assumption expands supported platforms or qualification.

Engineer/Production may preserve the accepted hosted evidence unchanged on the same PR under existing ownership. Actual native Windows11 x86_64 PS5.1+supported7 corrected28-case raw/JUnit evidence, both preserved destinations on failure, final packaged-script/archive/provenance identity and desktop checks remain pending. SAME-model/current-source anchors/tests/fingerprint verdicts and applicable non-bypass strict freshness/own-merge main CI/full-queue release/aggregate-owner gates remain intact. This follow-up supplies neither user approval nor COD107 activation. Reusable-memory assessment: no durable new knowledge warrants a write for this transient run.
