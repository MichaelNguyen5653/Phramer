<#
.SYNOPSIS
    Names Tauri's MSI as a release asset and writes its checksum file.

.DESCRIPTION
    `npm run tauri build -- --bundles msi` writes
    target\release\bundle\msi\Phramer_<version>_x64_en-US.msi. Releases and
    the in-app update check use the name 15.x established,
    Phramer-<version>-win64.msi, and a .sha256sum beside it in 15.x's exact
    layout: lowercase hash, two spaces, file name, CRLF, no BOM.

    The version is read from the MSI itself (ProductVersion), so the asset
    can never be named after a different version than it installs.

.EXAMPLE
    .\packaging\windows\release-assets.ps1 -Msi target\release\bundle\msi\Phramer_16.0.0_x64_en-US.msi -OutDir dist
#>
param(
    [Parameter(Mandatory = $true)] [string] $Msi,
    [Parameter(Mandatory = $true)] [string] $OutDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Msi = (Resolve-Path $Msi).Path
$installer = New-Object -ComObject WindowsInstaller.Installer
# Mode 0: read-only
$database = $installer.OpenDatabase($Msi, 0)
$view = $database.OpenView("SELECT Value FROM Property WHERE Property = 'ProductVersion'")
$view.Execute()
$record = $view.Fetch()
if (-not $record) { throw "$Msi has no ProductVersion" }
$version = $record.StringData(1)
$view.Close()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "ProductVersion '$version' is not MAJOR.MINOR.PATCH" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$name = "Phramer-$version-win64.msi"
$asset = Join-Path (Resolve-Path $OutDir).Path $name
Copy-Item $Msi $asset -Force

$hash = (Get-FileHash $asset -Algorithm SHA256).Hash.ToLower()
$sum = "$asset.sha256sum"
[System.IO.File]::WriteAllText($sum, "$hash  $name`r`n", (New-Object System.Text.UTF8Encoding $false))

Write-Host "Wrote $asset"
Write-Host "Wrote $sum ($hash)"
