# COD-21 Rust 1.98 Clippy cleanup — proposed bounded assignment

Status: **awaiting explicit human approval and Scryer sign-off before source edits**. Work CBG-C4-01/F02/S02 / COD-21; 7 November 2026, Australia/Darwin. This proposal is a prerequisite cleanup for the maintainer-approved root Rust1.98 minimum, not an architecture redesign or an expansion of C4 runtime features.

## Trigger and baseline

Independent default/gpt-6.1-terra/medium verification on integrated source `3bf045a10fc8684101a407c924efbbf645db84fe`, Rust/Cargo1.98.1/Darwin arm64, passed formatting and all66 scoped tests but failed strict all-target Clippy with15 diagnostics in existing paths. The increased declared minimum enables newer MSRV-aware idiom lints. The earlier owner cached Clippy pass is not current passing evidence. This gate remains unsatisfied; no suppression or lowered lint level is proposed.

Actual user Rust-minimum decision is [recorded separately](../approvals/cod21-rust-minimum.md); it is not represented as explicit approval of these nine newly scoped source files. Automatic approval review rejected `scryer.sign_off` for empty chore ledger `chg-w9q2zb` because that additional scope lacked an explicit human go-ahead. No rejected action was bypassed and none of these proposed legacy edits has started.

## Exact proposed changes and file ownership

After actual approval/sign-off, the existing sole technical owner `/root/cod21_implementation_2` (default/gpt-6.1-terra/medium) may change **only the listed idiom sites** in these additional files. The owner shares the workspace, preserves others' work, dispatches no agents and performs no repository Git mutation. Root serializes Git, tracking and delivery. Existing compiler ownership stays with the same owner.

| Additional writable file | Frozen diagnostic sites | Behavior-preserving change |
| --- | --- | --- |
| `src/artifact_store.rs` | 801,1847 | Constant-width2/512 immutable/mutable chunks to borrowed `as_chunks`/`as_chunks_mut` array views and equivalent iteration |
| `src/storage/maintenance_backup.rs` | 52 | Width2 borrowed chunk iteration |
| `src/storage/maintenance_candidate.rs` | 251,3184 | Width2/512 borrowed chunk iteration; remove only redundant slice-to-fixed-array conversions made unnecessary by the typed view |
| `src/storage/maintenance_legacy_association.rs` | 290 | Width2 borrowed chunk iteration |
| `src/storage/maintenance_legacy_stream.rs` | 216,219,1146 | Nonzero768 modulus predicate to is_multiple_of; width768 borrowed chunk iteration and redundant conversion cleanup |
| `src/storage/maintenance_native_projection.rs` | 60,1489 | Existing nonzero PAGE_BYTES/2048 modulus predicates to is_multiple_of |
| `src/storage/maintenance_origin.rs` | 606 | Existing fixed FRAME-width borrowed chunk iteration |
| `src/storage/maintenance_validation.rs` | 961 | Existing nonzero8 divisibility predicate to is_multiple_of; retain Boolean short-circuit order and other bit operations |
| `tests/support/storage_maintenance/collector.rs` | 482,2151 | Width512 borrowed chunk iteration in existing fixture checks |

Preserve all existing chunk boundaries, remainder handling, validation/error order, checksum coverage, formats, resource limits, zero-copy borrows, state/lease/confinement/publication/recovery guards and public signatures. All divisors/chunk widths are existing positive semantic constants; introduce no numerical cap or default. No additional cleanup, dependency, feature, suppression, file move or API exposure is authorized by this proposal. If a suggested lint remedy cannot preserve behavior exactly, stop that site for the coordinator rather than changing the contract.

## Authority and starting conditions

These legacy paths remain Scryer-governed; no native-authority exemption is invented from a file/work ID. Root fetched change-ledger/orientation/plan/sign-off/build/closure/proportionality/directive rules, opened `chg-w9q2zb`, oriented the exact files and captured existing drift. Graph Store node88 owns the production helpers; Graph Runtime owns the test-support scope. The only returned anchored artifact-store claim is resp234 (validated partition reuse); its meaning remains unchanged. No changed/new responsibility, dependency, model node or directive is proposed, so proportionality requires no invented claim/plan entry. Existing pending work, other open changes and global drift are outside this chore and must not be folded or reconciled as its result.

The graph MCP transport was unavailable at this stage. Read-only repository CLI fallback with no refresh found all9 File nodes and explicit change-impact context. Source snippets at the frozen diagnostic sites were inspected after discovery. No installer/rebuild, other-project state or integration-setting change was used.

This committed proposal and actual explicit human approval of the named scope, followed by successful `scryer.sign_off {change_id: chg-w9q2zb}`, are required before these source edits. Scope assignment is not approval. The separate confirmed C21 P2 cancellation fix in compile/validate.rs remains within already approved ownership and can proceed while this decision is pending.

## Verification and closure

Use explicit Rust1.98.1. Run unsuppressed all-target Clippy and formatting, affected artifact encoding/origin and storage parser/projection/replay/collector tests, the two existing attached partition-reuse tests, and the compiler/type/schema radius after integration. Reuse valid checks only while their inputs remain unchanged; independent verification will rebuild the corrected delivery in its own normal target to eliminate shared-target ambiguity, recording actual source/binary provenance. A fresh final review has already found one separate P2 checkpoint defect in the compiler; its correction and this cleanup need focused review against the final corrected snapshot. No new tests mirroring trivial idiom replacement are required; preserve and execute meaningful existing behavior tests.

No claims are folded or source-map/directive/history entries rewritten to obtain a pass. Close only this empty ledger through the configured tool once its bounded work/evidence is reconciled; preserve all unrelated pending intent and qualification. Session logs/evidence stay outside the implementation PR; its additional source/test-support edits are essential only because the approved minimum change exposed the strict lint gate. PR publication/merge/release and aggregate closure remain subject to their existing separate gates.
