# COD-107 — approved internal native-artifact retention (2026-10-31)

## Identity, decision and approval boundary

**COD-103 revision0.5**, **COD-107 / C103-R03 → C103-A03**: internal artifact-access method only. The actual user **“i approve your recommendation”** selects [retention for COD-107 only](https://linear.app/codebasegraph/issue/COD-107#comment-5b4b5aad-cbe8-4265-8db9-5ea351b7f386), answering the [specific recommendation](https://linear.app/codebasegraph/issue/COD-107#comment-3725f3b0-cb71-4298-a9b8-6b74c5788d96). It does not select external-builder substitution or acceptance deferral. No requirements choice remains open.

Original revision0.3/all companion contracts stay approved at **`316be4c4bb81d4f6e025016b12374836b4dcc1a8`**, [separate record](https://github.com/rabii-chaarani/codebaseGraph/blob/0e6008a1f46679a86aba85ac513beb05af94b0c0/docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md). COD-56 revision0.4 method stays unchanged at **`653177127041caed55b6628ff3a596a013ef0cd4`**, [separate linkage](https://github.com/rabii-chaarani/codebaseGraph/blob/9ed7f117bc605d5a365ab5b669d54e56182d8e07/docs/specs/COD-103/approvals/2026-10-31-cod56-emulated-qualification.md). These immutable approvals are preserved, not advanced to newer file bytes.

Production must commit the faithful reviewable revision0.5 method documents and persist a separate record of their exact paths/full frozen Git SHA, narrow scope and actual user decision **before dependent automation implementation**. This records approval already given; do not ask for another routine go-ahead. A file never embeds its own containing SHA. Freeze, approval linkage, implementation, executed qualification and closure remain distinct states.

## Problem and permitted method

C103-A03 already requires inventories of **actual generated native archives and publishable root/wiki crates**. The reported current PR path builds/smokes both native targets but retains the native archives only for push events, preventing independent retrieval of COD-107's actual same-source PR payloads. Source manifests, package-list predictions, prior-release bytes and unrelated fixture executables do not close this evidence gap.

Permit internal retention of the **already-built `macos-arm64` and `windows-x86_64` archives, checksum sidecars and source provenance** for the normal same-repository COD-107 PR at the exact qualified source. Scope the existing retention path to this issue only; no generic PR-retention feature. Other PRs, forks, descendants and unknown identities receive no retention through this exception. Preserve existing push retention and existing native target/output names, duration, build/test/smoke recipes and packaging rules. Production owns exact event/head/base predicates and placement; detailed guards and serialized ownership belong in the existing Scryer brief, not this requirements record.

This grants artifact access only, **not public release publication or PR-artifact promotion**. Repository public-source/evidence-artifact access permissions remain unchanged; no new permissions, privileged trigger, secrets, token, runner, settings, dependency or toolchain. No installer guard, runtime preference, build recipe, public contract, source-policy, Rust-only product policy or CI/security gate changes. No helper restoration or new Python CI/product/package inclusion. The approved optional local docs-only helper boundary and every packaging exclusion stay intact.

## Acceptance and failure behavior — planned, not executed

C103-A03 remains unchanged; this method supplies access to evidence, not an exemption:

- **Given** the qualified normal same-repository COD-107 PR source, **when** its existing native jobs build/smoke both targets and retention runs, **then** the actual archives, existing sidecars and provenance are available for independent same-source inventory. Retain run/attempt/event/PR/source and byte/hash identities.
- **Given** another PR, fork, descendant or unknown identity, **then** this exception does not retain artifacts for it. Existing push behavior is unaffected.
- **Given** absent/failed retention, a missing target/archive/sidecar/provenance, retrieval error or source/hash mismatch, **then** A03 acceptance/handoff remains blocked. Preserve diagnostics; do not manufacture inventory or count an unrun check as passing.
- **Given** the retrieved exact-source payloads, **when** independent verification inspects both actual native archives and generated root/wiki crates, **then** demonstrate the unchanged helper/Python exclusions and genuine existing archive/checksum/size/policy checks. No cross-compiled stand-in, manifest-only prediction, prior-release substitution or deferred acceptance.

No retention, archive inventory, native pass, policy-test result or CI success is asserted by this decision. Independent verification, fresh integrated final review/fix verification and all applicable CI workflows successful on the exact reviewed head/current base remain required before any merge.

## Impact and ownership

Only COD-103/COD-107's A03 access method changes. COD-108/COD-110 receive **no retention exception**, new prerequisite, acceptance waiver or release-method change. Original requirement/acceptance IDs, design/architecture, claims, full ordered queue/release contents, non-bypass freshness, serial own-merge-SHA full main CI, final source-bound artifact/publication verification and original aggregate closure owners stay unchanged. No new topology/claim/framework is warranted.

Engineer coordinates the single active COD-107 and existing Scryer ownership. Backend's current source/policy files remain exclusively assigned until STOP; Production serializes any later authorized automation/policy-test changes and Git versioning. This record assigns no overlapping technical files. Related documentation/evidence belongs in COD-107's same PR. Requirements writes only this decision and narrow spec/verification/delivery references; no source, Git/index, Cargo/build/test/CI/VM, wiki/tracker/model/settings writes. No reusable memory is needed beyond the canonical decision.
