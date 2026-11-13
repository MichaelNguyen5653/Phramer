# Scenario 3: same-version reinstall, repair, a rebuilt same-version MSI,
# and a downgrade to 15.0.0.
$msi15 = 'C:\installers\Phramer-15.0.0-win64.msi'
$msi = (Get-ChildItem 'C:\artifacts\Phramer-16.*-win64.msi' | Select-Object -First 1).FullName
$rebuild = Get-ChildItem 'C:\artifacts\rebuild\Phramer-16.*-win64.msi' -ErrorAction SilentlyContinue | Select-Object -First 1

$code = Invoke-Msiexec "/i `"$msi`" /qn" 'install-16'
Add-Check 'install 16: exit 0' ($code -eq 0) $code
$first = Get-PhramerProducts
$cliHash = Get-Sha256 $PhramerCli

$code = Invoke-Msiexec "/i `"$msi`" /qn" 'reinstall-same-msi'
$products = Get-PhramerProducts
Add-Check 'installing the same MSI again: exit 0, still one entry' ($code -eq 0 -and $products.Count -eq 1) ([ordered]@{ exit = $code; products = $products })

Remove-Item $PhramerCli -Force
$code = Invoke-Msiexec "/fa `"$msi`" /qn" 'repair'
Add-Check 'repair (msiexec /fa) restores a deleted file' ($code -eq 0 -and (Get-Sha256 $PhramerCli) -eq $cliHash) ([ordered]@{ exit = $code; restored = (Test-Path $PhramerCli) })

if ($rebuild) {
    $code = Invoke-Msiexec "/i `"$($rebuild.FullName)`" /qn" 'same-version-rebuild'
    $products = Get-PhramerProducts
    Add-Check 'a rebuilt 16.0.0 MSI (new product code) replaces the installed one' (
        $code -eq 0 -and $products.Count -eq 1 -and $products[0].productCode -ne $first[0].productCode) ([ordered]@{
            exit = $code; before = $first; after = $products
        })
}
else {
    Add-Fact 'sameVersionRebuild' 'not tested: no C:\artifacts\rebuild MSI'
}

$code = Invoke-Msiexec "/i `"$msi15`" /qn" 'downgrade-15'
$log = Get-Content (Join-Path $Results 'downgrade-15.log') -Encoding Unicode -ErrorAction SilentlyContinue
$message = @($log | Select-String -SimpleMatch 'A later version of' | ForEach-Object { [string]$_.Line.Trim() }) | Select-Object -First 2
$products = Get-PhramerProducts
Add-Check 'downgrade to 15.0.0 is refused (15.x MajorUpgrade rule)' ($code -ne 0 -and $products.Count -eq 1 -and $products[0].version -eq '16.0.0') ([ordered]@{
        exit = $code; message = $message; products = $products
    })
Add-Check '16 still intact after the refused downgrade' ((Get-FileVersion $PhramerExe) -eq '16.0.0' -and (Test-Path $PhramerCli)) (Get-InstallTree)

$code = Invoke-Msiexec "/x $($products[0].productCode) /qn" 'uninstall-16'
Add-Check 'uninstall 16: exit 0, nothing left' ($code -eq 0 -and (Get-PhramerProducts).Count -eq 0 -and -not (Test-Path $InstallDir)) ([ordered]@{ exit = $code; arp = (Get-ArpEntries) })
