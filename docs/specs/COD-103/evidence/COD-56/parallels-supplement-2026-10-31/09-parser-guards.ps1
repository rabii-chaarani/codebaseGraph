$ErrorActionPreference='Stop'
$r='C:\WINDOWS\TEMP\codebasegraph-cod56-c82bacc7-2f35-4871-b72d-9c82b13a4f29';$input='\\Mac\Home\.prime\agent\session-artifacts\01a18f18-3db0-73ba-a45a-f25faf5369ad\sub-575ea352\cod56-parallels-arm64-supplement'
if(Test-Path $r){throw 'root exists'}
[IO.Directory]::CreateDirectory($r)|Out-Null
Copy-Item -LiteralPath "$input\baseline.ps1" -Destination "$r\baseline.ps1"
Copy-Item -LiteralPath "$input\candidate.ps1" -Destination "$r\candidate.ps1"
function RunPS($arguments){
 $si=New-Object Diagnostics.ProcessStartInfo
 $si.FileName="$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
 $si.Arguments=$arguments;$si.UseShellExecute=$false;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true;$si.WorkingDirectory=$r;$si.EnvironmentVariables['PATH']="$env:SystemRoot\System32"
 $p=[Diagnostics.Process]::Start($si);$o=$p.StandardOutput.ReadToEnd();$e=$p.StandardError.ReadToEnd();$p.WaitForExit()
 [ordered]@{ExitCode=$p.ExitCode;Stdout=$o;Stderr=$e;Arguments=$arguments}
}
$hashes=@{baseline='BDD6F0EACE6FBD4EA25C9E09D2C6CB6C0B54A05955D024AA97D76443C58AE308';candidate='830D128241C90B6F224554DC49BFCF572A1D51107DED8B12823E71D97C58B7BB'}
$parsers=@();$guards=@();$functions=@()
foreach($v in @('baseline','candidate')){
 $p="$r\$v.ps1";$sha=(Get-FileHash $p -Algorithm SHA256).Hash;if($sha-ne$hashes[$v]){throw 'hash mismatch'}
 $t=$null;$e=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($p,[ref]$t,[ref]$e)
 $f=@($ast.FindAll({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name-eq'Assert-Binary'},$true))[0].Extent.Text
 [IO.File]::WriteAllText("$r\$v-function.txt",$f,(New-Object Text.UTF8Encoding($false)))
 $parsers += @{Variant=$v;SHA256=$sha;ParseErrorCount=$e.Count;Errors=@($e|% Message);FunctionSHA256=(Get-FileHash "$r\$v-function.txt").Hash;FunctionBase64=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($f))}
 if($e.Count){throw 'parse failure'}
 foreach($mode in @('dry','real')){
  $d="$r\$v-$mode";[IO.Directory]::CreateDirectory($d)|Out-Null;$src="$d\source";$bin="$d\bin";[IO.Directory]::CreateDirectory($src)|Out-Null;[IO.Directory]::CreateDirectory($bin)|Out-Null
  foreach($n in @('codebase-graph.exe','k-wiki.exe')){[IO.File]::WriteAllText("$bin\$n","old sentinel $n")}
  $before=@(Get-ChildItem $bin|%{(Get-FileHash $_.FullName).Hash})
  $a='-NoLogo -NoProfile -NonInteractive -File "'+$p+'" -SourceDir "'+$src+'" -BinDir "'+$bin+'"';if($mode-eq'dry'){$a+=' -DryRun'}
  $res=RunPS $a;$after=@(Get-ChildItem $bin|%{(Get-FileHash $_.FullName).Hash})
  $ok=$res.ExitCode-ne0 -and $res.Stderr.Contains('supports Windows x86_64') -and ($before-join',')-eq($after-join',') -and @(Get-ChildItem $bin).Count-eq2
  $guards+=@{Variant=$v;Mode=$mode;Process=$res;Before=$before;After=$after;Passed=$ok};if(-not$ok){throw 'guard assertion'}
 }
}
[ordered]@{Root=$r;Parsers=$parsers;Guards=$guards}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath "$input\09-parser-guards-result.json" -Encoding UTF8
Write-Output 'Parser2 and baseline/candidate guard4 passed'
