# COD-107 native retention YAML — Production handoff / STOP

Implemented **only `.github/workflows/ci.yml`** in `/private/tmp/codebasegraph-cod107-20261031`. No Gitstage/commit/push/PR/CIaction, Cargo/test execution, native.yml/backend/source/docs edit or dependency/settings change.

## Exact authority and snapshot

- HEAD **08d2c060bba8bf45db069fe53185f06f5b3d6cd9**, tree **09552870917f232f59005cd73e41c788b0163dca** unchanged.
- Approved retention method **e5adebfa1196a2074527cec4935522c43c773e39**, separate approval **08d2c060bba8bf45db069fe53185f06f5b3d6cd9**, user5b4b5aad-cbe8-4265-8db9-5ea351b7f386. Four method files plus approval byte/blob identities remain exact.
- Designer verbatim recipe SHA256 **d8556080b4b817522f12fed46e014b286ee368ce531f2f3ea6cdc7033fbbf64d** used. Existingbrief container-release-verifier/chg-fdm24r approved-retentionsectionSHA13833cd4…; source session bound only existingchg-xd2jtt. Rules/orient/semantic graph search+explicitchangeimpact read before editing; no model mutation/sign-off/fold/tracking write.
- Backend frozen working `crates/xtask/src/main.rs` **ec60404c430b36fce5619796b05cf0e9281ea24f04b2e5d7a513df2356091a4a**, `docs/release.md` **c4267010d8fa0746b39ab28e78f85ecc8659a4b18e5e8d5b25db81316867d931** remained exact. Did not read movingBackendtechnicalbodies or takeownership.

## Actual YAML

New ci.yml SHA256 **`3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9`** (old98f659e9…). Newjob **native_retention**, display “native artifact retention”, Ubuntu, exactlyoneunconditional Bashdecisionstep; output `retain` directly `${{ steps.decision.outputs.retain }}`. ExactscriptbodySHA256 **`1d55a7390065111b0b3641e67e02eb32a1b2454c35962156b33bf83863831758`**. Reads only standard mandatory GITHUB_*runnerinputs/eventJSON through jq, byte-case-sensitive branches/repositorynames/fournormalactivities; unknownvalidinputfalse, read/parse/nonobject/schema/multipleoutputs/mandatoryinput/outputwrite failure nonzero or no valid output per contract. No untrustedexpressioninterpolation, checkout/action/API/network/env/perms/timeout additions.

Native **needs only native_retention** and uploadinput **`${{ fromJSON(needs.native_retention.outputs.retain) }}`**. Retainfalse does NOT skip nativeverification. Allmatrix/sourceSHA/run-tests/retention90/reusableworkflow fields preserved. Requiredneeds exactsix **fmt,clippy,supply-chain,publish-dry-run,native,native_retention**; `always()` and completeexisting Bash/jqrequiredsteps remain byte/parsedidentical. No aggregate absent-keybehavior invented; exactdependency completeness comes from latertypedpolicytests.

`native.yml` unchanged SHA256 **cc4d668cb0dbc0d164e3f7a687f76f4020aeb5ef02b36eb56591521c75c3d683**. Existingtests→artifact/smoke→conditionalupload/pathnative-<target>/archive-sidecar-provenance/90daycaller and3daydiagnostics unchanged. Existingmainpush/publishsourceguard/triggers/permissions unchanged. No PRartifactpromotion/publicationauthority.

## Static checks actually performed

1. ExistingYAML syntax parsed with available PyYAML **BaseLoader** (preserves `on` key; orchestration-only parsing, not projectexecution). Selector object equals Designerrecipe except expectedfinalnewline introduced by actualYAMLliteral`|` versus Markdownfence; bodylogical lines exact. No runtimeeventmatrix executed.
2. Existinginstalled **`bash -n`** on extractedactualdecisionbody: **exit0**. No jq/event/actualbody execution or Cargo/test suite run.
3. `git diff --check -- .github/workflows/ci.yml`: **exit0**. Actualdiff onlynewcontroljob, nativeoneprerequisite/uploadbinding, requiredsixthdependency. Typedparsed checks verified identicalon/permissions/nativeotherfields/requiredexistingsteps.
4. Fivefrozenmethod/approvalfiles Gitblob+workingSHA256 and native/backendhashes recheckedunchanged. Currentstatus exactlydirtyci.yml plus prior2Backendfiles; nofourthfile.

Fieldanchors in proofJSON. No localpolicy/test/CI/artifactinventory/Windows/Scryerpass asserted. Backend now needs serialized actualYAML tests/guards per Designer section2. No constantschanged: native90/diagnostic3 preserved. A privateassertion initially compared Markdownrecipewithoutfinalnewline to actualYAML`|` withnewline; correctedcomparison forknownscalarformat only—no sourcealternative/retry. No reusablememorywritewarranted.

## Next owner / stop

**Production releases ci.yml writer now and stops all ci.yml/private writes.** Engineer may assign Backendexclusive main.rs/docsreleasefollow-up on actualhash `3e76687feaff8021d1edc6e0d91b77444b2196e1430bd346bb232b87049cbda9` for typedwiring checks+exactBash/jqactualbodyboundary/mutant/aggregatefixtures. No generalexpressionparser/newclaim/publicAPI. Independentaffected/fullverification/review and freshfullhostedtwotargetretainedarchive/sidecar/provenanceinventory/sourcebinding remain required beforeacceptance. UserallapplicableCIgreen/exacthead-currentbase/nonbypass/serialownmain/release/closuregates remain; noPRmerge/Done/stageclaim.
