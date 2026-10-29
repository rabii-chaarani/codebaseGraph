param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$NativeArguments
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class NativeQualificationErrorMode {
    [DllImport("kernel32.dll")] public static extern uint SetErrorMode(uint mode);
}
'@
[void][NativeQualificationErrorMode]::SetErrorMode(0x8003)
if ($env:QUALIFICATION_NATIVE_PATH) { $env:PATH = $env:QUALIFICATION_NATIVE_PATH }
& $Executable @NativeArguments
$nativeStatus = $LASTEXITCODE
Write-Output "NATIVE_EXIT_STATUS=$nativeStatus"
exit $nativeStatus
