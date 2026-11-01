# COD-107 — hosted availability02 closed evidence

**Confirmed blocker: Windows xtask test compilation emits unused-import warnings.** The four completed Windows jobs themselves succeeded; that does not satisfy the separate zero-compiler-warning gate. This is partial evidence, not final CI/A03 acceptance.

## Actual source and run

PR174 / CI **41269015161 attempt1**, `pull_request`, `.github/workflows/ci.yml`. PRhead **996967eb50d5f84f477525e4dccdc2f73421a029**, base **f69fc133c544e2ecd16a825a1e4c8f448c8d81b7**. Actual checkout/buildsource **S=13d319e442e06b137d46ac5065018e2677b13ae9**; immutableGitAPI parents `[f69,996]`; S/head share tree **301ec679880ca35888258dc7f4993f1a06b91959**. Fouractualcheckoutlogs, source-sha.txt and diagnosticreport.source_sha bind S. Runheadmetadata996 is not renamed as buildS.

Parent-authorized single availability read observed **20 currentjobs:9success/11stillrunning;runin_progress**, fourdiagnosticartifacts, **zero nativeproductarchives**. No laterstatus assumed. Production remains solewatcher; no secondwatch, polling, sleeps, reruns/cancel/dispatch.

## Actual artifact bytes and case accounting

All fourZIPs downloaded byactualartifactID/run/headbinding; APIexpired=false, exactAPI size+sha256digest matched fullprivateZIPbytes. EveryZIPmember content/type/path/duplicate/symlink/special safety/hash/CRC inspected without executing any contents. Allreportexecution.error=null and CLIunchanged=true; successfuldiagnosticupload IDs confirmed inactualjoblogs.

| Actual artifact | ZIP bytes | Actual ZIP SHA256 | Actualcommands | Rawpassed/failed/ignored* |
| --- | ---: | --- | ---: | ---: |
| 13069030952 / test-diagnostics-windows-x86_64-integration-shard4-1 | 32882 | `a744c7a2c16a0a7fd6235d9143586f7f2f199cbd22982d57f4539201b396e744` | 2 | 106/0/2 |
| 13068807777 / test-diagnostics-windows-x86_64-generation-shard2-1 | 6487 | `b756fa5b8d7f0527a43a28ef736d630822b443468c51cbad9f191c7ca31fea26` | 1 | 1/0/0 |
| 13068778350 / test-diagnostics-windows-x86_64-unit-shard2-1 | 55285 | `18896ed91afda0e61fd2cd7e59a03a51206b031e07acf1c761bf031e81dc338c` | 3 | 223/0/0 |
| 13068718229 / test-diagnostics-windows-x86_64-integration-shard1-1 | 34532 | `2a1dbc9da3c8ad22f83d1f71402a3169ad89afcf3032f1b9df18f2a1893c84d8` | 3 | 113/0/0 |

*Per-invocation result-group totals including repeats, **not unique platform coverage**. Combined443rawpassed/0failed/2ignored/3105filtered entries;9executions. Partial4of15artifacts, parallel/interleavedoutput and repeats prevent claiming this as uniquecompleteinventory. Fullcommands/source/shard/inventory/selection/rawresults retained inJSON/ZIPs. PlainCargo text is not JUnit; no XMLconversion/modelingest.

Twoignoredintegration4 cases remain explicit: `unrelated_worker_pid_fixture` (subprocessfixturelegacyworkerrecovery), `relative_barrier_subprocess_fixture` (subprocessfixturecwd-independentcleanup). Not counted as passes. Integration1 compile_fail doctests intentionally logE0505/E0507+abort thenok;3doctests passed, notCI/observerfailure. Rawerrorcontext retained.

Actualunit2 rawlog includes `tests::docs_python_exception_rejects_component_aliases_and_symlinks ... ok`; **does not prove optionalsymlinkprivilegebranch executed**. Thesefour artifacts do not establish explicitWindowsdrive/symlink branch accounting or privileged/unavailablebranch status. No Unix-cfgbodytests presented as Windowspasses.

## Actual compiler-warning blocker

Allfour platformlogs emit:

```text
warning: unused imports: `yaml_path`, `yaml_step_by_id`, and `yaml_step_by_name`
 --> crates\xtask\src\main.rs:3021:30
warning: `xtask` (bin "xtask" test) generated 1 warning
```

Repeatedcompiler captures/invocations are retained, not inflated as independentdefects. This is a realcompilerunused_imports warning, **not a cargo-auditbaseline advisory**. SourceS13d319/996 remains warning-affected. Coordinator notified; Backendseparateownership only. Verifier made no sourcefix/waiver/retest. A future repairedhead/run can replace acceptance, not erase/relabel this actual warning evidence.

## Limits and STOP

Stillmissing:11diagnosticartifacts, bothactualnativearchives+sidecars/provenance/memberbytes, fullfinalCI/check inventory, actualretaintrue/nativeproductupload, allcompilerwarnings/optimizedsmokes/packagechecks, installer/DLLinventory and explicitWindowsbranch qualification. No nativepayload/CodeQL/publicpublication/fullDesktopqualification success inferred. Existing local83/actual944crates are separate provenance, not replacement Windows/native evidence.

No source/Gitrefs/model/Cargo/build/install/settings/CIaction/API mutation/tracker/wikiwrite. Earlier5joblogs not redownloaded; existingrawcase/warning/source/ZIP receipts preserved. Onlyownedprivateavailabilityevidence written. No constants changed. EvidenceVerifierinherit/inherit/originaldispatch, no overrides/substitution/delegation; runtimemetadataunexposed. No reusablememorywrite warranted for transient partialrun.

**STOP all available-read/privatewrites until nextexplicitmeaningfulmilestone. No A03complete/107→108/Done/merge/main/release/aggregate claim.** Machine`result.json`, hashmanifest`files.json`, actualAPI/download/source/upload/case/warning receipts adjacent; fullartifactZIPs private in`../diagnostic-artifacts/`.
