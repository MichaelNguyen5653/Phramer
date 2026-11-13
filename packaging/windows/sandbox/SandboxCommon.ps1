# Helpers for the scenarios, dot-sourced inside Windows Sandbox by
# Run-Scenario.ps1. Results go to C:\results; see Invoke-SandboxScenario.ps1.

$Results = 'C:\results'
$UpgradeCode = '{42607922-E95D-4601-A561-F750286F20B6}'
$InstallDir = Join-Path $env:ProgramFiles 'Phramer'
$BinDir = Join-Path $InstallDir 'bin'
$PhramerExe = Join-Path $BinDir 'phramer.exe'
$PhramerCli = Join-Path $BinDir 'phramer-cli.exe'
$SettingsFile = Join-Path $env:APPDATA 'phramer\phramer.ini'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$AppPathsKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\phramer.exe'

$script:Checks = New-Object System.Collections.ArrayList
$script:Facts = [ordered]@{}

function Add-Check([string] $Name, [bool] $Pass, $Detail = $null) {
    [void]$script:Checks.Add([ordered]@{ name = $Name; pass = $Pass; detail = $Detail })
    Write-Host ("[{0}] {1} {2}" -f $(if ($Pass) { 'ok' } else { 'FAIL' }), $Name, ($Detail | ConvertTo-Json -Compress -Depth 6))
}

function Add-Fact([string] $Name, $Value) {
    $script:Facts[$Name] = $Value
    Write-Host ("fact {0} = {1}" -f $Name, ($Value | ConvertTo-Json -Compress -Depth 6))
}

function Save-Results([string] $Scenario, [datetime] $Started) {
    $failed = @($script:Checks | Where-Object { -not $_.pass })
    $result = [ordered]@{
        scenario = $Scenario
        pass     = ($script:Checks.Count -gt 0 -and $failed.Count -eq 0)
        started  = $Started.ToString('o')
        seconds  = [math]::Round(((Get-Date) - $Started).TotalSeconds)
        os       = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').BuildLabEx
        checks   = $script:Checks
        facts    = $script:Facts
    }
    $result | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $Results 'results.json')
}

