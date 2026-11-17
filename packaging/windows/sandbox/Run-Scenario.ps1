# Inside Windows Sandbox (the .wsb logon command): runs one scenario,
# writes C:\results\results.json and done.txt, then shuts the sandbox down.
param([Parameter(Mandatory = $true)] [string] $Scenario)

$ErrorActionPreference = 'Stop'
$started = Get-Date
Start-Transcript -Path 'C:\results\transcript.txt' | Out-Null
. (Join-Path $PSScriptRoot 'SandboxCommon.ps1')
try {
    Add-Fact 'whoami' (whoami)
    Add-Fact 'highIntegrity' ([bool](whoami /groups | Select-String 'S-1-16-12288'))
    Add-Fact 'webView2Override' (Use-SandboxWebView2)
    . (Join-Path $PSScriptRoot "scenario-$Scenario.ps1")
}
catch {
    Add-Check 'scenario ran to the end' $false ("{0} at {1}" -f $_.Exception.Message, $_.InvocationInfo.PositionMessage)
}
finally {
    try { Stop-Phramer } catch { }
    Save-Results $Scenario $started
    Stop-Transcript | Out-Null
    Set-Content -Path 'C:\results\done.txt' -Value (Get-Date).ToString('o')
    shutdown.exe /s /t 3
}
