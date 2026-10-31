# COD-56 Parallels Windows supplement

Status: bounded supplementary verification complete. No supported-platform/A09 closure.

## Binding
- PR173 head8359773eff395acc637ad2d221079de4fbf581e8 (parent binding), base2173d766c155583b1e8f979c303b560998cc7214.
- Candidate SHA256 830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb; baseline bdd6f0eace6fbd4ea25c9e09d2c6cb6c0b54a05955d024aa97d76443c58ae308. Guest shared and local source hashes independently match in12 report;17 captures after-execution hashes. Host candidate remains unchanged.
- Approved COD103316be4c4bb81d4f6e025016b12374836b4dcc1a8 / approval0e6008a1f46679a86aba85ac513beb05af94b0c0; existing CBG-PLAT-01 S05/R05/A09 at8a410c7 unchanged. No model/tracking/source/Git writes.
- EvidenceVerifier map inherit/inherit; no explicit overrides, runtime self-metadata unexposed. Independent of Production56.

## Actual environment
Windows11Pro10.0.26300 ARM64, SYSTEM SID scope; native WinPS5.1.26100.9711 ARM64. Signed System32 where.exe PEAA64 SHA7cf3631ebf7d5d8cefafbbd41d1ba9261a07eae1eaf5d4258db3acae7b0ca705.
Official portable PowerShell7.6.9 x64 ZIP verified against official release API and official hashes.sha256 before extraction. ZIP109620855 bytes SHAce7484804c0f0ad4dda10c958e71acdaf7eebfb22505c1b480f89e0e9f1da94d. Actual pwsh.exe PE8664, Authenticode Valid, SHA2a6769a3330dc1084d0154bfde7d164ab750ca7f700f313f9d417c1be78b2233. Guest-local child exited0: ProcessArchitecture X64, OSArchitecture Arm64, PS7.6.9, effective policy RemoteSigned. This is emulation on ARM64, not native Windows x86_64 qualification.

## Executed inventory
- Full source Parser.ParseInput: baseline/candidate zero errors under actual native WinPS5.1 (2 unique parser inputs). Repeated parser data in18 is setup, not extra coverage.
- Exact AST Assert-Binary, signed real ARM64 where.exe, fresh native5.1 children: four Continue cases in12; four Stop+StrictMode Latest cases in18. In EACH set baseline/candidate positive /? accepted native0; baseline impossible filename accepted despite native1; candidate impossible filename threw exact name/argument/exit1. Caller captured actual LASTEXITCODE; no body rewrite or fake PE. Baseline behavior is expected reproduction, not postfix failure. Strict native stderr did not terminate this actual invocation before explicit check. Exact extracted function source substring/hash verified on host.
- Full script File path execution: one harness launch11 blocked by existing execution policy (SecurityError UnauthorizedAccess); installer File-mode/guard cases NOT counted passed.
- Exact FULL source memory ScriptBlock invocation with explicit owned SourceDir/BinDir, intact guard: 8 negative checks in17 (2 shells x baseline/candidate x DryRun/real). ALL reject unsupported ARM64 with unchanged supports-Windows-x86_64 message; both old destination sentinel hashes unchanged, exactly2 files/no temp destination write. Candidate count4, baseline diagnostic count4. No preflight/native product branch reached. ScriptBlock is explicitly NOT File contract evidence.
- Actual emulated shell readonly architecture/version probe:16 exit0. Full source memory guard still sees Arm64 and rejects; no AMD64 fixture/matrix needed or invented.

## Limits and recovery history
Not 28/28 A09; no native Windows x86_64 PS5.1/7 complete matrix, final archive or product --help/--version qualification. No PS7 branch-mechanism claims with ARM where. No policy/registry/PATH-global/profile/account/VM power/settings changes. Process PATH only restricted to System32; current directory owned temp. No product constants changed. Conservative transport caps8192 then2000 were private harness bounds, not asserted OS limits.
Original02/04 transfer errors preserved; user-authorized small chunks08 produced17 successes (2176 exact baseline prefix), failed part17 no write per parent reconciliation. Earlier FIRST-chunk wording was incorrect and corrected in chunk-log-reconciliation.json. Parent earlier hidden.prime mirror/hash proof was false and corrected; actual Documents marker10 verified same bytes/hash. Direct pwsh prlctl14 API error not runtime evidence;15 found no owned pwsh process; authorized alternative guest-local launch16 supplied actual result. No blind retry of unknown mutation.

## Artifacts and ownership
Raw commands: commands.json; parser/mechanism12, strict18, memory guards17, architecture16, official provenance and release metadata/manifests; original failed raw preserved.
Guest runroot retained NoCleaned: `C:\WINDOWS\TEMP\codebasegraph-cod56-2e9284b8-47f1-472a-9cdc-b6a24cc3d62f`. Historical partial root retained: `C:\WINDOWS\TEMP\codebasegraph-cod56-11023706-4804-4438-b044-e9768538a0c8`. Host Documents owned staging retained: `/Users/rabii/Documents/codebasegraph-cod56-vm-11023706-4804-4438-b044-e9768538a0c8`. No personal/real install destinations touched. No live command remains known. No further tests requested/needed for this bounded assignment.
Memory assessment: no durable memory warranted from incomplete platform qualification; no memory writes authorized. Next owner Engineer/FinalReviewer to review actual supplement, Production to persist unchanged on same PR173; A09 remains pending.
