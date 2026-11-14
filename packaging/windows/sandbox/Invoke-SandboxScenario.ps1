<#
.SYNOPSIS
    Runs one install/upgrade scenario in a fresh Windows Sandbox and waits
    for its results.

.DESCRIPTION
    Writes a .wsb that maps this folder (the in-sandbox scripts) and the
    artifact folders read-only, and a new results folder read-write; starts
    Windows Sandbox with networking off; and waits until the scenario writes
    done.txt and shuts the sandbox down. On timeout it stops only the
    WindowsSandbox process this script started.

    Nothing is installed on the host. Windows Sandbox needs no elevation to
    start, and the logon command runs as the sandbox's administrator.

    Folders inside the sandbox:
        C:\harness     this folder (read-only)
        C:\artifacts   -ArtifactsDir: 16 MSIs, MSIX packages (read-only)
        C:\installers  -InstallersDir: the 15.0.0 MSI (read-only)
        C:\sdk         -SdkBinDir: Windows SDK x64 bin, for signtool (read-only)
        C:\results     a new folder under -ResultsDir (read-write)
        tests only: the checkout at its host path, and Node.js at
        C:\tools\nodejs (read-only); see scenario-tests.ps1

.EXAMPLE
    .\Invoke-SandboxScenario.ps1 -Scenario fresh -ArtifactsDir D:\out -ResultsDir D:\results
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('probe', 'fresh', 'upgrade', 'repair', 'msix', 'tests')]
    [string] $Scenario,
    [Parameter(Mandatory = $true)] [string] $ArtifactsDir,
    [Parameter(Mandatory = $true)] [string] $ResultsDir,
    [string] $InstallersDir,
    [string] $SdkBinDir,
    [int] $TimeoutMinutes = 20
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sandboxExe = Join-Path $env:SystemRoot 'System32\WindowsSandbox.exe'
if (-not (Test-Path $sandboxExe)) { throw "Windows Sandbox is not available ($sandboxExe)" }
if (Get-Process -Name 'WindowsSandbox*' -ErrorAction SilentlyContinue) {
    throw 'A Windows Sandbox is already running; only one can run at a time'
}

New-Item -ItemType Directory -Force -Path $ResultsDir | Out-Null
$runDir = Join-Path (Resolve-Path $ResultsDir).Path ("{0}-{1}" -f $Scenario, (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $runDir | Out-Null

function Get-MappedFolder([string] $HostFolder, [string] $SandboxFolder, [bool] $ReadOnly) {
    $folder = [System.Security.SecurityElement]::Escape((Resolve-Path $HostFolder).Path)
    "    <MappedFolder><HostFolder>$folder</HostFolder><SandboxFolder>$SandboxFolder</SandboxFolder><ReadOnly>$($ReadOnly.ToString().ToLower())</ReadOnly></MappedFolder>"
}

$maps = @(
    Get-MappedFolder $PSScriptRoot 'C:\harness' $true
    Get-MappedFolder $ArtifactsDir 'C:\artifacts' $true
    Get-MappedFolder $runDir 'C:\results' $false
)
if ($InstallersDir) { $maps += Get-MappedFolder $InstallersDir 'C:\installers' $true }
if ($SdkBinDir) { $maps += Get-MappedFolder $SdkBinDir 'C:\sdk' $true }
if ($Scenario -eq 'tests') {
    # Test executables find fixtures and scripts through the checkout path
    # compiled into them; settings_smoke drives the webview with Node.js
    $repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
    $maps += Get-MappedFolder $repo $repo $true
    $node = Get-Command node -ErrorAction SilentlyContinue
    if ($node) { $maps += Get-MappedFolder (Split-Path $node.Source) 'C:\tools\nodejs' $true }
}

# Networking off: nothing inside may download (WebView2 bootstrapper, Store)
$command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\harness\Run-Scenario.ps1 -Scenario $Scenario"
$wsb = @"
<Configuration>
  <Networking>Disable</Networking>
  <MappedFolders>
$($maps -join "`r`n")
  </MappedFolders>
  <LogonCommand><Command>$([System.Security.SecurityElement]::Escape($command))</Command></LogonCommand>
</Configuration>
"@
$wsbPath = Join-Path $runDir "$Scenario.wsb"
[System.IO.File]::WriteAllText($wsbPath, $wsb, (New-Object System.Text.UTF8Encoding $false))

$started = Get-Date
$process = Start-Process -FilePath $sandboxExe -ArgumentList "`"$wsbPath`"" -PassThru
Write-Host "Started Windows Sandbox (PID $($process.Id)) for '$Scenario'; results in $runDir"

# WindowsSandbox.exe hands the VM to WindowsSandboxRemoteSession and
# WindowsSandboxServer and exits. Those, started by this launch (none ran
# before it, checked above), are what a timeout has to stop.
$ours = @{ $process.Id = $process }
$watchUntil = (Get-Date).AddSeconds(60)
while ((Get-Date) -lt $watchUntil -and $ours.Count -lt 3) {
    foreach ($p in Get-Process -Name 'WindowsSandbox*' -ErrorAction SilentlyContinue) {
        if (-not $ours.ContainsKey($p.Id) -and $p.StartTime -ge $started.AddSeconds(-1)) { $ours[$p.Id] = $p }
    }
    Start-Sleep -Seconds 2
}
Write-Host ("Sandbox processes from this launch: {0}" -f (($ours.Values | ForEach-Object { "$($_.ProcessName) $($_.Id)" }) -join ', '))

$done = Join-Path $runDir 'done.txt'
$deadline = $started.AddMinutes($TimeoutMinutes)
$timedOut = $false
while (-not (Test-Path $done)) {
    if ((Get-Date) -gt $deadline) { $timedOut = $true; break }
    Start-Sleep -Seconds 5
}
$finished = Get-Date

# The scenario ends with shutdown /s; give the VM time to close by itself
$closeBy = (Get-Date).AddMinutes(2)
$running = { @($ours.Values | Where-Object { -not $_.HasExited }) }
while (@(& $running).Count -gt 0 -and (Get-Date) -lt $closeBy) { Start-Sleep -Seconds 2 }
$forced = $false
foreach ($p in @(& $running)) {
    Write-Warning "Stopping $($p.ProcessName) (PID $($p.Id)), started by this run"
    Stop-Process -Id $p.Id -Force
    $forced = $true
}

$summary = [ordered]@{
    scenario       = $Scenario
    started        = $started.ToString('o')
    scenarioDone   = $finished.ToString('o')
    seconds        = [math]::Round(($finished - $started).TotalSeconds)
    timedOut       = $timedOut
    sandboxStopped = if ($forced) { 'forced' } else { 'shut down by scenario' }
    resultsDir     = $runDir
}
$summary | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $runDir 'host-run.json')
$results = Join-Path $runDir 'results.json'
if (Test-Path $results) {
    $r = Get-Content $results -Raw | ConvertFrom-Json
    Write-Host ("{0}: {1} ({2} s)" -f $Scenario, $(if ($r.pass) { 'PASS' } else { 'FAIL' }), $summary.seconds)
    foreach ($check in $r.checks) {
        Write-Host ("  [{0}] {1}" -f $(if ($check.pass) { 'ok' } else { 'FAIL' }), $check.name)
    }
}
else {
    Write-Warning "No results.json (timed out: $timedOut)"
}
$summary
