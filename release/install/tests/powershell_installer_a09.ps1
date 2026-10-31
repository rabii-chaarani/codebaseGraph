param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [Parameter(Mandatory = $true)][string]$ExpectedInstallerSha256,
    [Parameter(Mandatory = $true)][string]$FixturePath,
    [Parameter(Mandatory = $true)][string]$WindowsPowerShellPath,
    [Parameter(Mandatory = $true)][string]$PowerShell7Path,
    [Parameter(Mandatory = $true)][string]$OutputRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Get-BytesHash {
    param([byte[]]$Bytes)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose() }
}

function Get-PathHash {
    param([string]$Path)
    if (-not [IO.File]::Exists($Path)) { return $null }
    return Get-BytesHash ([IO.File]::ReadAllBytes($Path))
}

function Write-Json {
    param([string]$Path, $Value)
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 30), $utf8)
}

function Require-AbsolutePath {
    param([string]$Path)
    if ($Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\/]+[\\/][^\\/]+[\\/])') { throw "Absolute Windows path required: $Path" }
    return [IO.Path]::GetFullPath($Path)
}

function Get-DerivedInstaller {
    param([byte[]]$Original)
    $fragment = " -or`n    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() -ne 'X64'"
    $text = [Text.Encoding]::UTF8.GetString($Original)
    $offset = $text.IndexOf($fragment, [StringComparison]::Ordinal)
    if ($offset -lt 0 -or $text.IndexOf($fragment, $offset + 1, [StringComparison]::Ordinal) -ge 0) {
        throw 'Expected exactly one approved architecture fragment'
    }
    if ($text -cnotmatch 'if \(\[Environment\]::OSVersion.Platform -ne \[PlatformID\]::Win32NT') {
        throw 'Windows platform predicate missing'
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes($text.Remove($offset, $fragment.Length))
    $removedBytes = [Text.Encoding]::UTF8.GetBytes($fragment)
    $byteOffset = [Text.Encoding]::UTF8.GetByteCount($text.Substring(0, $offset))
    $reconstructed = New-Object byte[] $Original.Length
    [Array]::Copy($bytes, 0, $reconstructed, 0, $byteOffset)
    [Array]::Copy($removedBytes, 0, $reconstructed, $byteOffset, $removedBytes.Length)
    [Array]::Copy($bytes, $byteOffset, $reconstructed, $byteOffset + $removedBytes.Length, $bytes.Length - $byteOffset)
    if ([Convert]::ToBase64String($reconstructed) -cne [Convert]::ToBase64String($Original)) {
        throw 'Derived copy changes bytes outside the approved removal'
    }
    return [pscustomobject]@{ Bytes = $bytes; Offset = $byteOffset; Removed = $fragment; Sha256 = (Get-BytesHash $bytes) }
}

function Read-NativeTrace {
    param([string]$Path)
    $records = New-Object 'System.Collections.Generic.List[object]'
    if (-not [IO.File]::Exists($Path)) { return ,$records.ToArray() }
    $stream = [IO.File]::OpenRead($Path)
    $reader = New-Object IO.BinaryReader($stream)
    try {
        while ($stream.Position -lt $stream.Length) {
            $version = $reader.ReadUInt32()
            $role = $reader.ReadUInt32()
            $argc = $reader.ReadUInt32()
            if ($version -ne 1 -or $role -notin @(1, 2) -or $argc -ne 1) {
                throw 'Invalid native trace version, role or argument count'
            }
            $length = $reader.ReadUInt32()
            if ($length -gt (($stream.Length - $stream.Position - 4) / 2)) { throw 'Truncated native UTF16 argument' }
            $units = $reader.ReadBytes([int]($length * 2))
            $argument = [Text.Encoding]::Unicode.GetString($units)
            $selectedExit = $reader.ReadUInt32()
            if ($selectedExit -notin @(0, 23, 29)) { throw "Native fixture fault/unexpected-argument exit $selectedExit" }
            $records.Add([pscustomobject]@{ Version = $version; Role = $role; Argc = $argc; Argument = $argument; SelectedExit = $selectedExit })
        }
        return ,$records.ToArray()
    }
    finally { $reader.Dispose(); $stream.Dispose() }
}

function Add-Assertion {
    param($Row, [string]$Name, [bool]$Passed, $Expected, $Actual)
    $Row.assertions.Add([pscustomobject]@{ name = $Name; passed = $Passed; expected = $Expected; actual = $Actual })
}

function Invoke-InstallerChild {
    param([string]$Shell, [hashtable]$Environment, [string]$StdoutPath, [string]$StderrPath)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Shell
    $start.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + $encodedControl
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($key in $Environment.Keys) { $start.EnvironmentVariables[$key] = [string]$Environment[$key] }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    $outFile = $null
    $errFile = $null
    $outTask = $null
    $errTask = $null
    $started = $false
    try {
        $outFile = [IO.File]::Create($StdoutPath)
        $errFile = [IO.File]::Create($StderrPath)
        $started = $process.Start()
        if (-not $started) { throw 'Installer child did not start' }
        $outTask = $process.StandardOutput.BaseStream.CopyToAsync($outFile)
        $errTask = $process.StandardError.BaseStream.CopyToAsync($errFile)
        $process.WaitForExit()
        [void]$outTask.GetAwaiter().GetResult()
        [void]$errTask.GetAwaiter().GetResult()
        return $process.ExitCode
    }
    finally {
        try {
            if ($started -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        }
        finally {
            try {
                if ($null -ne $outTask) { [void]$outTask.GetAwaiter().GetResult() }
                if ($null -ne $errTask) { [void]$errTask.GetAwaiter().GetResult() }
            }
            finally {
                if ($null -ne $outFile) { $outFile.Dispose() }
                if ($null -ne $errFile) { $errFile.Dispose() }
                $process.Dispose()
            }
        }
    }
}

function Write-JUnit {
    param([string]$Path, $Rows)
    $settings = New-Object Xml.XmlWriterSettings
    $settings.Indent = $true
    $settings.Encoding = $utf8
    $writer = [Xml.XmlWriter]::Create($Path, $settings)
    try {
        $failures = @($Rows | Where-Object { $_.state -eq 'failure' }).Count
        $errors = @($Rows | Where-Object { $_.state -in @('error', 'not_run') }).Count
        $writer.WriteStartDocument()
        $writer.WriteStartElement('testsuite')
        $writer.WriteAttributeString('name', 'COD-56-A09')
        $writer.WriteAttributeString('tests', [string]$Rows.Count)
        $writer.WriteAttributeString('failures', [string]$failures)
        $writer.WriteAttributeString('errors', [string]$errors)
        $writer.WriteAttributeString('skipped', '0')
        foreach ($row in $Rows) {
            $writer.WriteStartElement('testcase')
            $writer.WriteAttributeString('classname', 'COD56.A09.' + $row.shell)
            $writer.WriteAttributeString('name', $row.id)
            if ($row.state -eq 'failure') {
                $writer.WriteStartElement('failure')
                $writer.WriteAttributeString('message', 'Actual case assertions failed')
                $writer.WriteString((ConvertTo-Json -InputObject @($row.assertions | Where-Object { -not $_.passed }) -Depth 10))
                $writer.WriteEndElement()
            }
            elseif ($row.state -ne 'pass') {
                $writer.WriteStartElement('error')
                $writer.WriteAttributeString('message', $row.state)
                $writer.WriteString([string]$row.exception)
                $writer.WriteEndElement()
            }
            $writer.WriteStartElement('system-out')
            $writer.WriteString($row.evidence_path)
            $writer.WriteEndElement()
            $writer.WriteEndElement()
        }
        $writer.WriteEndElement()
        $writer.WriteEndDocument()
    }
    finally { $writer.Dispose() }
}

# The constant control contains no case interpolation; all inputs use child-only environment.
$childControl = @'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    $metadata = [ordered]@{
        PSVersion = $PSVersionTable.PSVersion.ToString()
        ShellPath = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
        OSVersion = [Environment]::OSVersion.VersionString
        OSArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        ProcessArchitecture = [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
        Invocation = 'fresh-child-memory-ScriptBlock-not-File'
    }
    [IO.File]::WriteAllText($env:CBG_A09_METADATA, ($metadata | ConvertTo-Json), (New-Object Text.UTF8Encoding($false)))
    $body = [IO.File]::ReadAllText($env:CBG_A09_INSTALLER)
    $installer = [ScriptBlock]::Create($body)
    if ($env:CBG_A09_MODE -eq 'DryRun') {
        & $installer -SourceDir $env:CBG_A09_SOURCE -BinDir $env:CBG_A09_BIN -DryRun
    }
    elseif ($env:CBG_A09_MODE -eq 'real') {
        & $installer -SourceDir $env:CBG_A09_SOURCE -BinDir $env:CBG_A09_BIN
    }
    else { throw 'Invalid private case mode' }
    exit 0
}
catch {
    [Console]::Error.WriteLine($_.ToString())
    exit 1
}
'@
$encodedControl = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($childControl))

