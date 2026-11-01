# COD-107 Windows import-warning fix — focused Final Review

**PASS for corrected-source pre-push review.** COD107-FR-002/P2 is resolved by the owner's minimal import fix. No unresolved or new confirmed P0–P3 defect. **Corrected Windows execution/zero-warning qualification is still pending.**

## Scope, settings and snapshot

SAME required fresh Final Reviewer, host-accepted `openai-codex/gpt-6.1-sol`/max, native default/non-fork; runtime self-metadata unexposed. Focused fix verification only, not a repeated integrated review. Existing C103-R03/A03 approvals316be/0e6008, retentione5ade/08d2, user5b4b5aad and source`chg-xd2jtt` unchanged; no new claim/approval/intent.

Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch`cod103-stack/cod-107`, HEAD**`996967eb50d5f84f477525e4dccdc2f73421a029`**, tree**`301ec679880ca35888258dc7f4993f1a06b91959`**, **only main.rs dirty**, SHA256**`26837ce559fd1bd973584321ab1fc7cd912ea6dd939b1714b9c956fe76be9594`**. This is not a new committed-head CI pass. Source read after graph semantic discovery/explicit change-impact context. All1930other files, docs1e67/ci3e766/nativecc4d, manifests/lock/config/resources and17README inputs remain unchanged. Independent before/after1931content hashes match.

Original integratedf775/crate39b13/finalhead3050 reports/results/manifests stay byte-identical. Original Windows path finding COD107-FR-001 remains fixed: all function/test/assertion bytes are unchanged by this correction.

## COD107-FR-002 — P2, resolved for this review

**Cause:** original `main.rs3021` imported `yaml_path`, `yaml_step_by_id`, `yaml_step_by_name` unconditionally inside `#[cfg(test)] mod tests`. Every direct use **inside that child module** is Unix-only. The supported Windows native-test compile removes those callers but retained the imports, emitting `unused_imports`.

**Actual consequence:** CI41269015161/attempt1, PR174 head996/basef69, actual generated buildS**`13d319e442e06b137d46ac5065018e2677b13ae9`**, tree301e, emitted the compiler warning. Four observed Windows jobs succeeded, but the repository's separate **zero-compiler-warning gate remained blocked**. This is neither a test failure nor a cargo-audit advisory. Three imports were added by COD107 retention tests; baselinef69 lacked them.

I independently rehashed/read all four original diagnostic ZIP `platform-test.log` members: artifacts13069030952,13068807777,13068778350,13068718229. Each contains the three-name warning at3021; repeated captures group into one root cause. All32closed availability02manifest entries match. Closed hosted report SHA**`542b90efda59cef9711e511373122ba871b6661986ce56c9370af9371ec760b4`** and raw compiler receipt remain original evidence; oldS/996warning is **not** retroactively cleared by current local results.

**Smallest correction:** final `main.rs3017–3025` removes exactly those three names from unconditional child imports and adds one `#[cfg(unix)] use super::{yaml_path, yaml_step_by_id, yaml_step_by_name};`. No lint allowance, test skip, body/assertion/API/default/recipe change.

**Focused scope/CFG trace:**

| Test-module item | Direct helpers | Existing guard |
| --- | --- | --- |
| `native_retention_step`,3588–3596 | yaml_step_by_id; yaml_path | `#[cfg(unix)]` |
| `required_rejects_retention_selector_failure`,4306–4314 | yaml_path; yaml_step_by_name | `#[cfg(unix)]` + test |

All calls of the first helper are inside Unix-only retention tests. `assert_retention_result`/`run_required_result` and their test callers also retain existing Unix guards. No other child-test reference needs these imports on Windows/non-Unix. On Unix the imports stay present for those exact callers. **Parent-module policy definitions/calls remain unconditional in a different lexical scope**, unaffected by the child import change.

