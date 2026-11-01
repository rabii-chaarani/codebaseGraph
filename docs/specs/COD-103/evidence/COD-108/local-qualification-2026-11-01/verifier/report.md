# COD-108 — independent local acceptance evidence

**PASS for the assigned local verification.** All102 genuine tests, full strict workspaceClippy, formatting and actual policy commands pass with **zero compiler warnings**. The current implementation is ready for fresh integrated Final Reviewer review. Model reconciliation and hosted qualification remain pending at their proper stages.

## Scope, approval and exact source

Existing **C103-R04/R05→A04/A05**, approved five-contract snapshot **316be4c4bb81d4f6e025016b12374836b4dcc1a8**, separate approval **0e6008a1f46679a86aba85ac513beb05af94b0c0**, `docs/specs/COD-103/approvals/2026-10-31-revision-0.3.md`, actual user “i already approved revision0.3” /decisionc8690556-e836-428d-bc81-079c701d2395. Relevant R04/R05 and Design§8.1–8.4 are byte-identical to that approved snapshot. Overall current spec/verification/delivery also contain authorized earlier COD56/COD107 method/evidence addenda; **not all five whole current documents are claimed byte-identical316**. Immutable approved readbacks/deltas retained; no approval link advanced.

Existing intent `chg-9xdkd4`, claims **resp-1rnbs9 /resp-tddbkj**, execution brief`chg-cw9n3h`. Authoritative ReleaseVerifier PLAN whole-description SHA **5f5a015b14521fc18f684e3c94f38c9723a01c59e855957376835032772c362d**. Preserved76304-characterprefix hashes475d7888…; exactappendix includes2leadingnewlines andhashes **cca688a71bdbc2fab94fb426ceee5879b12527f10e6c493af75694a374e9cf87**. Heading-to-end starts76306 andhashesc607cbf5…; these are different byte ranges, not conflicting content/approval.

Worktree`/private/tmp/codebasegraph-cod108-20261101`, branch`cod103-stack/cod-108`, baselineHEAD **90af76d3453a6f5cc0f032045ed48c4fd09c275c**, tree **cd05bfb5ebdb9652dfa8b6af90ce867c733667bb**. Before/afteridentity and exact4filehashes match:

| Actualdirtyfile | Bytes | SHA256 |
| --- | ---: | --- |
| NEW`crates/xtask/src/ci.rs` |48671|`b8971114c0ed2d79675b94cfb41133482c185c5ee8609a4282ed565c9136967f`|
| `crates/xtask/src/main.rs` |235135|`94e36cb3ceed27f45b552e42c9d6a52aa7cfef8dd70876f6e6ab4e562ec8891d`|
| `.github/workflows/ci.yml` |21095|`b59d7028101120f6885870cec054a65d9788f55d364dc0f2eb2b53bd4fd9c339`|
| `docs/release.md` |22125|`90d84f6614be6cd74b8340dd73b1a2c1000457af319475cf8939b1c316ebf486`|

All2014 other baseline trackedfile/link mode+blob pairs unchanged. Manifests/lock/dependencies/profiles/native_tests/native.yml/release.yml/actions/config unchanged. `release/workflow_tests.rs` stays **66c2921315dae1dad20a1cfd4eed620019d702da6c0e7e6ec3c1c43ff8ab0450**; actual CI-body tests are in the authorizedmain.rs, not a required forcedsplit. Frozen sourcecopies/diff/fullinputguard retained privately; no verifier source edits.

## Actual independent execution

Darwin25.6.0arm64, Rust/Cargo1.96.0, nextest0.9.140. Existing native0190.19/OpenSSL3.6.2/deployment13.3/jobs2 environment, defaultisolatedtarget. No customRUSTFLAGS/sharedtarget/features/profile/resource changes or dependency installs.

| Executed check | Actual result |
| --- | --- |
| `cargo fmt --all --check` |PASS|
| `cargo nextest list --offline --locked -p xtask --bin xtask --cargo-profile native-tests …` |Currentworktreecompile,102discovered,0ignored|
| Genuine`cargo nextest run … --profile cod108 --test-threads 1 --no-tests fail` |**102passed/0failed/0errors/0skipped**,0retries|
| `cargo clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` |PASS|
| Actual`cargo run --offline --locked -p xtask -- check-workflows` |`workflow policy passed`|
| Actual`cargo run --offline --locked -p xtask -- release-gate` |`release gate passed`|

GenuineJUnit `xtask-junit.xml` SHA **279e267e4e1368dbca2050cd32671bfa5eab2fc6a06321eacdbfc453d4d64e87**, run **5cd13f30-8219-4810-b8e1-623fbf0e9b94**. Everyactualcasename matches discovery. Currenttestbinary/compilerlog/packagepath/dependencyrecords and source-bound normalCLI binary hashes retained before/after. All raw logs have0compilerwarnings; no newCargo-audit stage or advisory waiver. **No syntheticXML/conversion/modelingest.**

### R04 semantic core and real-process proof

Thirteen current`ci::tests` cases execute complete PR165/167-shapedpositives and forbidden-field/context/history/external-lock/edge/version/path/mode/ancestry/acquisition cases, including realGit symlink/submodule/add/delete and NUL/newline rawdiff checks.

