param([string]$Executable = '', [int]$TimeoutSeconds = 300)
$ErrorActionPreference = 'Stop'
$tonalRoot = Split-Path $PSScriptRoot -Parent
if (-not $Executable) { $Executable = Join-Path $tonalRoot 'target\release\tonal.exe' }
$tonalOutput = Join-Path $tonalRoot 'target\performance'
New-Item -ItemType Directory -Force $tonalOutput | Out-Null
$tonalRun = Join-Path $tonalOutput ([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss'))
New-Item -ItemType Directory $tonalRun | Out-Null
$tonalPhases = Join-Path $tonalRun 'phases.jsonl'
$tonalOldProfile = $env:TONAL_PROFILE_TO
$tonalProcess = $null
try {
    $env:TONAL_PROFILE_TO = $tonalPhases
    $tonalProcess = Start-Process -FilePath (Resolve-Path $Executable).Path -WorkingDirectory $tonalRoot -WindowStyle Hidden -PassThru
    $env:TONAL_PROFILE_TO = $tonalOldProfile
    $tonalClock = [Diagnostics.Stopwatch]::StartNew()
    $tonalSamples = [Collections.Generic.List[object]]::new()
    $tonalPreviousCpu = 0.0
    $tonalPreviousAt = 0.0
    $tonalPhase = 'startup'
    while (-not $tonalProcess.HasExited) {
        if ($tonalClock.Elapsed.TotalSeconds -gt $TimeoutSeconds) { throw 'Performance scenario did not finish.' }
        Start-Sleep -Milliseconds 500
        $tonalProcess.Refresh()
        if ($tonalProcess.HasExited) { break }
        $tonalAt = $tonalClock.Elapsed.TotalSeconds
        $tonalCpu = $tonalProcess.TotalProcessorTime.TotalSeconds
        $tonalRecords = @()
        if (Test-Path -LiteralPath $tonalPhases) {
            $tonalRecords = @(Get-Content -LiteralPath $tonalPhases | ForEach-Object { $_ | ConvertFrom-Json })
            if ($tonalRecords.Count) { $tonalPhase = $tonalRecords[-1].phase }
        }
        $tonalPhaseAge = if ($tonalRecords.Count) { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()/1000.0 - $tonalRecords[-1].time } else { 0 }
        $tonalSamples.Add([pscustomobject]@{
            Seconds = [math]::Round($tonalAt,3)
            Phase = $tonalPhase
            PhaseAgeSeconds = [math]::Round($tonalPhaseAge,3)
            CpuOneCorePercent = [math]::Round(100 * ($tonalCpu-$tonalPreviousCpu)/($tonalAt-$tonalPreviousAt),3)
            CpuSeconds = $tonalCpu
            PrivateMiB = [math]::Round($tonalProcess.PrivateMemorySize64/1MB,2)
            WorkingMiB = [math]::Round($tonalProcess.WorkingSet64/1MB,2)
            Handles = $tonalProcess.HandleCount
            Threads = $tonalProcess.Threads.Count
        })
        $tonalPreviousCpu = $tonalCpu
        $tonalPreviousAt = $tonalAt
    }
    $tonalSamples | Export-Csv (Join-Path $tonalRun 'samples.csv') -NoTypeInformation
    if ($tonalProcess.ExitCode -ne 0) { throw 'The performance app exited with an error.' }
    # The process can exit between the final marker and the last process sample.
    $tonalDone = (Test-Path $tonalPhases) -and ((Get-Content $tonalPhases -Tail 1 | ConvertFrom-Json).phase -eq 'done')
    if (-not $tonalDone) { throw 'The app stopped before all performance phases completed.' }
    $tonalSummary = @($tonalSamples | Where-Object { $_.PhaseAgeSeconds -ge 3 -and $_.Phase -notin @('startup','loading','done') } | Group-Object Phase | ForEach-Object {
        $tonalCpuStats = $_.Group | Measure-Object CpuOneCorePercent -Average -Maximum
        $tonalMemory = $_.Group | Measure-Object PrivateMiB -Minimum -Maximum -Average
        [pscustomobject]@{Phase=$_.Name;Samples=$_.Count;MeanCpuOneCorePercent=[math]::Round($tonalCpuStats.Average,3);PeakCpuOneCorePercent=$tonalCpuStats.Maximum;MeanPrivateMiB=[math]::Round($tonalMemory.Average,2);MinPrivateMiB=$tonalMemory.Minimum;MaxPrivateMiB=$tonalMemory.Maximum;FinalPrivateMiB=$_.Group[-1].PrivateMiB}
    })
    $tonalReport = [pscustomobject]@{Executable=(Resolve-Path $Executable).Path;Sha256=(Get-FileHash $Executable -Algorithm SHA256).Hash;LogicalProcessors=[Environment]::ProcessorCount;SampleIntervalMs=500;Muted=$true;PlaylistWrites=$false;PlaybackPersistence=$false;Phases=$tonalSummary}
    $tonalReport | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $tonalRun 'report.json') -Encoding utf8
    $tonalSummary | ConvertTo-Json -Compress
    Write-Output ('Report: '+(Join-Path $tonalRun 'report.json'))
} finally {
    $env:TONAL_PROFILE_TO = $tonalOldProfile
    if ($null -ne $tonalProcess -and -not $tonalProcess.HasExited) { Stop-Process -Id $tonalProcess.Id -Force }
}
