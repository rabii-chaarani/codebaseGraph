param([Parameter(Mandatory=$true)][string]$RequestPath)
$ErrorActionPreference = 'Stop'
$request = Get-Content -LiteralPath $RequestPath -Raw | ConvertFrom-Json
foreach ($entry in $request.environment.psobject.Properties) {
    [Environment]::SetEnvironmentVariable($entry.Name, [string]$entry.Value, 'Process')
}
Set-Location -LiteralPath $request.cwd
$nativeArgs = @($request.arguments)
& $request.executable @nativeArgs
$nativeStatus = $LASTEXITCODE
Write-Output "NATIVE_EXIT_STATUS=$nativeStatus"
exit $nativeStatus
