# COD-108 — actual Windows failure checkpoint03

**FAIL: new COD108 test fixture is invalid on Windows. Acceptance/merge/next-issue progression is blocked. No rerun or source fix was performed.**

## Exact failure/source

CI **41317523731/attempt1**, PR175head **157a4170f21d8f75d7d02611dd4e4bdfc5ca1dfa**,base **90af76d3453a6f5cc0f032045ed48c4fd09c275c**, actualcheckout/buildS **c2e6a277d884036fa5170cba6933c4825706a1b0**, reviewedheadtree **2f8d78fab4fb9a11a21290e92da7f894fff0fd81**. Actualjobcheckout/log/source-sha.txt/report.source_shaagreeS. Frozen`ci.rs`SHA **b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f** matchesindependentlyverified157asource; no sourcechanged.

ActualWindowsunit/shard2job **124880650882 FAILED**. Namedcase:

`ci::tests::git_path_names_are_nul_delimited_and_extra_paths_force_full`

At **ci.rs:1246:43**, `fs::write(...).unwrap()` panics:

```text
Os { code: 123, kind: InvalidFilename,
message: "The filename, directory name, or volume label syntax is incorrect." }
```

Frozen1239–1271testcreates`unrelated name\nwith newline.txt` on the realfilesystem. Windowsrejects the newline filename **before the commit/classifierFullassertion**. This is a confirmedintroducedcross-platformtest-fixturedefect innewci.rs, not semanticclassifierfailure, flake, baselineassumption, expectednegative, ignoredcase or transporterror. Rawjob943–955 andactualdiagnostic556–568 retainactualpanic:thatxtaskinvocation2passed/1failed/libtestexit101. Nativewrapperstillrunsselectedcommands2/3successfully,thenproperlyreturnsfirstfailure/exit1.

## Actual byte evidence / partial snapshot

Oneactualsnapshot:**22jobs/checks,10success,1failure,11running**;4diagnosticsavailable includingoldgeneration2. No finalrun/native/requiredresultpredicted.

NewactualartifactZIPs, allAPIrun/attempt/head/expiry=false/size/digestverified against fullreceivedbytes; everymemberpath/type/duplicates/CRC/hash inspected safely:

| Artifact | Bytes | ActualSHA256 | Rawpassed/failed/ignored* |
| --- | ---: | --- | ---: |
| 13088199023 / Windowsunit2 FAILURE |55858|`ddd1c3a354b08a84dfc974e822af7e2f5f35c85ee232ff219d56dbb53a6e6f1b`|225/1/0|
| 13088990365 / Windowsunit4 SUCCESS |59490|`d4bb78aa2583a9f804e2c46a23e06093a4906ee7191ab9bb0722869eb6802fc6`|225/0/0|
| 13088343525 / Windowsintegration3 SUCCESS |33260|`a3c7d4e2e7d1891fc22201b79b6c2e25f3d86d210dab43afcb3bbe9920da7158`|109/0/0|

*8actualcommands/rawper-invocationcounts, notuniqueplatformcoverage. AllreportsCLIunchanged/sourceS; failedreportexecutions[0].error explicitlyexit101, laterexecutionerrorsnull. Entire3joblogsand3platformlogs have **0compilerwarnings**; realtestfailure remainsfailure. Expectedstoragefaultpanicselsewhereendingok are retained,separatefromthisactualfailedcase. RawCargologsnotJUnit; noXMLconversion/modelingest.

## Owner and minimal correction recommendation

Return **ci.rs test fixture** tooriginalBackendviaEngineer. Keep the realextra-path→Fullassertion onWindowsusingapermittedspacedfilename; retainrealnewlinefilesystemcaseonUnix and existingportablein-memoryNUL/newline`parse_raw_diff`test. Do notignore/dropcase/weakenoracle. Nofixchosenorexecutedbyverifier. Afterownerfreeze, assignedaffectedlocalgenuineevidence/review and correctedheadactualWindows/fullnormalCI mustpass; noautomaticrerun/VM/dependencygrant.

Currentfailureblocksacceptanceevenifother15jobsgreenlater. Windowsproductupstreammaybecomeskipped; **no COD108PRproductarchive/uploadexpected** underunchanged107retentionFALSE, so missingnonretainedbytesarenotthenewblocker. Do notsubstituteold107archivesorlocal102/Ubuntu101passes. Laterprotected-mainlight/mixedrollout/modelbindinggatesremainatproperstage.

Prior01all29/02all13entries byteexact; old7commonlogs/generation2ZIPnotredownloaded. Entirefailedstdout/stderr/callcommand/profile/features/source/platform/versionexcerpts andrawreports preserved. No source/Git/Cargo/model/API/CIwrite/rerun/cancel/watch/poll; no constantschangedormemorywritewarranted. **STOPallAPI/privatewritespendingEngineersourceownerfixgrant/currentnewrunstage. NoA04A05handoff/110/Done/merge/releaseclaim.** Machine/rawfailure/source/ZIP/memberaudit/hashmanifestadjacent.
