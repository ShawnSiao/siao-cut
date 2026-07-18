param(
    [string]$Core,
    [string]$Ffmpeg = "ffmpeg",
    [string]$Ffprobe = "ffprobe"
)

$ErrorActionPreference = "Stop"
if (-not $Core) { $Core = Join-Path $PSScriptRoot "..\..\..\target\debug\siaocut-core.exe" }
$Core = (Resolve-Path -LiteralPath $Core).Path
$testHome = Join-Path ([System.IO.Path]::GetTempPath()) ("siaocut-video-" + [guid]::NewGuid().ToString("N"))
$previousHome = $env:SIAOCUT_HOME
$previousIdle = $env:SIAOCUT_SERVICE_IDLE_MS
$launcher = $null

function Invoke-SiaoCut([string[]]$Arguments) {
    $raw = & $Core --json @Arguments | Out-String
    if ($LASTEXITCODE -ne 0) { throw "SiaoCut command failed: $($Arguments -join ' ') $raw" }
    $result = $raw | ConvertFrom-Json
    if ($result.status -ne "ok") { throw "SiaoCut returned $($result.status): $raw" }
    return $result
}

try {
    New-Item -ItemType Directory -Path $testHome | Out-Null
    $env:SIAOCUT_HOME = Join-Path $testHome "home"
    $env:SIAOCUT_SERVICE_IDLE_MS = "100"
    $media = Join-Path $testHome "source.mp4"
    & $Ffmpeg -y -hide_banner -loglevel error `
        -f lavfi -i "testsrc2=size=640x360:rate=30" `
        -f lavfi -i "sine=frequency=880:sample_rate=48000" `
        -t 8 -c:v libx264 -pix_fmt yuv420p -c:a aac $media
    if ($LASTEXITCODE -ne 0) { throw "Could not generate the video fixture" }

    $project = Invoke-SiaoCut @("import", $media, "--title", "Video pipeline test")
    $segmentA = Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "0", "--end", "2", "--text", "Opening line")
    $segmentCut = Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "2", "--end", "3", "--text", "um")
    $segmentB = Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "3", "--end", "8", "--text", "Final line")
    $detected = Invoke-SiaoCut @("cut", "detect", $project.projectId)
    $cutId = $detected.suggestions[0].id
    Invoke-SiaoCut @("cut", "apply", $project.projectId, $cutId) | Out-Null

    $prepared = Invoke-SiaoCut @("media", "prepare", $project.projectId)
    if ($prepared.artifacts.status -ne "ready") { throw "Preview artifacts are not ready" }
    if (-not (Test-Path -LiteralPath $prepared.artifacts.proxyPath)) { throw "Proxy video is missing" }
    if (-not (Test-Path -LiteralPath $prepared.artifacts.waveformPath)) { throw "Waveform is missing" }
    if ($prepared.artifacts.thumbnails.Count -lt 1) { throw "Thumbnails are missing" }
    if ([Math]::Abs([double]$prepared.project.timeline.outputDuration - 7.0) -gt 0.01) { throw "Timeline did not remove exactly one second" }
    $proxyTimestamp = (Get-Item -LiteralPath $prepared.artifacts.proxyPath).LastWriteTimeUtc
    $preparedAgain = Invoke-SiaoCut @("media", "prepare", $project.projectId)
    if ((Get-Item -LiteralPath $preparedAgain.artifacts.proxyPath).LastWriteTimeUtc -ne $proxyTimestamp) { throw "Ready proxy was regenerated without a source change" }

    $output = Join-Path $testHome "output.mp4"
    $export = Invoke-SiaoCut @("video", "export", $project.projectId, "--output", $output, "--burn-subtitles")
    $job = $export.job
    $deadline = (Get-Date).AddSeconds(90)
    while ($job.status -in @("queued", "running") -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $job = (Invoke-SiaoCut @("video", "status", $export.jobId)).job
    }
    if ($job.status -ne "completed") { throw "Video export did not complete: $($job.errorMessage)" }
    if (-not (Test-Path -LiteralPath $output)) { throw "Final video is missing" }
    if (-not (Test-Path -LiteralPath $job.manifestPath)) { throw "Export manifest is missing" }
    $duration = [double](& $Ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 $output)
    if ([Math]::Abs($duration - 7.0) -gt 0.05) { throw "Final duration does not match the preview timeline: $duration" }

    $token = [guid]::NewGuid().ToString("N")
    $cancelJobId = "x-test-$token"
    $cancelOutput = Join-Path $testHome "cancelled.mp4"
    $cancelPartial = Join-Path $testHome "cancelled.part.mp4"
    $stdout = Join-Path $testHome "cancel-create.json"
    $stderr = Join-Path $testHome "cancel-create.err"
    $launcher = Start-Process -FilePath $Core `
        -ArgumentList @("--json", "video", "export", $project.projectId, "--output", $cancelOutput, "--start-delay-ms", "3000", "--job-id", $cancelJobId) `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr `
        -WindowStyle Hidden `
        -PassThru
    Start-Sleep -Milliseconds 500
    $cancelled = Invoke-SiaoCut @("video", "cancel", $cancelJobId)
    $cancelJob = $cancelled.job
    $deadline = (Get-Date).AddSeconds(20)
    while ($cancelJob.status -in @("queued", "running") -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 100
        $cancelJob = (Invoke-SiaoCut @("video", "status", $cancelJobId)).job
    }
    if ($cancelJob.status -ne "cancelled") { throw "Export did not enter cancelled state: $($cancelJob.status)" }
    if (Test-Path -LiteralPath $cancelOutput) { throw "Cancelled export left a final output" }
    if (Test-Path -LiteralPath $cancelPartial) { throw "Cancelled export left a partial output" }

    [pscustomobject]@{
        status = "ok"
        projectId = $project.projectId
        sourceDuration = 8.0
        outputDuration = $duration
        removedDuration = 1.0
        thumbnails = $prepared.artifacts.thumbnails.Count
        exportStatus = $job.status
        cancelStatus = $cancelJob.status
        segments = @($segmentA.segment.id, $segmentCut.segment.id, $segmentB.segment.id)
    } | ConvertTo-Json
}
finally {
    if ($launcher -and -not $launcher.HasExited) { $launcher.WaitForExit(5000) | Out-Null }
    $env:SIAOCUT_HOME = $previousHome
    $env:SIAOCUT_SERVICE_IDLE_MS = $previousIdle
    if (Test-Path -LiteralPath $testHome) { Remove-Item -LiteralPath $testHome -Recurse -Force }
}