# msiexec with a verbose log in the results folder; returns the exit code
function Invoke-Msiexec([string] $Arguments, [string] $LogName) {
    $log = Join-Path $Results "$LogName.log"
    $p = Start-Process msiexec.exe -ArgumentList "$Arguments /l*v `"$log`"" -Wait -PassThru
    Write-Host "msiexec $Arguments -> $($p.ExitCode)"
    $p.ExitCode
}

# Installed Phramer products in every context (Windows Installer's view)
function Get-PhramerProducts {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $contexts = @{ 1 = 'user-managed'; 2 = 'user-unmanaged'; 4 = 'machine' }
    $list = @()
    foreach ($product in $installer.ProductsEx('', 's-1-1-0', 7)) {
        $name = try { $product.InstallProperty('ProductName') } catch { '' }
        if ($name -notlike 'Phramer*') { continue }
        # Late-bound properties read as $null through PowerShell's adapter
        $get = { param($n) $product.GetType().InvokeMember($n, 'GetProperty', $null, $product, $null) }
        $list += [ordered]@{
            productCode     = (& $get 'ProductCode')
            context         = $contexts[[int](& $get 'Context')]
            version         = $product.InstallProperty('VersionString')
            installLocation = try { $product.InstallProperty('InstallLocation') } catch { '' }
        }
    }
    , $list
}

# Programs and Features entries (uninstall keys in every hive Windows reads)
function Get-ArpEntries {
    $roots = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
    $list = @()
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        foreach ($key in Get-ChildItem $root) {
            $p = Get-ItemProperty $key.PSPath
            if ($p.PSObject.Properties['DisplayName'] -and $p.DisplayName -like 'Phramer*') {
                $list += [ordered]@{
                    key            = $key.Name
                    displayName    = $p.DisplayName
                    displayVersion = $(if ($p.PSObject.Properties['DisplayVersion']) { $p.DisplayVersion })
                    publisher      = $(if ($p.PSObject.Properties['Publisher']) { $p.Publisher })
                    systemComponent = $(if ($p.PSObject.Properties['SystemComponent']) { $p.SystemComponent })
                }
            }
        }
    }
    # Per-user Windows Installer products are listed from the installer's
    # own data rather than an Uninstall key
    $userData = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData'
    if (Test-Path $userData) {
        foreach ($props in Get-ChildItem $userData -Recurse -ErrorAction SilentlyContinue |
                Where-Object { $_.PSChildName -eq 'InstallProperties' }) {
            $p = Get-ItemProperty $props.PSPath
            if ($p.PSObject.Properties['DisplayName'] -and $p.DisplayName -like 'Phramer*') {
                $list += [ordered]@{
                    key            = $props.Name
                    displayName    = $p.DisplayName
                    displayVersion = $p.DisplayVersion
                    publisher      = $p.Publisher
                    systemComponent = $(if ($p.PSObject.Properties['SystemComponent']) { $p.SystemComponent })
                }
            }
        }
    }
    , $list
}

function Get-RegValue([string] $Key, [string] $Name) {
    if (-not (Test-Path $Key)) { return $null }
    $p = Get-ItemProperty $Key
    if ($p.PSObject.Properties[$Name]) { $p.$Name } else { $null }
}

function Get-LaunchEntries {
    [ordered]@{
        run      = Get-RegValue $RunKey 'Phramer'
        appPaths = Get-RegValue $AppPathsKey 'Path'
        appPathsKeyExists = Test-Path $AppPathsKey
    }
}

# Shortcuts named Phramer in the per-user and all-users Start Menu and desktop
function Get-PhramerShortcuts {
    $shell = New-Object -ComObject WScript.Shell
    $places = @(
        (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Phramer\Phramer.lnk'),
        (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Phramer\Phramer.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Phramer.lnk'),
        (Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'Phramer.lnk')
    )
    $list = @()
    foreach ($path in $places) {
        if (Test-Path $path) {
            $lnk = $shell.CreateShortcut($path)
            $list += [ordered]@{ path = $path; target = $lnk.TargetPath; workingDirectory = $lnk.WorkingDirectory }
        }
    }
    , $list
}

function Get-InstallTree {
    if (-not (Test-Path $InstallDir)) { return , @() }
    , @(Get-ChildItem $InstallDir -Recurse -File | ForEach-Object {
            $_.FullName.Substring($InstallDir.Length + 1)
        })
}

function Get-FileVersion([string] $Path) {
    if (-not (Test-Path $Path)) { return $null }
    (Get-Item $Path).VersionInfo.ProductVersion
}

function Get-Sha256([string] $Path) {
    if (-not (Test-Path $Path)) { return $null }
    (Get-FileHash $Path -Algorithm SHA256).Hash
}

function Get-WebView2Info {
    $guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    [ordered]@{
        machine64 = Get-RegValue "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid" 'pv'
        machine32 = Get-RegValue "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid" 'pv'
        user      = Get-RegValue "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid" 'pv'
        folder    = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Directory -ErrorAction SilentlyContinue | ForEach-Object Name)
    }
}

# Windows Sandbox shares the host's EdgeWebView folder but keeps its base
# image's registry, whose `pv` can name a version that folder lacks; the
# loader then reports no runtime. Point WebView2 at the newest version
# present, for every process of this user (shell launches included).
function Use-SandboxWebView2 {
    $info = Get-WebView2Info
    $root = "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application"
    $registered = $info.machine64, $info.machine32, $info.user | Where-Object { $_ } | Select-Object -First 1
    if ($registered -and (Test-Path (Join-Path $root "$registered\msedgewebview2.exe"))) {
        return [ordered]@{ registered = $registered; override = $null }
    }
    $newest = Get-ChildItem $root -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'msedgewebview2.exe') } |
        Sort-Object { [version]$_.Name } | Select-Object -Last 1
    if ($newest) {
        $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER = $newest.FullName
        [Environment]::SetEnvironmentVariable('WEBVIEW2_BROWSER_EXECUTABLE_FOLDER', $newest.FullName, 'User')
    }
    [ordered]@{ registered = $registered; override = $(if ($newest) { $newest.FullName }) }
}

# A console program's output and exit code, with a time limit
function Invoke-Program([string] $Path, [string[]] $Arguments, [int] $TimeoutSeconds = 30) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Path
    $psi.Arguments = ($Arguments | ForEach-Object { if ($_ -match '\s') { "`"$_`"" } else { $_ } }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
        $p.Kill()
        return [ordered]@{ exitCode = 'timeout'; stdout = $out.Result; stderr = $err.Result }
    }
    [ordered]@{ exitCode = $p.ExitCode; stdout = $out.Result.Trim(); stderr = $err.Result.Trim() }
}

