# COD-107 — independent Windows-import warning fix verification

**PASS: all authorized local checks at actual source268; 83/83 genuine cases and zero compiler warnings. Windows-hosted qualification remains pending.**

## Authority and actual source

ExistingC103-R03→A03/sourcechg-xd2jtt scope; approval316be4c4bb81d4f6e025016b12374836b4dcc1a8/0e6008a1f46679a86aba85ac513beb05af94b0c0; retentione5adebfa1196a2074527cec4935522c43c773e39/08d2c060bba8bf45db069fe53185f06f5b3d6cd9, decision5b4b5aad-cbe8-4265-8db9-5ea351b7f386. No newscope/modelclaim/oracle.

Worktree`/private/tmp/codebasegraph-cod107-20261031`, HEAD **996967eb50d5f84f477525e4dccdc2f73421a029**, tree **301ec679880ca35888258dc7f4993f1a06b91959**; **onlydirtymain.rsSHA25626837ce559fd1bd973584321ab1fc7cd912ea6dd939b1714b9c956fe76be9594**. Before/afterHEAD/tree/status/exactdiff identical. All1931trackedinputcontents guarded; everyother1930file unchanged.

Only3testmoduleimportnames moved into one`#[cfg(unix)]use`. Exactreversaloftheuseblock yields canonical076entirefile byte-for-byte. Thus wholeproductionprefix, everyproduction/testfunctionbody+signature, mappedimplementation/testfingerprintinputs unchanged. Canonicalrootstays944/main076; **not wholefile-equal268**.

Actualtestmodulecallers: Unix`native_retention_step`3588-3594(yaml_step_by_id/yaml_path), Unix`required_rejects_retention_selector_failure`4306-4312(yaml_path/yaml_step_by_name). Productionparentmoduleuses stayunconditional/unchanged. Originalownerhandoff **14678b927dc40aab1fd2047bbe1e2fe60955fca9420d211f1aadcb1f03e3f17d** retained; contradictoryhistoricalstaticwordingisexplicitlycorrectedbyseparateowneruse-auditappendix **215cc6e68ce204229a8792fcf65c3c53b1ff0cebb615349006eb7abe5f069d01**. Independentactualusecontextsretained; no authoritydependsontheoldwording.

## Actual independent execution

Darwinarm64 /Rust+Cargo1.96.0/nextest0.9.140. Normalexistingnative0190.19/OpenSSL3.6.2/macOSdeployment13.3/jobs2 environment; defaultworktreetarget/noRUSTFLAGS/sharedtarget/featuresoverride/depsinstall. PrivateJUnitprofileimportfix onlyreusesretries0/thread1/native-testsconfiguration.

| Executed check | Result |
| --- | --- |
| `cargo fmt --all --check` |PASS|
| `cargo nextest list --offline --locked -p xtask --bin xtask --cargo-profile native-tests ...` |PASS;actualcurrentcompilerbuiltworktreextask;83discovered/0ignored,same83names|
| `cargo nextest run ... --profile importfix --test-threads 1 --no-tests fail` |**83passed,0failed/errors/skipped**,0retries|
| `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` |PASS|
| `cargo run --offline --locked -p xtask -- check-workflows` |PASSactual`workflow policy passed`|
| `cargo run --offline --locked -p xtask -- release-gate` |PASSactual`release gate passed`|

**Zero compiler-warning lines** acrossallrawchecklogs. GenuineJUnit`xtask-junit.xml`, SHA256 **b9c2ed3354a67f1193701c0d07e288775a1f9cb076be16eb89f8be4105c29df1**, actualrun **236c28a8-7128-4d7a-a6cb-d13e7811dd75**, all83casenames matchactualinventory. Actualcompilerbuildlog/currentpackage/binarypath+fullbinaryhash andunchangedbinaryafterrun retained. No synthesizedXML, textconversion, newtestframework or inventedcasecounts.

## Scoped model and payload preservation

ActualsamecanonicalmodelsourceMap3+3/testMap3+10/currentresp14/15records exactclosedreconciliation; all13attachments(12uniquecases) existandpassed innewgenuine83. Everymappedfunctioninput unchanged byuse-onlydelta. Actualrecordedfingerprintsretained—notrecomputed/invented. Model/testresults/anchorfilebeforeafterhashes equal. **No maps/ingest/fold/newradius/globaldriftwrite**; originalXML2cde/run35a05db1 remains unchangedhistoricalexecution. Prior31unrelatedfiles/68stales/53unmatched345unseenremainoriginalclosedlimits, notcleared.

Root/wikiinclude/manifests/workspacelock/config/resources/**all17README** unchanged. Changedxtaskmain is selectedbyneitherpackage. Current216root+60wiki nongeneratedsourcepayloadmemberhashes equalalreadyinspectedactual944archives. No repack/list/nativebuild/relabel; historicalactualarchiveVCSstays944. Wholeprior1930files, notjustproduct-sourcehash, suppliedinputequality.

## Criterion and remaining gates

- C103-R03/A03 narrowlocalpolicy/test+no-warningportion: **PASS** current268 actualchecks; unchangedbehavior/oracle.
- Existingstaticworkflow/ordinarygate andscoped14/15inputpreservation: **PASS** exactsource+executedchecks; not fullA03/nativeacceptance.
- OriginalWindowswarning atoldrun41269015161a1/S13d319: preserved**all32closedavailability02entries** byteexact; oldjobsuccess≠zero-warningpass. ThisDarwinpassdoesnotretroactivelyclearit.
- **Mandatorypending:** futurecorrectedheadfullhostedWindows/compiler-warningaudit, allapplicableworkflowSUCCESS, bothactualnativearchive+sidecar/provenance/memberinventory andunchangedexistingplatform/qualification/review gates. NoWindowscompile/actualnewCI/payloadsuccessinferred.

Allclosed83/18CLI/actual944crate/model/996binding/hostedwarningreports preserveoriginalhashes. No extraCLI18rerunneededforunchangedproduction; no originalevidencelabeladvanced. Freshreview/finalsource-evidencecommit/PR174newheadfullCI remaintheirassignedowners.

No source/Git/API/CI/model/tracking/wiki/settings/credentialswrite. OnlyauthorizedlocalCargoandownedprivateevidence. No constants/oracles/policieschanged; privateprofilelabel/reportpath only. EvidenceVerifierinherit/inherit/originalaccepteddispatch, no override/substitution/delegation; runtimemetadataunexposed. No reusablememorywritewarrantedfortransientsourceproof.

**STOP allCargo/privatewritesafterhandoff. No A03handoff/107→108/Done/push/merge/main/release/aggregateclaim.** Machine`result.json`, actualcommands/logs/inventory/genuineJUnit/compilerbinary/source/model/payloadproofadjacent; closedhashmanifest`files.json`. Engineer+requiredFinalReviewer+Production nextunderexistinggates.