$InstallerPath = Require-AbsolutePath $InstallerPath
$FixturePath = Require-AbsolutePath $FixturePath
$WindowsPowerShellPath = Require-AbsolutePath $WindowsPowerShellPath
$PowerShell7Path = Require-AbsolutePath $PowerShell7Path
$OutputRoot = Require-AbsolutePath $OutputRoot
if (Test-Path -LiteralPath $OutputRoot) { throw 'OutputRoot must be new; no prior evidence is overwritten' }
[void][IO.Directory]::CreateDirectory($OutputRoot)

$scenarios = @('success_both', 'graph_probe_nonzero', 'wiki_probe_nonzero', 'missing_graph', 'missing_wiki', 'bad_manifest_graph_hash', 'bad_manifest_wiki_hash')
$shells = @([pscustomobject]@{ Name = 'WinPS51'; Path = $WindowsPowerShellPath }, [pscustomobject]@{ Name = 'PS7'; Path = $PowerShell7Path })
$rows = New-Object 'System.Collections.Generic.List[object]'
foreach ($shell in $shells) {
    foreach ($mode in @('DryRun', 'real')) {
        foreach ($scenario in $scenarios) {
            $id = 'a09::' + $shell.Name + '::' + $mode + '::' + $scenario
            $rows.Add([pscustomobject]@{
                id = $id; shell = $shell.Name; executable = $shell.Path; mode = $mode; scenario = $scenario
                state = 'not_run'; assertions = (New-Object 'System.Collections.Generic.List[object]'); exception = $null
                evidence_path = (Join-Path $OutputRoot ($shell.Name + '-' + $mode + '-' + $scenario))
                request = $null; child_exit = $null; metadata = $null; decoded_trace = @()
                original_sha256 = $null; derived_sha256 = $null; fixture_sha256 = $null; runner_sha256 = $null
                candidate_hashes = $null; manifest_sha256 = $null; old_hashes = $null; new_hashes = $null
            })
        }
    }
}
$plan = @($rows | Select-Object id, shell, executable, mode, scenario, evidence_path)
Write-Json (Join-Path $OutputRoot 'plan.json') $plan
$reportErrors = New-Object 'System.Collections.Generic.List[string]'
$abort = $null
try {
    if ($rows.Count -ne 28 -or @($rows.id | Select-Object -Unique).Count -ne 28) { throw 'Incomplete or duplicate registered matrix' }
    $expected = $ExpectedInstallerSha256.ToLowerInvariant()
    if ($expected -notin @('830d128241c90b6f224554dc49bfcf572a1d51107ded8b12823e71d97c58b7bb', 'bdd6f0eace6fbd4ea25c9e09d2c6cb6c0b54a05955d024aa97d76443c58ae308')) {
        throw 'ExpectedInstallerSha256 must identify the pinned corrected or baseline original'
    }
    foreach ($path in @($InstallerPath, $FixturePath, $WindowsPowerShellPath, $PowerShell7Path)) {
        if (-not [IO.File]::Exists($path)) { throw "Required file missing: $path" }
    }
    $originalBytes = [IO.File]::ReadAllBytes($InstallerPath)
    if ((Get-BytesHash $originalBytes) -cne $expected) { throw 'Original installer hash mismatch' }
    $derived = Get-DerivedInstaller $originalBytes
    $inputs = Join-Path $OutputRoot '_inputs'
    [void][IO.Directory]::CreateDirectory($inputs)
    $derivedPath = Join-Path $inputs 'derived-install.ps1'
    [IO.File]::WriteAllBytes((Join-Path $inputs 'original-install.ps1'), $originalBytes)
    [IO.File]::WriteAllBytes($derivedPath, $derived.Bytes)
    [IO.File]::WriteAllText((Join-Path $inputs 'removed-fragment.txt'), $derived.Removed, $utf8)
    $fixtureBytes = [IO.File]::ReadAllBytes($FixturePath)
    $fixtureHash = Get-BytesHash $fixtureBytes
    if ($fixtureHash -cne '08e9773855c82ded4837968683cfa7895d17fb7d9d3283b9670ce4dc0f1fca80') { throw 'Fixture differs from independently smoked PE' }
    $peOffset = [BitConverter]::ToInt32($fixtureBytes, 60)
    if ([BitConverter]::ToUInt32($fixtureBytes, $peOffset) -ne 17744 -or
        [BitConverter]::ToUInt16($fixtureBytes, $peOffset + 4) -ne 34404 -or
        [BitConverter]::ToUInt16($fixtureBytes, $peOffset + 24) -ne 523 -or
        [BitConverter]::ToUInt32($fixtureBytes, $peOffset + 24 + 112 + 14 * 8) -ne 0) { throw 'Fixture is not native AMD64 PE32+' }
    $runnerBytes = $utf8.GetBytes($MyInvocation.MyCommand.ScriptBlock.ToString())
    $runnerIdentity = 'invoked-ScriptBlock-text-UTF8'
    if ($null -ne $MyInvocation.MyCommand.PSObject.Properties['Path'] -and -not [string]::IsNullOrEmpty($MyInvocation.MyCommand.Path)) {
        $runnerBytes = [IO.File]::ReadAllBytes($MyInvocation.MyCommand.Path)
        $runnerIdentity = 'file-bytes'
    }
    $runnerHash = Get-BytesHash $runnerBytes
    [IO.File]::WriteAllBytes((Join-Path $inputs 'runner-invoked.ps1'), $runnerBytes)
    [IO.File]::WriteAllText((Join-Path $inputs 'child-control.ps1'), $childControl, $utf8)
    $os = Get-CimInstance -ClassName Win32_OperatingSystem
    Write-Json (Join-Path $OutputRoot 'identity.json') ([ordered]@{
        OriginalPath = $InstallerPath; OriginalSha256 = $expected; OriginalBytes = $originalBytes.Length
        DerivedPath = $derivedPath; DerivedSha256 = $derived.Sha256; RemovalByteOffset = $derived.Offset; RemovedFragment = $derived.Removed
        FixturePath = $FixturePath; FixtureSha256 = $fixtureHash; FixtureBytes = $fixtureBytes.Length
        RunnerSha256 = $runnerHash; RunnerIdentity = $runnerIdentity
        OS = $os.Caption; Version = $os.Version; Build = $os.BuildNumber
        OSArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        Invocation = 'approved-derived-copy-memory-ScriptBlock-not-File'; ProductSupportExpansion = $false
    })
    foreach ($row in $rows) {
        try {
            [void][IO.Directory]::CreateDirectory($row.evidence_path)
            $sourceDir = Join-Path $row.evidence_path 'source'
            $binDir = Join-Path $row.evidence_path 'bin'
            [void][IO.Directory]::CreateDirectory($sourceDir)
            [void][IO.Directory]::CreateDirectory($binDir)
            $tracePath = Join-Path $row.evidence_path 'native-trace.bin'
            $metadataPath = Join-Path $row.evidence_path 'shell.json'
            $stdoutPath = Join-Path $row.evidence_path 'stdout.bin'
            $stderrPath = Join-Path $row.evidence_path 'stderr.bin'
            $row.original_sha256 = Get-PathHash $InstallerPath
            $row.derived_sha256 = Get-PathHash $derivedPath
            $row.fixture_sha256 = Get-PathHash $FixturePath
            $row.runner_sha256 = $runnerHash
            if ($row.original_sha256 -cne $expected -or $row.fixture_sha256 -cne $fixtureHash) { throw 'Original/fixture bytes changed before case' }
            $caseDerived = Get-DerivedInstaller ([IO.File]::ReadAllBytes($InstallerPath))
            if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($derivedPath)) -cne [Convert]::ToBase64String($caseDerived.Bytes)) {
                throw 'Derived bytes differ from sole approved transformation before case'
            }
            $oldHashes = [ordered]@{}
            $candidateHashes = [ordered]@{}
            foreach ($name in @('codebase-graph.exe', 'k-wiki.exe')) {
                [IO.File]::WriteAllBytes((Join-Path $sourceDir $name), $fixtureBytes)
                [IO.File]::WriteAllText((Join-Path $binDir $name), ('old:' + $row.id + ':' + $name), $utf8)
                $oldHashes[$name] = Get-PathHash (Join-Path $binDir $name)
                $candidateHashes[$name] = Get-PathHash (Join-Path $sourceDir $name)
                if ($candidateHashes[$name] -cne $fixtureHash) { throw 'Candidate PE copy hash mismatch' }
            }
            $row.old_hashes = $oldHashes
            $row.candidate_hashes = $candidateHashes
            $manifestHashes = [ordered]@{ 'codebase-graph.exe' = $fixtureHash; 'k-wiki.exe' = $fixtureHash }
            $graphExit = '0'; $wikiExit = '0'
            $expectedTrace = @([pscustomobject]@{ Role = 1; Argument = '--help'; SelectedExit = 0 })
            switch ($row.scenario) {
                'success_both' { $expectedTrace += [pscustomobject]@{ Role = 2; Argument = '--version'; SelectedExit = 0 } }
                'graph_probe_nonzero' { $graphExit = '23'; $expectedTrace[0].SelectedExit = 23 }
                'wiki_probe_nonzero' { $wikiExit = '29'; $expectedTrace += [pscustomobject]@{ Role = 2; Argument = '--version'; SelectedExit = 29 } }
                'missing_graph' { [IO.File]::Delete((Join-Path $sourceDir 'codebase-graph.exe')); $expectedTrace = @() }
                'missing_wiki' { [IO.File]::Delete((Join-Path $sourceDir 'k-wiki.exe')) }
                'bad_manifest_graph_hash' { $manifestHashes['codebase-graph.exe'] = $(if ($fixtureHash[0] -eq '0') { '1' } else { '0' }) + $fixtureHash.Substring(1); $expectedTrace = @() }
                'bad_manifest_wiki_hash' { $manifestHashes['k-wiki.exe'] = $(if ($fixtureHash[0] -eq '0') { '1' } else { '0' }) + $fixtureHash.Substring(1) }
                default { throw 'Unregistered scenario' }
            }
            $manifestPath = Join-Path $sourceDir 'checksums.txt'
            $manifestText = $manifestHashes['codebase-graph.exe'] + '  codebase-graph.exe' + "`n" + $manifestHashes['k-wiki.exe'] + "  k-wiki.exe`n"
            [IO.File]::WriteAllText($manifestPath, $manifestText, $utf8)
            $row.manifest_sha256 = Get-PathHash $manifestPath
            if ([IO.File]::Exists($tracePath)) { throw 'Native trace must initially be absent' }
            $childEnvironment = @{
                CBG_A09_GRAPH_EXIT = $graphExit; CBG_A09_WIKI_EXIT = $wikiExit; CBG_A09_TRACE = $tracePath
                CBG_A09_INSTALLER = $derivedPath; CBG_A09_SOURCE = $sourceDir; CBG_A09_BIN = $binDir
                CBG_A09_MODE = $row.mode; CBG_A09_METADATA = $metadataPath
            }
            $row.request = [ordered]@{
                id = $row.id; scenario = $row.scenario; mode = $row.mode; executable = $row.executable
                shell_sha256 = (Get-PathHash $row.executable)
                arguments = ('-NoProfile -NonInteractive -EncodedCommand ' + $encodedControl)
                fixture_environment = [ordered]@{ CBG_A09_GRAPH_EXIT = $graphExit; CBG_A09_WIKI_EXIT = $wikiExit; CBG_A09_TRACE = $tracePath }
                source_dir = $sourceDir; bin_dir = $binDir; derived_installer = $derivedPath; metadata = $metadataPath
                stdout = $stdoutPath; stderr = $stderrPath; expected_trace = $expectedTrace
                expected_installer_success = ($row.scenario -eq 'success_both')
                original_sha256 = $row.original_sha256; derived_sha256 = $row.derived_sha256
                removal_byte_offset = $caseDerived.Offset; fixture_sha256 = $fixtureHash; runner_sha256 = $runnerHash
            }
            Write-Json (Join-Path $row.evidence_path 'request.json') $row.request
            $row.child_exit = Invoke-InstallerChild $row.executable $childEnvironment $stdoutPath $stderrPath
            Write-Json (Join-Path $row.evidence_path 'process-exit.json') ([ordered]@{ exit_code = $row.child_exit })
            $row.new_hashes = [ordered]@{
                'codebase-graph.exe' = (Get-PathHash (Join-Path $binDir 'codebase-graph.exe'))
                'k-wiki.exe' = (Get-PathHash (Join-Path $binDir 'k-wiki.exe'))
            }
            $row.metadata = ConvertFrom-Json ([IO.File]::ReadAllText($metadataPath))
            $row.decoded_trace = Read-NativeTrace $tracePath
            Write-Json (Join-Path $row.evidence_path 'decoded-trace.json') $row.decoded_trace
            $expectedSuccess = $row.scenario -eq 'success_both'
            Add-Assertion $row 'installer-status' (($row.child_exit -eq 0) -eq $expectedSuccess) $(if ($expectedSuccess) { 'zero' } else { 'nonzero' }) $row.child_exit
            Add-Assertion $row 'native-trace-count' ($row.decoded_trace.Count -eq $expectedTrace.Count) $expectedTrace.Count $row.decoded_trace.Count
            for ($i = 0; $i -lt [Math]::Min($expectedTrace.Count, $row.decoded_trace.Count); $i++) {
                $actual = $row.decoded_trace[$i]; $want = $expectedTrace[$i]
                Add-Assertion $row ('native-trace-' + $i) ($actual.Version -eq 1 -and $actual.Argc -eq 1 -and $actual.Role -eq $want.Role -and $actual.Argument -ceq $want.Argument -and $actual.SelectedExit -eq $want.SelectedExit) $want $actual
            }
            foreach ($name in @('codebase-graph.exe', 'k-wiki.exe')) {
                $desired = $oldHashes[$name]
                if ($expectedSuccess -and $row.mode -eq 'real') { $desired = $candidateHashes[$name] }
                Add-Assertion $row ('destination-' + $name) ($row.new_hashes[$name] -ceq $desired) $desired $row.new_hashes[$name]
            }
            Add-Assertion $row 'destination-file-count' (@([IO.Directory]::GetFiles($binDir)).Count -eq 2) 2 (@([IO.Directory]::GetFiles($binDir)).Count)
            $shellMajor = [version]$row.metadata.PSVersion
            $expectedMajor = $(if ($row.shell -eq 'WinPS51') { 5 } else { 7 })
            Add-Assertion $row 'shell-family' ($shellMajor.Major -eq $expectedMajor -and ($row.shell -ne 'WinPS51' -or $shellMajor.Minor -eq 1)) $expectedMajor $row.metadata.PSVersion
            Add-Assertion $row 'OS-architecture' ($row.metadata.OSArchitecture -eq 'Arm64') 'Arm64' $row.metadata.OSArchitecture
            $processArchitecture = $(if ($row.shell -eq 'WinPS51') { 'Arm64' } else { 'X64' })
            Add-Assertion $row 'process-architecture' ($row.metadata.ProcessArchitecture -eq $processArchitecture) $processArchitecture $row.metadata.ProcessArchitecture
            $row.state = $(if (@($row.assertions | Where-Object { -not $_.passed }).Count -eq 0) { 'pass' } else { 'failure' })
        }
        catch { $row.state = 'error'; $row.exception = $_.ToString() }
        finally {
            try { Write-Json (Join-Path $row.evidence_path 'result.json') $row }
            catch { $row.state = 'error'; $row.exception = [string]$row.exception + "`nResult persistence: " + $_.ToString(); $reportErrors.Add($row.id + ': ' + $_.ToString()) }
            try { Write-Json (Join-Path $OutputRoot 'results-partial.json') $rows.ToArray() }
            catch { $reportErrors.Add('Partial result persistence: ' + $_.ToString()) }
        }
    }
}
catch { $abort = $_.ToString() }

