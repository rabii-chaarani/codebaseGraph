# COD-20 implementation and verification evidence

## Identity and scope

Work CBG-C4-01/F02/S01 / [COD-20](https://linear.app/codebasegraph/issue/COD-20). [Execution brief revision 1](cod20-execution.md), committed before dependent code at 12c4917. Specification 0.3 approved at full SHA 66dff2564e76ad06cdf172332e2da318d150fc73; actual approval and scoped Git/bootstrap activation retained at ba5c7c543bd711b663e1dfc92d4bd58497ffb557. This session's user selected schema/types only and requested implementation of the proposed plan.

Initial implementation snapshot: e7684366296362599a32c558779e5bdb3b8df946. Corrected product snapshot: d0e75d0b7d46a78ee09cea639840e0ee1a4f592b (path primitive and regression fixtures only). Source baseline for this diff: ba5c7c543bd711b663e1dfc92d4bd58497ffb557; selected pilot source baseline remains 90e2ab6fae2767bdb158e19f0072d43568e80dbc. The prerequisite history inherits dev's 4.2.0 package versions and cc/find-msvc-tools differences from the pilot baseline; source/assets are identical before COD-20. No manifests or lockfiles changed in this issue. PR #187 remains an unmerged prerequisite targeting dev; no merge authority is inferred.

Environment: Darwin arm64, rustc 1.96.0 (ac68faa20 2026-05-25), cargo 1.96.0 (30a34c682 2026-05-25). Worktree /private/tmp/codebasegraph-cod20-20261107, normal target /private/tmp/codebasegraph-cod20-20261107/target. Initial available storage 68 GiB; no cleaner launched. Four intentional live wiki edits in the original checkout are untouched.

## Behavior and traceability

| Local criterion | Canonical trace | Implementation / verification boundary |
| --- | --- | --- |
| C20-A01 | R01/A01 foundation, C16-02 | Separate additive architecture ontology asset; nine node types/five relationships with common model/snapshot/state/schema metadata, declared legal endpoint pairs, no fabricated source location. Legacy source schema remains unchanged. |
| C20-A02 | R01/R02/A01–A03 foundation | Public architecture types distinguish authored hierarchy, pinned source/artifact references, source provenance and five explicit state labels; row identities include model/snapshot/logical ID. No acceptance or persistence transition is implemented. |
| C20-A03 | R01/R02 | Assembled architecture document validation returns stable code/subject diagnostics for duplicate entity/responsibility IDs, invalid endpoints/parents and contradictory parentage; same-level cross-system relationships are valid. |
| C20-A04 | C16-01, authored C16-03 | Strict IDs, schema version, digests, full Git object ID shapes, relative paths, nonempty text and byte-span ordering; reference round trips validate structure, not source bytes, Git object existence or evidence truth. |
| C20-A05 | Compatibility | New src/architecture module and export only; separate asset and integration fixtures; no installer/index/catalog/source graph behavior changes. |

Implementation anchors: src/architecture/mod.rs, assets/architecture_schema.json, src/lib.rs module export, tests/architecture_model.rs. New module uses serde/serde_json already present and standard collections; no adapter, storage, inference or k-wiki dependency.

The validator covers explicitly assembled architecture documents. Constraint/scenario ID closure, strict bounded YAML parsing/includes/duplicate keys, hashing, mapping qualification, state transition isolation, trust, native acceptance, hosted integration, pilot sufficiency, platform and release qualification retain downstream owners. Full canonical A01–A03 are not claimed satisfied by this foundation. The logical extension asset is not installed into runtime graph generations.

## Owner-executed checks

Single technical owner default/gpt-6.1-terra/medium, fresh prompt, exact assigned files, no nested delegation or Git mutation. Reported on quiescent snapshot subsequently committed as e768436:

- cargo test --test architecture_model: 9 passed.
- cargo test --lib schema: 15 passed, 849 filtered.
- cargo fmt --all -- --check: PASS.
- cargo clippy --lib --test architecture_model -- -D warnings: PASS.
- git diff --check: PASS.

Independent rerun and review results follow; owner statements alone do not close independent gates.

## Independent verification and final review

Independent verifier /root/cod20_verify, fresh default/gpt-6.1-terra/medium, PASS on e7684366296362599a32c558779e5bdb3b8df946. Executed cargo test --locked --test architecture_model (9/9), cargo test --locked --lib schema (15/15, 849 filtered), cargo fmt --all -- --check and cargo clippy --locked --test architecture_model -- -D warnings. Retained [architecture log](cod20-evidence/architecture_model.log), [schema log](cod20-evidence/schema.log), [Clippy log](cod20-evidence/clippy.log), [format log](cod20-evidence/fmt.log) and [environment](cod20-evidence/environment.txt) retained; redundant trailing blank lines were removed from nonempty retained logs for Git whitespace hygiene, with original output preserved in the temporary verification directories. Formatting produced empty stdout and exited successfully. Environment Darwin arm64 25.6.0, Rust/Cargo 1.96.0. No confirmed defects from these checks.

Verifier confirmed legacy schema SHA-256 b85a8d549c24c7193cf03d66c1e687148ec3602286bc9172e2e60c7f60ac3124 matches both baselines. Changed source/assets/tests are confined to four assigned files; src/lib.rs changes only one module export. Graph context attempts returned busy twice and the graph indexes the original checkout, so graph indexing is not evidence for these new files; source/diff and executed tests supply implementation evidence.

Initial final review identified confirmed P2 C20-F01 at e768436: RelativePath rejected legal Git filename docs/design:notes.md because it banned every colon, while admitting impossible NUL-containing path src/bad\0name.rs. Independent temporary Git add/ls-files established the legal case; a Rust consumer reproduced constructor and ArtifactRef behavior. The first probe compilation paired incompatible cached serde_core builds and failed; corrected matching rlibs compiled and reproduced the defect. This failed setup is not passing evidence. Original technical owner corrected the primitive at d0e75d0b7d46a78ee09cea639840e0ee1a4f592b: ordinary component colons are allowed, NUL and drive-prefixed syntax reject, and constructor/ArtifactRef/SourceRef regressions exercise both cases. Owner reran 9 focused tests, formatting, strict Clippy and diff checks successfully. Focused independent verifier /root/cod20_verify (same fresh default/gpt-6.1-terra/medium session, followup) PASS at d0e75d0b7d46a78ee09cea639840e0ee1a4f592b: cargo test --locked --test architecture_model (9/9), cargo fmt --all -- --check and cargo clippy --locked --test architecture_model -- -D warnings. [Final focused test log](cod20-evidence/final/architecture_model.log), [Clippy](cod20-evidence/final/clippy.log), [format](cod20-evidence/final/fmt.log) and [same environment record](cod20-evidence/environment.txt) retained. Prior 15 schema tests remain applicable because the correction changes neither schema nor legacy consumers. No confirmed defect from focused verification. Final verifier reported the same Darwin arm64/Rust-Cargo environment; the linked environment record is retained from its initial run, not a nonexistent final-run log.

Final reviewer /root/cod20_final_review, fresh default/gpt-6.1-sol/xhigh, applied adversarial-review-engineer plus engineering lenses. Initial review of e768436 found C20-F01 only; all remaining scoped paths had no confirmed P0–P3 defects or unresolved conditional risks. Focused correction review of d0e75d0b7d46a78ee09cea639840e0ee1a4f592b confirmed the original defect resolved and no remaining confirmed product defects. Reviewer independently executed 13 path cases through constructor, ArtifactRef, SourceRef and ModelRoot (52 checks), PASS; cases include colon/NUL, Unicode with colon, drive syntax, URL, absolute/backslash/empty/dot/traversal. Probe retained separately at /private/tmp/cod20-review-correction-dg3pceih; original reproduction preserved. Evidence readback found one broken link to a nonexistent final environment log; root corrected it to the applicable initial environment record before final evidence snapshot. Final evidence-only readback remains pending. The user-supplied AGENTS assignment settings select xhigh and take precedence over the skill's Max default. Both are bounded read-only sessions without conversation replay or nested dispatch.

## Reconciliation and delivery

Pending final checks/review, publication and independent readback. Aggregate closure stays with maintainers; no subsequent issue is dispatched in this session. No merge, release, runtime activation or platform qualification.

Reusable knowledge assessment: issue-specific contracts, boundaries and evidence are retained in the canonical brief and this report. No new reusable memory claim or curated wiki change is warranted for an uninstalled type foundation; no memory/wiki writes performed.