Exact076→268diff is import-only. Entire production prefix byte-identical (SHA`569b9bfcb669917a172408cb0c315d4791564f6c29fb21257eb5661fc7fbe70e`); every test function/signature/body/assertion after imports byte-identical (suffixSHA`1b3dcebfee7941fae2502a63f7d616b6e4fc6c885d519325d750022a3050dc8e`). No broader regression path found.

Original owner handoff SHA14678b92… is preserved. Its inaccurate platform-independent-child-test-use wording is explicitly corrected by separate erratum JSON**`215cc6e68ce204229a8792fcf65c3c53b1ff0cebb615349006eb7abe5f069d01`**, Markdownb17b4c68…. Actual source/cfg trace, not the old wording, establishes this conclusion.

## Actual independent affected checks inspected

Closed `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-a2a991d7/cod107-independent-import-warning-fix/report.md` SHA**`3e62e049f9ab85ac2478d68fec53208333723475a5f99c01983dd0fce5046e50`**; result**`6d0a02be2c04cfee1899e3177996bce33ad5b54f1e8b5ad0068a96eff938a0ef`**; files**`97a7fe4a092ab09e26153e74ba1e04bba856931adde89c05d6aaf3c7978ceefe`**. All28manifest entries rehashed and match. Actual commands/raw logs/XML/build provenance read, not just summary.

DarwinARM64/RustCargo1.96.0/nextest0.9.140, existing native019/OpenSSL3.6.2/deployment13.3/jobs2 environment. Current268 actually compiled from this worktree; native-tests binarySHA`477e2fe9e7f1ea16ce472cda20b51ce1845106871ea9bccc942b5d55bddec55e` unchanged after run. Genuine JUnitSHA**`b9c2ed3354a67f1193701c0d07e288775a1f9cb076be16eb89f8be4105c29df1`**, run**`236c28a8-7128-4d7a-a6cb-d13e7811dd75`**: **83/83passed,0failures/errors/skips**, exact actual inventory, retries0/thread1/native-tests. Format, strict **fullworkspace/alltargets/allfeatures Clippy -Dwarnings**, actual check-workflows and ordinary release-gate all exit0. **Zero compiler-warning lines** across raw local logs.

These are actual **macOS**, not Windows checks. Reviewer ran no Cargo/reproduction/fullreview rerun. Source-only cfg proof resolves this root cause for pre-push review; fresh actual Windows no-warning evidence is still due.

## Payload/model scope and remaining gates

Only changed file is xtaskmain, selected by neither actual root nor wiki package. Manifests/locks/config/resources/17README inputs unchanged. Verifier's216root+60wiki nongeneratedactual944member-source hashes remain equal; actual archives keep truthful **VCS944**, not268/996/newhead. No package/list/native build/relabel required or claimed for this import correction.

Mapped production/test function bodies and fingerprint inputs remain byte-identical; canonical root still944/wholefile076, **not** wholefile268. Existing exact SAME-model scoped14/15 records/attachments preserved; no new fingerprint algorithm, map/ingest/fold or globalgreen claim. Prior83/18CLI/package/model/hosted reports keep their original execution identities.

Next owners: Engineer/Production for correctedsource+faithful evidence commit/PR174push, then Verifier for **fresh corrected-head/current-base all-applicable fullCI SUCCESS**, actual Windows no-warning audit/branch accounting and both actual same-source native archive/sidecar/provenance/member inventories. Bind actual generatedPRmergeS to head/base/parents/tree; do not substitute oldS13d319archives or demand them before correctedsource push. Finalcommit identity wrap may be focused, not a fullreview restart. Current-headA03/107→108, merge/release/aggregate closure remain pending, with existing allgreen/non-bypass freshness/serial ownmainCI/fullqueue gates unchanged.

No unresolved decision or new stage/approval invented. No constants, sourcepolicy, behavior, oracle, testselection or lintlevel changed. Only this NEW private review directory written; no source/Git/model/API/CI/Cargo mutation or oldreportrewrite. Reusable knowledge assessed: source-specific finding/evidence, no memory write warranted/authorized. **CLOSED/STOP.**
