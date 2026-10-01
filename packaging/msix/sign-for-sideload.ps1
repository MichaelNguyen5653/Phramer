<#
.SYNOPSIS
    Makes a test-signed copy of a Store MSIX, for installing it locally
    before submission.

.DESCRIPTION
    Windows installs only signed packages, and the Store signs its own copy
    after certification. For testing before that, this signs a copy with a
    throwaway self-signed certificate whose subject equals the package's
    Publisher, exports the public certificate next to it, and deletes the
    private key, so the certificate can never sign anything else.

    Never upload the signed copy to Partner Center; upload the original.

    To install the copy, trust the exported .cer once, from an elevated
    PowerShell:
        Import-Certificate -FilePath <cer> -CertStoreLocation Cert:\LocalMachine\TrustedPeople
    then:
        Add-AppxPackage <msix>
#>
param(
    [Parameter(Mandatory = $true)] [string] $Msix,
    [Parameter(Mandatory = $true)] [string] $OutDir,
    [string] $Publisher = 'CN=C5782711-0775-466E-A81A-2D97D92962AD'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdk = Get-ChildItem (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin\10.*') -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'x64\signtool.exe') } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw 'No Windows SDK with signtool.exe found' }
$signtool = Join-Path $sdk.FullName 'x64\signtool.exe'

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$name = [System.IO.Path]::GetFileNameWithoutExtension($Msix)
$signed = Join-Path $OutDir "${name}_sideload.msix"
$cer = Join-Path $OutDir "${name}_sideload.cer"
Copy-Item $Msix $signed -Force

# Code signing usage, not a certificate authority
$cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher `
    -KeyUsage DigitalSignature -FriendlyName 'Phramer sideload test' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
try {
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null
    & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /s My $signed
    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
}
finally {
    Remove-Item -Path "Cert:\CurrentUser\My\$($cert.Thumbprint)" -DeleteKey -Force
}

Write-Host "Signed $signed; trust $cer to install it"
