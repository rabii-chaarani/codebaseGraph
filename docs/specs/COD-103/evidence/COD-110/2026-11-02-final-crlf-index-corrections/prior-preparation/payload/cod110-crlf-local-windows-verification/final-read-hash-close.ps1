$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-crlf-fixed-b5aa2377'
$out=Join-Path $stage 'final-closure'
if(Test-Path $out){throw 'Closure exists; refuse repeat'}
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'c110-fix-b5aa2377'
$expected=Get-Content -LiteralPath(Join-Path $stage 'expected-snapshot.json') -Raw|ConvertFrom-Json
$crlf=Get-Content -LiteralPath(Join-Path $stage 'controlled-crlf-tests\six-metadata-actual-crlf.json') -Raw|ConvertFrom-Json
$sha=[Security.Cryptography.SHA256]::Create()
function File-Sha([string]$path){$long=if($path.StartsWith('\\')){'\\?\UNC\'+$path.Substring(2)}else{'\\?\'+$path};$s=[IO.File]::OpenRead($long);try{return([BitConverter]::ToString($sha.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$s.Dispose()}}
function Write-Json($v,[string]$name){$v|ConvertTo-Json -Depth 16|Set-Content -LiteralPath(Join-Path $out $name) -Encoding UTF8}

try{
 $rows=@();$unexpected=@()
 foreach($p in $expected.tracked_file_sha256.PSObject.Properties){$desired=$p.Value;$type='exact-corrected-snapshot';if($expected.six_metadata -contains $p.Name){$desired=($crlf|Where-Object {$_.path -eq $p.Name}).sha256;$type='authorized-crlf-metadata'};$h=File-Sha(Join-Path $src $p.Name);$match=$h -eq $desired;$rows += [ordered]@{path=$p.Name;sha256=$h;expected=$desired;comparison=$type;matches=$match};if(-not $match){$unexpected+=$p.Name}}
 Write-Json $rows 'all-2696-source-after.json'
 $binaries=@();foreach($rel in @('target\native-tests\deps\xtask-5e2fcd04c8daf5d1.exe','target\native-tests\xtask.exe')){$p=Join-Path $src $rel;$h=File-Sha $p;$dest=Join-Path $out (Split-Path $p -Leaf);[IO.File]::Copy($p,$dest,$false);$binaries += [ordered]@{path=$p;sha256=$h;bytes=(Get-Item -LiteralPath $p).Length}}
 $all=@(Get-CimInstance Win32_Process)
 $left=@($all|Where-Object {($_.Name -in @('cargo.exe','rustc.exe','xtask.exe','xtask-5e2fcd04c8daf5d1.exe','git.exe')) -and $_.CommandLine -like '*c110-fix-b5aa2377*'})
 $oldWrappers=@($all|Where-Object {$_.Name -eq 'powershell.exe' -and ($_.CommandLine -like '*run-lf-full-suite.ps1*' -or $_.CommandLine -like '*run-controlled-crlf-tests.ps1*' -or $_.CommandLine -like '*run-production-cli.ps1*')})
 $closure=[ordered]@{observed_utc=[DateTime]::UtcNow.ToString('o');source_commit=$expected.commit;source_tree=$expected.tree;dirty_overlay=$expected.dirty_overlay;verified_tracked_files=$rows.Count;corrected_snapshot_exact_files=($rows.Count-6);authorized_crlf_files=6;unexpected_changes=$unexpected;binary_provenance=$binaries;private_index_sha256=(File-Sha(Join-Path $src '.git\index'));remaining_build_test_git=@($left|Select-Object ProcessId,ParentProcessId,Name,CommandLine);remaining_previous_wrappers=@($oldWrappers|Select-Object ProcessId,ParentProcessId,Name,CommandLine);quiescent=($left.Count -eq 0 -and $oldWrappers.Count -eq 0);staging_lease='Released after this final read/hash/export returns; source/target/CRLF preserved unchanged';no_further_guest_actions=$true}
 Write-Json $closure 'closure.json';$closure|ConvertTo-Json -Depth 12
 if($unexpected.Count -ne 0 -or $left.Count -ne 0 -or $oldWrappers.Count -ne 0){exit 125};exit 0
}catch{Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();no_retry=$true}) 'failure.json';Write-Error $_ -ErrorAction Continue;exit 125}
