<#
.SYNOPSIS
    Stages the release binaries and the workspace's live test executables
    for scenario-tests.ps1. Runs on the host; runs no test.

.DESCRIPTION
    Builds the test executables (`cargo test --workspace --release --no-run`,
    with the path remapping `npm run tauri build` uses, so the two builds
    share one cache), then the app (`npm run tauri build -- --no-bundle`), in
    that order: the test build leaves a phramer.exe in target\release that
    has no frontend inside (it loads the dev server's URL, so no window's
    page ever loads). Lists each test executable's tests with `--list`,
    which runs nothing. Copies into -ArtifactsDir:

        phramer.exe, phramer-cli.exe   the app, from `npm run tauri build`
        tests\<package>.<target>.exe   every test executable with a test to run
        tests\vcruntime140*.dll        the host's VC++ runtime, which they load
        tests.json                     the tests to run, one entry per test

    A test is staged when it is #[ignore]d (live and timing tests), when its
    name contains one of -DesktopFilter (desktop tests the host skips), or
    when it is in one of -DesktopTargets (test targets the host never runs).

.EXAMPLE
    .\packaging\windows\sandbox\Stage-SandboxTests.ps1 -ArtifactsDir $env:TEMP\phramer-tests
#>
param(
    [Parameter(Mandatory = $true)] [string] $ArtifactsDir,
    [string[]] $DesktopFilter = @('win::clipboard'),
    [string[]] $DesktopTargets = @('phramer-capture.live')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$release = Join-Path $root 'target\release'
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
$cargo = if ($cargo) { $cargo.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $env:Path = "$(Split-Path $cargo);$env:Path" }

# The flags of app/scripts/remap-paths.mjs, so this build does not
# invalidate the one `npm run tauri build` made
$saved = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $root
try {
    $env:CARGO_ENCODED_RUSTFLAGS = & node --input-type=module -e "import { withRemappedPaths } from './app/scripts/remap-paths.mjs'; process.stdout.write(withRemappedPaths(process.env).CARGO_ENCODED_RUSTFLAGS);"
    if ($LASTEXITCODE -ne 0) { throw 'node could not compute the path remapping' }
    # Cargo's progress goes to stderr, which Windows PowerShell turns into
    # errors when this script's output is redirected
    $ErrorActionPreference = 'Continue'
    $messages = & $cargo test --workspace --release --no-run --message-format=json
    if ($LASTEXITCODE -ne 0) { throw "cargo test --no-run failed ($LASTEXITCODE)" }
    # The app last, over the frontend-less phramer.exe the tests left
    $env:CARGO_ENCODED_RUSTFLAGS = $saved
    Push-Location (Join-Path $root 'app')
    try { & npm run tauri build -- --no-bundle | Out-Host } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "npm run tauri build failed ($LASTEXITCODE)" }
    $ErrorActionPreference = 'Stop'
}
finally {
    $ErrorActionPreference = 'Stop'
    Pop-Location
    $env:CARGO_ENCODED_RUSTFLAGS = $saved
}

$binaries = @()
foreach ($line in $messages) {
    if ($line -notmatch '"reason":"compiler-artifact"' -or $line -notmatch '"test":true') { continue }
    $m = $line | ConvertFrom-Json
    # Not the app's own binaries: run with --list they start the app's
    # command line parser, not a test harness
    if (-not $m.executable -or $m.target.kind -contains 'bin') { continue }
    # path+file:///C:/repos/phramer/crates/phramer-overlay#16.0.0 names the
    # package by its folder; #name@version when the two differ
    $fragment = $m.package_id.Split('#')[-1]
    $package = if ($fragment -match '^(.+)@') { $Matches[1] } else { ($m.package_id.Split('#')[0].TrimEnd('/') -split '/')[-1] }
    $binaries += [pscustomobject]@{ package = $package; target = $m.target.name; executable = $m.executable }
}

# Through Process rather than `&`: the app's own test executable is a
# windows-subsystem program, which `&` neither waits for nor reads
function Get-TestNames([string] $Exe, [string[]] $Arguments) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = (@($Arguments) + @('--list', '--format', 'terse')) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.CreateNoWindow = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $listed = $p.StandardOutput.ReadToEnd() -split "`r?`n"
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) { throw "$Exe --list failed ($($p.ExitCode))" }
    , @($listed | Where-Object { $_ -match ': test$' } | ForEach-Object { $_ -replace ': test$', '' })
}

$testsDir = Join-Path $ArtifactsDir 'tests'
if (Test-Path $testsDir) { Remove-Item -Recurse -Force $testsDir }
New-Item -ItemType Directory -Force -Path $testsDir | Out-Null
Copy-Item (Join-Path $release 'phramer.exe'), (Join-Path $release 'phramer-cli.exe') $ArtifactsDir
# Test executables link the VC++ runtime dynamically and Windows Sandbox's
# image has none (exit 0xC0000135): the host's copy goes beside them
foreach ($dll in 'vcruntime140.dll', 'vcruntime140_1.dll') {
    $source = Join-Path $env:SystemRoot "System32\$dll"
    if (Test-Path $source) { Copy-Item $source $testsDir }
}

$tests = @()
foreach ($b in $binaries) {
    $ignored = Get-TestNames $b.executable @('--ignored')
    $all = Get-TestNames $b.executable @()
    $key = "$($b.package).$($b.target)"
    $desktop = @($all | Where-Object {
            $name = $_
            ($ignored -notcontains $name) -and (($DesktopTargets -contains $key) -or
                @($DesktopFilter | Where-Object { $name.Contains($_) }).Count -gt 0)
        })
    if ($ignored.Count + $desktop.Count -eq 0) { continue }
    $staged = "$key.exe"
    Copy-Item $b.executable (Join-Path $testsDir $staged)
    foreach ($t in $ignored) { $tests += [ordered]@{ binary = $staged; test = $t; ignored = $true } }
    foreach ($t in $desktop) { $tests += [ordered]@{ binary = $staged; test = $t; ignored = $false } }
    Write-Host ("{0}: {1} ignored, {2} desktop" -f $staged, $ignored.Count, $desktop.Count)
}

$manifest = [ordered]@{
    staged = (Get-Date).ToString('o')
    commit = (& git -C $root rev-parse --short HEAD)
    tests  = $tests
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $ArtifactsDir 'tests.json')
Write-Host ("Staged {0} tests from {1} executables in {2}" -f $tests.Count, @($tests | ForEach-Object { $_.binary } | Sort-Object -Unique).Count, $ArtifactsDir)
