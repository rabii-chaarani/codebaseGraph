$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-win-repro-b5aa2377'
$out=Join-Path $stage 'controlled-crlf-direct-test'
if (Test-Path $out) { throw 'New case directory already exists; refuse repeat' }
New-Item -ItemType Directory -Path $out | Out-Null
$src=Join-Path $env:TEMP 'codebasegraph-cod110-win-repro-b5aa2377'
$expected=Get-Content -LiteralPath (Join-Path $stage 'expected-snapshot.json') -Raw|ConvertFrom-Json
$sha=[Security.Cryptography.SHA256]::Create()
function Hash-Bytes([byte[]]$bytes) { return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() }
function File-Sha([string]$path) { return Hash-Bytes ([IO.File]::ReadAllBytes($path)) }
function Write-Json($value,[string]$name) { $value|ConvertTo-Json -Depth 16|Set-Content -LiteralPath (Join-Path $out $name) -Encoding UTF8 }
$env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-msvc'
$env:CARGO_NET_OFFLINE='true'
$env:GIT_OPTIONAL_LOCKS='0'
try {
  $left=@(Get-CimInstance Win32_Process|Where-Object {$_.ProcessId -in @(6360,1540) -or (($_.Name -in @('cargo.exe','rustc.exe','xtask-5e2fcd04c8daf5d1.exe','git.exe')) -and $_.CommandLine -like '*codebasegraph-cod110-win-repro-b5aa2377*')})
  if ($left.Count -ne 0) { throw 'Owned baseline processes not quiescent; no new case' }
  $seven=@()
  foreach($prop in $expected.seven_source_sha256.PSObject.Properties) {
    $hash=File-Sha (Join-Path $src $prop.Name)
    if($hash -ne $prop.Value) { throw "Seven-source mismatch $($prop.Name)" }
    $seven += [ordered]@{path=$prop.Name;sha256=$hash;matches=$true}
  }
  Write-Json $seven 'seven-source-before.json'
  $indexPath=Join-Path $src '.git\index'
  $indexBefore=File-Sha $indexPath
  $before=@(); $after=@()
  foreach($rel in $expected.six_metadata) {
    $path=Join-Path $src $rel
    $bytes=[IO.File]::ReadAllBytes($path)
    $hash=Hash-Bytes $bytes
    $expectedHash=$expected.tracked_file_sha256.$rel
    if($hash -ne $expectedHash) { throw "Metadata is not exact committed LF: $rel" }
    $lf=0; $cr=0
    foreach($b in $bytes) { if($b -eq 10){$lf++}; if($b -eq 13){$cr++} }
    if($cr -ne 0 -or $lf -eq 0){throw "Invalid baseline EOL $rel"}
    $before += [ordered]@{path=$rel;bytes=$bytes.Length;sha256=$hash;cr=$cr;lf=$lf;crlf=0}
  }
  Write-Json $before 'six-metadata-before-lf.json'
  foreach($row in $before) {
    $path=Join-Path $src $row.path
    $bytes=[IO.File]::ReadAllBytes($path)
    $converted=New-Object byte[] ($bytes.Length+$row.lf)
    $j=0
    foreach($b in $bytes) { if($b -eq 10){$converted[$j]=13;$j++};$converted[$j]=$b;$j++ }
    [IO.File]::WriteAllBytes($path,$converted)
    $actual=[IO.File]::ReadAllBytes($path)
    $lf=0;$cr=0;$crlf=0
    for($i=0;$i -lt $actual.Length;$i++){if($actual[$i] -eq 10){$lf++};if($actual[$i] -eq 13){$cr++;if($i+1 -lt $actual.Length -and $actual[$i+1] -eq 10){$crlf++}}}
    $roundtrip=New-Object byte[] ($actual.Length-$crlf)
    $j=0
    for($i=0;$i -lt $actual.Length;$i++){if($actual[$i] -eq 13 -and $i+1 -lt $actual.Length -and $actual[$i+1] -eq 10){continue};$roundtrip[$j]=$actual[$i];$j++}
    $roundtripHash=Hash-Bytes $roundtrip
    if($crlf -le 0 -or $crlf -ne $row.lf -or $lf -ne $crlf -or $cr -ne $crlf -or $roundtripHash -ne $row.sha256){throw "Controlled CRLF validation failed: $($row.path)"}
    $after += [ordered]@{path=$row.path;bytes=$actual.Length;sha256=(Hash-Bytes $actual);cr=$cr;lf=$lf;crlf=$crlf;bare_lf=($lf-$crlf);bare_cr=($cr-$crlf);lf_roundtrip_sha256=$roundtripHash;matches_committed_lf=$true}
  }
  Write-Json $after 'six-metadata-actual-crlf.json'
  $indexAfter=File-Sha $indexPath
  if($indexAfter -ne $indexBefore){throw 'Private Git index changed during byte conversion'}
  Write-Json ([ordered]@{before_sha256=$indexBefore;after_sha256=$indexAfter;unchanged=$true}) 'private-git-index.json'
  $binary=Join-Path $src 'target\native-tests\deps\xtask-5e2fcd04c8daf5d1.exe'
  $exe=[IO.File]::ReadAllBytes($binary);$peOffset=[BitConverter]::ToInt32($exe,60);$machine=[BitConverter]::ToUInt16($exe,$peOffset+4)
  if($machine -ne 0x8664){throw 'Expected compiled x64 MSVC test PE'}
  $binaryHash=Hash-Bytes $exe
  $arguments='--exact release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout --nocapture'
  $start=[ordered]@{source_commit=$expected.commit;source_tree=$expected.tree;source=$src;binary=$binary;binary_sha256=$binaryHash;binary_bytes=$exe.Length;pe_machine='0x8664 AMD64/x64';provenance='Existing binary compiled once by Cargo native-tests in preceding LF case (52.18 seconds); no build or Cargo in this case';arguments=$arguments;started_utc=[DateTime]::UtcNow.ToString('o');case='six metadata LF-to-CRLF only; every LF roundtrip SHA equals committed bytes';invocation_count=1;retry_authorized=$false}
  Write-Json $start 'command-start.json'
  Write-Output ('CRLF_ACTUAL_START '+($start|ConvertTo-Json -Compress))
  $psi=New-Object Diagnostics.ProcessStartInfo
  $psi.FileName=$binary;$psi.Arguments=$arguments;$psi.WorkingDirectory=$src;$psi.UseShellExecute=$false;$psi.CreateNoWindow=$true;$psi.RedirectStandardOutput=$true;$psi.RedirectStandardError=$true
  $proc=New-Object Diagnostics.Process
  $proc.StartInfo=$psi
  if(-not $proc.Start()){throw 'Failed direct test launch'}
  $stdoutTask=$proc.StandardOutput.ReadToEndAsync();$stderrTask=$proc.StandardError.ReadToEndAsync()
  $proc.WaitForExit()
  $stdout=$stdoutTask.Result;$stderr=$stderrTask.Result;$code=$proc.ExitCode
  [IO.File]::WriteAllText((Join-Path $out 'test.stdout.txt'),$stdout,(New-Object Text.UTF8Encoding($false)))
  [IO.File]::WriteAllText((Join-Path $out 'test.stderr.txt'),$stderr,(New-Object Text.UTF8Encoding($false)))
  $result=[ordered]@{exit_code=$code;process_id=$proc.Id;has_exited=$proc.HasExited;ended_utc=[DateTime]::UtcNow.ToString('o');invocation_count=1;binary_sha256=$binaryHash;private_git_index_after_test_sha256=(File-Sha $indexPath);index_unchanged=((File-Sha $indexPath) -eq $indexBefore)}
  Write-Json $result 'command-result.json'
  $proc.Dispose()
  $remaining=@(Get-CimInstance Win32_Process|Where-Object {($_.Name -in @('cargo.exe','rustc.exe','xtask-5e2fcd04c8daf5d1.exe','git.exe')) -and $_.CommandLine -like '*codebasegraph-cod110-win-repro-b5aa2377*'})
  Write-Json ([ordered]@{observed_utc=[DateTime]::UtcNow.ToString('o');remaining_test_build_git=@($remaining|Select-Object ProcessId,ParentProcessId,Name,CommandLine);quiescent=($remaining.Count -eq 0);no_further_execution=$true}) 'post-test-quiescence.json'
  Write-Output ('CRLF_ACTUAL_END '+($result|ConvertTo-Json -Compress));Write-Output $stdout;Write-Output $stderr
  exit $code
} catch {
  Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();ended_utc=[DateTime]::UtcNow.ToString('o');test_started=(Test-Path(Join-Path $out 'command-start.json'));retry=$false}) 'failure.json'
  Write-Error $_ -ErrorAction Continue
  exit 125
}
