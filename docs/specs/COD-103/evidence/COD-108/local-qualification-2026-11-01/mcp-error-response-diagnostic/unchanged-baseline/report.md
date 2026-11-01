# COD-108 — unchanged exact native MCP reproduction

**One local unchanged test PASS, exit0, zero compiler warnings. The hosted Mac failure remains real and its cause is unresolved.**

## Actual source and command

Isolated`/private/tmp/codebasegraph-cod108-20261101` andcanonicalsource areclean **0413e543668f3e08003aa6a0d4ead350f49a61cc**, tree **5141d876f604bea22efaea7224f47d50b3a2b81a**, before/after. SelectedtestfileSHA **22cd4cef51368a0f3050641e7e437d7fbc4d798e6337b87c3bf1b446cf3081bc** unchanged; currentci525/source/config/nativeinputs guarded. No source/test changes.

Exactly **one invocation /0retries**:

```text
cargo test --workspace --offline --locked --profile native-tests --lib --bins --examples adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors -- --exact --nocapture
```

ActualPID23140 completedexit0. Exactlyonecaseexecuted:

```text
test adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 847 filtered out; finished in 7.82s
```

Fourotherworkspaceharnessesexecute0tests; total960filteredcases are **notpasses**. No wholeworkspace-suiteclaim or syntheticJUnit/XML/modelingest. Actualcompilerbuild`native-tests`optimized2m25s; bothrawstdout/stderr have0compiler-warninglines. Originalhandlelostduringkernelrestore; actualbash-doneexit0andrawlogsretained,noreplay/poll.

## Native environment / binary proof

Darwin25.6.0arm64/RustCargo1.96.0; existingnative0190.19/OpenSSL3.6.2/deployment13.3/jobs2. Actualconfigurednative-testsinheritsrelease withdebugassertions/overflowchecks/panicunwind; unchangedprojectHOME/testthreadsettings. Defaultisolatedtarget/noRUSTFLAGS/sharedtarget/customfeatures/profile/resource/configchangeorinstall.

ActualRunningrootlibbinary`target/native-tests/deps/codebase_graph-51c13b6bd229e743`, SHA **1780de7220ee4e61a5218b02b6744f047c298a1f9af63543296e8ac1f3241cca**. Fiveactualharnesspaths/fullhashes/dependencyfiles/nativeCargofingerprints retained. No featureflagswereadded: actualworkspaceunificationrootfeatures`default,bundled-windows-extensions`,Wiki`default`,xtask`[]` aredisclosed, notcalledfeature-free. Compilerlogbindscurrentworktree; source/HEAD/tree/cleanstatusunchangedafterexecution.

Rawstdout **afa9f027b3b37b6937a2056c19b9c75b90159bf65ff50a8898eae73cc2d2113a** /stderr **a0be724876a30740ec2718fdccf82a4a5d88e559441acacf7cf565401970f467**; exactpaths/size/hashinresultandmanifest. Actualnormaltesttemporaryruntime/Cargotargetoutputs only.

## Interpretation and remaining blocker

This establishes **one unchanged localpass**, notafix/rootcause/flakewaiverorhostedclear. Source-boundhostedMacjob124897498528/run41323156535a1/S1c173 trulyfailedid3graph_health.isError=trueatline220. Itsfullerrorbodywasnotlogged. A successful localrunproducesnofailingresponsepayload; differinghost/compiler/nativeenvironment/binaryidentitiesremainexplicit. Do notinferbaseline/unrelated/asynchronousstartupcausefromthelocalpass.

HostedCP03report8a018…all22files andearlierWindowsFR001passa9c8…all16filesremainbyteexact/separatehistory. Hostedscheduling/native/fullCI/currentacceptance/110/mergeremainblockedbytheactualMacfailure. Furtherboundedfullid3JSON/stderr/refresh-statecapture/sourceassertiondiagnosticownershiprequiresaseparategrant; no retries/testweakening/sourcechangeperformedhere.

OnlyauthorizedoneCargo/nativeexecution+read-onlysource/Gitguards+ownedprivateevidence. No source/Git/API/model/maps/ingest/fold/roottransition/tracker/wiki/settings/hostedCIactions, watch/poll/sleep ordependencyinstall. No constantschanged; oneobservationdoesnotwarrantreusablememory.

**STOPallCargo/execution/privatewritesafterhandoff. NoA04A05handoff/110/Done/merge/releaseclaim.** Machine`result.json`, rawlogs/commands/source/nativeprofile/compiler-harnessprovenance andclosed`files.json`adjacent.
