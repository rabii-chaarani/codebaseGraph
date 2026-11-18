# CBG-RESTORE-01 implementation and evidence

This records the reconstructed product at **dc6395251fc1145a80db2186e72dd216121c290b**, tree **079f4a375398c4a4cf14a718388ccf543cbf7182**. It supplements the unchanged [specification](spec.md), [ledger](preservation-ledger.md) and [execution brief](execution.md). The final published documentation revision must be rebound to these product inputs and independently checked. Required native CI is recorded against the published PR head in [PR #200 Checks](https://github.com/rabii-chaarani/codebaseGraph/pull/200/checks) before handoff.

## Decisions and source

- Restoration revision 0.1: approved [d809fe0f8289bbf7d167a891bd7c31160f27f0e1](https://github.com/rabii-chaarani/codebaseGraph/blob/d809fe0f8289bbf7d167a891bd7c31160f27f0e1/docs/specs/CBG-RESTORE-01/spec.md); actual decision in [approval.md](approval.md), committed at 663b2fc8d2790956975587dcab6b248bd40b5958.
- CI-0.1: approved d091767dd26d5b25f963d20129bd0abc982fc46b; actual decision in [ci-qualification-approval.md](ci-qualification-approval.md). Only dev is added to the existing strict PR-base policy, with full native qualification.
- SEC-0.1: approved fa25f63d989bf8984d808b9fe84a4f39d2fb05dc; actual decision in [security-audit-approval.md](security-audit-approval.md). Only ammonia 4.1.8→4.1.9, its checksum and wiki lower bound change. All 222 other lock package blocks/edges and audit rules are unchanged.
- Baselines remain main 90e2ab6fae2767bdb158e19f0072d43568e80dbc, dev 2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9 and reference v3.0.4 / ca115690b226384164cf364458a0e3255b8908d3. New commits descend from main; tags/version history remain intact. Product metadata remains 4.2.1.
- [COD-136](https://linear.app/codebasegraph/issue/COD-136) owns this restoration delivery. PR target is dev. Merge, release, installation, daemon/client changes and live-store activation are outside this work.

## Product and compatibility

The added collector, automatic scheduling, enrollment, reference/retirement engines and persistent cleanup admission are removed, including their callers. This is an explicit removal on current history.

**Breaking interfaces:** cleanup-only CLI confirmation, MCP confirmation/schema inputs, health maintenance projections and collector-specific public Rust fields/types are removed. In particular, --confirm-legacy-quiescence is unsupported and confirm_legacy_quiescence is absent from the MCP contract. There are no disabled compatibility stubs. Ordinary cleanup_pending/pending-run reporting remains meaningful for publication recovery and operation-owned cleanup.

Retained behavior includes fail-closed COPY/no candidate replay, sequential chunks and jumbo records; R1 removal of workload ceilings with native pool defaults; native path validation; current macOS ARM64/Windows x64 and Windows extensions; Windows profiles/overrides/EXE/CMD/BAT discovery; macOS runtime packaging; security, dependency, installer, package and COD-103 release safeguards. All 72 internal PR153 commits have a behavior disposition in the preserved audit, rather than being applied wholesale.

Mandatory publication, journal, containment and pointer/directory-sync failures remain primary. Once publication is durably resolved, optional disposal returns committed success with pending work/diagnostics and preserves retry evidence. All three successful Managed recovery paths record Published proof before optional retention or run disposal. Retry identity survives later generations and last-reader cleanup. Actual proof-write failure remains conservatively pending even when the former candidate disappears; no outcome is guessed. Direct terminal-run disposal and benign disappearance do not hide a valid graph. Reader protection and native validation remain in their existing owners.

## Acceptance evidence

The private durable archive is the session's restore-v304-preservation directory. Raw logs, failed attempts, snapshots and executable inventories stay there. References below are relative to that archive.

| Criterion | Executed or inspected evidence |
| --- | --- |
| REST-A01 | Independent preservation reconstruction checked13,325entries,11 worktrees / four external roots,354refs,13,861objects and14 COD-25 inputs. independent-verification.json and roundtrip.json preserve the methods/limits. |
| REST-A02 | Independent Sol/xhigh review of the integrated removal and subsequent corrections, against main and v3.0.4; collector caller inventory, owned blob mapping and72-entry PR153 audit. Final source review: implementation/review-dc639-pass.json. |
| REST-A03 | Managed/Direct publication, reader/writer exclusion, failed-build/cancellation/owner-loss/crash/containment regressions run in the independent879-test workspace. Owner's final storage suite:61 passed / one subprocess helper ignored. Compound pointer/root-sync, retirement/proof-write and later-publication faults retain evidence and permit only proven recovery. |
| REST-A04 | COPY uncertainty, sequential chunking, jumbo records, PK/connectors/search/reopen and fresh-invocation recovery are included in the independent workspace; native0.19 inputs and root compilation were verified. No removed resource ceiling is restored. |
| REST-A05 | Independent isolated build/health and65-second stdio plus 65-second HTTP MCP probes: eight stable tools, no cleanup-related input terms, no maintenance health projection, retained historical marker, successful health and owned-process shutdown. Ordinary refresh/reader retirement occurred and was permitted. implementation/independent-final/a05-probe.py/json and a05-http-probe.py/json. |
| REST-A06 | Retained client, installer, packaging and release-policy regressions run locally where applicable. Supported-platform qualification requires final-head native macOS/Windows CI, including artifact jobs; older native greens are not transferred. |
| REST-A07 | Original C4 refs/contracts/activation and COD-25commit042b3698036c2b253a29d9146bdf401a61559d64 plus14copied inputs/diff were independently rechecked. Companion order/status below is preserved; no C4 implementation/provider/Q1/native-YAML acceptance is claimed here. |
| REST-A08 | Catalogs cover1,022 commits,875 outside reference ancestry,354 original refs and144 working changes with zero unclassified inventory entries. Full behavior decomposition remains recorded separately from inventory classification. |
| REST-A09 | Final independent checks matched all354 original refs,11HEADs/logicalindices and captured source entries except declared owned wiki/Scryer administration. Main's13statusrecords match capture. Config SHA c13025ca36ca1e7937ba9e19888a2d1dc5dfba2f2e181a2665a623e0fc662f1f and generation38507-1794541994361265000-1 are unchanged. |
| REST-A10 | Independent fmt and strict workspace/all-target/all-feature Clippy passed;879 workspace tests passed,0failed,2subprocess helpers skipped. Final-head native CI remains the publication gate. Actual executables/native receipts are retained before target reuse. |
| REST-A11 | Existing committed Scryer intent and approved amendments were reconciled with source/callers/tests. No foreign pending claim was folded or main fingerprint verdict fabricated. Canonical specs/approvals remain unchanged; wiki/Linear reconciliation is recorded below. |
| REST-A12 | Fresh executing verifier: default/Terra/medium/fork-none. Fresh independent prompt-only final reviewer: default/Sol/xhigh/fork-none, adversarial-review-engineer; all confirmed P0–P3 corrected and focused source review passed. Final documentation/publication rebind and required CI must also pass before handoff. |

### Independent execution

Source checkout: /private/tmp/codebasegraph-restore-v304-review-20261117-proof-final, clean at the source SHA above.

Owned build target: /private/tmp/codebasegraph-restore-v304-finalverify-20261117-5c2c12c/target, reused serially by the same verifier with explicit CARGO_TARGET_DIR after preserving the earlier execution. No unrelated owner shares it. The verifier cleaned only its own root/lbug artifacts, then actually rebuilt both root and native bridge. Fresh link receipts name its own target and the approved native019 inputs.

Toolchain: cargo/rustc1.98.1 (48a229cea), aarch64-apple-darwin; nextest0.9.140. Workspace all features; test profile retains debug assertions and debuginfo. CARGO_INCREMENTAL=0 conserves compiler-cache space without changing the product resource policy.

Commands:

```text
cargo +1.98.1 fmt --all -- --check
cargo +1.98.1 clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo +1.98.1 nextest run --locked --offline --workspace --all-features --config-file <independent-final/nextest.toml> --profile final --test-threads 4 --no-fail-fast
```

Environment: LBUG_LIBRARY_DIR and LBUG_INCLUDE_DIR point to /private/tmp/codebasegraph-identity-evidence-20261117/native019; LBUG_VERSION=0.19.0; OPENSSL_LIB_DIR=/opt/homebrew/Cellar/openssl@3/3.6.2/lib; MACOSX_DEPLOYMENT_TARGET=13.3. Verified SHA256: liblbug.a d2e9a5c37617b0affb01f18f6ef48bb1531f5744f5ba06c849226842617ad142; lbug.h 7da5b7e36d6610e43a1512e4997ac123ba0886cf5598ae44aca5449aa02a8aa3; lbug.hpp 0b463bef0fa7a9ba1bb14cee12b23bf5bc7c2d6cfc4b3779d6f7cbe92bf4831d.

Result:879 passed / 0 failed / 2 skipped in 645.374s. Actual JUnit implementation/independent-final/dc639-junit-actual.xml has SHA256 **75038901bb757b5f31e839810cafb233799709f9074d734d5ad1f3e3697683d2**. implementation/independent-final/dc639-executables.json inventories 86 preserved Mach-O executables/dylibs with original/copy hashes and distinct inodes. dc639-lbug-output.txt records fresh native linkage. dc639-verification-report.json binds commands, environment, source and limitations.

## Preservation and reconciliation details

Bundle:88,795,715bytes, SHA256440e5ccfbac9f023c5e2e2a660d356eb5e62e44cb659a212e7d187af3c7eab99. Additional C4 evidence remained in place with its independent inventory. No lost implementation was recreated from descriptions.

| Catalog | SHA256 |
| --- | --- |
| commit-disposition-catalog.json | 5f7bbce1c348bf2455faa5e601483f74cfc3ce7250c161b613a7c4ffd2cef6c3 |
| ref-disposition-catalog.json | 19dc8268bf286b69f9e746e5039d18a673d1e165d42e3be5d371376672d5c89f |
| working-change-disposition-catalog.json | eefd455075acd11f67e84d3fa11058e20408e791948a90ad766c295c17cf33ff |
| catalog-coverage.json | 55386393c75a7144609a0de9cf7ef64f74c4b6ffff225e3da77e5f49ef666519 |
| implementation/owner/pr153-commit-audit.json | 8546d07c3142de504c510174ab7fff7d66bb9ab6176f0a741bb0515676e2d8f6 |

The product patch against frozen main, restricted to approved product ownership, has SHA2565d0b160d6e6564b79a80b389126baed631be398fd1d4b272d8d708ac2bf94fa5 at dc639. Conventional removal/correction commits retain original history and explicit caller-level fixes.

Two originally captured internal snapshot refs were later absent. Their object f1ab536136cb4eb3ba1cacffc0e08f2d06da1697 is a **tree** and remained present. Root restored only those exact absent refs using an atomic create-only transaction; independent follow-up matched all 354 refs. The cause of disappearance is unknown. Original branches/newer refs were not moved.

The process-census preparation sentence is qualified by implementation/quiescence-scope-correction.json: an early post-filtered process output did not prove global absence. Preservation relies on owner quiescence, exact file/index/ref/HEAD stability and independent replay. Unrelated running services/builds were not stopped.

The [wiki reconciliation brief](wiki-reconciliation-brief.md) assigns only a historical retirement-status note. Configured k_wiki MCP preserved the original 89,090-byte page as an exact prefix; final main-page SHA299e44d80f87f2542248bcfd69215d7dfc505c561f8d7eca7aa2b54226f4a3a8. Only the1742-byte MCP-authored tail (SHA0cb4ab5cc830c35cc9e95c2a135f2c5b273e1b09bcf2fba05ef336db16c9dcf7) is ported here; other owners' uncommitted wiki content stays separate. Registered-bundle recommended validation and links returned zero diagnostics, and the configured repository .kwiki/site build produced 305 routes / two assets. The registration rejects isolated worktree bundle roots, so no isolated MCP success is claimed. The initially misdirected 307-file static output was byte-preservingly archived outside the source tree; canonical generated state was not manually edited.

Linear impact notes link this work on COD-113/125/126/135 while preserving their Canceled/Backlog/In Review/In Progress states and closure owners. Scryer chg-3a1ewe has no invented responsibility; existing committed Graph Store resp-108/109/232/233/234 and Materialization resp-238/71/72/231/270, coordinator and public-contract intent remain the reference, with approved R1 taking precedence over stale memory-budget wording. Other issues' planned entries remain intact. The isolated checkout has no model binding; branch JUnit was never assigned to main's source fingerprint, and no global foreign drift reconciliation was performed.

Low-space actions were limited to two root-owned never-executed target clones and 22 explicitly consented completed root-package incremental-cache directories. Actual owner/verifier binaries, runtime dependencies, native SDKs, shared targets and Cargo home were preserved; cargo-reclaim/service policy was unchanged. All actual build target paths are recorded above/in owner handoffs.

## Retained failures and boundaries

- The earlier 5c2 default-concurrency workspace had865 passes / one 25-second refresh-startup timeout/twohelpers skipped. The identical binary passed that case alone in 7.26s. Current four-thread execution passed it in 11.379s; this does not establish the earlier cause or weaken its assertion.
- The first dc639-junit.xml copy was stale5c2 data. It is retained as a failed evidence attempt; only dc639-junit-actual.xml/hash above is proof. The interrupted overbroad 5c2-binaries clone is likewise not the authoritative executable inventory.
- Earlier pointer/proof-loss reproductions, sandbox failures, initial FIFO capture refusal and ref-check correction retain their original failed meanings. No old green is rebound without input verification.
- The earlier 5c2 hosted run passed all four Windows integration shards and macOS native tests, but a Windows unit job failed before tests on a StrawberryPerl HTTP/2 download for vcpkg OpenSSL. It does not qualify the corrected final head. Final CI/audit must pass unchanged; existing audit warning handling/time-parsing exception is not broadened.
- Source graph MCP is unavailable; live CLI reports generation_gate_binding_changed. Executed frozen-main dependency/change-impact/callgraph receipts and hash-bound source/caller coverage provide bounded orientation; empty no-FTS search is not complete discovery.
- Seventy-five worktrees were already absent/prunable. Unrecorded former working bytes and data outside the declared capture cannot be claimed recovered.
- Genuine unconfirmed publication-proof failures remain pending. No new collector, admission quota, timeout workaround or live-store recovery is introduced.

## Separate companion handoff

After the verified restoration PR handoff, a fresh coordinator takes one ready issue at a time: preserve COD-16–19 contracts/decisions/verified activation; port COD-20 types, COD-21 compiler, COD-22 identities and COD-23 evidence in prerequisite order; recover COD-25 committed/uncommitted work as WIP; retain COD-24's sequencing afterward without implementing its unfinished scope.

Original branches, PRs and approval SHAs remain immutable. New companion branches record old-to-new mappings and recheck callers/contracts against the reconstructed source. Neutral extraction/lifecycle controls remain; incidental edits to deleted collector files do not carry forward. Porting does not qualify COD-25/provider/Q1 or activate native YAML. Other unmerged documentation/proposals and identity/COD-113 WIP retain their original owning snapshots/status.

Existing memory on native path identity, containment proof and frozen-binary provenance covers the reusable constraints applied here; no filler memory record is added. Aggregate issue/milestone closure remains with its owner.
