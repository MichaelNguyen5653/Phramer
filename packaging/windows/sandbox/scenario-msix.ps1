# Scenario 4: the Store MSIX. Test-signed here with a throwaway certificate
# that exists only in this sandbox; installed, checked against 15's Store
# manifest, launched, updated to a 16.0.1.0 build of the same identity with
# the user's settings in place, and removed.
$IdentityName = 'Michael-Nguyen.Phramer'
$Publisher = 'CN=C5782711-0775-466E-A81A-2D97D92962AD'
$FamilyName = 'Michael-Nguyen.Phramer_czsk4xdqz4qyg'
$signtool = 'C:\sdk\signtool.exe'

$work = 'C:\work'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$packages = [ordered]@{
    '16.0.0.0' = (Get-ChildItem 'C:\artifacts\msix\Phramer_16.0.0.0_x64.msix').FullName
    '16.0.1.0' = (Get-ChildItem 'C:\artifacts\msix\Phramer_16.0.1.0_x64.msix').FullName
}

$cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
    -FriendlyName 'Phramer sandbox test' -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
$cer = Join-Path $work 'test.cer'
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null
$signed = [ordered]@{}
foreach ($version in $packages.Keys) {
    $copy = Join-Path $work (Split-Path $packages[$version] -Leaf)
    Copy-Item $packages[$version] $copy
    $sign = Invoke-Program $signtool @('sign', '/fd', 'SHA256', '/sha1', $cert.Thumbprint, '/s', 'My', $copy) 60
    Add-Check "sign $version with the sandbox-only test certificate" ($sign.exitCode -eq 0) $sign
    $signed[$version] = $copy
}

# 16.0.0.0
Add-AppxPackage -Path $signed['16.0.0.0']
$pkg = @(Get-AppxPackage -Name $IdentityName)
Add-Check 'install 16.0.0.0: one package, family name as in the Store' ($pkg.Count -eq 1 -and $pkg[0].Version -eq '16.0.0.0' -and $pkg[0].PackageFamilyName -eq $FamilyName) ($pkg | Select-Object Name, Version, PackageFamilyName, Publisher, InstallLocation)

function Get-ManifestFacts([string] $Path) {
    [xml]$x = Get-Content $Path -Raw
    $n = { param($name) @($x.SelectNodes("//*[local-name()='$name']")) }
    $identity = (& $n 'Identity')[0]
    $app = (& $n 'Application')[0]
    $visual = (& $n 'VisualElements')[0]
    $startup = (& $n 'StartupTask')[0]
    $protocol = (& $n 'Protocol')[0]
    $alias = (& $n 'ExecutionAlias')[0]
    [ordered]@{
        identityName         = $identity.Name
        publisher            = $identity.Publisher
        architecture         = $identity.ProcessorArchitecture
        displayName          = (& $n 'DisplayName')[0].InnerText
        publisherDisplayName = (& $n 'PublisherDisplayName')[0].InnerText
        logo                 = (& $n 'Logo')[0].InnerText
        applicationIds       = @((& $n 'Application') | ForEach-Object { $_.Id })
        executable           = $app.Executable
        entryPoint           = $app.EntryPoint
        visual               = @($visual.DisplayName, $visual.Description, $visual.BackgroundColor, $visual.Square150x150Logo, $visual.Square44x44Logo) -join ' | '
        startupTask          = @($startup.TaskId, $startup.Enabled, $startup.DisplayName) -join ' | '
        protocol             = $(if ($protocol) { @($protocol.Name, $protocol.Parameters) -join ' | ' })
        alias                = $(if ($alias) { $alias.Alias })
        capabilities         = @((& $n 'Capability') | ForEach-Object { $_.Name })
        deviceFamily         = @((& $n 'TargetDeviceFamily') | ForEach-Object { "$($_.Name) $($_.MinVersion) $($_.MaxVersionTested)" })
        version              = $identity.Version
    }
}
$installed = Get-ManifestFacts (Join-Path $pkg[0].InstallLocation 'AppxManifest.xml')
$v15 = Get-ManifestFacts 'C:\artifacts\msix\v15-AppxManifest.xml'
$differences = @(foreach ($key in $v15.Keys) {
        if ($key -eq 'version') { continue }
        $a = $v15[$key] | ConvertTo-Json -Compress
        $b = $installed[$key] | ConvertTo-Json -Compress
        if ($a -ne $b) { "$key`: 15=$a 16=$b" }
    })
Add-Fact 'manifest16' $installed
Add-Check "identity, application, startup task, protocol and alias match 15's Store manifest" ($differences.Count -eq 0) $differences

