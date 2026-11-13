# Run inside the package's context (Invoke-CommandInDesktopPackage), where
# StartupTask resolves against the package manifest. Writes the task's state.
param([string] $Out = 'C:\results\startup-task.json')
try {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
        $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
        $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
    } | Select-Object -First 1
    $type = [Windows.ApplicationModel.StartupTask, Windows.ApplicationModel, ContentType = WindowsRuntime]
    $task = $asTask.MakeGenericMethod($type).Invoke($null, @($type::GetAsync('PhramerStartup')))
    [void]$task.Wait(15000)
    $result = [ordered]@{ taskId = $task.Result.TaskId; state = $task.Result.State.ToString() }
}
catch {
    $result = [ordered]@{ error = $_.Exception.ToString() }
}
$result | ConvertTo-Json | Set-Content -Encoding utf8 $Out
