# COD-108 — actual macOS unit failure checkpoint03

**FAIL: current macOS MCP test receives an error from graph_health. Full CI/108 acceptance/110 progression/merge remain blocked. The underlying error cause is not yet known.**

## Actual source / failed execution

ExactCI **41323156535/attempt1**, PR175head **0413e543668f3e08003aa6a0d4ead350f49a61cc**,base107 **90af76d3453a6f5cc0f032045ed48c4fd09c275c**. Actualnativejobcheckout anddiagnostic source-sha.txt/report.source_sha agree **S=1c1730dfd06f238a6f6546e7e842a3988e52d61e**. ImmutableGitAPIparents[90af,0413],tree **5141d876f604bea22efaea7224f47d50b3a2b81a** exactlyreviewedheadtree. Notrunmetadatahead-as-S.

Completedjob **124897498528** `native (macos-arm64) / native tests (macos-arm64, unit)` is **failure**: setup/checkout/dependencies succeed, Runplatformtests fails, diagnosticupload succeeds. Actualcommand:

```text
cargo test --workspace --locked --profile native-tests --lib --bins --examples
```

Namedfailure **`adapters::cli::tests::mcp::mcp_stdio_serves_tools_and_tool_errors`**, source **src/adapters/cli/tests/mcp.rs:220:5**. Wholejob756–761/platform504–509:

```text
assertion `left == right` failed
  left: Bool(true)
 right: false
```

The frozen test's thirdresponse is **id3 graph_health**. It expected`result.isError=false`; actualvalueistrue. Initialize/list/count assertionsprecedingline220passed. Later healthstructuredContent.ok/refresh,searchandforbiddenquery assertionsare **notreached**. No test-ignore/expectednegative/failurelaundering: actualrootlibrary **845passed/1failed/2ignored**, libtestexit101→nativewrapperexit1. One nestedchildfixture adds1rawpass, notnewunique coverage. Discoverylists961workspacecases(harnesses848/0/10/0/103), but the failure stops later workspace targets; **not103Macxtaskpasses**.

## What is and is not known

Actualfullgraph_healthJSON/errorbody was **not printed or archived**. The assertion includes onlytheBoolean. Therefore startup/readiness,storage,environmentorother runtimecause is **UNDETERMINED**; recalledstartupmemoryisadvisory,notthisfailure'srootcause. Wholejobandplatformlog compilerwarnings:**0**. This is runtime/tool-response/testcontractfailure, notcompiler/setup/transportobserverfailure.

Graph-firstlookup+runtimecontextfoundthetest. A laterhelpergraphqueryreturned`graph executor is busy`; that is a separateobservertoolerror, **not the CIcause**. No graphretry/rebuild/configchange.

ExactfailingtestfileSHA **22cd4cef51368a0f3050641e7e437d7fbc4d798e6337b87c3bf1b446cf3081bc**, Gitblob036619cf5b36ab0317b5d92af7ea8e990337a7ba, isunchanged90af→157a→0413. **Unchanged source does not establish baseline/flake/unrelated classification**; no comparablebaselineexecutionorcausalisolationwasdone. Exactsourcecopy/staticreachretainedprivately.

## Actual diagnostic bytes

Artifact **13092100404 /test-diagnostics-macos-arm64-unit-shard0-1**, **46954bytes**, SHA256 **647fb6f9228a42aaaf2c74182e55c0bb3d3c676858a355238113280a0846de70**. APIrun/head/attempt/expiry=false/size/digest matchfullreceivedbytes. All5members safelychecked:wholeplatformlog,source-sha,inventory,ignoredlist,realreport—everymemberbytes/path/type/duplicate/link/special/CRC/hash verified. Nothingexecuted; no syntheticJUnit/conversion/modelingest. ActualreportCLIunchanged/sourceS, execution.error=`native test failed with status exit status: 101`.

Twoignoredsyntheticpublic-producerfixturecasesremainexplicit/notpassedbranches. Unrelatedintentionalfaultpanicselsewhereendingokarenotthisnamedfailure. Rawjob/platformlog/stderr/commands/source/environment/fullmemberhashes retained.

## Next owner / STOP

Engineer should assign boundedMCP/runtime diagnosis. Underaseparategrant, focusonlytheexactnamednative-tests case andcapturethe **full id3 graph_healthresponse + relevantstderr/refreshstate** beforechoosingafix. PreserveisError=falseassertion; no blindretry/ignore/timeout/resourcepolicy change or weakenedoracle recommendation fromaboolalone. No repro/fixwasexecutedhere.

Currentrunwasstillin_progressatonesnapshot; nofinalglobalconclusion/otherjob/outcomeprediction. NoCOD108PRproductuploadexpectedbyunchanged107-onlyretentionFALSE; missingunretainedarchivesarenotthisblocker. EarlierWindowsportableFR001pass/sourceS1c173andoldWindows157a/Sc2e6Win123failurebothremainhonestseparatehistory. CP01all29/CP02all16closedentriesbyteexact; noold/otherlogsorZIPredownload.

Onlyread-onlyexactrun/job/API+singleMacZIP/sourceauditandownedprivateoutputs. NoCargo/repro/source/Git/model/API/CIwrite, rerun/cancel/watch/poll/settings/dependencychange. No constantschanged ormemoryrecordwarrantedbeforeverifiedrootcause. **STOPallAPI/privatewritesafterhandoff. NoA04A05handoff/110/Done/merge/releaseclaim.** Machine/rawproof/hashmanifestadjacent.
