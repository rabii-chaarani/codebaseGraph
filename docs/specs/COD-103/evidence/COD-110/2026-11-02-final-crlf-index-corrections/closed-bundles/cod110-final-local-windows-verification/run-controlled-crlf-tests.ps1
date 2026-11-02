$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-final-b5aa2377'
$out=Join-Path $stage 'controlled-crlf-tests'
if(Test-Path $out){throw 'New case exists; refuse repeat'}
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'c110-final-b5aa2377'
$expected=Get-Content -LiteralPath(Join-Path $stage 'expected-snapshot.json') -Raw|ConvertFrom-Json
$env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-msvc';$env:CARGO_NET_OFFLINE='true';$env:GIT_OPTIONAL_LOCKS='0';$env:CARGO_TARGET_DIR=Join-Path $src 'target'
$toolbin='C:\Users\rabii\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin';$env:PATH=$toolbin+';'+$env:PATH
$sha=[Security.Cryptography.SHA256]::Create()
function File-Sha([string]$path){$long=if($path.StartsWith('\\')){'\\?\UNC\'+$path.Substring(2)}else{'\\?\'+$path};$s=[IO.File]::OpenRead($long);try{return([BitConverter]::ToString($sha.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$s.Dispose()}}
function Write-Json($v,[string]$name){$v|ConvertTo-Json -Depth 16|Set-Content -LiteralPath(Join-Path $out $name) -Encoding UTF8}
function Native-Run([string]$file,[string]$arguments,[string]$prefix){
 $psi=New-Object Diagnostics.ProcessStartInfo;$psi.FileName=$file;$psi.Arguments=$arguments;$psi.WorkingDirectory=$src;$psi.UseShellExecute=$false;$psi.CreateNoWindow=$true;$psi.RedirectStandardOutput=$true;$psi.RedirectStandardError=$true
 $proc=New-Object Diagnostics.Process;$proc.StartInfo=$psi
 Write-Json ([ordered]@{file=$file;arguments=$arguments;workdir=$src;source_commit=$expected.commit;source_tree=$expected.tree;dirty_overlay=$expected.dirty_overlay;tar_sha256=$expected.tar_sha256;started_utc=[DateTime]::UtcNow.ToString('o');environment=@{RUSTUP_TOOLCHAIN=$env:RUSTUP_TOOLCHAIN;CARGO_NET_OFFLINE=$env:CARGO_NET_OFFLINE;GIT_OPTIONAL_LOCKS=$env:GIT_OPTIONAL_LOCKS;CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR};invocation_count=1}) ($prefix+'-start.json')
 Write-Output ('ACTUAL_COMMAND_START '+$file+' '+$arguments)
 if(-not $proc.Start()){throw 'Process did not start'};$o=$proc.StandardOutput.ReadToEndAsync();$e=$proc.StandardError.ReadToEndAsync();$proc.WaitForExit();$code=$proc.ExitCode
 [IO.File]::WriteAllText((Join-Path $out ($prefix+'.stdout.txt')),$o.Result,(New-Object Text.UTF8Encoding($false)));[IO.File]::WriteAllText((Join-Path $out ($prefix+'.stderr.txt')),$e.Result,(New-Object Text.UTF8Encoding($false)))
 Write-Json ([ordered]@{exit_code=$code;has_exited=$proc.HasExited;process_id=$proc.Id;ended_utc=[DateTime]::UtcNow.ToString('o');invocation_count=1}) ($prefix+'-result.json');$proc.Dispose();Write-Output ('ACTUAL_COMMAND_END exit='+$code);if($code -ne 0){throw ('Actual command failed exit='+$code)}
}
function Hash-Bytes([byte[]]$b){return([BitConverter]::ToString($sha.ComputeHash($b))).Replace('-','').ToLowerInvariant()}
try{
 $binary=Join-Path $src 'target\native-tests\deps\xtask-5e2fcd04c8daf5d1.exe'
 $pe=[IO.File]::ReadAllBytes($binary);$offset=[BitConverter]::ToInt32($pe,60);$machine=[BitConverter]::ToUInt16($pe,$offset+4)
 if($machine -ne 0x8664){throw 'Expected AMD64 test PE'}
 Write-Json ([ordered]@{path=$binary;sha256=(File-Sha $binary);bytes=$pe.Length;machine='0x8664 AMD64';source_commit=$expected.commit;dirty_overlay=$expected.dirty_overlay;provenance='Compiled by preceding source-bound full Cargo xtask suite; no rebuild in direct tests'}) 'test-binary-provenance.json'
 $index=Join-Path $src '.git\index';$indexBefore=File-Sha $index
 $before=@();$after=@()
 foreach($rel in $expected.six_metadata){
   $path=Join-Path $src $rel;$b=[IO.File]::ReadAllBytes($path);$h=Hash-Bytes $b;$wanted=$expected.tracked_file_sha256.$rel
   if($h -ne $wanted){throw ('Not exact committed LF metadata '+$rel)}
   $lf=0;$cr=0;foreach($v in $b){if($v -eq 10){$lf++};if($v -eq 13){$cr++}}
   if($cr -ne 0 -or $lf -eq 0){throw 'Invalid LF control'}
   $before += [ordered]@{path=$rel;sha256=$h;bytes=$b.Length;lf=$lf;cr=$cr;crlf=0}
 }
 Write-Json $before 'six-metadata-lf-before.json'
 foreach($row in $before){
   $path=Join-Path $src $row.path;$b=[IO.File]::ReadAllBytes($path);$converted=New-Object byte[] ($b.Length+$row.lf);$j=0
   foreach($v in $b){if($v -eq 10){$converted[$j]=13;$j++};$converted[$j]=$v;$j++}
   [IO.File]::WriteAllBytes($path,$converted);$b=[IO.File]::ReadAllBytes($path)
   $cr=0;$lf=0;$crlf=0;for($i=0;$i -lt $b.Length;$i++){if($b[$i] -eq 10){$lf++};if($b[$i] -eq 13){$cr++;if($i+1 -lt $b.Length -and $b[$i+1] -eq 10){$crlf++}}}
   $roundtrip=New-Object byte[] ($b.Length-$crlf);$j=0;for($i=0;$i -lt $b.Length;$i++){if($b[$i] -eq 13 -and $i+1 -lt $b.Length -and $b[$i+1] -eq 10){continue};$roundtrip[$j]=$b[$i];$j++}
   $rh=Hash-Bytes $roundtrip
   if($crlf -ne $row.lf -or $cr -ne $crlf -or $lf -ne $crlf -or $crlf -le 0 -or $rh -ne $row.sha256){throw ('CRLF roundtrip failed '+$row.path)}
   $after += [ordered]@{path=$row.path;sha256=(Hash-Bytes $b);bytes=$b.Length;cr=$cr;lf=$lf;crlf=$crlf;bare_cr=($cr-$crlf);bare_lf=($lf-$crlf);lf_roundtrip_sha256=$rh;matches_committed_lf=$true}
 }
 Write-Json $after 'six-metadata-actual-crlf.json'
 $indexAfter=File-Sha $index;if($indexAfter -ne $indexBefore){throw 'Index changed'}
 Write-Json ([ordered]@{before_sha256=$indexBefore;after_sha256=$indexAfter;unchanged=$true}) 'private-index-crlf.json'
 Native-Run $binary '--exact release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout --nocapture' 'real-root-positive'
 Native-Run $binary '--exact release::tests::candidate_metadata_accepts_crlf_and_mixed_six_file_layouts_without_writing --nocapture' 'crlf-mixed-positive'
 Native-Run $binary '--exact release::tests::candidate_metadata_keeps_rejecting_bare_cr_ambiguous_and_mismatched_inputs --nocapture' 'malformed-negative'
 Native-Run $binary '--exact ci::tests::classifier_keeps_raw_crlf_metadata_ineligible --nocapture' 'classifier-negative'
 Native-Run $binary '--exact ci::tests::verify_checkout_accepts_stale_stat_without_mutating_index --nocapture' 'stale-stat-index-positive'
 $postIndex=File-Sha $index;if($postIndex -ne $indexBefore){throw 'Index changed after direct tests'}
 $post=@();foreach($row in $after){$h=File-Sha(Join-Path $src $row.path);if($h -ne $row.sha256){throw ('Direct tests changed metadata '+$row.path)};$post += [ordered]@{path=$row.path;sha256=$h;unchanged=$true}}
 Write-Json $post 'six-metadata-after-tests.json'
 Write-Json ([ordered]@{index_before=$indexBefore;index_after=$postIndex;unchanged=$true;ended_utc=[DateTime]::UtcNow.ToString('o');test_invocations=5;no_cargo_or_build=$true}) 'direct-tests-complete.json'
 Get-ChildItem -LiteralPath $out -Filter '*.stdout.txt'|ForEach-Object {Write-Output $_.Name;Get-Content -LiteralPath $_.FullName}
 exit 0
}catch{Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();ended_utc=[DateTime]::UtcNow.ToString('o');no_retry=$true}) 'failure.json';Write-Error $_ -ErrorAction Continue;exit 125}
