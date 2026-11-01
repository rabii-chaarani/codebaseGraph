# COD-107 independent available local verification

**Executed local checks PASS. A03 is NOT complete.** No wholeproduct/platform/archive/107→108 qualification is claimed.

## Exact source and authority

Worktree `/private/tmp/codebasegraph-cod107-20261031`, branch`cod103-stack/cod-107`, HEAD **`08d2c060bba8bf45db069fe53185f06f5b3d6cd9`**, tree **`09552870917f232f59005cd73e41c788b0163dca`**, plus **exactthree dirtyfiles** ci.yml/main.rs/docsrelease. Baselinef69; HEADaloneisnot testedsource. Allassigned/read-onlyhashes/status/HEAD/tree unchangedafterexecution; diffcheckpasses.

Originalapproval316be/0e6008 anddesign§6SHA8ae7be05… retain C103-R03/A03. User-approvedretentionrev0.5freeze`e5adebfa1196a2074527cec4935522c43c773e39`, approval`08d2c060bba8bf45db069fe53185f06f5b3d6cd9`, decision5b4b5aad-cbe8-4265-8db9-5ea351b7f386. CurrentcanonicalcarrierfullSHA475d7888…/Designercontractd8556080… read; existingchg-xd2jttboundonly, **nomodel/maps/ingestion/fold**. EvidenceVerifierinherit/inherit, originalaccepteddispatch/nooverride; runtime metadataunexposed.

| Input | Before/afterSHA256 |
| --- | --- |
| `crates/xtask/src/main.rs` | `e4d256e451c03f447a4e374ed361d0d0b23d2898977d51953995fa307d896f1e` |
| `docs/release.md` | `1e67f4949647c10a48f9184db230454572d666a756b68c203690398f525c7954` |
| `.github/workflows/ci.yml` | `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9` |
| `.github/workflows/native.yml` | `cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683` |

## Historical pre-fix preservation

Initialindependentformat47558passed; staticinspectionconfirmedunguardedprivate retention_pr_event hadall13callsinsideUnix-onlytest. No compile/testhadstarted. CargoSTOPtransferredtooriginalowner; only`#[cfg(unix)]`added. Independentbyte-reversalexactlyreconstructspre-fix47558. Finalmain **e4d256…** nowtested; originalformat/staticreceiptkeptseparate, notrelabeled. Fixhandoff81321ed9… read. Noverifierfix/lintsuppression/Windowscompilerpassclaim.

## Actual independent execution

| Check | Exit | Walltime |
| --- | ---: | --- |
| `format-final` | 0 | 2.105s |
| `xtask-inventory` | 0 | finaldurationunavailable |
| `independent-full-xtask` | 0 | 30.917s |
| `check-workflows` | 0 | 1.017s |
| `ordinary-release-gate` | 0 | 0.469s |
| `strict-workspace-clippy` | 0 | 68.898s |
| `final-whitespace-source-identity` | 0 | 0.086s |

Exactfullcommands/environment/outputs in `post-cfg-final/command-records.json`; fullrawlogsretained. Allsourceguardedafterfix; nocancel/retry/zero-testpass.

**83/83actualnextestcasespassed**,0failures/errors/skips, retries0/one thread, Cargo**native-tests**, run **`83002165-99cd-489a-863c-4ca3ba24d0c2`**. GenuineJUnit **`post-cfg-final/xtask-junit.xml`**, SHA256 **`75671e56886ac4caf87c69aa564fb48767073efbfbd4dd21a1bdeb4253d9b4ae`**. All83namesexactmatchactualinventory; all7newdocs/retentiontests actuallyran. Namedtestcountisnotthenumberofgroupedfixtures/subcases.

ActualCargo inventory/package/compilelogbindthisworktreeandnative-testsexecutable; binarySHA256 **`6af0feb90ae1398cf32fcbc668e13d1236123d33341492f6e593e2688f6a24f1`** unchangedafterexecution. Rootprofileoptimized/assertions/overflow/unwind preserved; nofeaturesoverride/sharedtarget/othercheckoutexecutablereuse. `xtask-binary-provenance.json`/inventory/rawbuildlogretainproof.

FullworkspacealltargetsallfeaturesClippy **-Dwarningspassed**, **0warningdiagnostics** inalllocalexecutionlogs. Actualcheck-workflowsandordinaryrelease-gatepassed. This isnotWindows-host/fullproductruntimequalification.

## Criterion → realtest → whatactuallyran

