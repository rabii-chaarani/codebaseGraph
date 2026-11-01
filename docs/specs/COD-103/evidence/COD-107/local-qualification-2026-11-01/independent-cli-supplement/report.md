# COD-107 — independent actual CLI coverage supplement

**18/18 actual CLI assertion cases passed.** This closes only the previously noted omitted local matrix cells. It does not rewrite the initial83JUnit report or complete A03/package/native/platform gates.

## Frozen executable/source binding

Existing actualCLI `/private/tmp/codebasegraph-cod107-20261031/target/debug/xtask`, SHA256 **`337d4a7a1996e9d909dcac1651f8b4b6d845ecedf5ac483b67911b11d2745277`**,5399288bytes, is the same debugtarget invoked by prior independent Cargo check-workflows/release-gate on frozene4d256. Actualxtask.d sourcepaths, priorcommands and sourcehashes are in `source-cli-provenance-before.json`; CLI/sourcehashes unchangedafterall18. No rebuild/sharedtarget/othercheckoutexecutable.

WorktreeHEAD08d2/tree095528 plus exact3dirtyfiles remain prior sourceidentity: main **`e4d256e451c03f447a4e374ed361d0d0b23d2898977d51953995fa307d896f1e`**, docs1e67f494…/ci3e76687…/read-onlynativecc4d668…. Allfullhashesbefore/afterretained. ParentexplicitlyauthorizedthisboundedactualCLI supplement, not a newsource/testframework or approval.

## Method and actual matrix

Pre-registered9scenarios×2modes=18. Each freshprivatefixture copies **exactly** the9supported paths in actual existing `release_gate_fixture` and adds a valid`docs/allowed.py`comment-onlyhelper. Baselinecompletegatepassed under ordinaryandconfirmed-productionmodes. Eachnegativehasonlyoneindependentmutation. ActualCLI childcwdpointsatthatfixture; no globalcwd/env/repositorychange.

- Fourboundarynegatives: `mydocs/helper.py`, `knowledge/helper.py`, `testing/helper.py`, `outside.py`; eachrejectedwithspecificactual`python-file-present`diagnosticwhilevaliddocshelperstayedallowed.
- PresentbutincompleteSECURITY retains`Reporting a Vulnerability`and`graph_query`, omits`--allow-remote`; actual`security-policy-incomplete`only, bothmodes.
- Actual validYAML steps (notcomments): `uses: actions/setup-python@v5`; `run: pip install -r docs/requirements.txt`; `run: python docs/allowed.py`, oneseparatefixtureeach. Existingrealworkflowcheckrejecteditscorrespondingexactforbiddenmarker inbothmodes. **These steps were parsed/checked, never executed**: noPython,pip,network,installorCIrun.
- Everycase records exactargv/childcwd/inputfileSHA256/trueexit/stdout/stderr/actualassertions. Negativesreturned1, expecteddiagnosticwaspresent, FAILlistcontainedonlythetargeteddiagnostic; noYAMLparsefailure, incompletefixture/version/confirmationfailure orfalsepositiveallowed-docsrejection masked the result.
- Confirmed-productionflagsare exactly existing fixture-supported `--production --confirm release-environment --confirm private-vulnerability-reporting`; noactualexternalproductionconfirmation supplied.

| ActualcaseID | ProcessExit | Assertionresult |
| --- | ---: | --- |
| `complete_fixture_baseline__ordinary` | 0 | pass |
| `complete_fixture_baseline__confirmed-production` | 0 | pass |
| `reject_mydocs_python__ordinary` | 1 | pass |
| `reject_mydocs_python__confirmed-production` | 1 | pass |
| `reject_knowledge_python__ordinary` | 1 | pass |
| `reject_knowledge_python__confirmed-production` | 1 | pass |
| `reject_testing_python__ordinary` | 1 | pass |
| `reject_testing_python__confirmed-production` | 1 | pass |
| `reject_outside_python__ordinary` | 1 | pass |
| `reject_outside_python__confirmed-production` | 1 | pass |
| `reject_incomplete_security__ordinary` | 1 | pass |
| `reject_incomplete_security__confirmed-production` | 1 | pass |
| `reject_actual_workflow_python_setup__ordinary` | 1 | pass |
| `reject_actual_workflow_python_setup__confirmed-production` | 1 | pass |
| `reject_actual_workflow_python_dependency__ordinary` | 1 | pass |
| `reject_actual_workflow_python_dependency__confirmed-production` | 1 | pass |
| `reject_actual_workflow_python_run__ordinary` | 1 | pass |
| `reject_actual_workflow_python_run__confirmed-production` | 1 | pass |

All18registeredIDs executedexactlyonce;0assertionfailures/errors/unrun. Case`request.json`/`result.json`andrawstdout/stderrretainedalongsideactualfixtureinputs. Completefixedinventory=`registered-case-plan.json`; results=`completed-results.json`. Nooracleloosening/retry.

## Separate prior evidence / preserved closed artifacts

Original independentreport SHA256 **`652f051ad71b6ca34f868d862154ad4f2df574177567c8588f655d7122005c18`**, result3632a36b… andmanifestb836d112… independentlyreverified **unchanged**. Its83genuineJUnitpasses/run83002165…/XMLSHA75671e56… and strictworkspace/alltargets/allfeaturesClippyexit0/0warnings stayseparate,notrerun. Its statedomissions remainaccuratehistory; **this newappendix**, not a rewrite, suppliesactualCLIcoverage.

**No syntheticJUnit:** these18are rawCLIassertioncases, not18additionalJUnitresults or101JUnit/uniquetests. No Cargo/testsuite/rebuild/modelingestion/sourcewriter invoked. Onlyownnewfixtures/receipts/reportwritten; no repository/source/Git/tracker/model/settings/VM/CI/dependencywrite. No productconstant/policychanged. EvidenceVerifierinherit/inherit,nooverride/substitution/dispatch; runtimemetadataunexposed. No reusablememorywarranted/authorizedfortransientcoverage.

## Remaining gates / STOP

Actualroot/wiki.crateinventories awaitreviewedcleancommittedsource+ProductionSTOP; noallow-dirty/packagehere. Actualsame-sourcebothnativearchive+sidecar+provenanceinventories awaitnecessaryrealCOD107PR/fullCI. FreshFinalReviewer/currentheadallhostedCI/sourcepayloadbinding/currentcanonical107modelreconciliationremain. ThisdoesnotassertfullA03/107→108/Done/main/release/aggregateclosure.

**Engineer/FinalReviewer next; allsupplementalCLI/privatewrites STOPafterhandoff.** Machine`result.json`, fixedplanandallrawcasesadjacent, closedhashmanifest`files.json`.
