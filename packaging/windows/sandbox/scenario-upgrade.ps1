# Scenario 2: 15.0.0 installed and used, then 16 installed over it.
$msi15 = 'C:\installers\Phramer-15.0.0-win64.msi'
$msi = (Get-ChildItem 'C:\artifacts\Phramer-16.*-win64.msi' | Select-Object -First 1).FullName
Add-Fact 'msi' $msi

$code = Invoke-Msiexec "/i `"$msi15`" /qn" 'install-15'
Add-Check 'install 15.0.0 silently: exit 0' ($code -eq 0) $code
$products15 = Get-PhramerProducts
Add-Fact 'products15' $products15
Add-Fact 'arpKeys15' (Get-ArpEntries)
Add-Fact 'shortcuts15' (Get-PhramerShortcuts)
$tree15 = Get-InstallTree
Add-Fact 'installTree15Count' $tree15.Count

# 15's settings: a known file (with keys 16 does not know), read by 15's
# daemon, then launch at sign-in switched on through 15's own CLI
New-Item -ItemType Directory -Force -Path (Split-Path $SettingsFile) | Out-Null
[System.IO.File]::WriteAllText($SettingsFile, (Get-KnownSettingsText), (New-Object System.Text.UTF8Encoding $false))
$daemon15 = Start-Daemon $PhramerExe 'daemon-15'
Add-Check '15.0.0 daemon starts' (-not $daemon15.HasExited) ([ordered]@{ pid = $daemon15.Id; titles = (Get-VisibleTitles $daemon15.Id) })
$autostart = Invoke-Program $PhramerCli @('config', '--autostart', 'true')
Add-Fact 'cli15Autostart' $autostart
Start-Sleep -Seconds 3
Stop-Phramer
$launch15 = Get-LaunchEntries
Add-Check '15.0.0 wrote the Run value for its exe' ($launch15.run -eq $PhramerExe) $launch15

$before = [System.IO.File]::ReadAllBytes($SettingsFile)
Copy-Item $SettingsFile (Join-Path $Results 'phramer-before.ini')
$hashBefore = Get-Sha256 $SettingsFile
Add-Fact 'settingsAfter15' (Get-Lines $SettingsFile)

$code = Invoke-Msiexec "/i `"$msi`" /qn" 'upgrade-16'
Add-Check 'install 16 over 15 silently: exit 0' ($code -eq 0) $code

$products = Get-PhramerProducts
Add-Fact 'arpKeys' (Get-ArpEntries)
Add-Check 'exactly one Programs and Features entry, 16.0.0' ($products.Count -eq 1 -and $products[0].version -eq '16.0.0') $products
Add-Check "15.0.0's product code is gone" (-not ($products | Where-Object { $_.productCode -eq $products15[0].productCode })) $products15

$tree = Get-InstallTree
$leftovers = @($tree | Where-Object { $_ -match 'Qt6|vc_redist|vcruntime|msvcp|\.qm$|^lib\\|^include\\|^bin\\(platforms|styles|imageformats|iconengines|tls|generic|networkinformation|translations)\\' })
Add-Check "15.0.0's files are gone" ($leftovers.Count -eq 0) ([ordered]@{ leftovers = $leftovers; tree = $tree })
Add-Check 'phramer.exe at the 15.x path is 16.0.0' ((Get-FileVersion $PhramerExe) -eq '16.0.0') (Get-FileVersion $PhramerExe)

$launch = Get-LaunchEntries
Add-Check 'Run value kept, naming the 16 exe' ($launch.run -eq $PhramerExe -and (Test-Path $launch.run)) $launch
Add-Check 'settings file untouched by the upgrade' ((Get-Sha256 $SettingsFile) -eq $hashBefore) (Get-Sha256 $SettingsFile)
$startMenu = @(Get-PhramerShortcuts | Where-Object { $_.path -like '*Start Menu*' })
Add-Check 'one Start Menu shortcut, targeting the 16 exe' ($startMenu.Count -eq 1 -and $startMenu[0].target -eq $PhramerExe) (Get-PhramerShortcuts)

