$ErrorActionPreference = 'Stop'
$stage = 'C:\Mac\Home\Documents\codebasegraph-cod110-win-repro-b5aa2377'
$out = Join-Path $stage 'actual-local-temp'
New-Item -ItemType Directory -Path $out | Out-Null
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'
$env:CARGO_NET_OFFLINE = 'true'
$env:GIT_OPTIONAL_LOCKS = '0'
$src = Join-Path $env:TEMP 'codebasegraph-cod110-win-repro-b5aa2377'
if (Test-Path -LiteralPath $src) { throw 'Owned guest TEMP source already exists; refuse reuse' }
$expected = Get-Content -LiteralPath (Join-Path $stage 'expected-snapshot.json') -Raw | ConvertFrom-Json
$sha = [System.Security.Cryptography.SHA256]::Create()
function File-Sha([string]$path) {
  $stream = [System.IO.File]::OpenRead($path)
  try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant() } finally { $stream.Dispose() }
}
function Write-Json($value, [string]$name) { $value | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $out $name) -Encoding UTF8 }
function Metadata-Bytes {
  $rows = @()
  foreach ($relative in $expected.six_metadata) {
    $path = Join-Path $src $relative
    $bytes = [IO.File]::ReadAllBytes($path)
    $lf = 0; $cr = 0; $crlf = 0
    for ($i = 0; $i -lt $bytes.Length; $i++) {
      if ($bytes[$i] -eq 10) { $lf++ }
      if ($bytes[$i] -eq 13) { $cr++; if ($i+1 -lt $bytes.Length -and $bytes[$i+1] -eq 10) { $crlf++ } }
    }
    $rows += [ordered]@{path=$relative; bytes=$bytes.Length; sha256=(File-Sha $path); cr=$cr; lf=$lf; crlf=$crlf; bare_lf=($lf-$crlf); bare_cr=($cr-$crlf)}
  }
  return $rows
}
try {
  $actualTarHash = File-Sha (Join-Path $stage 'snapshot.tar')
  if ($actualTarHash -ne $expected.tar_sha256) { throw 'Archive hash mismatch' }
  New-Item -ItemType Directory -Path $src | Out-Null
  & tar.exe -xf (Join-Path $stage 'snapshot.tar') -C $src 1> (Join-Path $out 'extract.stdout.txt') 2> (Join-Path $out 'extract.stderr.txt')
  if ($LASTEXITCODE -ne 0) { throw "Archive extraction exit $LASTEXITCODE" }
  $hashRows = @()
  foreach ($property in $expected.tracked_file_sha256.PSObject.Properties) {
    $actualHash = File-Sha (Join-Path $src $property.Name)
    $hashRows += [ordered]@{path=$property.Name; sha256=$actualHash; expected=$property.Value; matches=($actualHash -eq $property.Value)}
    if ($actualHash -ne $property.Value) { throw "Extracted source mismatch: $($property.Name)" }
  }
  Write-Json $hashRows 'all-extracted-input-hashes.json'
  Write-Json (Metadata-Bytes) 'six-metadata-extracted-lf.json'
  Push-Location $src
  & git.exe config --show-origin --get-all core.autocrlf 1> (Join-Path $out 'existing-core-autocrlf.stdout.txt') 2> (Join-Path $out 'existing-core-autocrlf.stderr.txt')
  $configExit = $LASTEXITCODE
  & git.exe init 1> (Join-Path $out 'git-init.stdout.txt') 2> (Join-Path $out 'git-init.stderr.txt')
  if ($LASTEXITCODE -ne 0) { throw "Private git init exit $LASTEXITCODE" }
  & git.exe -c core.autocrlf=false add -- $expected.six_metadata 1> (Join-Path $out 'git-add.stdout.txt') 2> (Join-Path $out 'git-add.stderr.txt')
  if ($LASTEXITCODE -ne 0) { throw "Private git add exit $LASTEXITCODE" }
  & git.exe -c user.name=COD110-local-regression -c user.email=cod110-regression@example.invalid -c commit.gpgsign=false -c core.hooksPath=NUL commit -m 'test: bind unchanged six metadata snapshot' 1> (Join-Path $out 'git-commit.stdout.txt') 2> (Join-Path $out 'git-commit.stderr.txt')
  if ($LASTEXITCODE -ne 0) { throw "Private git commit exit $LASTEXITCODE" }
  & git.exe ls-files --eol -- $expected.six_metadata 1> (Join-Path $out 'git-eol-before.txt') 2> (Join-Path $out 'git-eol-before.stderr.txt')
  & git.exe -c core.autocrlf=true checkout-index --force --all 1> (Join-Path $out 'git-controlled-checkout.stdout.txt') 2> (Join-Path $out 'git-controlled-checkout.stderr.txt')
  if ($LASTEXITCODE -ne 0) { throw "Controlled Windows checkout exit $LASTEXITCODE" }
  & git.exe ls-files --eol -- $expected.six_metadata 1> (Join-Path $out 'git-eol-after.txt') 2> (Join-Path $out 'git-eol-after.stderr.txt')
  & git.exe rev-parse HEAD 1> (Join-Path $out 'private-fixture-commit.txt')
  Write-Json (Metadata-Bytes) 'six-metadata-controlled-crlf.json'
  $seven = @()
  foreach ($property in $expected.seven_source_sha256.PSObject.Properties) {
    $actualHash = File-Sha (Join-Path $src $property.Name)
    $seven += [ordered]@{path=$property.Name; sha256=$actualHash; expected=$property.Value; matches=($actualHash -eq $property.Value)}
    if ($actualHash -ne $property.Value) { throw "Seven source identity mismatch: $($property.Name)" }
  }
  Write-Json $seven 'seven-source-hashes-before-test.json'
  $env:CARGO_TARGET_DIR = Join-Path $src 'target'
  $cargo = 'C:\Users\rabii\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe'
  $rustc = 'C:\Users\rabii\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe'
  $env:PATH = (Split-Path $cargo) + ';' + $env:PATH
  $args = @('test','--offline','--locked','-p','xtask','--bin','xtask','--profile','native-tests','release::tests::candidate_metadata_accepts_current_real_generated_six_file_layout','--','--exact','--nocapture')
  $start = [ordered]@{scope='One unchanged COD110 local Windows regression; controlled command-local autocrlf=true only on six metadata files in disposable fixture'; source_commit=$expected.commit; source_tree=$expected.tree; archive_sha256=$actualTarHash; source=$src; command=$cargo; arguments=$args; rustc=$rustc; runtime_arch='Existing x86_64 Windows MSVC Rust 1.99.1 under ARM64 Windows 11 emulation'; environment=@{RUSTUP_TOOLCHAIN=$env:RUSTUP_TOOLCHAIN; CARGO_NET_OFFLINE=$env:CARGO_NET_OFFLINE; GIT_OPTIONAL_LOCKS=$env:GIT_OPTIONAL_LOCKS; CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR}; git_existing_core_autocrlf_exit=$configExit; started_utc=[DateTime]::UtcNow.ToString('o'); invocation_count=1; retry_authorized=$false}
  Write-Json $start 'command-start.json'
  Write-Output ('ACTUAL_CARGO_COMMAND_START ' + ($start | ConvertTo-Json -Depth 8 -Compress))
  $process = Start-Process -FilePath $cargo -ArgumentList $args -WorkingDirectory $src -NoNewWindow -PassThru -Wait -RedirectStandardOutput (Join-Path $out 'cargo.stdout.txt') -RedirectStandardError (Join-Path $out 'cargo.stderr.txt')
  $result = [ordered]@{exit_code=$process.ExitCode; ended_utc=[DateTime]::UtcNow.ToString('o'); process_id=$process.Id; invocation_count=1; source_commit=$expected.commit; source_tree=$expected.tree; retry_performed=$false}
  Write-Json $result 'command-result.json'
  Write-Json (Metadata-Bytes) 'six-metadata-after-test.json'
  Write-Output ('ACTUAL_CARGO_COMMAND_END ' + ($result | ConvertTo-Json -Compress))
  Get-Content -LiteralPath (Join-Path $out 'cargo.stdout.txt')
  Get-Content -LiteralPath (Join-Path $out 'cargo.stderr.txt')
  Pop-Location
  exit $process.ExitCode
} catch {
  Write-Json ([ordered]@{error=$_.Exception.Message; detail=$_.ToString(); ended_utc=[DateTime]::UtcNow.ToString('o'); actual_test_started=(Test-Path (Join-Path $out 'command-start.json')); retry_performed=$false}) 'environment-or-staging-failure.json'
  Write-Error $_ -ErrorAction Continue
  exit 125
}
