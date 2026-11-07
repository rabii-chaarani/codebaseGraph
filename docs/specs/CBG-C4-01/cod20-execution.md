# COD-20 execution brief — revision 1

Status: authorized bounded implementation; tests/review pending. Work CBG-C4-01/F02/S01, Linear https://linear.app/codebasegraph/issue/COD-20; aggregate closure remains with maintainers.

## Authority and decisions

Canonical specification 0.3: [approved full SHA](https://github.com/rabii-chaarani/codebaseGraph/blob/66dff2564e76ad06cdf172332e2da318d150fc73/docs/specs/CBG-C4-01/spec.md). Reuse [actual Scope A/B approval](approvals/cod19-approval.md) and [verified scoped Git/bootstrap activation](activation/cod19-activation.md), published at ba5c7c543bd711b663e1dfc92d4bd58497ffb557. C16-01/02 and authored record shapes in C16-03 govern this slice. Historical Scryer procedural references resolve through this bridge; no Scryer claims/runtime changes.

This session's user selected “Schema and types (Recommended)” and then requested “Implement the proposed plan.” Deliver additive ontology definitions, validated transport-neutral Rust types and compatibility fixtures only. No runtime schema installation, YAML compiler/hash implementation, inference, persistence, operation adapters, native acceptance, hosted activation, merge/release or next issue advancement. Full A01–A03 outcomes retain downstream owners; this slice supplies their foundation evidence.

## Baseline, placement and ownership

Selected pilot source baseline remains 90e2ab6fae2767bdb158e19f0072d43568e80dbc. Implementation branch feat/cod-20-architecture-types starts at published prerequisite ba5c7c543bd711b663e1dfc92d4bd58497ffb557 (PR #187, unmerged). That revision inherits dev's version/dependency differences from the selected pilot baseline; no src/assets difference exists. Do not relabel qualification of this revision as qualification of the original pilot. Current main checkout and its four intentional wiki edits remain untouched.

Workflow: delegated, one complete technical owner (code_implementation: default/gpt-6.1-terra/medium, fresh prompt). Root coordinates serialized Git/tracking/delivery. Independent verification and final review remain separate. Technical owner owns exactly src/architecture/**, new assets/architecture_schema.json, src/lib.rs (module export only), and new tests/architecture_model.rs. No manifests/lockfiles or legacy ontology changes. Root owns this brief and cod20-evidence.md plus evidence under docs/specs/CBG-C4-01/cod20-evidence/. Any additional ownership requires coordinator reassignment before edits.

Place new behavior in a transport-neutral architecture module using existing serde/serde_json only. Publish logical ontology definitions as an additive extension asset in the shared ontology format; do not add them to assets/graph_schema.json yet because its current consumers install/index every definition. Export the architecture schema and types through the new module; no runtime caller or dependency on CLI/MCP, storage, extraction or k-wiki. Existing source ontology remains authoritative for extracted code. Native types and extension asset define one matching contract, with parity fixtures preventing divergence.

Graph discovery/context found src/lib.rs, legacy ontology includes at src/api/catalog_support.rs, src/search_index/build.rs and src/staging_writer/accumulator.rs, and schema consumers in src/db_writer. Existing graph tooling is available, sometimes slow/busy; it indexes the original checkout, so new files have no graph entries yet. Verify mutable facts against source. No architectural drift is reconciled by this slice.

## Required behavior and acceptance

- C20-A01 (R01/A01): nine architecture node and five relationship definitions, version 0.1, distinct from source types; required common model/snapshot/state/schema metadata; no fabricated source location. Preserve legacy schema bytes and source semantics.
- C20-A02 (R01/R02, A01–A03 foundation): strict authored hierarchy types and typed pinned artifact/source provenance; accepted/candidate/observed/predicted/evaluation serialization remains distinct. Namespace row identity by model/snapshot/logical ID. No candidate/source or accepted ID aliasing.
- C20-A03 (R01/R02): whole-model validation reports stable subject/code diagnostics for duplicate IDs, invalid endpoints, missing/contradictory parentage and cross-level relationships; same-level cross-system intent is legal. No silent filtering or invented source identities.
- C20-A04: strict primitive/reference validation and fixtures cover malformed versions, IDs, digests, full Git object IDs/relative paths/spans and nonempty authored responsibilities. Typed records do not claim byte/evidence qualification without execution.
- C20-A05: compatibility fixtures prove source schema untouched and no installer/index/catalog behavior changes; no fake verified mappings/evaluations or authority promotion.

## Verification, evidence and delivery

Owner runs formatting checks, focused observable model/schema tests, affected legacy schema consumers and strict relevant Clippy/build checks. Run checks appropriate to this bounded additive module; record actual command, revision, environment, outcomes and limitations. Before a large Rust build inspect free disk (initial available 68 GiB); no cleaners. Use this isolated worktree's normal target directory, record actual path, and preserve evidence outside build cleanup roots. Independent Terra/medium verifier executes acceptance checks. Fresh default/gpt-6.1-sol/xhigh reviewer uses adversarial-review-engineer against final integrated snapshot, baseline, acceptance and evidence. Fix all confirmed P0–P3 findings and recheck affected paths. Root reviews actual diff, commits Conventional Commits, pushes and opens PR targeting dev, explicitly disclosing PR #187 prerequisite history. Independent publication readback required. No merge or release.

Reusable knowledge assessment at completion: avoid filler memory; retain issue-specific evidence here. Update curated knowledge only if a new durable boundary/constraint needs explanation, through MCP and its required checks. Existing approved wiki/spec identities are preserved.