- `docs_python_gate_accepts_root_and_nested_helpers_in_both_modes`: completecopiedgatefixture, actualrelease_gate(args) inisolatedchildCWD; docs/helper.py+nestedhelper acceptedordinaryandconfirmed-productionroutes; generateddirsroot+nestedexcluded. Fixtureconfirmflagsarenotliveexternalattestations.
- `docs_python_gate_rejects_neighbors_root_tooling_and_workflow_python`: allowedhelperdoesnotexemptdocs-extra/srcdocs/root/scripts/legacyPython,pyproject/requirementsdev,missingSECURITY andappendedpython-mworkflowmarker; bothroutesassertdiagnosticsandallowedhelpernotrejected.
- `docs_python_exception_rejects_component_aliases_and_symlinks`: actualpathalias/missingmetadatafailureandrealboth-gateroutesfordocsfile→outside/outsidefile→docs/symlinkdocsdironcapableMac. Windowsdrive/symlinkprivilegebranchesnotexecutedhere.
- `workflow_policy_retention_wiring_is_closed`: actualparsedYAMLpositive+typedwiringmutants, includingexact6needs/output/directfromJSON/source/targets/run-tests/Booleanupload/retention/nativebeforeartifactconstraints.
- `native_retention_actual_body_is_cod107_only`: actualci.ymlBash/jqexecutedinisolatedclearedchildenv;9truepositives,registeredfalsecases,3semanticmutants andinput/outputfailures; nocopiedRustdecisionoracle.
- `native_retention_invalid_input_cannot_succeed`: actualmalformed/nonobject/multipleJSONbodyfailureswithnovalidoutput.
- `required_rejects_retention_selector_failure`: actualunchangedrequiredBashbodyrejectspresentfailure/cancelled/skipped/missingresult; allsuccessinclretainfalsepasses. **Entirelyabsentselectorkeysucceedsasunchangedbodydoes**; exactsixdependencytypedstaticnegativeiscompleteness, noruntimeabsencepromiseinvented.

### Explicit coverage gaps — not hidden behind83passes

Actualdocsgatefixture**doesnotexecute** explicitmydocs/knowledge/testing/directoutside.pynegatives, incomplete(rathermissing)SECURITY, oractualworkflowsetup/dependencymarkers(ratheranappendedpython-mcomment). Nofailedtest/productregressionisprovedbythoseomissions, buttheyremainunexecutedmatrixcellsagainstthecanonicalbrief. Parentnotified; owner/acceptancetracemustresolvebeforefullA03assertion. Parser/helper/sourceinspectiondoesnotsubstituteforactualmissingcases. Noverifierassertionweakening/sourcefix.

WindowsBash/jq/hostwarningexecutionandWindowsdrive/symlinkprivilegeaccountingremainlaterhostedchecks; Unixcfgexclusionisnotpass. OnWindowshelpernowcfgmatchescaller, butnoWindowscompilewasrunhere. Testsourcehasexplicitprivilege-unavailablereturn, sofuturetestaggregatepasscannotbeclaimedasymlinkpasswithoutbranch/loginspection.

## Environment and preserved gates

macOS26.6.2/build25G83ARM64; Rust/Cargo1.96.0,nextest0.9.140,Bash3.2.57,jq1.7.1. Existingactualnative019staticlib+bothheadersindependentlyrehashedtoverified0.19.0identities. DocumentedLBUGpaths/version,developerOpenSSL3.6.2,deployment13.3,existingjobs2resourceenv/defaultisolatedtarget/projectserialtests+target/test-home retained. Noinstall/cachecopy/sourcefallback/customRustflags/LBUG_SHARED/profile/featureschange. Inventorycompletioncrossedkernelrestore: exit0actualnotification/inventoryretained, finaldurationunavailable,norepeatfortiming.

**Not run:** root/wiki.cratepackagingwhilecurrentthreefilesdirty; noallow-dirty/fakeclean. Reviewedtechnical/evidencecommit+ProductionSTOPthenaseparateactualcleancrateinventoryslotremains. Necessaryreal107PR/fullCIthenbothretainedsame-sourcearchive+sidecar+provenance/memberreadbackremainsA03acceptance; nooldv405/crosscompiled/syntheticinventorysubstitute. No productnativebuild/VM/fullruntimecohort/CIactionhere.

FreshintegratedFinalReviewer/focusedfixacceptance,currentheadfullhostedCI/allapplicableworkflows, actualcrate+nativepayloadinventories/samecanonicalmodelcurrent-sourceevidencereconciliationremain. Currentcanonicalmodelrootf69isnotfuture107; nomaps/ingest/fold. No107→108/Done/main/release/aggregateclosure.

Onlyownprivateconfig/reports/JUnitandnormalisolatedbuild/testoutputswritten; nosource/docs/tests/Gitrefs/tracker/wiki/model/CI/settings/credentials/dependencywrites. No productconstant/policy/oraclechanged; knownjobs2envresourcechoiceisnotnewpolicy. Reusableknowledgeassessment: transientexecutionevidence,no memorywritewarranted/authorized.

**Nextowner:** Engineerdisposesprecisecoveragelimitsandassignsfreshreview/Productioncommitordering; laterpayloadslotsremain. **Source/Cargo/reportwritesSTOPPED; exclusiveCargoleasereleasedafterhandoff.** Machine`result.json`; closedfilehashmanifest`files.json`; historicalprefixreceiptsretained.
