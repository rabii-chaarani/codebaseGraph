# CBG-COPY-01 R1 — Resource-ceiling removal execution

[Approved amendment](resource-ceilings-amendment.md) and [exact user approval](resource-ceilings-approval.md) govern this follow-up after `8e47d2b`. Historical Scope A reports are retained, not relabelled as post-removal evidence.

## Implemented scope

Removed worker/Rust memory options from CLI, config, API, native requests and refresh health; removed RSS-triggered worker/phase termination, artificial source/partition/artifact admission estimates and the application-derived native pool. Native database opens use the actual Ladybug defaults with independent read/write thread tuning preserved. Removed the obsolete application `MemoryBudgetExceeded` error contract rather than passing enormous sentinel limits.

Retained spill/chunk and concurrency options as optimizations. Normal rows rotate at the target; individually larger valid records form their own chunks/runs. Merge opens at most the existing fan-in per level and progresses through multiple jumbo runs. Search documents retain the existing read-format bound; targets no longer reject a valid document solely for exceeding a batch size.

Unchanged: no same-candidate mutation replay, native result error propagation, original-error primacy, bounded diagnostic ingestion/context, COPY-query versus explicit-checkpoint labels, Managed/Direct publication and previous-bundle preservation, containment/ownership/cancellation, corruption/path/format checks, protocol bounds, cleanup per-pass policy, dependency/native version and supported platforms.

## Executed evidence

[Machine-readable checks and limitations](resource-removal-evidence/checks.json) identify commands/log hashes and failures corrected during integration. Local macOS arm64: 134 focused cases, 42 config/API/ownership cases, 2 real CLI COPY/reopen/control/preservation cases, the legacy stored-budget case, the owned-allocation case, mixed/jumbo merge cases, actual MCP schema case and 3 doctests pass. Strict all-workspace/all-target/all-feature Clippy passes. Native-test profile compiles. The corrected jumbo native JSON oracle passes (1/1); [independent final review](resource-ceilings-final-review.md) reports no confirmed P0–P3 source defects and verifies the corrected oracle and its passing log. Final formatting, all-target/all-feature workspace compilation and strict Clippy pass on the reviewed hashes.

The first jumbo native test incorrectly expected the syntax `text` preview to hold a complete 2 MiB comment. The parser intentionally keeps 160 characters there. Scratch read-only native inspection found the full value in JSON metadata (2,097,217 encoded bytes). The corrected oracle reads/parses that field; no product truncation fix or weakened logical expectation was introduced.

Natural allocation, engine sizing and I/O limits remain; removing application enforcement does not promise unlimited resources. The existing search read-format guard, security/protocol bounds and maintenance per-pass quotas remain deliberately unchanged.

## Qualification and handoff

Scryer change `chg-vhbvd3` records the approved four existing-claim amendments; `chg-67epjd` records implementation ownership. Registered model/graph tools still reference the original checkout; no passing test verdict is falsely attached to old source fingerprints. Linear remains verification-pending. Canonical wiki explanations were updated through k-wiki, then transferred with PR-specific lifecycle qualifications preserved. Validation/links/site build passed; stale old memory-budget operational recipe was quarantined without erasing history.

Additional post-removal cancellation and unknown-containment process cases each passed (1/1). These execute the post-removal source-bound binary, not an earlier budget-enforcing binary. Full post-removal workspace/native-platform qualification remains with the new PR153 CI. Earlier pre-amendment native lifecycle16/16 pass is retained only as historical evidence; its interrupted long full-workspace run is not a passing result. No merge, deployment, daemon restart or live graph rewrite is performed.
