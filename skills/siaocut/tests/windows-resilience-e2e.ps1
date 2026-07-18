param(
    [string]$Core = "",
    [string]$Ffmpeg = "ffmpeg"
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
if (-not $Core) { $Core = Join-Path $root 'target\release\siaocut-core.exe' }
$Core = (Resolve-Path -LiteralPath $Core).Path
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempRoot ('siaocut-resilience-' + [guid]::NewGuid().ToString('N'))
$env:SIAOCUT_HOME = Join-Path $testRoot 'home'
$env:SIAOCUT_SERVICE_IDLE_MS = '100'

function Invoke-CoreJson([string[]]$Arguments) {
    $raw = & $Core --json @Arguments | Out-String
    if ($LASTEXITCODE -ne 0) { throw "SiaoCut command failed: $($Arguments -join ' ') $raw" }
    $value = $raw | ConvertFrom-Json
    if ($value.status -ne 'ok') { throw $value.message }
    return $value
}

function Invoke-CoreError([string[]]$Arguments) {
    $stdout = Join-Path $testRoot ('error-' + [guid]::NewGuid().ToString('N') + '.json')
    $stderr = "$stdout.err"
    $process = Start-Process -FilePath $Core -ArgumentList (@('--json') + $Arguments) -RedirectStandardOutput $stdout -RedirectStandardError $stderr -Wait -PassThru -WindowStyle Hidden
    $stdoutText = if (Test-Path -LiteralPath $stdout) { [IO.File]::ReadAllText($stdout, [Text.Encoding]::UTF8) } else { '' }
    $stderrText = if (Test-Path -LiteralPath $stderr) { [IO.File]::ReadAllText($stderr, [Text.Encoding]::UTF8) } else { '' }
    $raw = ($stdoutText + $stderrText).Trim()
    if ($process.ExitCode -eq 0) { throw "Expected command to fail: $($Arguments -join ' ')" }
    return $raw | ConvertFrom-Json
}

try {
    New-Item -ItemType Directory -Force -Path $testRoot | Out-Null
    $source = Join-Path $testRoot 'source.mp4'
    & $Ffmpeg -y -hide_banner -loglevel error -f lavfi -i 'testsrc2=size=640x360:rate=30' -f lavfi -i 'sine=frequency=660:sample_rate=48000' -t 8 -c:v libx264 -pix_fmt yuv420p -c:a aac $source
    if ($LASTEXITCODE -ne 0) { throw 'Could not generate resilience media fixture.' }
    $project = Invoke-CoreJson @('import', $source, '--title', 'Windows resilience test')
    Invoke-CoreJson @('transcript', 'add', $project.projectId, '--start', '0', '--end', '8', '--text', 'Resilience test') | Out-Null

    $moved = Join-Path $testRoot 'moved.mp4'
    Move-Item -LiteralPath $source -Destination $moved
    $missingAudit = Invoke-CoreJson @('audit', $project.projectId)
    if ($missingAudit.audit.ready -or -not ($missingAudit.audit.issues.code -contains 'media-missing')) { throw 'Moved media was not reported as missing.' }
    $wrong = Join-Path $testRoot 'wrong.mp4'
    [IO.File]::WriteAllBytes($wrong, [byte[]](1, 2, 3, 4))
    $wrongRelink = Invoke-CoreError @('project', 'relink', $project.projectId, $wrong)
    if ($wrongRelink.code -ne 'media_hash_changed') { throw 'Mismatched media was not rejected.' }
    $relinked = Invoke-CoreJson @('project', 'relink', $project.projectId, $moved)
    $storedPath = $relinked.project.media.sourcePath -replace '^\\\\\?\\', ''
    $expectedPath = (Resolve-Path -LiteralPath $moved).Path -replace '^\\\\\?\\', ''
    if (-not [string]::Equals($storedPath, $expectedPath, [StringComparison]::OrdinalIgnoreCase)) { throw "Matching moved media was not relinked: $($relinked.project.media.sourcePath)" }

    $env:SIAOCUT_TEST_AVAILABLE_SPACE_BYTES = '1'
    Start-Sleep -Milliseconds 250
    $lowDisk = Invoke-CoreError @('video', 'export', $project.projectId, '--output', (Join-Path $testRoot 'low-disk.mp4'))
    if ($lowDisk.code -ne 'disk_space_low') { throw 'Low disk did not return disk_space_low.' }
    Remove-Item Env:SIAOCUT_TEST_AVAILABLE_SPACE_BYTES
    Start-Sleep -Milliseconds 250

    $output = Join-Path $testRoot 'recovered.mp4'
    $export = Invoke-CoreJson @('video', 'export', $project.projectId, '--output', $output, '--start-delay-ms', '30000')
    $deadline = (Get-Date).AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $job = (Invoke-CoreJson @('video', 'status', $export.jobId)).job
    } while (-not $job.workerPid -and (Get-Date) -lt $deadline)
    if (-not $job.workerPid) { throw 'Export worker PID was not recorded.' }
    Stop-Process -Id $job.workerPid -Force
    Start-Sleep -Seconds 6
    $interrupted = (Invoke-CoreJson @('video', 'status', $export.jobId)).job
    if ($interrupted.status -ne 'interrupted') { throw "Killed export was not reconciled: $($interrupted.status)" }

    $retried = (Invoke-CoreJson @('video', 'retry', $export.jobId)).job
    $deadline = (Get-Date).AddSeconds(90)
    while ($retried.status -in @('queued', 'running') -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $retried = (Invoke-CoreJson @('video', 'status', $export.jobId)).job
    }
    if ($retried.status -ne 'completed' -or -not (Test-Path -LiteralPath $output)) { throw "Retried export did not complete: $($retried.errorMessage)" }

    [pscustomobject]@{
        windowsBuild = [Environment]::OSVersion.Version.Build
        actualSleepWake = $false
        simulatedWorkerInterruption = $true
        movedMedia = 'relinked-after-hash-check'
        mismatchedMedia = $wrongRelink.code
        lowDisk = $lowDisk.code
        killedExport = $interrupted.status
        retriedExport = $retried.status
        outputBytes = (Get-Item -LiteralPath $output).Length
    } | ConvertTo-Json
} finally {
    Remove-Item Env:SIAOCUT_TEST_AVAILABLE_SPACE_BYTES -ErrorAction SilentlyContinue
    Remove-Item Env:SIAOCUT_SERVICE_IDLE_MS -ErrorAction SilentlyContinue
    Remove-Item Env:SIAOCUT_HOME -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
