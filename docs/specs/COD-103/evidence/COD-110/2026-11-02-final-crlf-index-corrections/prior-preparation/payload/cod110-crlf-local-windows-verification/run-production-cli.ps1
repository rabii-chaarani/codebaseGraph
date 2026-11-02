$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-crlf-fixed-b5aa2377'
$out=Join-Path $stage 'production-cli'
if(Test-Path $out){throw 'CLI evidence exists; refuse repeat'}
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'c110-fix-b5aa2377'
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
 $index=Join-Path $src '.git\index';$indexBefore=File-Sha $index
 Native-Run (Join-Path $toolbin 'cargo.exe') 'build --offline --locked -p xtask --bin xtask --profile native-tests' 'current-cli-build'
 $binary=Join-Path $src 'target\native-tests\xtask.exe'
 $bytes=[IO.File]::ReadAllBytes($binary);$offset=[BitConverter]::ToInt32($bytes,60);$machine=[BitConverter]::ToUInt16($bytes,$offset+4);if($machine -ne 0x8664){throw 'Expected current AMD64 CLI PE'}
 Write-Json ([ordered]@{path=$binary;sha256=(File-Sha $binary);bytes=$bytes.Length;machine='0x8664 AMD64';source_commit=$expected.commit;dirty_overlay=$expected.dirty_overlay;provenance='One current cargo build in preceding command, exact corrected source; not old baseline binary'}) 'cli-binary-provenance.json'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'rev-parse HEAD' 'fixture-head'
 $head=[IO.File]::ReadAllText((Join-Path $out 'fixture-head.stdout.txt')).Trim()
 if($head -notmatch '^[0-9a-f]{40}$'){throw 'Private metadata fixture HEAD invalid'}
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'ls-files --eol -- Cargo.toml Cargo.lock crates/k-wiki/Cargo.toml crates/xtask/Cargo.toml .release-please-manifest.json CHANGELOG.md' 'git-normalized-eol'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'diff --quiet HEAD --' 'git-normalized-clean'
 Native-Run $binary ('release-metadata --source-dir "'+$src+'" --source-sha '+$head) 'release-metadata'
 $result=Get-Content -LiteralPath(Join-Path $out 'release-metadata.stdout.txt') -Raw|ConvertFrom-Json
 if($result.version -ne '4.0.5' -or $result.tag_name -ne 'v4.0.5' -or $result.source_sha -ne $head -or [string]::IsNullOrWhiteSpace($result.notes)){throw 'Production metadata result inconsistent with fixture'}
 $control=Get-Content -LiteralPath(Join-Path $stage 'controlled-crlf-tests\six-metadata-actual-crlf.json') -Raw|ConvertFrom-Json
 $rows=@();foreach($row in $control){$h=File-Sha(Join-Path $src $row.path);if($h -ne $row.sha256){throw ('Production CLI changed metadata '+$row.path)};$rows += [ordered]@{path=$row.path;sha256=$h;unchanged=$true}}
 $indexAfter=File-Sha $index;if($indexAfter -ne $indexBefore){throw 'CLI or build changed private index'}
 Write-Json ([ordered]@{version=$result.version;tag_name=$result.tag_name;source_sha=$result.source_sha;notes=$result.notes;notes_nonempty=$true;metadata_unchanged=$rows;index_before_sha256=$indexBefore;index_after_sha256=$indexAfter;index_unchanged=$true;ended_utc=[DateTime]::UtcNow.ToString('o')}) 'production-result-validation.json'
 Get-Content -LiteralPath(Join-Path $out 'release-metadata.stdout.txt')
 exit 0
}catch{Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();ended_utc=[DateTime]::UtcNow.ToString('o');no_retry=$true}) 'failure.json';Write-Error $_ -ErrorAction Continue;exit 125}
