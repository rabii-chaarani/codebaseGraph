# COD108 workflow consumer handoff

Completed only `.github/workflows/ci.yml` and `docs/release.md` under the explicit Engineer grant. Approved design§8 at316be/approval0e6008, C103-R04/R05→A04/A05, existing chg-cw9n3h and two signed-off claims preserved. No other source/contract/permission changed.

## Actual snapshot

- Worktree `/private/tmp/codebasegraph-cod108-20261101`, branch cod103-stack/cod-108, existing HEAD/base90af76d3453a6f5cc0f032045ed48c4fd09c275c. No Git command/mutation run in this slice.
- YAML SHA256 `b59d7028101120f6885870cec054a65d9788f55d364dc0f2eb2b53bd4fd9c339` (21095B); docs SHA256 `90d84f6614be6cd74b8340dd73b1a2c1000457af319475cf8939b1c316ebf486` (22125B).
- Frozen ci.rs b8971114… and main c175e0df… remain unchanged, as do release/workflow_tests.rs, native.yml, release.yml and manifests/lock. Full hashes in result.
- Exact two-file diff: consumer.patch. Actual Bash bodies and uncalled jq syntax wrappers are included only as private static-check evidence.

## Actual implementation and fixture anchors

- classify L35; identity step `identity` L45; immutable trusted B checkout L111; separate B/object checkout L120; decision step `decision` L129.
- Classify outputs `mode` and compact `decision`. identity outputs `candidate`, `base_sha`, `head_sha`; actual script environment and all JSON fields appear in result.actual_interfaces/decision_fields.
- Identity body uses runner/event repository identity, current PR/main API and exact approved creator/branch/label/B/H. Eligibility acquisition problems choose full. Candidate-only B checkouts have persist-credentials:false; their failures are tolerated only to emit full. H is fetched into the separate data store, never checked out/executed. Cargo CLI/manifests/lock/cwd are trusted B with private target directory. No compiler cache, publication secret or head helper admission.
- repository-policy L183 is the sole ordinary release-gate command and includes bounded xtask regressions. Common jobs stay mandatory; existing heavy jobs depend on explicit full.
- required L319, actual Bash `aggregate` step L337: exact dependency key/result set, schema/mode/run/event/B/H and fresh current API checks in both modes. Lightweight requires positive metadata plus exactly skipped heavy jobs and final trust/currentmainB; full requires heavy successes. Results and decision enter as environment JSON, not interpolated PR text. Final read is not a server merge transaction.
- native_retention L266 byte-identical; native L299 matrix/inputs/90days/recipes unchanged. Only scheduling prerequisites/full-mode condition changed. Label events and literal108 base do not expand107 retention. No native/release/action file edited.

## Executed static evidence and limits

YAML parses; exact unchanged job/input/trigger invariants and single policy invocation pass static assertions. All6 embedded Bash bodies pass bash -n. All18 extracted jq filters compile inside uncalled functions; no predicates/API/body fixtures were run. These are syntax/structure results, NOT behavior or hosted scheduling evidence. No Cargo/build/test/Git/API/tracker/wiki/model command or write. Existing typed Rust policy is pending Backend alignment against real YAML, not weakened or declared passing.

Expected event/mode/failure matrix and actual env/step/output/needs inventories are in result.json. Backend must execute the actual identity/decision/aggregate bodies with closed fixtures, reconcile policy and exact-base/activity checks, and cover failure/missing/cancelled/stale/partial-input cases. Independent verification/fresh review/full bootstrap PR follow. Actual trusted lightweight rollout remains after protected-main deployment/full own-main CI and before final release PR merge; no unit/config/PR substitute is asserted.

Settings: host-requested openai-codex/gpt-6.1-sol/high, runtime self-inspection unexposed/no known mismatch. Assumptions/constants are explicit in result. No reusable memory warranted. No true consumer contract blocker found. YAML/docs/source/Git/Cargo/private writes STOP and both file leases are released on this handoff; next owner Engineer/Backend. No110/Done/merge/release authorization inferred.
