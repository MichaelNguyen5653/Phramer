# Scenario 1: fresh 16 MSI install, daemon, settings window, silent uninstall.
$msi = (Get-ChildItem 'C:\artifacts\Phramer-16.*-win64.msi' | Select-Object -First 1).FullName
Add-Fact 'msi' $msi
$webView2 = Get-WebView2Info
Add-Fact 'webView2' $webView2

$code = Invoke-Msiexec "/i `"$msi`" /qn" 'install-16'
Add-Check 'install 16 silently: exit 0' ($code -eq 0) $code

$products = Get-PhramerProducts
Add-Fact 'arpKeys' (Get-ArpEntries)
Add-Check 'one Programs and Features entry, version 16.0.0' ($products.Count -eq 1 -and $products[0].version -eq '16.0.0') $products

$tree = Get-InstallTree
Add-Fact 'installTree' $tree
Add-Check 'executables in Program Files\Phramer\bin' (($tree -contains 'bin\phramer.exe') -and ($tree -contains 'bin\phramer-cli.exe')) $tree
Add-Check 'phramer.exe product version 16.0.0' ((Get-FileVersion $PhramerExe) -eq '16.0.0') (Get-FileVersion $PhramerExe)

$shortcuts = Get-PhramerShortcuts
$startMenu = @($shortcuts | Where-Object { $_.path -like '*Start Menu*' })
Add-Check 'Start Menu shortcut Phramer\Phramer.lnk targets bin\phramer.exe' ($startMenu.Count -eq 1 -and $startMenu[0].target -eq $PhramerExe) $shortcuts

$version = Invoke-Program $PhramerCli @('--version')
Add-Check 'phramer-cli --version reports 16.0.0' ($version.stdout -match '16\.0\.0') $version

$daemon = Start-Daemon $PhramerExe 'daemon'
Add-Check 'daemon starts and keeps running' (-not $daemon.HasExited) ([ordered]@{ pid = $daemon.Id; titles = (Get-VisibleTitles $daemon.Id) })
Start-Sleep -Seconds 2
Add-Fact 'launchEntriesAfterFirstStart' (Get-LaunchEntries)
Add-Fact 'settingsAfterFirstStart' (Get-Lines $SettingsFile)
Save-Screenshot 'fresh-daemon'

$hasWebView2 = [bool]($webView2.machine64 -or $webView2.machine32 -or $webView2.user)
if ($hasWebView2) {
    $config = Invoke-Program $PhramerCli @('config')
    $opened = Wait-Window $daemon.Id 'Phramer Settings' 30
    Add-Check 'phramer-cli config opens the settings window (WebView2)' $opened ([ordered]@{
            cli = $config; titles = (Get-VisibleTitles $daemon.Id)
            webviews = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue).Count
        })
    Start-Sleep -Seconds 3
    Save-Screenshot 'fresh-settings'
}
else {
    Add-Check 'WebView2 runtime present in the sandbox' $false $webView2
}

Stop-Phramer
$code = Invoke-Msiexec "/x `"$msi`" /qn" 'uninstall-16'
Add-Check 'uninstall silently: exit 0' ($code -eq 0) $code
Add-Check 'no Programs and Features entry left' ((Get-PhramerProducts).Count -eq 0) (Get-ArpEntries)
$launch = Get-LaunchEntries
Add-Check 'no Run value and no App Paths entry left' (-not $launch.run -and -not $launch.appPathsKeyExists) $launch
Add-Check 'Program Files\Phramer removed' (-not (Test-Path $InstallDir)) (Get-InstallTree)
Add-Check 'shortcuts removed' ((Get-PhramerShortcuts).Count -eq 0) (Get-PhramerShortcuts)
Add-Check 'settings file kept, as 15.x left it' (Test-Path $SettingsFile) $SettingsFile
