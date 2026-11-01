# COD-107 — independent Windows path-oracle fix verification

**Assigned affected local checks PASS; A03 remains incomplete.** Actual source and all reports/commands below are independently identified; no Windows execution or platform acceptance is claimed.

## Actual source identity and narrow fix

Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch`cod103-stack/cod-107`, actual HEAD **`2c4ed59bacfdc316f61e202eb6b5eaaa6acca767`**, tree **`86b0d6aa4bebf645a59dd5bb12bd7beeffd8ca2d`**, plus **only main.rs dirty**. Its testedSHA256 is **`076dd22a1da16a6d651016b7227536f1d65de766ff6c09a3dcb02631c51af5da`**. Status/HEAD/tree/diffstat and all source/config/manifests/lock hashes remained identical after checks; diffcheckexit0.

Original owner handoff SHA256`bd0169a8bfd30dcaa6d470661ff62817e0e943ef7f41b6f789c74d291a2ae1c7` is preserved verbatim. Its08d2/tree0955heading is stale; **actual independent Git identity above governs**, not that heading.

Actualdiff changes only the two existing tests' expected diagnostic paths and owned String borrows. Six paths now use `Path::new(...).join(...).display()` just as production renders native relative paths: docs-extra/neighbor.py,src/docs/neighbor.py,scripts/helper.py,src/codebase_graph/helper.py,docs/allowed.py(false-positive guard),outside/linked.py. Fixed roottooling/security/workflow diagnostic prefixes stay literal. No output normalization, alternate separator allowance, cfg skip, production/oracle logic or accepted outcome changed. The forbidden allowed-docs diagnostic assertion remains mandatory.

All production bytes before the cfg(test) module are identical to prior e4d256 source (proof in `unchanged-production-reuse-proof.json`). Original approvals316be/0e6008 and retentione5ade/08d2/C103-R03→A03 remain unchanged; existingcontext/sourcechg-xd2jtt, no modelwrite.

| Current input | SHA256 before/after |
| --- | --- |
| `crates/xtask/src/main.rs` | `076dd22a1da16a6d651016b7227536f1d65de766ff6c09a3dcb02631c51af5da` |
| `docs/release.md` | `1e67f4949647c10a48f9184db230454572d666a756b68c203690398f525c7954` |
| `.github/workflows/ci.yml` | `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9` |
| `.github/workflows/native.yml` | `cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683` |

## Actual independent execution

| Actual check | Exit | Walltime |
| --- | ---: | --- |
| `format` | 0 | 2.090s |
| `inventory` | 0 | final duration unavailable |
| `full-xtask-independent` | 0 | 30.714s |
| `check-workflows` | 0 | 0.872s |
| `ordinary-release-gate` | 0 | 0.472s |
| `strict-workspace-clippy` | 0 | 1.260s |
| `final-whitespace-and-source-guard` | 0 | 0.093s |

**83/83 real tests passed**,0failures/errors/skips, native-tests Cargo profile, outside-tree reporting config, retries0/one thread/no-testsfail. Exact actual inventory matches every emittedcase. Both affected tests really executed:
- `tests::docs_python_gate_rejects_neighbors_root_tooling_and_workflow_python`
- `tests::docs_python_exception_rejects_component_aliases_and_symlinks`

Genuine JUnit:`xtask-junit.xml`, SHA256 **`2cde0488dcf33b8704d607a9a4c86683eb8837dad610ab5bfde441d2a63f3c1c`**, run **`35a05db1-a42f-440e-8385-828272d0c7fe`**, runner30.338s/wall30.714s. Actual current Cargo compile line names this worktree,83caseinventory/sourcepackage is4.0.5, target/native-tests binarySHA256 **`7e49ba09a6c2cdba8fe423fe6cf084b27c0f2fac90a02ce666ab6b65077c4b1a`**, unchanged after checks. `binary-provenance.json`/inventory/rawbuildlog retain the exact sourcebinding; no othercheckout/sharedtarget assumption.

Formatting, actualcheck-workflows and ordinaryrelease-gate allpass. Strict `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` passed; **0warning lines** across current compile/test/policy/Clippylogs. No syntheticXML, conversion, ignoredcase/zero-testpass, automaticretry or sourcefix.

## Windows/source and historical-evidence limits

This actual execution is macOS26.6.2ARM64/RustCargo1.96/nextest0.9.140; existing verified native019/OpenSSL3.6.2/deployment13.3/jobs2/defaultisolatedtarget/projectserialconfiguration preserved. Testsource now derives expected separators correctly by native Path semantics, but **Macpass is not Windowspass**. Windows drive assertion/symlink privilege/runtime branches and native warning qualification remain later actual hosted evidence.

Original closed artifacts remain exact and historical:
- Initial83JUnit/format/strictClippyreportSHA **652f051ad71b6ca34f868d862154ad4f2df574177567c8588f655d7122005c18**.
-18rawCLI supplement reportSHA **e5f328b35682ec357f461825be1824fc2eea342f8f6b87211ef5bf5b9263d2d1**: reused only as prior actual behavior evidence on byte-identical production/input behavior; not newly executed or labelled076dsource.
- Actual clean2c4crateinventory reportSHA **10a8cfef2be894a5d007ab32189cfadebf977f209bb8c46585664f31df2c5479**: archived VCS2c4 remains exact, not relabelled dirty076d/futureHEAD. No package/noallow-dirty rerun here. Later final-head payload-input proof/VCS disclosure remains.

No counts are added as new unique tests. All closed reports checked unchanged. CurrentcanonicalScryerrootf69 is not future107; no attachments/maps/ingest/fold/rootchange/globaldrift. No source/Gitrefs/tracker/wiki/model/CI/VM/settings/credentials/dependency writes. Only own private config/rawlogs/JUnit/reports and normal assigned target outputs. No nativearchive/productruntimecohort/package build.

FreshsameFinalReviewer/focusedfix acceptance, faithful related source/evidence commit/PR/full exacthead allCI, actual both retained same-source native archive+sidecar+provenance payload inventories, applicable cleanfinalcrate/sourcebinding and currentcanonicalmodel reconciliation remain. **No A03complete/107→108/Done/main/release/platformaggregate assertion.**

EvidenceVerifierinherit/inherit original accepted dispatch/nooverride/substitution/nestedrole; runtime metadataunexposed. No constants/policy/oraclechanged; no memorywrite warranted/authorized for transient verification.

**Engineer/FinalReviewer/Production next; Cargo/source/private report writes STOPPED and exclusivelease released on handoff.** Machine`result.json`, closedhashmanifest`files.json`, exactcommands/sourcebeforeafter/provenance/realJUnit adjacent. Inventorycompletion crossedkernelrestore: exit0actualnotification preserved, finaldurationunavailable; no timing repeat.