$daemon = Start-Daemon $PhramerExe 'daemon-16'
Add-Check '16 daemon starts after the upgrade' (-not $daemon.HasExited) ([ordered]@{ pid = $daemon.Id; titles = (Get-VisibleTitles $daemon.Id) })
# After an upgrade the welcome shows but can be skipped (a first run's cannot)
$welcome = Wait-Window $daemon.Id 'Welcome to Phramer' 30
$optional = Test-LogLine 'daemon-16' 'welcome: window created (mandatory: false)'
Add-Check 'welcome shows after the upgrade, not mandatory' ($welcome -and $optional) ([ordered]@{ shown = $welcome; optionalInLog = $optional })
Save-Screenshot 'upgrade-welcome'
$skipped = Invoke-WindowButton $daemon.Id 'Welcome to Phramer' 'Skip'
$closed = $skipped -and (Wait-NoWindow $daemon.Id 'Welcome to Phramer')
Add-Check 'Skip closes the welcome' $closed ([ordered]@{ skipClicked = $skipped; titles = (Get-VisibleTitles $daemon.Id) })
Add-Fact 'trayIconPromoted' (Show-TrayIcon)
Save-Screenshot 'upgrade-tray'
$check = Invoke-Program $PhramerCli @('config', '--check')
Add-Fact 'cli16Check' $check
$config = Invoke-Program $PhramerCli @('config')
$opened = Wait-Window $daemon.Id 'Phramer Settings' 30
Start-Sleep -Seconds 3
Save-Screenshot 'upgrade-settings'
Add-Check '16 settings window opens on the old settings' $opened (Get-VisibleTitles $daemon.Id)
Start-Sleep -Seconds 2
Stop-Phramer

$after = [System.IO.File]::ReadAllBytes($SettingsFile)
Copy-Item $SettingsFile (Join-Path $Results 'phramer-after.ini')
# The skippable welcome records the new version as it opens; nothing else changes
$expected = (New-Object System.Text.UTF8Encoding $false).GetBytes(
    (Get-KnownSettingsText).Replace('welcomeTourShownFor=v15.0.0', "welcomeTourShownFor=v$($products[0].version)"))
$same = ($expected.Length -eq $after.Length) -and (-not (Compare-Object $expected $after -SyncWindow 0))
$added = @(Compare-Object (Get-Lines (Join-Path $Results 'phramer-before.ini')) (Get-Lines $SettingsFile) |
        ForEach-Object { '{0} {1}' -f $_.SideIndicator, $_.InputObject })
$hashAfter16 = Get-Sha256 $SettingsFile
Add-Check 'settings file after 16 ran: byte-identical but for welcomeTourShownFor' $same ([ordered]@{ hashBefore = $hashBefore; hashAfter = $hashAfter16; lineDiff = $added })

$code = Invoke-Msiexec "/x `"$msi`" /qn" 'uninstall-16'
Add-Check 'uninstall 16 silently: exit 0' ($code -eq 0) $code
Add-Check 'no Programs and Features entry left' ((Get-PhramerProducts).Count -eq 0) (Get-ArpEntries)
$launch = Get-LaunchEntries
Add-Check 'no Run value and no App Paths entry left' (-not $launch.run -and -not $launch.appPathsKeyExists) $launch
Add-Check 'Program Files\Phramer removed' (-not (Test-Path $InstallDir)) (Get-InstallTree)
Add-Check 'shortcuts removed' ((Get-PhramerShortcuts).Count -eq 0) (Get-PhramerShortcuts)
Add-Check 'settings file kept and unchanged' ((Get-Sha256 $SettingsFile) -eq $hashAfter16) (Get-Sha256 $SettingsFile)
