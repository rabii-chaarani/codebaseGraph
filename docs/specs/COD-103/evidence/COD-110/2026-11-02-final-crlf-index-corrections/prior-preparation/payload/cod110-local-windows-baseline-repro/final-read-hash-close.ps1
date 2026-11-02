$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-win-repro-b5aa2377'
$out=Join-Path $stage 'final-closure'
if(Test-Path $out){throw 'Closure artifacts already exist'}
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'codebasegraph-cod110-win-repro-b5aa2377'
$expected=Get-Content -LiteralPath(Join-Path $stage 'expected-snapshot.json') -Raw|ConvertFrom-Json
$crlf=Get-Content -LiteralPath(Join-Path $stage 'controlled-crlf-direct-test\six-metadata-actual-crlf.json') -Raw|ConvertFrom-Json
$sha=[Security.Cryptography.SHA256]::Create()
function File-Sha([string]$path){$long=if($path.StartsWith('\\')){'\\?\UNC\'+$path.Substring(2)}else{'\\?\'+$path};$s=[IO.File]::OpenRead($long);try{return([BitConverter]::ToString($sha.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$s.Dispose()}}
$rows=@();$unexpected=@()
foreach($prop in $expected.tracked_file_sha256.PSObject.Properties){$h=File-Sha(Join-Path $src $prop.Name);$desired=$prop.Value;$kind='exact-committed';if($expected.six_metadata -contains $prop.Name){$desired=($crlf|Where-Object {$_.path -eq $prop.Name}).sha256;$kind='authorized-six-metadata-crlf'};$matches=$h -eq $desired;$rows += [ordered]@{path=$prop.Name;sha256=$h;expected=$desired;comparison=$kind;matches=$matches};if(-not $matches){$unexpected+=$prop.Name}}
$rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath(Join-Path $out 'all-2696-source-hashes-after.json') -Encoding UTF8
$binary=Join-Path $src 'target\native-tests\deps\xtask-5e2fcd04c8daf5d1.exe'
$binaryHash=File-Sha $binary
[IO.File]::Copy($binary,(Join-Path $out 'xtask-5e2fcd04c8daf5d1.exe'),$false)
$left=@(Get-CimInstance Win32_Process|Where-Object {$_.ProcessId -in @(6360,1540,6112) -or (($_.Name -in @('cargo.exe','rustc.exe','xtask-5e2fcd04c8daf5d1.exe','git.exe')) -and $_.CommandLine -like '*codebasegraph-cod110-win-repro-b5aa2377*')})
$closure=[ordered]@{observed_utc=[DateTime]::UtcNow.ToString('o');source_commit=$expected.commit;source_tree=$expected.tree;verified_tracked_files=$rows.Count;unchanged_files=($rows.Count-6);authorized_crlf_only_files=6;unexpected_changes=$unexpected;binary_sha256=$binaryHash;private_git_index_sha256=(File-Sha(Join-Path $src '.git\index'));remaining_cargo_test_git_original_wrapper=@($left|Select-Object ProcessId,ParentProcessId,Name,CommandLine);quiescent=($left.Count -eq 0);scope='Final read/hash/export only. No further guest actions authorized after return. Isolated source and target preserved; staging lease released.'}
$closure|ConvertTo-Json -Depth 8|Set-Content -LiteralPath(Join-Path $out 'closure.json') -Encoding UTF8
$closure|ConvertTo-Json -Depth 8
if($unexpected.Count -ne 0 -or $left.Count -ne 0){exit 125}
exit 0