An independent **26-case realCLI supplement** uses CURRENTactualsixmetadatafiles in OWNprivate immutableGitobjects, never projectrefs. Actualnormal`xtask ci-mode` returns lightweight for complete patch+majorbumps; returns validfull for externalversion/source/checksum/edge, arbitraryfield/dependencypath, duplicateJSON/version, historyedit, partial/mismatchedmetadata, decrease/leadingzero, executable/symlink/rename/newline-shelllikepath, staleancestry/missingobject/repo; invalidSHAs/repeated/unknownarguments failnonzero/noJSON. Exactargv/cwd/status/stdout/stderr/blob/tree/commitidentities retained. Testversions4.0.6/5.0.0 are fixtureinputs, **not actualreleasechoice**; shelllikepath/notes neverexecute.

Existing assignedcore bounds remain **2,097,152bytes per rawdiff and perblob** (`ci.rs`15–16): boundedmax+1 acquisition failsclosedfull. No guard raised/removed or resourcepolicy waived; no generalparser/framework added by verification.

### R04/R05 actual workflow consumers, not just strings

Current102JUnit executes **actual YAML identity, decision and required Bash/jq bodies** through closedfixtures. Unknowngh/git/cargocalls fail; success asserts APIcallorder and exact fetchargv, classifierargv/cwd in protectedB`trusted-base`, separateHobjectstore and validsemanticresponse binding. Trustedcandidate/fullfallback/failedB-or-Hacquisition/invalidhelper output preserve freshvalidfullrecords. These are localclosedstub API/Cargo responses, **not hostedGitHub scheduling/provenance**; realRustclassifier semantics are separately exercised above.

Actualrequired full+trustedlight positives and missing/extra/schema/mode/reason/common/heavy/cancelled/skip/staleB/H/trust/label/API failures execute; intentionalheavy skips are accepted only for positivecurrentlight. An independent **12-case extracted-current-required-body supplement** proves main-fullpositive/noPRAPI, main-lightnegative, eachof8missingjobkeys, malformedresult andcancelledheavy reject. Strictunknownsubprocessstubs log0calls. The26+12supplementcases are rawprocess evidence, **not38newJUnit/uniqueprojecttests**.

TypedYAML mutations and static readback preserve the exact8requiredneeds; classify/fmt/supply-chain/repository-policy/native_retention mandatory; heavyclippy/package/native full-only; singlecommonreleasegate+xtasktestowner; literalmain+COD105/106/56/107/108bases and6activities; main-onlypush,readonly/secret-free/protectedBcheckouts/isolatedtrustedcompileroutput. Existing107actualretentionbody/negative/invalid/wiringtests pass; native90days/diagnostics3/sourcegithub.sha/native buildrecipe remainunchanged. No extra native/archive/productruntime suite needed for this CI-only localphase.

## Model, history and remaining stage gates

Canonicalroot remains **944/source076/sameoriginal.scryer**, **not** current108main94e36/coreb897. Existing rules/health/radius/claims were read; actualmodel/testresults/planhashes unchanged. Both108claims staypending; no map/attachment/ingest/fold/roottransition/globaldriftclear. **Do not treat oldcanonical12claims as108passes or ingest this report againstold944.** Engineer later owns legitimatecorrectsourcebinding and separatelyassigned proportionatereconciliation, not modelcopy/rebuildfallback.

All63 authorhandoff entries(2report/result+61rawlogs) were hash/size/readbackchecked. Earlierauthorfixture/oraclefailures remainhistorical; currentindependent102pass binds latest4hashes, not an authorsummary or rewrittenprovenance. PriorCOD107/oldwarning/actual944crate history untouched.

- Next: **fresh required Sol/MAX/default/nonfork integrated Final Reviewer** at these actual4hashes; then separatelyauthorized Productionevidence/sourcecommit/own108→107PR.
- Own108PR stays **FULL/bootstrap**. Freshmainretarget and ownmerge-SHAfullmainCI stayfull. No unbornhostedrun is imposed as a prepushgate.
- Actualtrustedgeneratedlightweight/commonchecks/heavyskipped plus mixed/untrustedfull hostedrollout is due **after protected-main deployment, before finalreleasePRmerge**; localfixtures do notreplaceit. Preserve finalaffectedrollout afterCOD110sharedchanges atitsauthorizedstage. COD110isnotactive.
- SupportedWindows/fullnativecurrent108qualification and allapplicableexacthead/baseCI/non-bypassstrictfreshness/serialown-mainCI/fullqueuerelease/publicindependentverification/aggregateowners unchanged.

Livegraphsemanticsearch+explicitcontext discoveredmain/module/currentYAML; canonical944graphwasorientationonly, no rebinding/rebuild. Newscope memory/wiki consulted; memoryadvisory. Linearconnectionverificationpending/unavailable; no update/settingsbypass claimed. No source/Git/projectrefs/API/CI/tracker/wiki/model mutation. Onlyownedprivateevidence/fixtures plusauthorizedCargo targetoutputs. No constantschanged byverifier. No durablememorywritewarranted for transientrun-specificevidence.

**STOPallCargo/execution/privatewritesafterhandoff. NoA04/A05aggregateclosure/nextissue/Done/push/merge/release permission.** Machine`result.json`, exactcommands/rawstdoutstderr/genuineJUnit/acceptancetrace/sourcebinaryproof and fullhash`files.json` adjacent.
