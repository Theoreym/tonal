param(
    [Parameter(Mandatory=$true)][ValidateSet('paused_hidden','playing_hidden','paused_visible','playing_visible')][string]$Scenario,
    [int]$Seconds = 30,
    [int]$WarmupSeconds = 10
)
$ErrorActionPreference = 'Stop'
$tonalRoot = Split-Path $PSScriptRoot -Parent
$tonalOutput = Join-Path $tonalRoot 'target\spotify-comparison'
New-Item -ItemType Directory -Force $tonalOutput | Out-Null
function Get-AppSnapshot {
    $tonalTree = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name)
    $tonalRows = [Collections.Generic.List[object]]::new()
    foreach ($tonalApp in @('tonal','Spotify')) {
        $tonalIds = [Collections.Generic.HashSet[int]]::new()
        foreach ($tonalItem in $tonalTree) {
            if ($tonalItem.Name -ieq ($tonalApp+'.exe')) { [void]$tonalIds.Add([int]$tonalItem.ProcessId) }
        }
        if (-not $tonalIds.Count) { throw ('App not running: '+$tonalApp) }
        do {
            $tonalChanged = $false
            foreach ($tonalItem in $tonalTree) {
                if ($tonalIds.Contains([int]$tonalItem.ParentProcessId) -and $tonalIds.Add([int]$tonalItem.ProcessId)) { $tonalChanged = $true }
            }
        } while ($tonalChanged)
        foreach ($tonalProcessId in $tonalIds) {
            try {
                $tonalProcess = Get-Process -Id $tonalProcessId -ErrorAction Stop
                $tonalRows.Add([pscustomobject]@{App=$tonalApp;Id=$tonalProcessId;StartTicks=$tonalProcess.StartTime.ToUniversalTime().Ticks;CpuSeconds=$tonalProcess.TotalProcessorTime.TotalSeconds;PrivateBytes=$tonalProcess.PrivateMemorySize64;WorkingBytes=$tonalProcess.WorkingSet64;Handles=$tonalProcess.HandleCount;Threads=$tonalProcess.Threads.Count})
            } catch {
                # A transient helper may have exited; fail if no primary app survives below.
                continue
            }
        }
    }
    return $tonalRows
}
Start-Sleep -Seconds $WarmupSeconds
$tonalPrevious = @{}
$tonalInitial = @(Get-AppSnapshot)
foreach ($tonalRow in $tonalInitial) { $tonalPrevious["$($tonalRow.Id):$($tonalRow.StartTicks)"]=$tonalRow.CpuSeconds }
$tonalClock = [Diagnostics.Stopwatch]::StartNew()
$tonalBefore = 0.0
$tonalSamples = [Collections.Generic.List[object]]::new()
while ($tonalClock.Elapsed.TotalSeconds -lt $Seconds) {
    Start-Sleep -Milliseconds 500
    $tonalCurrent = @(Get-AppSnapshot)
    $tonalAt = $tonalClock.Elapsed.TotalSeconds
    $tonalInterval = $tonalAt-$tonalBefore
    foreach ($tonalApp in @('tonal','Spotify')) {
        $tonalAppRows = @($tonalCurrent | Where-Object App -eq $tonalApp)
        if (-not $tonalAppRows.Count) { throw ('Process metrics unavailable: '+$tonalApp) }
        $tonalCpuDelta = 0.0
        foreach ($tonalRow in $tonalAppRows) {
            $tonalKey = "$($tonalRow.Id):$($tonalRow.StartTicks)"
            if ($tonalPrevious.ContainsKey($tonalKey)) { $tonalCpuDelta += [math]::Max([double]0,[double]($tonalRow.CpuSeconds-$tonalPrevious[$tonalKey])) }
            else { $tonalCpuDelta += $tonalRow.CpuSeconds }
        }
        $tonalSamples.Add([pscustomobject]@{
            App=$tonalApp;Scenario=$Scenario;Seconds=[math]::Round($tonalAt,3);IntervalSeconds=$tonalInterval
            CpuSeconds=$tonalCpuDelta;CpuOneCorePercent=100*$tonalCpuDelta/$tonalInterval
            PrivateMiB=($tonalAppRows | Measure-Object PrivateBytes -Sum).Sum/1MB
            WorkingSetSumMiB=($tonalAppRows | Measure-Object WorkingBytes -Sum).Sum/1MB
            Processes=$tonalAppRows.Count;Handles=($tonalAppRows | Measure-Object Handles -Sum).Sum;Threads=($tonalAppRows | Measure-Object Threads -Sum).Sum
        })
    }
    $tonalPrevious=@{}
    foreach ($tonalRow in $tonalCurrent) { $tonalPrevious["$($tonalRow.Id):$($tonalRow.StartTicks)"]=$tonalRow.CpuSeconds }
    $tonalBefore=$tonalAt
}
$tonalStamp=[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
$tonalBase=Join-Path $tonalOutput ($tonalStamp+'-'+$Scenario)
$tonalSamples | Export-Csv ($tonalBase+'.csv') -NoTypeInformation -Delimiter ',' -Encoding utf8
$tonalSamples | ConvertTo-Json -Depth 4 | Set-Content ($tonalBase+'-samples.json') -Encoding utf8
$tonalSummary=@(foreach ($tonalApp in @('tonal','Spotify')) {
    $tonalRows=@($tonalSamples | Where-Object App -eq $tonalApp)
    $tonalDuration=($tonalRows | Measure-Object IntervalSeconds -Sum).Sum
    $tonalCpu=100*($tonalRows | Measure-Object CpuSeconds -Sum).Sum/$tonalDuration
    $tonalMemory=$tonalRows | Measure-Object PrivateMiB -Average -Minimum -Maximum
    $tonalCounts=$tonalRows | Measure-Object Processes -Minimum -Maximum
    $tonalInitialCpu=($tonalInitial | Where-Object App -eq $tonalApp | Measure-Object CpuSeconds -Sum).Sum
    $tonalFinalCpu=($tonalCurrent | Where-Object App -eq $tonalApp | Measure-Object CpuSeconds -Sum).Sum
    [pscustomobject]@{App=$tonalApp;Scenario=$Scenario;DurationSeconds=[math]::Round($tonalDuration,3);Samples=$tonalRows.Count;CpuOneCorePercent=[math]::Round($tonalCpu,4);CpuMachinePercent=[math]::Round($tonalCpu/[Environment]::ProcessorCount,4);InitialCpuSeconds=$tonalInitialCpu;FinalCpuSeconds=$tonalFinalCpu;PrivateMiB=[math]::Round($tonalMemory.Average,2);MinPrivateMiB=[math]::Round($tonalMemory.Minimum,2);MaxPrivateMiB=[math]::Round($tonalMemory.Maximum,2);MinProcesses=$tonalCounts.Minimum;MaxProcesses=$tonalCounts.Maximum}
})
$tonalReport=[pscustomobject]@{SamplerVersion=2;CapturedUtc=[DateTime]::UtcNow.ToString('o');Scenario=$Scenario;LogicalProcessors=[Environment]::ProcessorCount;WarmupSeconds=$WarmupSeconds;CsvNumberCulture=[Globalization.CultureInfo]::CurrentCulture.Name;Method='Sum of private committed memory and CPU time of each app and all discovered descendants. User confirms playback/window state. No app state or settings changed by this script.';Limitations='Shared Windows services and GPU memory/utilization are excluded. Working-set sums may double-count shared pages and are not the headline memory metric. Codec, content and audio quality are not controlled. Exited processes can lose their final sub-sample CPU time.';Results=$tonalSummary}
$tonalReport | ConvertTo-Json -Depth 6 | Set-Content ($tonalBase+'.json') -Encoding utf8
$tonalSummary | ConvertTo-Json -Compress
Write-Output ('Report: '+$tonalBase+'.json')
