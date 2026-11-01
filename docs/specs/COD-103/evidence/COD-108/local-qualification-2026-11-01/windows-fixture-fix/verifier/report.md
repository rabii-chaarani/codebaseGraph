# COD-108 — independent Windows fixture correction verification

**PASS for affected local verification:103/103 genuine tests, all assigned checks and zero compiler warnings. Actual corrected-head Windows qualification remains pending.**

## Exact scope and source

Worktree`/private/tmp/codebasegraph-cod108-20261101`, branch`cod103-stack/cod-108`, HEAD **157a4170f21d8f75d7d02611dd4e4bdfc5ca1dfa**, tree **2f8d78fab4fb9a11a21290e92da7f894fff0fd81**. Onlydirty`ci.rs`: **49894bytes/SHA25652595249047f49a2b8c9dece9fb48fbbafda6cd5e70849c5b8ffaf252c0f400c**. Before/afterHEAD/tree/status/exactdiff andall2293trackedinputhashes match. All2292otherblob/modepairs equalHEAD, includingmain94e36/YAMLb59d/docs90d8/workflow_tests66c292/manifests/lock/config/native/release/actions.

Wholeproductionprefixbeforecfg(test) is unchanged **28536bytes/SHA2561233773bf939f15216e20e61e42e55c4676edd75aa316baa2240852f0379c9e7**. Onlytestmodulechanged. Existingapproved316be4c4bb81d4f6e025016b12374836b4dcc1a8/separate0e6008a1f46679a86aba85ac513beb05af94b0c0/Design8/R04R05/A04A05/twoexistingclaimsintentunchanged. No newapproval/modelstage.

## Actual failure → narrow correction

OldCI41317523731/a1/sourceSc2e6a277…/job124880650882 genuinelyfailed `ci::tests::git_path_names_are_nul_delimited_and_extra_paths_force_full` atoldci.rs1246:WindowsInvalidFilename123creatingnewlinefilename **before classifier assertion**. Closed53e4a65a…/failedZIP13088199023ddd1c3a3… remainbyteexact, notexpected/ignored/flakeorretrocleared.

Actualtestdiff now:

1. Existing **allOSrealGitextra-pathcase** usesWindows-valid`unrelated-extra.txt`, stillassertingexactFullchanged-path-set. No cfg-disabling orignore.
2. Added **cfg(unix)realGitnewlinecase** `git_newline_path_names_remain_nul_delimited`, retainingrealnewlinefilesystemcoverage andsameFulloracle.
3. Existing **allOSrawNUL/newlineparser** body remainsbyte-identical andunguarded.

No productionclassifier/CLI/schema/trust/workflow/resourceconstantchange. Current103executedthese3affectedcasesandall14corecases, notjustsource-stringchecks.

## Independent executed evidence

Darwin25.6.0arm64/RustCargo1.96.0/nextest0.9.140; existingnative0190.19/OpenSSL3.6.2/deployment13.3/jobs2environment. Defaultisolatedtarget/noRUSTFLAGS/sharedtarget/features/profile/resourceoverride/dependencyinstall. OnlytemporaryprivateJUnitprofilefixturefix,retries0/thread1/native-tests.

| Actual executed check | Result |
| --- | --- |
| `cargo fmt --all --check` |PASS|
| Actualnextestinventory/native-tests/currentcompile |103discovered/0ignored; prior102+1Unixnewline|
| Genuinefullrelevantnextest103/retries0/thread1 |**103passed,0failed/errors/skipped**|
| `cargo clippy --offline --locked -p xtask --all-targets --all-features -- -D warnings` |PASS|
| Actual`check-workflows` |`workflow policy passed`|
| Actualordinary`release-gate` |`release gate passed`|

GenuineJUnit **xtask-junit.xml**, SHA256 **809c8d1142a3489976d463780e315b361d5491265d2c827398d2c0b50cb5b9a5**, actualrun **e3e5aba4-afa5-4857-a5c3-a7d88b8f2701**. Everycasename matches actualinventory; binary/compilerlog/worktree/package/profile/sourcehashbeforeafterproofretained. Zero compiler-warninglines acrossallrawlogs. No synthesizedXML/conversion/modelingest. **103rdtestisUnix-only,not103Windows passes.**

Priorf98fullworkspaceClippy/26realCLI+12actualaggregatebodyevidence remains applicable to unchangedproduction/allotherinputs; notrerunorrelabeled. Onefull103coversaffected3/core14/retainedconsumerregressions—no gratuitousduplicatefullsuite/newproduct/native/packagegate. Prior102XML279e…/run5cd13… andall349closedfiles remainhistorical/exact. Oldfailureall26filesalsoexact. Owneroriginalreport/result/manifestsand16rawfiles were read/hashed; interimrustfmtdiffpreserved, finalauthorDarwinpassnotcalledWindowsqualification.

## Pending and STOP

ActualcorrectedheadWindows/fullnormalCI remainsmandatoryafterseparatelyauthorizedcorrectioncommit/push. ThisDarwinpassdoesnotretroclearoldSc2e6Windowsfailure. No needwaitrestofoldrun/unretainednativearchivesbeforefixpush; COD108PRretentionFALSEunchanged/no newarchivegate.

Canonical944/source076/sameoriginal.scryer isnotcurrent108source. Both108claimsremainpending; no maps/ingest/fold/roottransition/hash-onlymodelpass. Correctedtestbody/newUnixcasearechangedtestinputs, notfalselyunchangedfingerprints. Priorproductionbodyproofalone grantsnomodelreconciliation.

Next: **SAMErequiredReviewerfocusedfixconfirmation**, thenEngineer/Productionseparatecommit/push/freshfullCIgrant. No fullreviewrestart/newapprovalstage, A04A05handoff/110/Done/merge/main/release/aggregateclaim.

OnlyauthorizedaffectedCargo/defaulttarget+ownedprivateevidence; no source/Git/API/model/tracking/wiki/settings/CIaction/oldrunread/watch/poll. No verifierconstantschanged ormemorywritewarranted. **STOPallCargo/execution/privatewritesafterclosedhandoff.** Machine`result.json`, exactcommands/rawlogs/current103JUnit/sourceprefix/input/binaryproof andclosed`files.json`adjacent.
