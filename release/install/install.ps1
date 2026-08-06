param(
    [string]$SourceDir = $PSScriptRoot,
    [string]$BinDir = $(Join-Path $HOME ".local/bin"),
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ExpectedHashes {
    param([string]$ChecksumsPath)

    $hashes = @{}
    foreach ($line in Get-Content -LiteralPath $ChecksumsPath) {
        if ($line -match '^([0-9a-fA-F]{64})\s+(.+)$') {
            $hashes[$Matches[2].Trim()] = $Matches[1].ToLowerInvariant()
        }
    }
    return $hashes
}

function Assert-Binary {
    param(
        [string]$Name,
        [string]$SourceRoot,
        [hashtable]$Hashes,
        [string]$ProbeArgument
    )

    $binaryPath = Join-Path $SourceRoot $Name
    if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
        throw "Missing binary $Name in $SourceRoot"
    }
    if (-not $Hashes.ContainsKey($Name)) {
        throw "checksums.txt does not contain $Name"
    }
    $actualHash = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $Hashes[$Name]) {
        throw "SHA256 mismatch for $Name"
    }
    & $binaryPath $ProbeArgument | Out-Null
}

function Install-Binary {
    param(
        [string]$Name,
        [string]$SourceRoot,
        [string]$DestinationRoot
    )

    $sourcePath = Join-Path $SourceRoot $Name
    $destinationPath = Join-Path $DestinationRoot $Name
    $tempPath = Join-Path $DestinationRoot ('.' + $Name + '.tmp.' + [guid]::NewGuid().ToString('N'))
    try {
        Copy-Item -LiteralPath $sourcePath -Destination $tempPath -ErrorAction Stop
        Move-Item -LiteralPath $tempPath -Destination $destinationPath -Force -ErrorAction Stop
    }
    finally {
        if (Test-Path -LiteralPath $tempPath) {
            Remove-Item -LiteralPath $tempPath -Force -ErrorAction Stop
        }
    }
}

$resolvedSourceDir = (Resolve-Path -LiteralPath $SourceDir).Path
$checksumsPath = Join-Path $resolvedSourceDir 'checksums.txt'
if (-not (Test-Path -LiteralPath $checksumsPath -PathType Leaf)) {
    throw "Missing checksums.txt in $resolvedSourceDir"
}

$expectedHashes = Get-ExpectedHashes -ChecksumsPath $checksumsPath
Assert-Binary -Name 'codebase-graph.exe' -SourceRoot $resolvedSourceDir -Hashes $expectedHashes -ProbeArgument '--help'
Assert-Binary -Name 'k-wiki.exe' -SourceRoot $resolvedSourceDir -Hashes $expectedHashes -ProbeArgument '--version'

if ($DryRun) {
    Write-Output "Validated codebase-graph.exe and k-wiki.exe from $resolvedSourceDir"
    exit 0
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Install-Binary -Name 'codebase-graph.exe' -SourceRoot $resolvedSourceDir -DestinationRoot $BinDir
Install-Binary -Name 'k-wiki.exe' -SourceRoot $resolvedSourceDir -DestinationRoot $BinDir

Write-Output "Installed codebase-graph.exe and k-wiki.exe to $BinDir"
