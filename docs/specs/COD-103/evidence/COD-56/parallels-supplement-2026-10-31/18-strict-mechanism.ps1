$ErrorActionPreference='Stop'
$r='C:\WINDOWS\TEMP\codebasegraph-cod56-2e9284b8-47f1-472a-9cdc-b6a24cc3d62f';$input='\\Mac\Home\Documents\codebasegraph-cod56-vm-11023706-4804-4438-b044-e9768538a0c8'
$parsers=@();$cases=@()
$w="$env:SystemRoot\System32\where.exe";$wh=(Get-FileHash $w).Hash
foreach($v in @('baseline','candidate')){
$p="$input\$v.ps1";$bytes=[IO.File]::ReadAllBytes($p);$s=[Text.Encoding]::UTF8.GetString($bytes)
$sha=(Get-FileHash $p).Hash;$expected=if($v-eq'baseline'){'BDD6F0EACE6FBD4EA25C9E09D2C6CB6C0B54A05955D024AA97D76443C58AE308'}else{'830D128241C90B6F224554DC49BFCF572A1D51107DED8B12823E71D97C58B7BB'};if($sha-ne$expected){throw 'hash'}

$t=$null;$e=$null;$ast=[Management.Automation.Language.Parser]::ParseInput($s,[ref]$t,[ref]$e)
$f=@($ast.FindAll({param($n)$n-is[Management.Automation.Language.FunctionDefinitionAst]-and$n.Name-eq'Assert-Binary'},$true))[0].Extent.Text
[IO.File]::WriteAllText("$r\$v-function.txt",$f,(New-Object Text.UTF8Encoding($false)))
$parsers+=@{Variant=$v;SHA256=$sha;GuestLocalSHA256=(Get-FileHash "$r\$v.ps1").Hash;Errors=$e.Count;FunctionSHA256=(Get-FileHash "$r\$v-function.txt").Hash;FunctionBase64=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($f))}
foreach($arg in @('/?','cod56-never-exists-11023706-4804-4438-b044-e9768538a0c8')){
$caller=@'
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest;$env:PATH="$env:SystemRoot\System32";Set-Location 'C:\WINDOWS\TEMP\codebasegraph-cod56-2e9284b8-47f1-472a-9cdc-b6a24cc3d62f'
$h=@{'where.exe'='__HASH__'};$exception=$null;$accepted=$false
try{Assert-Binary -Name 'where.exe' -SourceRoot "$env:SystemRoot\System32" -Hashes $h -ProbeArgument '__ARG__';$accepted=$true}catch{$exception=$_.Exception.Message}
[ordered]@{Accepted=$accepted;Exception=$exception;NativeExit=$LASTEXITCODE;ProbeArgument='__ARG__';ErrorActionPreference=$ErrorActionPreference}|ConvertTo-Json -Compress
'@
$caller=$caller.Replace('C:\WINDOWS\TEMP\codebasegraph-cod56-2e9284b8-47f1-472a-9cdc-b6a24cc3d62f',$r).Replace('__HASH__',$wh.ToLowerInvariant()).Replace('__ARG__',$arg)
$payload=$f+"`n"+$caller
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName="$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe";$si.Arguments='-NoLogo -NoProfile -NonInteractive -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($payload));$si.UseShellExecute=$false;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true;$si.WorkingDirectory=$r;$si.EnvironmentVariables['PATH']="$env:SystemRoot\System32"
$proc=[Diagnostics.Process]::Start($si);$stdout=$proc.StandardOutput.ReadToEnd();$stderr=$proc.StandardError.ReadToEnd();$proc.WaitForExit()
$cases+=@{Variant=$v;Argument=$arg;HarnessExit=$proc.ExitCode;Stdout=$stdout;Stderr=$stderr;PayloadBase64=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($payload))}
}
}
[ordered]@{Root=$r;Parsers=$parsers;Cases=$cases;WhereSHA256=$wh;Policy=@(Get-ExecutionPolicy -List|%{"$($_.Scope):$($_.ExecutionPolicy)"})}|ConvertTo-Json -Depth 12|Set-Content "$input\18-strict-mechanism-result.json" -Encoding UTF8
Write-Output 'Completed parser2/mechanism4'