# The startup task, asked for from inside the package
try {
    Invoke-CommandInDesktopPackage -PackageFamilyName $FamilyName -AppId 'Phramer' -Command 'powershell.exe' `
        -Args '-NoProfile -ExecutionPolicy Bypass -File C:\harness\startup-task-probe.ps1 -Out C:\results\startup-task-16.0.0.0.json' -PreventBreakaway
    Start-Sleep -Seconds 10
}
catch { Add-Fact 'invokeInPackageError' $_.Exception.Message }
$task = Get-Content 'C:\results\startup-task-16.0.0.0.json' -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
Add-Check 'startup task PhramerStartup registered (disabled until the user turns it on)' ($task -and $task.taskId -eq 'PhramerStartup') $task

Start-Process "shell:AppsFolder\$FamilyName!Phramer"
Start-Sleep -Seconds 8
$running = @(Get-Process phramer -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*WindowsApps*' })
Add-Check 'launch from the package: phramer.exe runs from WindowsApps' ($running.Count -ge 1) ($running | Select-Object Id, Path)
Save-Screenshot 'msix-launched'

$alias = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\phramer-cli.exe'
$version = Invoke-Program $alias @('--version')
Add-Check 'phramer-cli execution alias answers --version' ($version.stdout -match '16\.0\.0') $version

# The user's settings, written by the packaged app itself, plus a key 16
# does not know added by hand
$set = Invoke-Program $alias @('config', '--maincolor', '#123456', '--filename', 'msix-test_%F')
Add-Fact 'cliConfig' $set
Start-Sleep -Seconds 2
Stop-Phramer
$redirected = Join-Path $env:LOCALAPPDATA "Packages\$FamilyName\LocalCache\Roaming\phramer\phramer.ini"
$where = [ordered]@{ redirected = (Test-Path $redirected); real = (Test-Path $SettingsFile) }
Add-Fact 'settingsLocation' $where
$ini = if ($where.redirected) { $redirected } else { $SettingsFile }
Add-Check 'the packaged app saved its settings' (Test-Path $ini) $where
Add-Content -Path $ini -Value "`r`n[ThirdPartyGroup]`r`nunknown%20key=kept across the update" -NoNewline -Encoding UTF8
Copy-Item $ini (Join-Path $Results 'msix-phramer-before.ini')
$hashBefore = Get-Sha256 $ini

# 16.0.1.0 over it
Add-AppxPackage -Path $signed['16.0.1.0']
$pkg = @(Get-AppxPackage -Name $IdentityName)
Add-Check 'update to 16.0.1.0: one package, same family name' ($pkg.Count -eq 1 -and $pkg[0].Version -eq '16.0.1.0' -and $pkg[0].PackageFamilyName -eq $FamilyName) ($pkg | Select-Object Name, Version, PackageFamilyName)
Add-Check 'settings file unchanged by the update' ((Get-Sha256 $ini) -eq $hashBefore) ([ordered]@{ path = $ini; before = $hashBefore; after = (Get-Sha256 $ini) })

Start-Process "shell:AppsFolder\$FamilyName!Phramer"
Start-Sleep -Seconds 8
$running = @(Get-Process phramer -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*WindowsApps*16.0.1.0*' })
Add-Check 'the updated package launches' ($running.Count -ge 1) ($running | Select-Object Id, Path)
$check = Invoke-Program $alias @('config', '--check')
Add-Fact 'cliCheckAfterUpdate' $check
Start-Sleep -Seconds 2
Stop-Phramer
Copy-Item $ini (Join-Path $Results 'msix-phramer-after.ini')
Add-Check 'settings still byte-identical after 16.0.1.0 ran' ((Get-Sha256 $ini) -eq $hashBefore) (Get-Sha256 $ini)
$keys = Get-Lines $ini
Add-Check 'the settings set before the update are still there' (($keys -contains 'uiColor=#123456') -and ($keys -contains 'filenamePattern=msix-test_%F') -and ($keys -contains 'unknown%20key=kept across the update')) $keys

try {
    Invoke-CommandInDesktopPackage -PackageFamilyName $FamilyName -AppId 'Phramer' -Command 'powershell.exe' `
        -Args '-NoProfile -ExecutionPolicy Bypass -File C:\harness\startup-task-probe.ps1 -Out C:\results\startup-task-16.0.1.0.json' -PreventBreakaway
    Start-Sleep -Seconds 10
}
catch { }
Add-Fact 'startupTaskAfterUpdate' (Get-Content 'C:\results\startup-task-16.0.1.0.json' -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json)
Stop-Phramer

Remove-AppxPackage -Package $pkg[0].PackageFullName
Start-Sleep -Seconds 3
Add-Check 'uninstall: package gone' (@(Get-AppxPackage -Name $IdentityName).Count -eq 0) $null
Add-Check 'uninstall: execution alias gone' (-not (Test-Path $alias)) $alias
Add-Fact 'packageDataAfterUninstall' (Test-Path (Join-Path $env:LOCALAPPDATA "Packages\$FamilyName"))
