# COD-56 VM supplement — same-review follow-up

**Outcome: the inspected supplement plus provenance addendum faithfully supports its limited claims. No confirmed P0–P3 defect or unresolved provenance blocker.** It does not establish A09 or change the prior source verdict/endpoint authority. Production may preserve this evidence, with its limitations and addendum, on the same PR173.

## Binding and independence

Read-only evidence follow-up, not a new source review or delivery stage. PR173 source head `8359773eff395acc637ad2d221079de4fbf581e8` is coordinator-supplied; baseline `2173d766c155583b1e8f979c303b560998cc7214`. Inspected saved candidate/baseline bytes match the earlier reviewed bytes exactly: candidate blob `9890b016699948634200624e85b868616c14617e`, SHA256 `830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb`; baseline SHA256 `bdd6f0eace6fbd4ea25c9e09d2c6cb6c0b54a05955d024aa97d76443c58ae308`. Original `cod56-final-review.md` remains unchanged at SHA256 `56972838324112c8a481120509d4d03991f0d0818df652ea4c4387afac220662`.

Same FinalReviewer mapping/coordinator-reported accepted pair **gpt-6.1-sol/max**; required requested `agent_type: default`, `fork_turns: none`, fresh original review context. Runtime model/effort/agent/fork self-metadata remains unexposed. No dispatch, tests, guest commands, Git, tracking, model, source or evidence-file writes by this reviewer. Only this separate review report is authored; no constants changed. Existing approved316be4/approval0e6008/platform S05/R05/A09 authority is unchanged.

## Actual evidence and coverage

Inspected report, both manifests/addendum, core raw/derived JSON, exact harness/launch payloads, commands, failure logs, saved sources and retained Documents originals. Original90 manifest entries and all three addendum entries match actual hashes/lengths. Relevant memory was recalled; prior governing method/source review remains applicable.

- **Environment:** actual Windows11Pro10.0.26300 ARM64; native Windows PowerShell5.1.26100.9711, SYSTEM. Actual signed System32 `where.exe` is ARM64 PE0xAA64. No native Windows x86_64 consumer environment was established.
- **Parser:** baseline/candidate full-source ParseInput each reports zero errors under native5.1. These are two unique inputs; repeated setup data is not extra coverage.
- **Native5.1 function mechanism:** `12` contains four Continue cases; distinct `18` contains four Stop+StrictMode Latest cases. Decoded payloads begin with the exact unmodified source AST function, with matching source/function hashes and staged harness bytes. Each set records both variants accepting `where.exe /?` exit0; baseline accepts the impossible-name exit1; candidate catches the exact nonzero diagnostic with native exit1. This demonstrates the changed branch with a real checksum-valid ARM64 executable. Harness exit0 reflects its catch/report wrapper, not full-installer process success or failure. No PS7 probe-branch mechanism is claimed.
- **Emulated shell:** official PowerShell7.6.9 x64 ZIP bytes109620855 and SHA256 `ce7484804c0f0ad4dda10c958e71acdaf7eebfb22505c1b480f89e0e9f1da94d` match the inspected staged ZIP, release API asset digest and official hash manifest. Extraction log records pwsh PE0x8664, valid signature and hash `2a6769a3330dc1084d0154bfde7d164ab750ca7f700f313f9d417c1be78b2233`. `16` records actual guest-local child exit0, PS7.6.9, Process X64, OS Arm64. This is emulation, not native x86_64 qualification.
- **Intact host guard:** `17` contains exactly eight unique cases: two shells × baseline/candidate × DryRun/real. Each full-source memory ScriptBlock invocation exits1 with the unsupported-host diagnostic, empty stdout and both expected old sentinel hashes unchanged. Candidate accounts for four cases, baseline four. The unchanged guard rejects Arm64 even in the X64 process; neither native-product probe nor replacement path was reached. These are memory-invocation checks, **not `-File` contract evidence**.
- **Failures retained:** `11` File harness launch is blocked by the existing execution policy; `14` direct prlctl launch has an API error. Neither counts as runtime success. Subsequent guest-local child evidence is distinct. Earlier transfer/marker errors and wording corrections remain documented; no policy change, guard bypass or architectural support expansion is inferred.

## Provenance resolution

Root `12`/`18` JSON files are reserialized derived copies, not byte-exact guest originals. The addendum now states the UTF8-BOM/whitespace/newline conversion. Independently read `original-raw` copies match retained Documents originals byte-for-byte; their decoded JSON values exactly equal the derived copies:

| Report | Actual original bytes / SHA256 | Actual derived bytes / SHA256 |
| --- | --- | --- |
| 12 | 17853 / `639f0c7887e0ebc34d816a158bb07c9c72cefe1d4200980fa66f10200ff1ae1c` | 14625 / `922962b1b6862a2f35ec6986b922362cc001fc464a96056977c30a846158da0c` |
| 18 | 17997 / `6fdf2b79ac5ea662015e2ecf4dffb87e60b52502685344fa37de1d1d8f9feeae` | 14769 / `abf93b48b642c36b9f5542b4aecca32f4b87284721095246875d378f2f03f7f1` |

The earlier parent size/hash concern mixed representations and was withdrawn; no evidence mutation or rerun is established. No raw file was rewritten for this review.

Evidence root: `/Users/rabii/.prime/agent/session-artifacts/01a18f18-3db0-73ba-a45a-f25faf5369ad/sub-575ea352/cod56-parallels-arm64-supplement`.

- `report.md`: `e5f64a9d3dac31a210793f20be3774fe8c50e37e507b4010b3d290e91e44f1d1`.
- `manifest.json`: `4c2cc979f54d1c2625abbbae04d60f252c65cb3cfb1e0eec739bceea084e6bda`.
- `raw-provenance-addendum.md`: `2b084ea545f60dd20eae44b8a68420a9669ce1f180998a568b3b60a6525e5da9`.
- `raw-provenance-manifest.json`: `657c7f69aed1dec88261eb3a86f088947bb54c8a9365f2f7ba8fcb227a7a52a1`.

## Remaining limits / next owner

No full28 corrected A09 matrix, native Windows11 x86_64 two-shell qualification, PS7 probe branch, AMD64 controlled product fixtures, successful real install, final packaged-script/archive qualification, or hosted CI conclusion is supplied by this supplement. No synthetic JUnit/pass record was created. Unsupported-host rejection is expected behavior, not a defect. Source review remains valid within its original boundary; this follow-up adds no gate or approval. Engineer/Production preserve the bounded evidence and continue existing qualification obligations before COD56→107/A09/Done/main/release. Reusable-knowledge assessment: no new durable memory warranted from incomplete platform qualification; no memory writes.
