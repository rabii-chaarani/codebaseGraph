# COD-105 — Scryer exact-worktree evidence binding

Date: 2026-10-29. **Read-only investigation. Result: no supported independent source-root/worktree override was found in the connected Scryer interface or inspected documentation. Exact-worktree Scryer reconciliation remains pending.** This is not a claim that no future/server-internal capability could exist.

## Exact question and source identity

Keep the same authoritative codebaseGraph model at `/Users/rabii/Projects/Repositories/codebaseGraph/.scryer/`, including all committed intent, current approved/draft changes and stable claim IDs. Evidence must describe the actual COD-105 implementation in `/private/tmp/codebasegraph-cod103-cod105-20261029`, not the old dirty canonical checkout.

Independent Verifier's published report `/tmp/cod105-independent-evidence.md` and JSON bind tested source to initial HEAD `c8b8e7395c0a83bb8a2fe53a5add97df0d9cf617` **plus three uncommitted file hashes**, not HEAD alone:

| File | Tested SHA-256 |
|---|---|
| `crates/xtask/src/main.rs` | `652dbeb10cfbeabef04b61fa2429356be7155f70be90d05822ff0d278df985d4` |
| `.github/workflows/ci.yml` | `98f659e98ad2060f503785390bc810b453a3eb23518b372e85122d2090f1fcda` |
| `docs/release.md` | `88efe80f119558fdc3e913211c4ebdde11a131a9d5b349248cb1b5a4b8e0c5a3` |

Verifier executed focused nextest JUnit15/15 and full xtask51/51 plus formatting/strict xtask Clippy/workflow/release gates; this worker did not execute them. Genuine report `/private/tmp/cod105-evidence/c8b8e739-652dbeb10cfb/cod105-focused.xml`, SHA-256 `580f693dafbc418d493990064b0b29da9e59564cc0168a580b2b1e61395f856b`. There are7 actual new named tests; grouped assertions are not extra testcases. Real two-target packaged smoke, hosted CI/freshness and independent final review remain separate pending gates.

## Supported live contracts inspected

Installed Scryer app identity is **0.4.21**. Existing MCP registration launches `/Applications/scryer.app/Contents/MacOS/scryer-mcp` with no arguments. `~/.scryer/projects.json` lists only the canonical codebaseGraph absolute root; the implementation worktree has no `.scryer` model. Model top-level schema contains version/nodes/links/groups/sourceMap/testMap/boundaries/concerns, not a separate source-root or revision binding.

Relevant live schemas:

- `read_model(project?, node?, layer?)`: chooses the model at that project path.
- `update_source_map(project?, entries?, test_entries?, schemas?, boundaries?)`: source/test locations have `pattern` described as **project-relative file path**, plus optional symbol/line/endLine. No independent source-root/revision/commit/worktree parameter.
- `ingest_test_report(project?, path)`: `path` is an absolute or project-relative JUnit report path. No source snapshot/hash/revision or separate model/source project parameter.
- `get_test_radius(project?)`, `orient(project?,task?,files?)` and `mark_implemented(project?,...)` likewise expose no source-root override. `mark_implemented` is not needed for unchanged bug-restoration claims and must not be used to invent completion here.

Binding `test-verdicts` says verdicts are fingerprint-keyed on the claim's implementation and attached tests. `source-map` defines project-relative implementation/test locations; `test-attachment` says test names identify real exercised cases. `anchor-completeness` forbids anchors for unimplemented intent. Thus an XML path pointing into the implementation worktree is **only a report location**; it does not establish that fingerprints read source/tests from that worktree while `project` still selects the old root.

### Executed read-only selector check

`read_model(project="/private/tmp/codebasegraph-cod103-cod105-20261029", node="node-1")` returned:

> No model exists at project:/private/tmp/codebasegraph-cod103-cod105-20261029 yet (model file is absent). ... If you meant a different project, pass its absolute path as project.

No worktree model/directory was created by this read. Its suggestion to create a new model is inappropriate here: this is the same repository and must retain its authoritative architecture, not fork/copy the model or fabricate another project.

## Documentation/CLI limits

