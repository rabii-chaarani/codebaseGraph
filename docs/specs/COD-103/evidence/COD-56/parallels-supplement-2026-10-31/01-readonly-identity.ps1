$ErrorActionPreference='Stop'
$os=Get-CimInstance Win32_OperatingSystem
$w=Join-Path $env:SystemRoot 'System32\where.exe'
$b=[IO.File]::ReadAllBytes($w);$pe=[BitConverter]::ToInt32($b,60);$machine=[BitConverter]::ToUInt16($b,$pe+4)
[ordered]@{OS=$os.Caption;Version=$os.Version;Build=$os.BuildNumber;OSArchitecture=[Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString();ProcessArchitecture=[Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString();PSVersion=$PSVersionTable.PSVersion.ToString();IsSystem=([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -eq 'S-1-5-18');Temp=$env:TEMP;WherePath=$w;WhereSHA256=(Get-FileHash $w -Algorithm SHA256).Hash;WherePEMachine=('0x{0:X4}' -f $machine);WhereSignature=(Get-AuthenticodeSignature $w).Status.ToString()}|ConvertTo-Json -Compress
