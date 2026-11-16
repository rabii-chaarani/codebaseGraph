# CBG-IDENTITY-01 — Managed identity across reboots

## 1. Identity, authority and approval

| Field | Value |
| --- | --- |
| Work | [COD-135](https://linear.app/codebasegraph/issue/COD-135), separate Maintenance bug fix |
| Canonical specification | `docs/specs/CBG-IDENTITY-01/spec.md`, proposed revision **0.1**, 2026-11-16 |
| Source/authoring base | main `90e2ab6fae2767bdb158e19f0072d43568e80dbc` |
| Branch | `codex/fix-managed-identity` |
| Authority | Scryer `chg-r9y7bh`, execution brief in `node-110.description` within that same change; existing Graph Store, lifecycle, API/CLI and supervision owners |
| Status | Proposed contracts and execution preparation; no product implementation, exact-revision approval or live enrollment |
| Closure owner | Maintainer/user; no automatic aggregate closure |
| Approval | None. A separate later record must identify this path, full approved Git SHA, exact scope/model snapshot and actual user decision |

The user selected a durable fix plus current-store recovery; a separate recovery command; disabling automatic collection now; managed stores only; explicit original-store/filesystem attestation with the other identity/ownership checks retained; and a reviewed PR followed by current-store recovery/rebuild after concrete preview approval. The user then requested “Please implement the plan”. This authorizes contract preparation and the selected workflow, not approval of uncreated final bytes. Commit/publish these contracts and obtain matching exact-revision approval before dependent code. Never pre-create an approval record or infer live enrollment from plan approval. The captured [unsigned model delta](architecture-proposal.json) registers genuine claims `resp-y5d311` (Graph Store enrollment), `resp-xpv8tb` (lifecycle recovery coordination) and `resp-1e1m51` (API-core collection exclusion); no existing pending entry is reassigned.

The user explicitly cancelled COD-113 because automatic collection is being retired. COD-113 is cancelled, not fixed; preserve its branches, unfinished source, approvals and failure/binary evidence in place. Its qualification exceptions do not transfer. This issue is not dependent on completing it. Before any shared source edits, independently verify old writers are quiescent; a Canceled status is not that proof.

Scryer remains authority for this runtime/storage scope; CBG-C4-01's scoped bootstrap is not an exemption. Preserve pending CBG-REBUILD-01 and unrelated claims/directives/history. Reuse genuine existing responsibilities for restorative work; model the new explicit recovery operation and removal of automatic effects before code.

Original wiki contracts stay at their original concepts/locations: `architecture/graph-storage-lifecycle`, `architecture/graph-disk-cleanup-contract`, `architecture/worker-recovery-regression-contract`, `architecture/invariants`, `architecture/graph-runtime` and `architecture/materialization-pipeline`. This proposed delta retires automatic collection, not ownership, confinement, publication or containment safety. Do not copy or silently supersede their approval histories. A narrow dated amendment must link the actual approved replacement scope before behavior changes.

## 2. Observed failure and desired result

The current managed root is `/Users/rabii/Projects/Repositories/codebaseGraph/.codebaseGraph/storage`. Its valid 512-byte CGGGATE1 binding stores device `16777232`; the current device is `16777235`. Eight checked inode values match and its SHA-256 payload checksum is valid. The guard compares the recorded tuple with fresh native identity and returns `generation_gate_binding_changed`.

The binding was created November 7, the selected graph published November 13, and this machine booted November 16 at 08:46 Darwin time. Device renumbering across reboot is consistent with these facts; no historical volume UUID or binary source attestation is invented. Native-access and quiesced full-build retries, health and daemon initialization all failed at this guard. The daemon is stopped. Configuration/root/binding bytes remain unchanged by the investigation.

The owning representation persists Unix `st_dev` as `FileIdentity.volume`. Retained graph file-index/hash inspection found 27 identity-related modules, plus raw device/inode persistence in Unix FIFO containment proofs. Automatic collector activation is intended to be optional, yet its generation gate is a mandatory native-open prerequisite. Fix both the surviving persistence problem and that obsolete dependency.

Success means the same managed root operates through the verified fixed runtime, survives later device renumbering, publishes the requested fresh graph, and has no automatic disk collection. Recovery does not replace, clear or relocate the store.

### Scope and exclusions

Include managed-store admission/identity, surviving execution/ownership/publication/cache safety, explicit recovery, automatic collection detachment, matching helper protocols, regression tests, documentation and current-store activation after preview approval.

Keep graph database/storage format v2, source manifest v5, configuration schema v3, source/parser/ontology/search meanings, native SDK/backend, resource policy, transport protections and current managed root. Introduce only versioned private identity/recovery/helper metadata. No two-slot redesign or global watcher-code removal: those remain CBG-REBUILD-01. Use this repository's existing watcher-off configuration during recovery/rebuild and restored daemon operation.

No custom Direct recovery, external-destination metadata rewrite, historical bulk cleanup, PID/age-based deletion, global equality relaxation, forced enrollment or new platform support. Preserve Direct path spelling, journal keys and behavior apart from the selected retirement of automatic collection. Preserve Windows volume-serial/file-index semantics and required regression/build checks; do not borrow COD-113's macOS-only exception.

## 3. Requirements and shared identity contracts

| ID | Required behavior |
| --- | --- |
| ID-R01 | Separate live native identity from persistent managed identity. Keep device/inode for current handles, locks and path checks; never globally ignore device equality or serialize it as a new durable managed identity. |
| ID-R02 | A durable managed file reference binds persistent volume identity and file ID, with the owning record's creation incarnation, membership and checksum checks retained. UUID plus inode alone is not ownership authority. |
| ID-R03 | On macOS, capture ATTR_VOL_UUID through the held resource descriptor using fgetattrlist, validate the returned shape/nonzero UUID and revalidate the pinned native resource. Unavailable/ambiguous identity is explicit and fail-closed, not a transient-device fallback. |
| ID-R04 | Version surviving managed safety records and Unix containment proofs. New Unix proofs include durable FIFO identity and trusted kernel boot-session identity. Different boot identity establishes only that an old kernel process instance cannot be live; it does not establish filesystem ownership or permission to signal a PID. |
| ID-R05 | Normal enrolled-store reads/builds use primary identity admission and current native leases, not retired collector bindings. Known retirement, unresolved publication or containment cannot be silently overridden. Native handles retain protection for their complete lifetime. |
| ID-R06 | New identity formats are explicit. Legacy metadata is interpreted only under its original rules or the confirmed recovery context. Valid legacy read compatibility does not authorize silently writing new identity metadata: legacy managed writes requiring the new scheme return recovery-required. A genuine fresh-store creator may initialize the new scheme directly; emptiness or a missing active pointer alone is not creator authority. Malformed, unsupported or incomplete required safety state blocks the affected operation. |
| ID-R07 | Direct targets receive no new recovery/enrollment path. Their spelling, locks, transactional journals and API behavior retain compatibility; common supervision changes require Direct regression coverage. |

### Types and metadata

Keep a live `NativeFileIdentity` boundary for current OS resource comparison. Add a distinct `DurableFileIdentity` for surviving managed persistent safety records: a tagged volume identity (macOS UUID; existing Windows volume serial) and file ID. Keep creation-incarnation/nonces in the ownership records that already require them. Legacy fixed-word identities remain a distinct read-only decoding representation, not implicitly comparable to live identity.

Do not mass-convert the retired collector's binary/JSON formats. Old collector records remain historical and inert after enrollment. New primary records, helper proofs and their versioned codecs use durable identity. Retain only the legacy readers required to validate current primary safety or recovery; no legacy mutation engine may be reachable as an automatic or recovery side effect.

The committed primary descriptor is `storage_root/.identity-state.json`, private schema **IdentityState/1**. It binds the managed root's persistent volume/file identity, existing coordination-file creation incarnation, a new enrollment ID, accepted preview fingerprint, source/config identity-bearing fields, current admission/helper protocol requirements and the fact that automatic collection is retired. The legacy assertion is explicitly recorded as operator enrollment, not a fabricated historical UUID proof. Mutable nonidentity configuration is not permanently frozen by this descriptor.

Use new typed metadata rather than adding UUID bytes to old fixed-width identity words. Preserve existing data/manifest formats. Unsupported private metadata versions produce an actionable upgrade/recovery error, not a legacy fallback. Windows and existing non-macOS build/test behavior must remain compatible; no new unsupported-platform guarantee is made.

## 4. Retire automatic collection safely

| ID | Required behavior |
| --- | --- |
| ID-R08 | The fixed runtime starts no background/idle collector, collection timer, completion/Drop-triggered scan or health-triggered collection. There is no re-enable switch for the retired feature. |
| ID-R09 | Preserve actual writer exclusion, reader/native-use leases, ordinary publication recovery, cancellation/containment and synchronous cleanup of positively owned operation outputs. Do not remove a safety check merely because it previously lived in a maintenance module. |
| ID-R10 | Historical runs/generations/cache and obsolete cleanup records remain untouched unless ordinary explicit lifecycle work has complete current ownership authority. Uncertain historical state is retained/reported; this fix adds no broad historical deletion or legacy collection migration. |
| ID-R11 | Enrolled managed stores are fenced against older runtimes/helpers. Under actual exclusive ownership, upgrade the existing artifact-use coordination header to CGCOOR03, preserving the same lock inode and creation incarnation. New primary admission recognizes the version; older code must refuse managed entry even if native device values later equal old values again. |
| ID-R12 | Do not delete/reset old gate files or rewrite their recorded device values. Enrolled-store primary admission replaces their mandatory role. Read-only legacy inspection may honor verified retirement denials; unknown state cannot authorize reopening a retired object or deleting history. |

The producer/read-lease layer must remain independent of collection scheduling. Detach MaintenanceRuntime construction, callbacks and effect dispatch from API/coordinator/health/refresh completion; keep the one canonical Graph Store native-use owner. Remove the generation/workspace collector gate from enrolled primary admission only after the new descriptor/fence is valid. No blanket catch-and-ignore of identity errors is permitted.

The existing artifact-use header is a fixed creation-incarnation record. The protocol upgrade preserves its nonce and file identity; it is not a fictitious new creation. Update it through the held exclusive file handle, not by replacing a lock inode. Keep its exact before-image and planned after-image in the recovery transaction. Validate unsupported-version behavior against the real older executable, including native-open, staged-bootstrap and collection entrypaths. If the fence is not demonstrably closed, activation remains blocked.

The previous `check-health --confirm-legacy-quiescence` administration cannot continue to trigger collection. Retain source compatibility for ordinary false/default library health requests; reject true/CLI administrative collection requests with an explicit retired-operation message and recovery guidance. Remove the administration property from the MCP schema and explicitly reject retired tool inputs, rather than silently accepting them. MCP graph tools remain non-administrative reads. Do not treat the old confirmation as consent to identity enrollment.

## 5. Explicit recovery operation

| ID | Required behavior |
| --- | --- |
| ID-R13 | Register one typed IdentityRecovery operation with CLI/library surfaces only. It has Preview, Apply(expected fingerprint), and Resume(operation ID) variants. No MCP write tool or read/startup-triggered repair is added. |
| ID-R14 | Recovery-specific runtime preparation resolves only configuration, managed root and current filesystem/control facts. It must work while normal graph startup rejects the legacy binding; it does not construct/open the normal graph runtime as an admission prerequisite. |
| ID-R15 | Preview is noncreating and read-only, including lock probes. Report exact root/config binding, current durable identity, observed legacy differences, required-record checks, planned metadata changes, blockers and a deterministic fingerprint. No control capabilities or private nonces are exposed. |
| ID-R16 | Apply requires original-store/filesystem attestation and quiescence confirmation plus actual verified exclusive ownership, checksum, inode, incarnation, membership, confinement and containment checks. Recompute/compare the fingerprint under authority immediately before effects. Confirmation supplies only the missing legacy filesystem assertion; every other required mismatch or unknown remains a blocker. |
| ID-R17 | Preserve before-images and use a durable, versioned recovery transaction. Commit new identity separately from graph publication. Do not change the root, database/search/manifest contents or active pointer during identity recovery. |
| ID-R18 | Interrupted recovery blocks ordinary writes and exposes recovery-incomplete until explicit resume. Resume verifies the same durable root/operation and current authority, finishes idempotently without duplicate enrollment, and never guesses or forces ambiguous metadata. |
| ID-R19 | Rollback is limited to pre-commit metadata restoration under complete authority and known before/after state. After commit, do not silently restore old protocols or data; report committed recovery independently of later build/graph failure. |
| ID-R20 | Recovery never builds. Observe graph readiness separately: serve an existing snapshot only after ordinary validation and retirement checks succeed; otherwise report an explicit rebuild requirement. Matching completed apply/resume is a no-op with actual current status, not another revision increment. |

### Public invocation and outcome

CLI:

```text
codebase-graph recover-identity --config <path> --preview --json
codebase-graph recover-identity --config <path> --apply <fingerprint> --confirm-original-store --confirm-quiescence --json
codebase-graph recover-identity --config <path> --resume <operation-id> --confirm-quiescence --json
```

Preview is the default when no effect mode is selected. Modes are mutually exclusive. Reject Direct selectors and mismatched source/config/store identity before effects. The library request uses the same canonical selector, modes, confirmations and output meaning; adapters translate rather than implement separate rules. Normal health/search/context/query remain free of recovery effects.

The outcome contains operation ID, root identity, `metadata_commit` (`not_committed`, `committed`, `unknown`), `recovery_state` (`preview`, `blocked`, `incomplete`, `enrolled`, `already_enrolled`), graph readiness, applicable active generation, blockers and bounded primary/secondary errors. It never calls a successful metadata transition a successful graph build. CLI effects return success only for an established completed enrollment/no-op; blocked, incomplete and uncertain outcomes are non-success. Cancellation waits for required safety steps and reports actual transaction certainty.

The preview fingerprint binds the exact proposed effect set, identity-bearing configuration, observed current root/volume, required control-file membership/identity/content, coordination incarnation and relevant publication/containment facts. The effect list distinguishes required primary safety from inert historical collector state. A mismatch outside the explicitly attested legacy device/volume equivalence is not automatically repaired. Unknown required execution state remains blocked; unrelated historical data is preserved without asserting ownership.

### Transaction and fence ordering

Use `storage_root/.identity-recovery/` for one current IdentityRecoveryJournal/1 transaction and private before-images. Reserve recovery/install scope, acquire the actual target writer and current native resource-use/worker/phase exclusions in the frozen authority order, verify quiescence/containment, then revalidate the preview. No existing lock is replaced or recreated to manufacture exclusion.

Order effects: durable intent and before-images → CGCOOR03 fence through the held file → durable new primary identity/proof records → atomic IdentityState/1 commit. Journal each authorized effect and its synchronization/readback; torn or substituted files require provable recovery from the exact intent, otherwise preserve and block. No host-reboot survival claim rests on a stored transient device number.

Before commit, either verified rollback or explicit resume restores a coherent state. After commit, the descriptor and matching receipt establish the enrolled protocol; before-image retention is evidence, not permission to downgrade. Keep minimal current transaction/receipt state and preserve the old collector artifacts. No age-based removal, PID signalling or automatic drain of cancelled collection work is introduced.

New Unix helper proofs bind a durable FIFO resource and boot session while retaining authenticated control and FIFO-writer/group-anchor checks. For a legacy proof without a persistent UUID, only the confirmed recovery context may validate the exact current resource equivalence and existing emptiness proof; do not infer emptiness from PID or timestamps. For new proofs, boot change never supplies deletion ownership by itself. Same-boot unknown or surviving containment preserves evidence and blocks conflicting writes.

## 6. Compatibility, ownership and execution

| ID | Required behavior |
| --- | --- |
| ID-R21 | Use one complete technical owner for coupled types/codecs/admission/recovery/helper changes. Coordinator serializes contracts, tracking and Git. Preserve COD-113 WIP and other work; verify source-writer quiescence and exact file reassignment before code. |
| ID-R22 | Bind exact spec/model approval before dependent implementation; keep live apply gated on the concrete preview fingerprint. Plan approval is neither final-byte approval nor a claim that future checks passed. |
| ID-R23 | Preserve all other stores/data and Direct contracts. Do not silently upgrade their identity metadata. An unsupported older runtime must not be used against this enrolled store; activate matching verified binaries/helpers deliberately. |
| ID-R24 | Before completion, independently verify final source/runtime identities, applicable native tests and older-runtime refusal, then obtain fresh Sol/xhigh adversarial final review/focused fixes. Do not inherit cancelled COD-113 or other issue-specific platform exceptions. |

Authoring is direct root ownership. After approval, a single complete implementation owner uses the repository's code_implementation settings (Terra/medium). Shared source/types/manifests/lockfiles have one owner; no nested worker dispatch. Independent verification uses Terra/medium. Final Review is a fresh default-agent, fork-none Sol/xhigh session using adversarial-review-engineer; the explicit user table overrides the referenced skill's Max default. Fix every confirmed P0–P3 and reverify affected paths.

Preparation/code radius is the existing Graph Store native/persistent identity and primary admission, artifact use, managed publication/workspaces, containment and Unix helpers; API contracts/registry/preparation/presentation; CLI/bootstrap/coordinator/refresh automatic-completion wiring; dedicated recovery modules and tests/docs. Exact writable files and matching source hashes belong in the authority execution record before dispatch, not an inferred license to edit every maintenance module. Do not absorb COD-113's uncommitted changes or D00's unrelated pending model entries.

The original native graph is unavailable. Bootstrap discovery uses this repository's retained manifest file index with per-file content hashes matching main, followed by source-level callable/persistence tracing. Record semantic graph coverage gaps; do not serve a raw database around the guard. Restore ordinary graph search/context checks once the fixed runtime can access the store.

## 7. Acceptance and planned verification

All product methods are planned until executed against the approved final source. Requirement ranges are inclusive.

| Criterion | Requirements | Observable check |
| --- | --- | --- |
| ID-A01 | ID-R01–R04 | A production-created fixture sees changed native device with unchanged durable volume/file/incarnation and reopens after enrollment; different UUID, reused inode/nonce, missing UUID and unbound legacy data fail safely. Use a test-only provider seam, not forged live ownership records; verify the real descriptor API on macOS. |
| ID-A02 | ID-R04/R05/R09 | Native process/guardian loss, FIFO EOF, boot changes and reused PIDs preserve existing containment guarantees. Boot identity never grants filesystem deletion authority; unknown required proof blocks writes. Include cancellation, Windows job behavior and actual supported native helpers. |
| ID-A03 | ID-R08–R12 | Startup/idle/health/completion/Drop and source changes schedule zero automatic collection. Historical/foreign sentinels stay unchanged. Existing writer/read leases, explicit publication recovery and positively owned operation cleanup remain effective. |
| ID-A04 | ID-R13–R16 | CLI/library preview succeeds without normal graph initialization, creates no files/metadata, and agrees across surfaces. Wrong fingerprint/root/attestation/quiescence/ownership/containment refuses before effects. MCP has no recovery write tool. |
| ID-A05 | ID-R11/R17–R19 | Fault/cancel every intent/fence/write/flush/commit boundary; repeat resume/rollback as permitted. Preserve lock inodes, creation incarnation, before-images and graph bytes. Corrupt/tampered/unknown state never forces a reset. |
| ID-A06 | ID-R11/R23 | The frozen actual old executable cannot read/write/collect through managed and guarded native entrypaths after enrollment, including a fixture where native device values equal old values. No staged-bootstrap fallback admits the old protocol; new helpers match protocol before database work. |
| ID-A07 | ID-R17/R19/R20 | Metadata commit certainty and graph readiness are separate. Build failure after recovery leaves enrollment committed and data intact; recovery does not silently rebuild, re-publish, downgrade or claim graph success. Completed calls are idempotent and report observed state. |
| ID-A08 | ID-R05–R07/R10/R23 | Ordinary reads/full builds, cache reuse, publication failure and Direct journal/path/alias cases retain behavior. No unrelated Direct metadata rewrite; historical unknown state is preserved, not bulk-cleaned or reactivated as retired data. |
| ID-A09 | ID-R21–R24 | Same final source/artifacts pass applicable current stable macOS ARM64 and Windows11 x86_64 checks, required build/lint/tests, independent verification and fresh review. Preserve required other build checks without expanding support claims. Retain genuine reports/JUnit and source/runtime closure outside cleanup roots. |
| ID-A10 | ID-R15–R20/R22–R24 | After exact preview approval, enroll this root, run the explicit full graph build, then pass health/search/context/read-only query and restored daemon checks with watcher off. Root/endpoint remain unchanged; other stores/services are not enrolled or restarted. |

Native macOS qualification includes a disposable APFS image/remount fixture to exercise real persistent UUID/device behavior where available; no host reboot or current-store mutation is a test prerequisite. If remount does not change the device value, report that observation rather than calling it renumbering coverage. A deterministic provider seam covers device/boot transitions and collision/unknown cases. Freeze old executable/runtime dependencies for the mixed-version negative tests before changing the installation.

## 8. Delivery and live activation

First publish this concrete specification/unsigned architecture proposal and obtain exact-revision approval. Then implement and verify in the isolated main-based bug-fix branch. Source/runtime qualification, confirmed-defect resolution and the authorized PR to main precede current-store activation. Merge/public release and unrelated installations remain separate authority; a reviewed local fixed runtime must have exact source/hash/helper provenance.

Before live apply, present its actual read-only preview/fingerprint and obtain the operator's store-specific enrollment approval. Verify no required writers/helpers remain, preserve source/data before-images and freeze runtime dependencies. Apply through the public recovery operation, never manual byte edits. Then run the requested normal full build against the same managed root; validate health, search, context and a read-only graph query before declaring a fresh graph published.

Use this repository's existing `refresh.policy=off` setting for restored daemon operation, retaining source/storage identity and the current endpoint. This operational setting follows the user's explicit-rebuild direction; global watcher-code removal remains CBG-REBUILD-01. Preserve other configuration fields and fail rather than overwrite concurrent edits. A metadata-recovered/build-failed outcome remains truthful and does not undo recovery.

Before large Rust builds check free space; use a unique target under the user-configured rust-agent-builds repository/work directory, never a shared redirected target or competing cleaner. Preserve frozen executables and dependencies outside cleanup roots and record exact paths. Record reviewed/tested revision/environment, genuine results/limitations, PR/commit/approval references and original-contract amendments. Assess reusable knowledge without filler. Maintainer retains closure; no passing product evidence, live enrollment or approval is asserted by this proposal.
