<#
Runs the workspace's live tests inside Windows Sandbox: every #[ignore]d
test (live smoke and timing tests) and the desktop tests the host skips
(phramer-capture's `live` target, the `win::clipboard` tests). None of them
may run on a developer's desktop; here they get a desktop of their own.

Usage, from the repository root on the host (no elevation):

    # 1. Build the test executables, then the app with its frontend
    #    (npm run tauri build), and stage them (runs no test)
    .\packaging\windows\sandbox\Stage-SandboxTests.ps1 -ArtifactsDir <artifacts>

    # 2. Run them in a fresh sandbox; prints one line per test
    .\packaging\windows\sandbox\Invoke-SandboxScenario.ps1 -Scenario tests `
        -ArtifactsDir <artifacts> -ResultsDir <results> -TimeoutMinutes 90

Stage-SandboxTests.ps1 parameters: -ArtifactsDir (required); -DesktopFilter,
test-name substrings to run although not ignored (default win::clipboard);
-DesktopTargets, <package>.<target> whose tests all run (default
phramer-capture.live).

Inside the sandbox each test runs on its own, in staging order:

    <exe> <test> --exact [--ignored] --nocapture --test-threads=1

from a local, writable copy of C:\artifacts at C:\phramer, as from an
installed folder, with
PHRAMER_SMOKE_EXE=C:\phramer\phramer.exe (phramer-cli.exe beside it) and a
time limit of $TestTimeoutSeconds. Before the first test the daemon runs
once and is stopped, so the profile is one Phramer has run in (the smoke
tests require the settings file and the sign-in entry to stay unchanged,
and a first run writes both). The checkout is mapped read-only at
its host path, because test executables find fixtures and scripts through
the CARGO_MANIFEST_DIR compiled into them (the checkout must be on C:), and
the host's Node.js at C:\tools\nodejs, for settings_smoke's DevTools driver.

Results (the run's folder under -ResultsDir):
    results.json      one check per test: pass, status (PASS, FAIL, TIMEOUT),
                      seconds, exit code, whether the test printed "skipped",
                      Phramer processes left running (stopped before the next
                      test), the output's last lines
    tests\<n>.out.txt and tests\<n>.err.txt, each test's full stdout, stderr
    tests\<n>.png     the screen after a test that did not pass

The sandbox has one monitor, at the resolution of its window, and networking
off: tests that need a second monitor or the network say so in their output.
#>

$TestTimeoutSeconds = 300
$manifest = Get-Content 'C:\artifacts\tests.json' -Raw | ConvertFrom-Json
$logs = Join-Path $Results 'tests'
New-Item -ItemType Directory -Force -Path $logs | Out-Null

# Run from a local, writable copy, as from an installed folder; the mapped
# one is read-only
$run = 'C:\phramer'
Copy-Item 'C:\artifacts' $run -Recurse
$env:PHRAMER_SMOKE_EXE = Join-Path $run 'phramer.exe'
$env:RUST_BACKTRACE = '1'
if (Test-Path 'C:\tools\nodejs\node.exe') { $env:Path = "C:\tools\nodejs;$env:Path" }


# A profile Phramer has run in once, as on a user's machine: the first run
# writes the settings file and the sign-in entry, which the smoke tests
# require to stay unchanged
$env:PHRAMER_NO_WELCOME = '1'
Start-Process -FilePath $env:PHRAMER_SMOKE_EXE | Out-Null
Remove-Item Env:PHRAMER_NO_WELCOME
$deadline = (Get-Date).AddSeconds(30)
while (-not (Test-Path $SettingsFile) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
Start-Sleep -Seconds 5
Get-Process -Name 'phramer', 'msedgewebview2' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Add-Fact 'warmUp' ([ordered]@{ settings = (Get-Lines $SettingsFile); launch = (Get-LaunchEntries) })

Add-Fact 'stagedCommit' $manifest.commit
Add-Fact 'staged' $manifest.staged
Add-Fact 'node' (Test-Path 'C:\tools\nodejs\node.exe')
Add-Fact 'screen' ("{0}" -f (& {
            Add-Type -AssemblyName System.Windows.Forms
            [void][PhramerHarness.Windows]::SetProcessDPIAware()
            [System.Windows.Forms.Screen]::AllScreens | ForEach-Object { "$($_.Bounds.Width)x$($_.Bounds.Height)" }
        }))

# Phramer and Snipping Tool processes a test left behind
function Get-Leftovers {
    , @(Get-Process -Name 'phramer', 'phramer-cli', 'SnippingTool', 'ScreenClippingHost', 'msedgewebview2' -ErrorAction SilentlyContinue |
            ForEach-Object { "$($_.ProcessName) $($_.Id)" })
}

$n = 0
foreach ($t in $manifest.tests) {
    $n++
    $name = "{0}: {1}" -f ($t.binary -replace '\.exe$', ''), $t.test
    $exe = Join-Path $run "tests\$($t.binary)"
    $arguments = @("`"$($t.test)`"", '--exact')
    if ($t.ignored) { $arguments += '--ignored' }
    $arguments += '--nocapture', '--test-threads=1'

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = $arguments -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = Join-Path $run 'tests'
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $p = [System.Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $finished = $p.WaitForExit($TestTimeoutSeconds * 1000)
    if (-not $finished) { & taskkill.exe /T /F /PID $p.Id | Out-Null; $p.WaitForExit(10000) | Out-Null }
    $watch.Stop()
    $stdout = $out.Result
    $stderr = $err.Result
    [System.IO.File]::WriteAllText((Join-Path $logs "$n.out.txt"), $stdout)
    [System.IO.File]::WriteAllText((Join-Path $logs "$n.err.txt"), $stderr)

    # Exit code 0 without "1 passed" means the filter matched nothing
    $ran = $stdout -match '(?m)^test result: ok\. 1 passed'
    $status = if (-not $finished) { 'TIMEOUT' } elseif ($p.ExitCode -eq 0 -and $ran) { 'PASS' } else { 'FAIL' }
    if ($status -ne 'PASS') { try { Save-Screenshot "tests\$n" } catch { } }
    $leftovers = Get-Leftovers
    if ($leftovers.Count -gt 0) {
        Get-Process -Name 'phramer', 'phramer-cli', 'SnippingTool', 'ScreenClippingHost', 'msedgewebview2' -ErrorAction SilentlyContinue | Stop-Process -Force
        Start-Sleep -Seconds 2
    }
    $tail = @(($stdout + "`n" + $stderr) -split "`r?`n" | Where-Object { $_ -ne '' } | Select-Object -Last 25)
    Add-Check $name ($status -eq 'PASS') ([ordered]@{
            n         = $n
            status    = $status
            seconds   = [math]::Round($watch.Elapsed.TotalSeconds, 1)
            exitCode  = $(if ($finished) { $p.ExitCode } else { $null })
            skipped   = $stdout.Contains('skipped:')
            leftovers = $leftovers
            tail      = $tail
        })
}
