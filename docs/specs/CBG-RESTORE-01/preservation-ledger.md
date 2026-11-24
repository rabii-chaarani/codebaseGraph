# CBG-RESTORE-01 preservation and change-disposition ledger

Revision 0.1; [COD-136](https://linear.app/codebasegraph/issue/COD-136). This is a decision ledger for the [specification](spec.md), not permission to apply future source changes. Original refs/approvals remain immutable.

## Coverage and disposition rules

The durable checkpoint protects all 354 pre-existing refs, all 1,022 recorded commit objects and every declared current working/evidence entry. It retains three stash refs/commits through the original Git database and explicit preservation refs. Raw object/ref catalogs, 11 worktree snapshots, four external snapshots and independent roundtrip records remain outside the PR.

The external commit catalog covers every locally present commit outside the v3.0.4 ancestry, including unmerged and reflog-only commits; the ref catalog covers every original ref. Existing unmerged work remains unmerged unless its specific companion port is assigned. These catalogs are source inventories, not mass implementation approval.

Every commit has one destination: retained delivered behavior, mixed behavior requiring decomposition, separate native C4 companion, preserved related WIP/evidence, preserved unmerged proposal or historical metadata. Every current working change is assigned to its owner below. No default classification authorizes dropping source.

Mixed production files are resolved against the referenced observable behavior and original acceptance criteria, not by filename or a whole-commit cherry-pick. Before retaining an edit, its source/behavior mapping and affected callers must be recorded in the execution ledger. A genuine unresolved behavior or missing required source stops that edit.

## Main-line changes after v3.0.4

| Original main commit | Disposition and contract |
| --- | --- |
| 75a99c840aa53e75a3f14df8dab41979c9efa19c | Split PR153: remove the collector-specific implementation; retain the independent subsets below; keep original contracts/evidence at their existing paths. |
| 7038e66ff8271ffddc511f879cf8d28353a09b2d | Preserve release 4.0.0 metadata/history; no restoration tag rewrite. |
| 34eac4f8644adc046093babd84ee1fdbffdae853 | Retain Python-evidence archival/package-policy fix and its original locator/evidence. |
| 9557b3806ce46cb1acadd95cfbf712c07ea8556d | Preserve release 4.0.1 history. |
| 62dcb10305af07e24be7b1a960407f47135bf93c | Split COD-52/98: retain applicable native directory-scan correctness and COPY process-instance test repairs; collector-only receipt/selection edits retire with their caller. Do not weaken surviving oracles. |
| 9f69a0bd4b468eaa8652d933d518fafb8c596b6a | Preserve release 4.0.2 history. |
| c4e9767d8f3ec92c40dd2f809b8421b8f9c78742 | Retain Windows default bundled bytes, explicit asset checks, patched pulldown-cmark and evidence locator. |
| 5889ed09954a4a07efd2590dd1bd380ee77be06a | Preserve release 4.0.3 history. |
| a07e6d1e322622b7cee5b5358816cb3d54c17647 | Retain verified-assets-before-immutable-publication ordering and negative/recovery tests. |
| 7125900c740297be5c60821dbeb9811f32a5894a | Preserve release 4.0.4 history. |
| c60a00c1d747b73789cd6edc7ee20601182a35ca | Retain relocatable macOS OpenSSL supply/audit/install and the approved bounded START acknowledgement repair. Preserve loader/containment callers when reconstructing shared files. |
| 6a98fb0ef4a6bbde02f1556683401eeded49574e | Preserve release 4.0.5 history. |
| 01792cb097f6ca23f679396871ef273006732120 | Keep COD-111 specification/proposal and approval history; no unimplemented worktree feature is activated. |
| 053ef0e6b85259056a5766e637a0edd94a682b4e | Retain COD-103 smoke, CI mode/freshness, docs-helper, installer and release-preflight safeguards; omit only collector-specific code whose caller is removed. Preserve exact scope/qualification limits. |
| 184c679becda618bd34ad1e085ab574c0c2fa1eb | Preserve release 4.1.0 history. |
| c04c8a53d33a7f8107e965e503fc4f700880bde3 | Retain deterministic verified dry-run crate normalization. |
| ff9062cdfaaa8ae8c0665882a431f33cee422dca | Preserve release 4.2.0 history. |
| 2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9 | Retain Windows client profile/override/executable/shim discovery, same-target verification and process fixtures. |
| 53c8555f78102846fa4e3f4486543d52e04574c4 | Retain current 4.2.1 version metadata and release history until normal release automation chooses a later version. |
| c678450ba3cf04a76def250e11aafa869d3319f6 | Retain merged cc lock update. |
| 90e2ab6fae2767bdb158e19f0072d43568e80dbc | Retain merged download-artifact action update. |

## Independent subsets inside the original cleanup branch

The original origin/codex/automatic-graph-disk-cleanup ref and all 72 internal post-base commits are preserved. Do not merge that branch into the reconstruction.

| Source | Active product treatment |
| --- | --- |
| 8e47d2b3c74b72b56a0b6b212d1bf939c8088ca7 | Retain fail-closed schema/COPY/hash/checkpoint handling, no candidate mutation replay, sequential JSON/CSV output, bounded diagnostics and Managed/Direct failure/reopen tests. |
| 2243e590493d917079c12f719624bc89477842a4 | Retain R1 removal of workload ceilings/options/RSS termination and native default-pool policy; keep streaming/chunk tuning and singleton jumbo-record support. Its collector-file edits retire with the collector. |
| 55819ba7ff03f26c51f243435c41c9fceb60908c | Retain selected macOS ARM64/Windows x64 support and matching release/assets/installer policy; omit collector-specific platform implementations. No Linux/Intel support claim is restored. |
| 993ce0b59c0518626732cbbb49edc74f35d243b5 | Preserve recompressed extension bytes and original raw-byte identity/verification; no dependency/ABI adoption. |
| 862a8f88d485d5bd257ebd9456c194156fd4ee94 / d6dd34493e9da6f80847240bafcc92dbcaed2c38 (full SHAs in external catalog) | Retain applicable cfg/help/platform fixture repairs, including truthful parallel-default help; omit tests/interfaces that exclusively assert the removed collector. |
| Other cleanup-branch production commits | Collector inventory, lifetime retirement authority, enrollment, reference epochs, persistent gates, cohort recovery and automatic scheduling are related retired behavior; preserve source/evidence in the checkpoint. Existing noncollector ownership/containment requirements still govern retained code. |
| Documentation, approvals and test evidence | Keep canonical identities, revisions, original failed/passing meanings and paths. New retirement status links this work; it does not rewrite old approvals or execution claims. |

## Native C4 and unmerged work

| Existing source / records | Destination and status |
| --- | --- |
| COD-16/17/18 contracts and COD-19 ba5c7c543bd711b663e1dfc92d4bd58497ffb557 activation | Preserve original Git/Linear resolution, local ignored policy and approved curated wiki wording; no global authority rollback. |
| COD-20 f000bc1c90c788593ad0e1825ea6d03f7f439900 / PR188 | Separate companion feature branch; same typed C4/ontology contract. |
| COD-21 e9bcaf0cb21e4865ffe54387833954940b7c7d58 / PR189 | Separate compiler companion after types; retain the approved Rust 1.98/1.98.1 compatibility decision. Exclude incidental lint edits to retired collector modules. |
| COD-22 d3f1dd5b9147543b84460a832381df67387588b9 / PR190 | Separate identity/artifact companion after compiler; preserve linked-content and hashing semantics, qualification/retention history. |
| COD-23 7c81720f3cb32c392e34833033b0ccec6766e2ca / PR192 | Separate evidence companion after identities; retain neutral guarded extraction/control changes without importing C4 into lower layers. |
| COD-25 042b3698036c2b253a29d9146bdf401a61559d64 | Local code protected by the bundle and original branch; recovered WIP companion after COD-23, not a completed producer/provider/Q1 feature. |
| COD-25 verified-metadata-helper-run-02 | Preserve all 14 hash-bound copied inputs and full tracked diff 15cc725e8b963a21fdc01008ddda38b28bec4dda45e44e13fa02cad8ab028e8f, plus earlier snapshots. Preserve actual failures/warnings and source/runtime receipts separately. |
| COD-24/25 sequencing fcc293b4f8f09854e1cae905ebb3016d68c5d27c | Preserve COD-25-before-COD-24 and remaining blockers/closure owners; no unfinished COD-24 implementation. |
| Wiki navigation PR150 / ff007bbd68ea7c340c8b904679d16b4971c074b7 | Preserve on its own unmerged feature line; port independently when assigned, without the old cleanup ancestry. |
| COD-111, COD-125/126, dependency proposals and other original unmerged refs | Preserve original refs/status and every unique commit in the bundle/catalog. Do not activate proposals or fold them into the restored product merely because they exist. |

## Working changes and evidence

| Captured owner | Disposition |
| --- | --- |
| Main: 13 tracked/untracked knowledge/memory entries | Preserve original bytes. Native authority/compiler/evidence material stays with C4; cleanup/status text receives only an owned historical-retirement clarification after actual implementation. Curated changes use k-wiki MCP, not manual edits. |
| Identity worktree: 46 status entries | Preserve all working source, helper identities and contract supplements. This is unfinished COD-135 work; do not apply it wholesale, overwrite it or claim recovery completed. Unrelated hardening may be carried only with its explicit behavior mapping and owning constraints. |
| COD-113 worktree: 18 status entries | Preserve cancelled collector-repair source and original approvals/evidence. Cancellation is not a verified fix or authority to erase its work. |
| Native-CI worktree: 67 untracked diagnostic entries | Preserve as historical evidence outside the product PR; retain actual revision and failed/passing status. |
| Other existing worktrees | Preserve HEAD, logical index, source and metadata exactly, including clean historical worktrees. |
| Existing COD-25/24/135 evidence and RTK roots | Additional durable copies and independent replay verified; originals, frozen binaries and runtime dependencies remain in place. FIFO identities are not transplanted into a live protocol. |
| Missing/prunable historical worktrees | 75 were already absent. Commit objects survive; no claim is made for unrecorded working bytes. Missing identified required source stops its port until a hash-bound copy or owner decision exists. |

Current status/index coverage is exact at capture time; a resumed writer requires a new checkpoint before its source is touched. Source archives are private local artifacts, not PR attachments or product files.