Inspected published local Scryer README setup/MCP/test-backing documentation and docs search. Setup documents linking a project and `scryer-mcp init` (writes AI integration configuration); it does not document a separate authoritative-model/source-worktree binding command. Worktree documentation concerns falsification probes, which create/reset isolated deliberately broken source and are not an ingest binding API for an existing implementation worktree. `open_probe` is neither authorized nor an appropriate workaround.

Invoking installed binary with `--help` and `--version` did not expose a CLI contract: both entered stdio MCP mode and ended with `connection closed: initialized request`. Do not infer an undocumented `--project`, `--source-root`, environment override or init/relink command from these attempts.

The neighboring Scryer source repository's own graph lookup failed read-only because its graph database was absent/empty. No Scryer implementation source was read, no graph built/rebuilt and no undocumented internal path or schema behavior is asserted. The connected live tool schema/error is the relevant supported-interface evidence.

## What must not be done

- Do not attach new COD-105 tests or ingest their passing XML against the old dirty root and call its fingerprints tested. The new functions are absent there and the implementation differs.
- Do not create a second model/project, copy `.scryer`, change registered project/settings/config, temporarily replace canonical source, symlink roots/files, use escaping/absolute source-map patterns or fabricate verdict records. None is a supported or authorized binding route.
- Do not use force-fold, generation primitives or global `reconcile_drift` to suppress this limitation. Preserve unrelated pending claims and drift.
- Do not revise resp15/16/17/256 merely to get evidence accepted. COD-105 restores existing behavior; COD-108's approved/draft architecture is unrelated.

## Bounded procedure after Production commits the tested snapshot

1. Production records the full implementation commit and proves committed implementation/test/workflow bytes equal the verified snapshot. Preserve Verifier's exact commands, report hashes and source identity in Git-tracked canonical evidence. A commit alone does not retest changed bytes.
2. Continue independently authorized final review/PR/hosted verification within the existing approval. Report Scryer reconciliation as **pending due to source-root binding**, not passing. This limitation alone need not prevent PR creation when disclosed; it prevents claiming complete reconciliation/Done and does not waive any other acceptance gate.
3. Owner supplies a **documented supported same-model source-worktree binding** if one exists in Scryer0.4.21, or separately authorizes a later legitimate canonical-checkout reconciliation when its user work can be preserved and exact tested code is actually present. No such mutation is authorized by this report. Do not promise a command absent from the live schema/docs.
4. Only once the same authoritative model can demonstrably resolve the exact committed implementation and test bytes: read scoped `locate`/model for resp15/16/17/256, confirm no behavior/claim changes, update only applicable existing implementation anchors and genuine new test attachments. Preserve existing unrelated anchors/tests; attach the actual new MCP parser cases to claims they exercise, not every test to every claim. Broader native artifact/provenance claims still need actual artifact evidence.
5. Before ingestion, independently verify source/test hashes at the **actual fingerprint root**, not only report paths. Ingest each genuine report once with `ingest_test_report` using that verified binding; inspect recorded/unmatched/ambiguous/unmentioned results and scoped test radius. XML/report identity and tested source must agree. Missing/new tests or stale/wrong-source fingerprints leave the gate pending. No global drift acknowledgement is implied.

Steps4–5 are conditional gates, **not executable now** through a discovered override. They use existing interfaces only after a legitimate binding is established.

## Small owner decision / recommended disposition

**Recommendation:** preserve independent local/PR evidence and keep exact-source Scryer anchor/test/verdict reconciliation explicitly pending. Ask the Scryer owner for an official same-model worktree binding contract, or schedule a separately approved canonical-root reconciliation after preserving user work. Do not expand COD-105 into a new architecture/model or integration feature.

If no supported same-model binding exists, the precise tool limitation is: the exposed `project` selector couples model location and source/test fingerprint root, while ingest accepts a report path but no source-root/revision attestation. Resolving that capability belongs to Scryer tooling/owner scope or an explicit later root-reconciliation decision, not silent codebaseGraph configuration edits. No evidence or requirement waiver is inferred.

## Changes and reusable knowledge

No source, Git, Linear, model, claims, anchors, tests, verdicts, settings, registrations or integration configuration changed; no build/test/rebase/graph rebuild or global reconciliation executed. Only this requested `/tmp` report was written. CLI help/version/read-only graph attempts are not tests. Constants unchanged. No reusable repository memory recorded: this is a version-specific interface limitation requiring owner confirmation, not a verified permanent architecture rule.