# The daemon, with stderr (its log) captured to the results folder
function Start-Daemon([string] $Exe, [string] $LogName) {
    $p = Start-Process -FilePath $Exe -RedirectStandardError (Join-Path $Results "$LogName-stderr.log") `
        -RedirectStandardOutput (Join-Path $Results "$LogName-stdout.log") -PassThru
    Start-Sleep -Seconds 6
    $p.Refresh()
    $p
}

function Stop-Phramer {
    Get-Process -Name 'phramer', 'phramer-cli' -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
}

if (-not ('PhramerHarness.Windows' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace PhramerHarness {
    public static class Windows {
        delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
        [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int max);
        [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
        [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
        // Titles of the visible top-level windows of one process
        public static string[] VisibleTitles(int processId) {
            var titles = new List<string>();
            EnumWindows((hwnd, l) => {
                uint pid;
                GetWindowThreadProcessId(hwnd, out pid);
                if (pid == processId && IsWindowVisible(hwnd)) {
                    var text = new StringBuilder(256);
                    GetWindowTextW(hwnd, text, 256);
                    titles.Add(text.ToString());
                }
                return true;
            }, IntPtr.Zero);
            return titles.ToArray();
        }
    }
}
'@
}

function Get-VisibleTitles([int] $ProcessId) {
    , @([PhramerHarness.Windows]::VisibleTitles($ProcessId))
}

# Waits for a visible window with this title in the process
function Wait-Window([int] $ProcessId, [string] $Title, [int] $Seconds = 30) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if ((Get-VisibleTitles $ProcessId) -contains $Title) { return $true }
        Start-Sleep -Milliseconds 500
    }
    $false
}

# Settings as 15.0.0 left them for a user who finished its welcome tour,
# plus keys 16 does not know. Written byte for byte; QSettings' layout.
# 16, a new major version, shows its tour again and records
# welcomeTourShownFor only once the tour's last step is reached, as 15 did.
function Get-KnownSettingsText {
    @(
        '[General]',
        'contrastUiColor=#270032',
        'drawColor=#ff0000',
        'drawThickness=7',
        'filenamePattern=upgrade-test_%F_%H-%M',
        'savePath=C:/Users/Public/Pictures',
        'savePathFixed=true',
        'showDesktopNotification=false',
        'showHelp=false',
        'showWelcomeMessage=false',
        'startupLaunch=true',
        'uiColor=#123456',
        'userColors=picker, #800000, #ff0000, #ffff00, #00ff00, #008000, #00ffff, #0000ff, #ff00ff, #800080',
        'welcomeTourShownFor=v15.0.0',
        'zzFutureKeyNot16=kept verbatim',
        '',
        '[Shortcuts]',
        'TYPE_COPY=Ctrl+C',
        'TYPE_SAVE=Ctrl+S',
        '',
        '[ThirdPartyGroup]',
        'unknown%20key=with spaces and = sign',
        ''
    ) -join "`r`n"
}

# Moves Phramer's tray icon out of the overflow so a screenshot shows it
function Show-TrayIcon {
    $root = 'HKCU:\Control Panel\NotifyIconSettings'
    $promoted = @()
    foreach ($key in Get-ChildItem $root -ErrorAction SilentlyContinue) {
        $path = Get-RegValue $key.PSPath 'ExecutablePath'
        if ($path -like '*phramer.exe') {
            Set-ItemProperty $key.PSPath -Name 'IsPromoted' -Value 1 -Type DWord
            $promoted += $path
        }
    }
    Start-Sleep -Seconds 2
    , $promoted
}

# The primary screen as a PNG in the results folder (tray, open windows)
function Save-Screenshot([string] $Name) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    # Physical pixels, so the whole screen (tray included) is captured
    [void][PhramerHarness.Windows]::SetProcessDPIAware()
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save((Join-Path $Results "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
}

# Plain strings: Get-Content's carry provider properties into the JSON
function Get-Lines([string] $Path) {
    if (-not (Test-Path $Path)) { return , @() }
    , [string[]][System.IO.File]::ReadAllLines($Path)
}
