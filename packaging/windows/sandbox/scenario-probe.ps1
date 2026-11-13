# What the sandbox offers before anything is installed: WebView2, AppX
# deployment, sideloading, and what the mapped folders hold.
Add-Fact 'webView2' (Get-WebView2Info)
Add-Fact 'appxService' ((Get-Service AppXSvc -ErrorAction SilentlyContinue).Status | Out-String).Trim()
Add-Fact 'allowAllTrustedApps' (Get-RegValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Appx' 'AllowAllTrustedApps')
Add-Fact 'appModelUnlock' (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue | Select-Object AllowAllTrustedApps, AllowDevelopmentWithoutDevLicense)
Add-Fact 'vcRuntime' (Test-Path "$env:SystemRoot\System32\vcruntime140.dll")
Add-Fact 'artifacts' @(Get-ChildItem C:\artifacts -ErrorAction SilentlyContinue | ForEach-Object Name)
Add-Fact 'installers' @(Get-ChildItem C:\installers -ErrorAction SilentlyContinue | ForEach-Object Name)
Add-Fact 'phramerProducts' (Get-PhramerProducts)
Add-Check 'probe ran' $true
