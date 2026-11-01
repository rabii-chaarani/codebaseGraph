# COD108 — graph_health error-response diagnostic

This same-PR175 diagnostic changes only the existing assertion failure message in src/adapters/cli/tests/mcp.rs. It prints pretty JSON for responses[2] while retaining the exact isError=false oracle, all five requests, fixture setup, downstream assertions and runtime behavior. It is NOT a causal fix or hosted failure waiver. Reviewed test SHA256 is772e2d7c7ad22cc4155875b5cdbf5a480ab688891a7a29ebd563ea6e7349164a/19557bytes. All2370 other prior tracked blob/mode pairs and17 README/package paths remain unchanged.

## Real failed history and limits

[Actual Mac failure report](mac-failure/report.md), original manifest/raw job124897498528/source/member audit preserve CI41323156535/attempt1 at head0413e543668f3e08003aa6a0d4ead350f49a61cc, actual nativeS1c1730dfd06f238a6f6546e7e842a3988e52d61e. graph_health id3 returned isErrortrue; only its Boolean was printed, not full error JSON. Root cause remains UNKNOWN. The final run completed FAILURE:25jobs/checks,22success/2failure/1skipped; Macunit andrequired failed, Macartifact skipped. [Production final snapshot history](production-final-run/report.md) records closed watcher14755exit1/log5202967bytes/SHA09dda9ce…; no manual rerun/cancel.

[Prior Windows milestone](windows-fixture-pass/report.md) proves the portable extra-path fixture on actual Windows unit2 only; full-platform unique coverage is not inferred from Windows summary successes. Original157a/Sc2e6 Windows filename failure and0413/f98/102/d278/focused fixture-fix history remain untouched, not relabelled or silently superseded.

## Diagnosis, unchanged baseline and narrowed source scope

[Backend read-only diagnosis](backend-diagnosis/report.md), [result](backend-diagnosis/result.json) and unchanged original manifest preserve the82dca/68af handoff and its original proposals. A subsequent explicit Backend addendum narrowed the chosen edit to formatting existing responses[2] in the assertion failure message only: no subprocess/MCP method changes or added method assertions. Provenance: Engineer's diagnostic-commit assignment in this session relays that author addendum and binds actual source772e2d7c…; no separate author addendum file was supplied or fabricated. Original Backend report bytes are not rewritten to imply its earlier broader proposal was implemented.

[Unchanged native baseline](unchanged-baseline/report.md) retains all14original closed files: one unchanged selected native test passed locally, not a waiver or root-cause result. [Independent diagnostic verification](independent-after/report.md), original manifest/commands/provenance/raw logs and [genuine JUnit](independent-after/mcp-diagnostic-junit.xml) record one Cargo execution and one nextest execution of the SAME1unique MCP case, both passed with zero warnings/failures/errors/skips/retries. Formatting and strict root Clippy passed. JUnit SHA256ff64e0c40547145a28223304455e55973f24e809420742080fccf302f602c6cc. The failure-only response payload was NOT emitted because these local runs passed. No full-workspace suite/Windows/new hosted pass is claimed.

[Same focused Final Reviewer](focused-review/report.md), original result/manifest report PASS for this diagnostic-only source and affected local evidence; report379fe3ee… retains the hosted unresolved-cause limit. Production did not rerun Cargo/tests. Prior independent and review histories remain as authored.

## Preservation and pending work

[Copy selection map](copy-selection-map.json) records every original path/hash/size/mode and explicit physical exclusions; original author reports/results/manifests/empty logs/raw historical whitespace remain byte-identical. ZIP/native/NUL/redundant source/full inventories/model/cache/private unrelated APIs are excluded. No README/executable/package input or credential is added. Existing0413 evidence/index/authorfiles are unchanged.

Authority remains approved316be4c4bb81d4f6e025016b12374836b4dcc1a8 plus separate approval0e6008a1f46679a86aba85ac513beb05af94b0c0/decisionc8690556-e836-428d-bc81-079c701d2395, existing COD108chg-cw9n3h and empty proportional diagnosisledgerchg-c6x8xk. No new claim/approval/architecture is invented. resp50 current-source genuine-XML reconciliation remains PENDING under separate canonical-source authorization; no model verdict/fold/old-root ingestion is performed by this commit. Canonical clean0413 retains oldtest22cd until a separate operational grant.

After actual committed identity checks and separate ordinary push/comment permission, fresh FULL CI must qualify actual new-head Mac behavior and all workflows. Old completed failure needs no cancellation and missing old artifacts/unborn new CI are not precommit gates. A repeated error must now expose its response for diagnosis; passing alone is not proof of a causal fix. Full A04/A05/post-main lightweight rollout/model gates remain at their approved stages. No Done/COD110/main retarget/merge/release/aggregate closure follows.
