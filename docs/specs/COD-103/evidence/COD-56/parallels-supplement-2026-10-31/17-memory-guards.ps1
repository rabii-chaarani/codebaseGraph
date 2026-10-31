$ErrorActionPreference='Stop';$r='C:\WINDOWS\TEMP\codebasegraph-cod56-2e9284b8-47f1-472a-9cdc-b6a24cc3d62f';$input='\\Mac\Home\Documents\codebasegraph-cod56-vm-11023706-4804-4438-b044-e9768538a0c8';$cases=@()
foreach($shell in @('native51','emulated79')){
$exe=if($shell-eq'native51'){"$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"}else{"$r\pwsh-x64\pwsh.exe"}
foreach($v in @('baseline','candidate')){foreach($mode in @('dry','real')){
$d="$r\guard-$shell-$v-$mode";[IO.Directory]::CreateDirectory($d)|Out-Null;$src="$d\source";$bin="$d\bin";[IO.Directory]::CreateDirectory($src)|Out-Null;[IO.Directory]::CreateDirectory($bin)|Out-Null
foreach($n in @('codebase-graph.exe','k-wiki.exe')){[IO.File]::WriteAllText("$bin\$n","old $n")}
$before=@(Get-ChildItem $bin|%{(Get-FileHash $_.FullName).Hash})
$payload="& ([scriptblock]::Create([IO.File]::ReadAllText('$r\$v.ps1'))) -SourceDir '$src' -BinDir '$bin'";if($mode-eq'dry'){$payload+=' -DryRun'}
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName=$exe;$si.Arguments='-NoLogo -NoProfile -NonInteractive -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($payload));$si.UseShellExecute=$false;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true;$si.WorkingDirectory=$r
$p=[Diagnostics.Process]::Start($si);$o=$p.StandardOutput.ReadToEnd();$e=$p.StandardError.ReadToEnd();$p.WaitForExit()
$after=@(Get-ChildItem $bin|%{(Get-FileHash $_.FullName).Hash});$ok=$p.ExitCode-ne0-and$e.Contains('supports Windows x86_64')-and($before-join',')-eq($after-join',')-and@(Get-ChildItem $bin).Count-eq2
$cases+=@{Shell=$shell;Variant=$v;Mode=$mode;Invocation='Exact full source ScriptBlock memory, not File';Exit=$p.ExitCode;Stdout=$o;Stderr=$e;Before=$before;After=$after;PreservedAndGuardRejected=$ok;SourceSHA256=(Get-FileHash "$r\$v.ps1").Hash;Payload=$payload}
if(-not$ok){throw 'guard assertion'}
}}}
$cases|ConvertTo-Json -Depth 8|Set-Content "$input\17-memory-guards.json" -Encoding UTF8