foreach ($row in $rows) {
    if ($row.state -eq 'not_run') { $row.exception = 'Not executed: ' + $abort }
}
try { Write-Json (Join-Path $OutputRoot 'results.json') $rows.ToArray() }
catch { $reportErrors.Add('Final result persistence: ' + $_.ToString()) }
try { Write-JUnit (Join-Path $OutputRoot 'desktop-installer-junit.xml') $rows }
catch { $reportErrors.Add('JUnit persistence: ' + $_.ToString()) }
$passCount = @($rows | Where-Object { $_.state -eq 'pass' }).Count
$summary = [ordered]@{
    registered = $rows.Count; passed = $passCount
    failures = @($rows | Where-Object { $_.state -eq 'failure' }).Count
    errors = @($rows | Where-Object { $_.state -in @('error', 'not_run') }).Count
    abort = $abort; reporting_errors = $reportErrors.ToArray()
    invocation = 'approved-derived-copy-fresh-child-memory-not-File'
    qualification = 'COD-56 checkout logic only; not ARM64 product support or final archive qualification'
}
try { Write-Json (Join-Path $OutputRoot 'summary.json') $summary }
catch { $reportErrors.Add('Summary persistence: ' + $_.ToString()) }
if ($passCount -ne 28 -or $rows.Count -ne 28 -or @($rows.id | Select-Object -Unique).Count -ne 28 -or $reportErrors.Count -ne 0 -or $null -ne $abort) {
    [Console]::Error.WriteLine('A09 incomplete or failed; retained evidence at ' + $OutputRoot)
    exit 1
}
Write-Output ('A09: 28/28 actual case assertions passed; evidence at ' + $OutputRoot)
exit 0
