# CBG-RESTORE-01 / PR200 Direct committed-read follow-up

Execution supplement for COD-136, review thread PRRT_kwDOSZ0Wus64bVWQ / comment4599530494. Baseline 6115439262005d1de984c5837e88f057c7317205, tree b2a56029eacc507a5b8f0a8a79207f73fe99e9eb. The maintainer requested resolving the comment, received the decision-complete plan, then explicitly said “Please implement the plan.” This restores unchanged approved revision 0.1 REST-R03/R05/R11 and A03/A10/A12; no new public behavior, compatibility exception or spec approval is invented.

Approved package d809fe0f8289bbf7d167a891bd7c31160f27f0e1, actual663b2fc8d2790956975587dcab6b248bd40b5958, CI d091767 and SEC fa25f63 remain immutable. Runtime authority is committed Scryer Graph Store node-88 publication/concurrency/recovery intent plus approved amendments. Proportional empty ledger chg-krba2h records this restorative bugfix; no new responsibility, foreign fold or main fingerprint test verdict.

## Problem and selected implementation

DirectStore::begin_read currently propagates cleanup_candidate_set failure after discovering no journal. A symlinked candidate or Windows delete-denied candidate therefore blocks a healthy committed graph. Frozen main 90e2 already has the unrelated corrective behavior omitted in reconstruction; port only that behavior, never its collector/lifetime-registration dependencies.

1. Keep begin_read returning the existing WriterLease. Inspect publication-journal presence with symlink_metadata. A genuinely missing journal returns the shared lease without candidate disposal; metadata errors are not treated as absence. An existing journal still takes the original containment/validation/replay/error path. Keep writer lock acquisition/relocking failure primary and hold the shared lease through all native reads.
2. Restore the noncreating DirectStore::candidate_cleanup_pending helper from frozen main. Inspect deterministic database-candidate bundle/sidecar and manifest-candidate paths with symlink_metadata; any entry or inspection uncertainty is pending. Never follow a live/dangling symlink or authorize deletion from presence.
3. In BOTH explicit and configured Direct runtime paths, OR that diagnostic into existing cleanup_pending after read resolution. pending_runs remains actual run count; no candidate count, response type, serialized schema or new read-session type is added.
4. Leave existing run-workspace recovery and writer/publication cleanup unchanged. Residue can be removed by existing positively owned cleanup or a later writer once its obstruction/protection is resolved; no-journal reads themselves do not delete it. No collector/background janitor/new persistent guard or interface returns.

Required-journal malformed/forged/checksum/containment/promotion/durability errors must not be blanket-caught or converted into optional success. No change to raw Direct destination keys, native path validation, reader/writer exclusion or no-replay contracts.

## Exact workflow and ownership

Delegated: one complete technical owner, default/gpt-6.1-terra/medium/fork-none, responsible ONLY:
- src/storage/direct.rs
- src/api/context.rs
- src/storage/test_harness.rs

Root alone writes this brief, Git/Linear/authority/delivery and public review reply/resolution. Technical owner shares the codebase, preserves others' edits, has no nested dispatch or Git/model/wiki/tracking/live mutations. No manifest/lock/config/new file ownership is granted; request reassignment before unlisted writes.

C4 coordinator acknowledged source/Git/queue hold. Its clean contracts cf9493edce3692054c4d6c17b53d2fdb562b632e and types6b6b273d0c37e43929c7d01aae11822783ad9bbe remain intact; existing remote checks may finish read-only. Do not rewrite/force-push either original or companion subject. Record the verified follow-up SHA for that coordinator afterward.

Pre-change source/ref/logical-index checkpoint is private implementation/pr200-direct-read-followup/checkpoint.json. Prior full preservation and qualified611bundle remain valid; preserve all current source/evidence and original main13working entries. No merge/release/installed binary/daemon/client/live graph change.

## Acceptance and verification

First preserve a regression that fails on611 for the reviewer's actual no-journal/healthy committed graph case, then correct it. Use short canonical temporary paths and owned fixtures.

- No-journal storage reads succeed with regular candidate residue, existing/dangling symlinks and unknown candidate entry types; pending diagnostic is true, committed database/manifest/sidecars and foreign targets/links unchanged, journal remains absent.
- Actual health/query succeed through BOTH explicit and configured Direct selectors while candidate residue is protected; graph_readable true, cleanup_pending true, pending_runs remains 0 when no runs exist. Normal no-residue status is false.
- Native Windows candidate opened without FILE_SHARE_DELETE does not block the committed read; ordinary writer/candidate cleanup remains strict while obstructed.
- Shared read lease excludes an exclusive writer until released. Removing the fixture obstruction and using existing owned cleanup/writer flow clears residue; a fresh read reports clean.
- Existing journal-phase replay, missing/corrupt/symlinked or forged journal/source/checksum cases, and unknown protected phase with a journal still fail safely/retain evidence. No-journal protected-phase read behavior remains available, with candidate presence pending.

Execute focused storage foundation + owning API tests, then applicable workspace checks, formatting and strict workspace/all-target/all-feature Clippy with cargo +1.98.1 and verified native0.19 inputs. Check df before large builds, use one normal owned target, preserve actual executed CLI/test binaries/non-system dependency closures and source/toolchain/command/JUnit identities before any later build. Leave cargo-reclaim and shared/frozen targets unchanged; do not reuse older C4 evidence targets.

Fresh independent Terra/medium execution and fresh prompt-only read-only Sol/xhigh adversarial review are separate gates; fix all confirmed P0–P3 and focus-reverify. Source graph MCP is unavailable; prior issue-owned frozen-main File/Method discovery and explicit change-impact/dependency/callgraph receipts locate all three target files. No-FTS semantic search is incomplete, so compare exact current source/callers against both baselines; never use a live guard bypass/rebuild.

Freeze exact patch and quiesce for root diff/staging/Conventional Commit. Rebind canonical evidence and fresh required Mac/Windows CI on the new PR200 head; previous 611 / CI845 green is not new-head qualification. Root replies with the verified fix SHA/actual evidence, then resolves only the identified review thread. Historical Windows reset/LEAK causes remain unknown unless separately established; do not relabel them or claim their fix.
