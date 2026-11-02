$ErrorActionPreference='Stop'
$stage='C:\Mac\Home\Documents\codebasegraph-cod110-final-b5aa2377'
$out=Join-Path $stage 'production-cli-stale-stat'
if(Test-Path $out){throw 'CLI evidence exists; refuse repeat'}
New-Item -ItemType Directory -Path $out|Out-Null
$src=Join-Path $env:TEMP 'c110-final-b5aa2377'
$buildSource=$src
$expected=Get-Content -LiteralPath(Join-Path $stage 'expected-snapshot.json') -Raw|ConvertFrom-Json
$env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-msvc';$env:CARGO_NET_OFFLINE='true';$env:GIT_OPTIONAL_LOCKS='0';$env:CARGO_TARGET_DIR=Join-Path $src 'target'
$toolbin='C:\Users\rabii\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin';$env:PATH=$toolbin+';'+$env:PATH
$sha=[Security.Cryptography.SHA256]::Create()
function File-Sha([string]$path){$long=if($path.StartsWith('\\')){'\\?\UNC\'+$path.Substring(2)}else{'\\?\'+$path};$s=[IO.File]::OpenRead($long);try{return([BitConverter]::ToString($sha.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$s.Dispose()}}
function Write-Json($v,[string]$name){$v|ConvertTo-Json -Depth 16|Set-Content -LiteralPath(Join-Path $out $name) -Encoding UTF8}
function Native-Run([string]$file,[string]$arguments,[string]$prefix,[int]$expectedExit=0){
 $psi=New-Object Diagnostics.ProcessStartInfo;$psi.FileName=$file;$psi.Arguments=$arguments;$psi.WorkingDirectory=$src;$psi.UseShellExecute=$false;$psi.CreateNoWindow=$true;$psi.RedirectStandardOutput=$true;$psi.RedirectStandardError=$true
 $proc=New-Object Diagnostics.Process;$proc.StartInfo=$psi
 Write-Json ([ordered]@{file=$file;arguments=$arguments;workdir=$src;source_commit=$expected.commit;source_tree=$expected.tree;dirty_overlay=$expected.dirty_overlay;tar_sha256=$expected.tar_sha256;started_utc=[DateTime]::UtcNow.ToString('o');environment=@{RUSTUP_TOOLCHAIN=$env:RUSTUP_TOOLCHAIN;CARGO_NET_OFFLINE=$env:CARGO_NET_OFFLINE;GIT_OPTIONAL_LOCKS=$env:GIT_OPTIONAL_LOCKS;CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR};invocation_count=1}) ($prefix+'-start.json')
 Write-Output ('ACTUAL_COMMAND_START '+$file+' '+$arguments)
 if(-not $proc.Start()){throw 'Process did not start'};$o=$proc.StandardOutput.ReadToEndAsync();$e=$proc.StandardError.ReadToEndAsync();$proc.WaitForExit();$code=$proc.ExitCode
 [IO.File]::WriteAllText((Join-Path $out ($prefix+'.stdout.txt')),$o.Result,(New-Object Text.UTF8Encoding($false)));[IO.File]::WriteAllText((Join-Path $out ($prefix+'.stderr.txt')),$e.Result,(New-Object Text.UTF8Encoding($false)))
 Write-Json ([ordered]@{exit_code=$code;has_exited=$proc.HasExited;process_id=$proc.Id;ended_utc=[DateTime]::UtcNow.ToString('o');invocation_count=1}) ($prefix+'-result.json');$proc.Dispose();Write-Output ('ACTUAL_COMMAND_END exit='+$code);if($code -ne $expectedExit){throw ('Actual command unexpected exit='+$code+' expected='+$expectedExit)}
}
function Hash-Bytes([byte[]]$b){return([BitConverter]::ToString($sha.ComputeHash($b))).Replace('-','').ToLowerInvariant()}
function Save-Index([string]$prefix){
 $p=Join-Path $src '.git\index';$bytes=[IO.File]::ReadAllBytes($p);[IO.File]::WriteAllBytes((Join-Path $out ($prefix+'.index.raw')),$bytes)
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'ls-files --stage' ($prefix+'-modes')
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'ls-files -v' ($prefix+'-flags')
 return (Hash-Bytes $bytes)
}
try{
 Native-Run (Join-Path $toolbin 'cargo.exe') 'build --offline --locked -p xtask --bin xtask --profile native-tests' 'current-cli-build'
 $binary=Join-Path $buildSource 'target\native-tests\xtask.exe';$pe=[IO.File]::ReadAllBytes($binary);$offset=[BitConverter]::ToInt32($pe,60);$machine=[BitConverter]::ToUInt16($pe,$offset+4);if($machine -ne 0x8664){throw 'Expected final AMD64 CLI PE'}
 Write-Json ([ordered]@{path=$binary;sha256=(File-Sha $binary);bytes=$pe.Length;machine='0x8664 AMD64';source_commit=$expected.commit;dirty_overlay=$expected.dirty_overlay;provenance='One current offline Cargo build from exact final no-refresh source'}) 'cli-binary-provenance.json'
 $src=Join-Path $buildSource 'cli-six-metadata-fixture';if(Test-Path $src){throw 'New CLI fixture exists; refuse reuse'};New-Item -ItemType Directory -Path $src|Out-Null
 $lfBytes=@{}
 foreach($rel in $expected.six_metadata){$b=[IO.File]::ReadAllBytes((Join-Path $buildSource $rel));$text=[Text.Encoding]::UTF8.GetString($b).Replace("`r`n","`n");$lf=[Text.Encoding]::UTF8.GetBytes($text);if((Hash-Bytes $lf) -ne $expected.tracked_file_sha256.$rel){throw ('Metadata roundtrip not committed '+$rel)};$lfBytes[$rel]=$lf;$dest=Join-Path $src $rel;[IO.Directory]::CreateDirectory((Split-Path $dest))|Out-Null;[IO.File]::WriteAllBytes($dest,$lf)}
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'init --quiet' 'fixture-git-init'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'config --local core.autocrlf true' 'fixture-local-autocrlf'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'add -- Cargo.toml Cargo.lock crates/k-wiki/Cargo.toml crates/xtask/Cargo.toml .release-please-manifest.json CHANGELOG.md' 'fixture-git-add'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' '-c user.name=COD110-final-verifier -c user.email=cod110-final@example.invalid -c commit.gpgsign=false -c core.hooksPath=NUL commit -m "test: bind committed metadata fixture"' 'fixture-git-commit'
 Native-Run 'C:\Program Files\Git\cmd\git.exe' 'rev-parse HEAD' 'fixture-head'
 $head=[IO.File]::ReadAllText((Join-Path $out 'fixture-head.stdout.txt')).Trim();if($head -notmatch '^[a-f0-9]{40}$'){throw 'Fixture HEAD invalid'}
 $notesText=[Text.Encoding]::UTF8.GetString($lfBytes['CHANGELOG.md']);$begin=$notesText.IndexOf('## [4.0.5]');$end=$notesText.IndexOf("`n## ",$begin+1);$expectedNotes=if($end -ge 0){$notesText.Substring($begin,$end-$begin).Trim()}else{$notesText.Substring($begin).Trim()}
 foreach($case in @('lf','crlf','mixed')){
  $eol=@();$mtimes=@()
  foreach($rel in $expected.six_metadata){
   $lf=$lfBytes[$rel];$stream=New-Object IO.MemoryStream;$line=0
   foreach($v in $lf){if($v -eq 10){if($case -eq 'crlf' -or ($case -eq 'mixed' -and $line%2 -eq 0)){$stream.WriteByte(13)};$line++};$stream.WriteByte($v)}
   $actual=$stream.ToArray();$stream.Dispose();$path=Join-Path $src $rel;[IO.File]::WriteAllBytes($path,$actual)
   $cr=0;$ln=0;$pairs=0;for($i=0;$i -lt $actual.Length;$i++){if($actual[$i] -eq 10){$ln++};if($actual[$i] -eq 13){$cr++;if($i+1 -lt $actual.Length -and $actual[$i+1] -eq 10){$pairs++}}}
   $roundtrip=[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($actual).Replace("`r`n","`n"));$rh=Hash-Bytes $roundtrip
   if($rh -ne $expected.tracked_file_sha256.$rel){throw 'Case roundtrip mismatch'}
   if($case -eq 'crlf' -and ($pairs -le 0 -or $pairs -ne $ln -or $pairs -ne $cr)){throw 'Case not all CRLF'}
   if($case -eq 'mixed' -and ($pairs -le 0 -or $pairs -eq $ln)){throw 'Case not mixed'}
   $before=[IO.File]::GetLastWriteTimeUtc($path);$stale=$before.AddSeconds(-10);[IO.File]::SetLastWriteTimeUtc($path,$stale);$read=[IO.File]::GetLastWriteTimeUtc($path)
   if($read -ne $stale){throw 'Stale mtime did not apply'}
   $eol += [ordered]@{path=$rel;bytes=$actual.Length;sha256=(Hash-Bytes $actual);cr=$cr;lf=$ln;crlf=$pairs;bare_cr=($cr-$pairs);bare_lf=($ln-$pairs);lf_roundtrip_sha256=$rh;matches_committed_lf=$true}
   $mtimes += [ordered]@{path=$rel;before_utc=$before.ToString('o');stale_utc=$read.ToString('o');delta_seconds=-10;content_sha256=(File-Sha $path)}
  }
  Write-Json $eol ($case+'-actual-eol.json');Write-Json $mtimes ($case+'-stale-mtimes.json')
  $indexBefore=Save-Index ($case+'-before')
  Native-Run $binary ('release-metadata --source-dir "'+$src+'" --source-sha '+$head) ($case+'-release-metadata')
  $indexAfter=Save-Index ($case+'-after')
  $beforeRaw=[IO.File]::ReadAllBytes((Join-Path $out ($case+'-before.index.raw')));$afterRaw=[IO.File]::ReadAllBytes((Join-Path $out ($case+'-after.index.raw')))
  $equal=([Convert]::ToBase64String($beforeRaw) -ceq [Convert]::ToBase64String($afterRaw))
  if(-not $equal -or $indexBefore[-1] -ne $indexAfter[-1]){throw ('Raw index changed '+$case)}
  $value=Get-Content -LiteralPath(Join-Path $out ($case+'-release-metadata.stdout.txt')) -Raw|ConvertFrom-Json
  $schema=@($value.PSObject.Properties.Name|Sort-Object);if(($schema -join ',') -ne 'notes,source_sha,tag_name,version'){throw 'Unexpected production metadata schema'}
  if($value.source_sha -ne $head -or $value.version -ne '4.0.5' -or $value.tag_name -ne 'v4.0.5' -or $value.notes -ne $expectedNotes){throw 'Production metadata result mismatch'}
  foreach($row in $eol){if((File-Sha(Join-Path $src $row.path)) -ne $row.sha256){throw 'CLI altered metadata'}}
  Write-Json ([ordered]@{case=$case;exit_code=0;source_sha=$value.source_sha;version=$value.version;tag_name=$value.tag_name;schema=$schema;notes_equal_exact_committed_section=$true;metadata_unchanged=$true;raw_index_bytes_equal=$equal;index_before_sha256=$indexBefore[-1];index_after_sha256=$indexAfter[-1];mtime_stale_seconds=-10}) ($case+'-validation.json')
 }
 $dirtyPath=Join-Path $src 'Cargo.toml';$dirty=[IO.File]::ReadAllBytes($dirtyPath)+[Text.Encoding]::UTF8.GetBytes("`n# deliberate dirty-negative fixture`n");[IO.File]::WriteAllBytes($dirtyPath,$dirty);[IO.File]::SetLastWriteTimeUtc($dirtyPath,[IO.File]::GetLastWriteTimeUtc($dirtyPath).AddSeconds(-10))
 Write-Json ([ordered]@{path='Cargo.toml';sha256=(File-Sha $dirtyPath);expected='Reject real tracked content change despite stale-stat, no normalization waiver';mtime_utc=[IO.File]::GetLastWriteTimeUtc($dirtyPath).ToString('o')}) 'dirty-negative-input.json'
 $dirtyBefore=Save-Index 'dirty-before'
 Native-Run $binary ('release-metadata --source-dir "'+$src+'" --source-sha '+$head) 'dirty-release-metadata' 1
 $dirtyAfter=Save-Index 'dirty-after'
 $dirtyEqual=([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $out 'dirty-before.index.raw'))) -ceq [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $out 'dirty-after.index.raw'))))
 if(-not $dirtyEqual){throw 'Dirty negative mutated raw index'}
 $errorText=[IO.File]::ReadAllText((Join-Path $out 'dirty-release-metadata.stderr.txt'));if($errorText -notlike '*tracked*changes*'){throw 'Dirty negative did not reject tracked changes'}
 Write-Json ([ordered]@{exit_code=1;oracle='expected rejection';stderr=$errorText;raw_index_bytes_equal=$dirtyEqual;index_before_sha256=$dirtyBefore[-1];index_after_sha256=$dirtyAfter[-1];dirty_content_sha256=(File-Sha $dirtyPath);fixture_preserved_dirty=$true}) 'dirty-negative-validation.json'
 exit 0
}catch{Write-Json ([ordered]@{error=$_.Exception.Message;detail=$_.ToString();ended_utc=[DateTime]::UtcNow.ToString('o');no_retry=$true}) 'failure.json';Write-Error $_ -ErrorAction Continue;exit 125}
