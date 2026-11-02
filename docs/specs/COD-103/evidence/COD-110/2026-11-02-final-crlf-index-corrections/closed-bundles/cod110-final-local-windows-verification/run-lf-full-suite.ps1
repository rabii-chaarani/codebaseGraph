$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-final-b5aa2377'
$out=Join-Path $stage 'lf-full-suite'
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'c110-final-b5aa2377'
if(Test-Path $src){throw 'Owned new corrected source exists; refuse reuse'}
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
try{
 if((File-Sha(Join-Path $stage 'snapshot.tar')) -ne $expected.tar_sha256){throw 'Snapshot archive hash mismatch'}
 New-Item -ItemType Directory -Path $src|Out-Null
 & tar.exe -xf (Join-Path $stage 'snapshot.tar') -C $src
 if($LASTEXITCODE -ne 0){throw 'Guest-local extract failed'}
 $rows=@();foreach($p in $expected.tracked_file_sha256.PSObject.Properties){$h=File-Sha(Join-Path $src $p.Name);$match=$h -eq $p.Value;$rows += [ordered]@{path=$p.Name;sha256=$h;expected=$p.Value;matches=$match};if(-not $match){throw ('Source mismatch '+$p.Name)}}
 Write-Json $rows 'all-2696-source-before.json'
 $eol=@();foreach($rel in $expected.six_metadata){$b=[IO.File]::ReadAllBytes((Join-Path $src $rel));$lf=0;$cr=0;foreach($v in $b){if($v -eq 10){$lf++};if($v -eq 13){$cr++}};if($cr -ne 0){throw 'Expected LF original metadata'};$eol += [ordered]@{path=$rel;bytes=$b.Length;sha256=(File-Sha(Join-Path $src $rel));cr=$cr;lf=$lf;crlf=0}}
 Write-Json $eol 'six-metadata-lf.json'
 Push-Location $src
 $ErrorActionPreference='Continue'
 & git.exe init --quiet 1>(Join-Path $out 'git-init.stdout.txt') 2>(Join-Path $out 'git-init.stderr.txt');if($LASTEXITCODE -ne 0){throw 'Private Git init failed'}
 & git.exe -c core.autocrlf=false add -- $expected.six_metadata 1>(Join-Path $out 'git-add.stdout.txt') 2>(Join-Path $out 'git-add.stderr.txt');if($LASTEXITCODE -ne 0){throw 'Private Git add failed'}
 & git.exe -c user.name=COD110-fixed-verification -c user.email=cod110-fixed@example.invalid -c commit.gpgsign=false -c core.hooksPath=NUL commit -m 'test: bind unchanged six release metadata files' 1>(Join-Path $out 'git-commit.stdout.txt') 2>(Join-Path $out 'git-commit.stderr.txt');if($LASTEXITCODE -ne 0){throw 'Private Git commit failed'}
 $ErrorActionPreference='Stop'
 & git.exe rev-parse HEAD|Set-Content -LiteralPath(Join-Path $out 'private-fixture-commit.txt') -Encoding UTF8
 Write-Json ([ordered]@{sha256=(File-Sha(Join-Path $src '.git\index'))}) 'private-index-before.json'
 Pop-Location
 Native-Run (Join-Path $toolbin 'cargo.exe') 'test --offline --locked -p xtask --bin xtask --profile native-tests -- --test-threads=1 --nocapture' 'full-suite'
 Get-Content -LiteralPath(Join-Path $out 'full-suite.stdout.txt')
 Get-Content -LiteralPath(Join-Path $out 'full-suite.stderr.txt')
 exit 0
}catch{Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();ended_utc=[DateTime]::UtcNow.ToString('o');no_retry=$true}) 'failure.json';Write-Error $_ -ErrorAction Continue;exit 125}
