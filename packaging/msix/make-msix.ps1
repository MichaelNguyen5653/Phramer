<#
.SYNOPSIS
    Builds the Microsoft Store MSIX package from the Store build's executables.

.DESCRIPTION
    Expects -InstallDir to hold phramer.exe and phramer-cli.exe from the
    `store` feature build, made from app\ with
        npm run tauri build -- --no-bundle --features store
    (target\release). Only those two files are packed. They link the C
    runtime statically (tauri-build), so the package carries no runtime DLLs.

    The package is left unsigned: the Store re-signs every package it
    certifies. For a sideload test, sign a copy with a certificate whose
    subject equals -Publisher (see README.md).

    Variant "store" declares the ms-screenclip protocol; "noscreenclip" is
    the fallback without it, in case certification objects to the
    declaration. The app adapts to whichever it finds in its manifest.
#>
param(
    [Parameter(Mandatory = $true)] [string] $InstallDir,
    [Parameter(Mandatory = $true)] [string] $Version,
    [Parameter(Mandatory = $true)] [string] $OutDir,
    [ValidateSet('store', 'noscreenclip')] [string] $Variant = 'store',
    # From Partner Center > Product identity. Public values, not secrets.
    [string] $IdentityName = 'Michael-Nguyen.Phramer',
    [string] $Publisher = 'CN=C5782711-0775-466E-A81A-2D97D92962AD',
    [string] $PublisherDisplayName = 'Michael-Nguyen'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Tool([string] $Path, [string[]] $Arguments) {
    & $Path @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$(Split-Path $Path -Leaf) failed with exit code $LASTEXITCODE"
    }
}

# The Store's rules: four fields of 0-65535, a non-zero first field, and a
# fourth field it reserves for itself and requires to be 0
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
    throw "Version '$Version' must be MAJOR.MINOR.PATCH"
}
foreach ($field in $Matches[1..3]) {
    if ([int]$field -gt 65535) { throw "Version field $field exceeds 65535" }
}
if ([int]$Matches[1] -eq 0) { throw 'The major version must not be 0' }

$here = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $here '..\..')).Path
$executables = 'phramer.exe', 'phramer-cli.exe'
foreach ($exe in $executables) {
    if (-not (Test-Path (Join-Path $InstallDir $exe))) {
        throw "$exe not found in $InstallDir; build with --features store first"
    }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$layout = Join-Path $OutDir "layout-$Variant"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Path $layout | Out-Null
foreach ($exe in $executables) { Copy-Item (Join-Path $InstallDir $exe) $layout }

Copy-Item -Path (Join-Path $here 'Assets') -Destination (Join-Path $layout 'Assets') -Recurse
Copy-Item (Join-Path $repo 'packaging\win-installer\LICENSE\GPL-3.0.txt') (Join-Path $layout 'GPL-3.0.txt')
Copy-Item (Join-Path $repo 'THIRD-PARTY-NOTICES.txt') $layout

$tooBig = @(Get-ChildItem (Join-Path $layout 'Assets') -File | Where-Object { $_.Length -ge 204800 })
if ($tooBig.Count -gt 0) {
    throw "Images of 200 KB or more fail certification: $($tooBig.Name -join ', ')"
}

$manifest = Get-Content (Join-Path $here 'AppxManifest.xml.in') -Raw
if ($Variant -eq 'noscreenclip') {
    $manifest = [regex]::Replace($manifest,
        '(?s)[ \t]*<!-- BEGIN screenclip.*?<!-- END screenclip -->\r?\n', '')
    if ($manifest -match 'ms-screenclip') { throw 'The noscreenclip variant still declares ms-screenclip' }
}
$fields = [ordered]@{
    '@IDENTITY_NAME@'          = $IdentityName
    '@PUBLISHER_DISPLAY_NAME@' = $PublisherDisplayName
    '@PUBLISHER@'              = $Publisher
    '@VERSION@'                = $Version
}
foreach ($field in $fields.Keys) {
    $value = [System.Security.SecurityElement]::Escape($fields[$field])
    $manifest = $manifest.Replace($field, $value)
}
if ($manifest -match '@[A-Z_]+@') { throw "Unfilled manifest field: $($Matches[0])" }
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, $utf8)

# The newest SDK that has the packaging tools; the runner image decides
# which that is, so it is found rather than named
$sdk = Get-ChildItem (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin\10.*') -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'x64\makeappx.exe') } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw 'No Windows SDK with makeappx.exe found' }
$tools = Join-Path $sdk.FullName 'x64'
Write-Host "Using Windows SDK tools from $tools"

# The resource index covers only the manifest and Assets. Indexed from the
# whole layout, makepri would read Qt's plugin folders and file names as
# resource qualifiers.
$priRoot = Join-Path $OutDir "pri-$Variant"
if (Test-Path $priRoot) { Remove-Item -Recurse -Force $priRoot }
New-Item -ItemType Directory -Path $priRoot | Out-Null
Copy-Item (Join-Path $layout 'Assets') (Join-Path $priRoot 'Assets') -Recurse
Copy-Item (Join-Path $layout 'AppxManifest.xml') $priRoot
$priConfig = Join-Path $OutDir "priconfig-$Variant.xml"
Invoke-Tool (Join-Path $tools 'makepri.exe') @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
Invoke-Tool (Join-Path $tools 'makepri.exe') @('new', '/pr', $priRoot, '/cf', $priConfig,
    '/mn', (Join-Path $priRoot 'AppxManifest.xml'), '/of', (Join-Path $layout 'resources.pri'), '/o')

$suffix = if ($Variant -eq 'store') { '' } else { "_$Variant" }
$msix = Join-Path $OutDir "Phramer_$Version.0_x64$suffix.msix"
Invoke-Tool (Join-Path $tools 'makeappx.exe') @('pack', '/d', $layout, '/p', $msix, '/h', 'SHA256', '/o')

Write-Host "Packaged $msix"
if ($env:GITHUB_OUTPUT) {
    "msix_$Variant=$msix" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
}
