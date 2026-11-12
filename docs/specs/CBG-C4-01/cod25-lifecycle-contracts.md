# COD-25 lifecycle prerequisite — proposed supplement 0.1

## Identity and approval continuity

Work: CBG-C4-01/F04/S01 / COD-25, R04/R12, A06. Date: 12 November 2026, Australia/Darwin. Status: **proposed narrow scope/ownership amendment; no additional source permission yet**.

The maintainer already approved [C25 supplement0.1](https://github.com/rabii-chaarani/codebaseGraph/blob/f7f10a21c45914b369fdfb9e8f0f364f10efc235/docs/specs/CBG-C4-01/cod25-contracts.md) and [brief1](https://github.com/rabii-chaarani/codebaseGraph/blob/f7f10a21c45914b369fdfb9e8f0f364f10efc235/docs/specs/CBG-C4-01/execution/COD-25.md) at full SHA `f7f10a21c45914b369fdfb9e8f0f364f10efc235`. The [actual approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/e36e9e581441f9ead8e6ff5febec7234e5a6c5e8/docs/specs/CBG-C4-01/approvals/cod25-approval.md) preserves that grant. This proposal does not withdraw, rewrite or silently advance it. Existing C25-01–09/C25-A01–A08 remain requirements.

The approved brief excludes all existing process_lifecycle source and expressly stops affected work if its APIs are insufficient. This proposal adds one necessary owned prerequisite, `src/process_lifecycle/mod.rs`, for the controlled-startup behavior below. The revised [brief3](execution/COD-25.md) retains the original file list and adds that exact exception. Actual human approval of the published full SHA/paths of these two amendment documents must precede that source change or dependent capture implementation.

Product source remains exact `7c81720f3cb32c392e34833033b0ccec6766e2ca`, tree `5e71d82b0c0fcf55d7447876d19e6acbceba791c`. The original main pilot, all prerequisite approvals, immutable PR194 subject, original wire/identity contracts and downstream owners remain unchanged. This proposal contains no product code, execution logs, review report or test-output artifact.

## C25-L01 — Problem and bounded architectural placement

At the pinned source, [PreparedChild::prepare](https://github.com/rabii-chaarani/codebaseGraph/blob/7c81720f3cb32c392e34833033b0ccec6766e2ca/src/process_lifecycle/mod.rs#L145) creates private startup state and spawns the guardian before polling READY for its own fixed fifteen-second interval. It accepts no caller control callback and returns no owned handle until readiness. Its timeout closes the owner pipe and returns a String. The exposed descriptor/recovery APIs are available after READY; workspace cleanup probing does not give the caller control of the in-progress preparation. Consequently a cancellation/deadline during that library-controlled wait cannot be observed or acted upon by the capture caller under the approved no-edit boundary.

This is an API/contract feasibility finding, not a claim that existing production behavior regressed or a runtime failure was reproduced. A supervisor thread postpones the same hidden wait; parsing private startup files in the capture adapter adds an unauthorized protocol dependency. The remedy belongs in Runtime execution, not in Rust resolution or Native Architecture.

Read-only Scryer location identifies this file under Graph Runtime `container-product-runtime` with no anchored file claim. Preserve that boundary, all existing model/directive/history/pending work and unrelated closure owners. For this scoped COD-25 prerequisite, the approved Git/bootstrap authority and the eventual exact amendment decision govern its execution record; do not invent a Scryer node/claim/ledger or migrate Runtime authority. A missing/conflicting binding directive or ownership conflict still stops affected work for the owner.

No existing platform-specific lifecycle, storage/maintenance source or serialization protocol is assigned. The original package/dependency/feature/public-operation restrictions remain. If the single-module change cannot satisfy the contract without those other changes, report the exact remaining requirement before further implementation.

## C25-L02 — Additive neutral controlled-preparation API

Keep `PreparedChild::prepare(SpawnSpec) -> Result<PreparedChild, String>` callable with its current defaults, timing origin, errors and existing callers unchanged. Keep SpawnSpec literal fields, current launch/ready/control/receipt formats and containment protocol versions unchanged. Share implementation where appropriate instead of introducing a competing lifecycle owner.

Add a crate-private controlled entrypoint and typed failure/recovery vocabulary in the same module:

```rust
pub(crate) enum PreparationStop {
    Cancelled,
    DeadlineExceeded,
    ResourceLimit,
}

impl PreparedChild {
    pub(crate) fn prepare_controlled(
        spec: SpawnSpec,
        check: &mut dyn FnMut() -> Result<(), PreparationStop>,
    ) -> Result<Self, PreparationFailure>;
}
```

`PreparationFailure` has private fields and read-only `kind()`/`message()` getters plus `take_recovery(&mut self) -> Option<PendingPreparation>`. Its kind distinguishes `Stopped(PreparationStop)` from infrastructure failure. Preserve the caller's original detailed error/diagnostics in the invoking bridge where necessary; do not collapse resource/cancellation/integrity distinctions into message matching.

`PendingPreparation` is an opaque, non-Clone, non-Deserialize runtime owner, constructed only from the actual startup operation. It retains the real guardian/owner-lifetime resources and operation-private job identity needed for recovery before an accepted READY descriptor exists. It is not a caller-built BuildDescriptorV2 and cannot qualify containment by a path, PID or submitted boolean.

Expose only `request_cancel(&mut self) -> Result<(), String>` and nonblocking `poll_recovery(&mut self) -> Result<Option<RecoveryOutcome>, String>` on that owner. Cancellation requests use the actual retained ownership channel; they do not start target work or recreate an interrupted request. Pending means no confirmed outcome; a containment-unknown outcome does not release retained resources or authorize workspace removal. Only independently established empty/terminated-empty containment permits final resource release.

The new controls are Runtime vocabulary. They import no Native Architecture types or policies and add no configured numerical default. The Native bridge supplies its existing effective deadline/cancellation/work check; the runtime checks it before dispatch, around fallible startup stages and inside each library-controlled READY/recovery poll. A stopped controlled call must not continue the fixed legacy fifteen-second poll merely because that API once did so. It never sends START after interruption.

The entrypoint still returns PreparedChild only after genuine readiness and existing containment validation. Checks do not create a partial successful object. Opaque platform/OS calls remain subject to the original cooperative-control limits; the change addresses the runtime's own polling/ownership gap, not hard real-time interruption of arbitrary native code.

## C25-L03 — Failure, cleanup and compatibility

Before dispatch, a proved pre-dispatch failure returns its actual error without claiming a child exists. After a real child is created, every interruption/infrastructure/READY-parse/error path must either independently confirm cleanup or return/retain its actual PendingPreparation owner. Close/request cancellation through the retained owner channel promptly when the callback stops the operation. Preserve the stop reason even when cleanup remains unknown; cleanup failure is additional context, not a replacement success.

Runtime capture must take and supervise any pending owner through actual completion/recovery. An error or dropped caller must not drop the last live ownership/permit merely because the caller deadline elapsed. Use invocation-owned runtime supervision, not a second durable task service or global detached registry. Retain the owned workspace and useful recovery identity while termination is unconfirmed. Do not delete its records, infer termination from a timeout/guardian exit alone, forge a descriptor from partially published files, or replay the build.

The new method's caller can return an interrupted source-evidence error with an accurate recovery diagnostic while runtime ownership remains live. Generic architecture consumers need no platform process types. Existing EvidenceError public methods, source-evidence signatures, v1 wire values and legacy PreparedChild callers remain compatible; any private recovery retention must honor those ownership lifetimes.

The legacy preparation entrypoint retains its current behavior for existing callers, including its historical startup interval and String contract. Do not silently apply Native Architecture deadlines globally, change storage/maintenance recovery semantics, widen visibility beyond the crate-private lifecycle surface, add dependencies or change Unix/Windows protocol implementations. Actual existing-call compatibility and new controlled-path cleanup require tests.

## C25-L04 — Verification and delivery gates

These checks are planned; no product build/test is claimed:

| ID | Required observable verification |
| --- | --- |
| C25-LA01 | Through actual contained helper startup with delayed READY, cancel and expire the caller deadline before readiness. Check that the controlled poll observes the reason, sends no START and confirms cleanup or retains the actual pending owner. Keep debug-only barrier coverage distinguished from production-source reasoning. |
| C25-LA02 | Inject startup/READY parsing/guardian exit and cleanup-unconfirmed failures at pre/post-dispatch phases. No error branch loses the last actual owner, fabricates readiness/emptiness or removes an unconfirmed workspace. Exercise later recovery without replay. |
| C25-LA03 | Existing PreparedChild prepare/start/wait/terminate/recovery tests and affected existing callers retain their API/default/error behavior; no SpawnSpec/wire/protocol/dependency change. No Native imports enter the lifecycle module. |
| C25-LA04 | Exercise the new path through actual source-evidence preparation, not only a lifecycle reducer. C25 resource/cancellation/error diagnostics and retained ownership agree. Then run C25-A01–A08, including actual pilot sufficiency and final snapshot verification/review. |

Use the original one complete technical owner and independent executing verifier/fresh final reviewer settings. The owner is quiescent before this proposed reassignment; no product file has changed. After actual amendment approval, root records that exact grant and reissues bounded GO with one owner for the added module. No membership/assignment alone supplies authority.

Publish only this substantive amendment and its required brief in the amendment PR; no logs, execution/review evidence files, checkers or unnecessary documents. Keep the original approved PR194 frozen. Canonical approval/progress records remain separately linked. No Q2/Q3 expansion, weaker guarantee, merge/release, unrelated Maintenance work or downstream/aggregate closure is granted by this proposal.
