param(
    [string]$TestUrl = 'https://www.youtube.com/watch?v=HOfdboHvshg',
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$core = Join-Path $root 'target\debug\siaocut-core.exe'
$ytDlp = Join-Path $root 'apps\desktop\src-tauri\runtime\yt-dlp\yt-dlp.exe'
$ffmpeg = Join-Path $root 'apps\desktop\src-tauri\runtime\ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $root 'apps\desktop\src-tauri\runtime\ffmpeg\ffprobe.exe'
foreach ($path in $core, $ytDlp, $ffmpeg, $ffprobe) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required executable is missing: $path"
    }
}

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = [IO.Path]::GetFullPath((Join-Path $temporaryRoot ('siaocut-source-e2e-' + [guid]::NewGuid().ToString('N'))))
if (-not $work.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Resolved test directory is outside the system temporary directory.'
}
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Invoke-Core {
    param([string[]]$Arguments)
    $raw = & $core --json @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw ($raw | Out-String)
    }
    return (($raw | Out-String) | ConvertFrom-Json)
}

function Wait-SourceStatus {
    param(
        [string]$JobId,
        [string[]]$Wanted,
        [int]$Seconds = 30
    )
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        Start-Sleep -Milliseconds 100
        $response = Invoke-Core -Arguments @('source', 'status', $JobId)
        $script:maxProgress = [Math]::Max($script:maxProgress, [double]$response.sourceJob.progress)
        $script:statuses.Add([string]$response.sourceJob.status) | Out-Null
        if ($Wanted -contains [string]$response.sourceJob.status) {
            return $response.sourceJob
        }
        if ([string]$response.sourceJob.status -in @('failed', 'interrupted')) {
            throw "Source job ended as $($response.sourceJob.status): $($response.sourceJob.errorMessage)"
        }
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for source job status: $($Wanted -join ', ')"
}

$previousHome = $env:SIAOCUT_HOME
$previousDirect = $env:SIAOCUT_DIRECT
$previousYtDlp = $env:SIAOCUT_YTDLP
$previousFfmpeg = $env:SIAOCUT_FFMPEG
$previousFfprobe = $env:SIAOCUT_FFPROBE
try {
    $env:SIAOCUT_HOME = $work
    $env:SIAOCUT_DIRECT = '1'
    $env:SIAOCUT_YTDLP = $ytDlp
    $env:SIAOCUT_FFMPEG = $ffmpeg
    $env:SIAOCUT_FFPROBE = $ffprobe
    $script:maxProgress = 0.0
    $script:statuses = [Collections.Generic.List[string]]::new()

    $before = Invoke-Core -Arguments @('project', 'list')
    if ($before.projects.Count -ne 0) { throw 'Fresh test home unexpectedly contains projects.' }
    $preview = Invoke-Core -Arguments @('source', 'inspect', $TestUrl)
    $started = Invoke-Core -Arguments @(
        'source', 'start', $TestUrl,
        '--confirm-media-id', [string]$preview.source.siteMediaId,
        '--start-delay-ms', '1200'
    )
    $jobId = [string]$started.sourceJob.id
    $cancelRequested = Invoke-Core -Arguments @('source', 'cancel', $jobId)
    if (-not $cancelRequested.sourceJob.cancelRequestedAt) {
        throw 'Cancel request was not persisted.'
    }
    $cancelled = Wait-SourceStatus -JobId $jobId -Wanted @('cancelled') -Seconds 10
    $afterCancel = Invoke-Core -Arguments @('project', 'list')
    if ($afterCancel.projects.Count -ne 0) {
        throw 'Cancelled download created a project.'
    }

    $resumed = Invoke-Core -Arguments @('source', 'resume', $jobId)
    if ($resumed.sourceJob.attemptCount -ne 2) {
        throw 'Explicit resume did not increment the attempt count.'
    }
    $completed = Wait-SourceStatus -JobId $jobId -Wanted @('completed') -Seconds 90
    $after = Invoke-Core -Arguments @('project', 'list')
    if ($after.projects.Count -ne 1) { throw 'Completed download did not create exactly one project.' }
    $project = $after.projects[0]
    $output = [IO.Path]::GetFullPath([string]$completed.outputPath)
    $controlledDirectory = [IO.Path]::GetFullPath([string]$completed.outputDirectory)
    if (-not $output.StartsWith($controlledDirectory, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Downloaded media escaped the controlled import directory.'
    }
    if ([IO.Path]::GetExtension($output) -ne '.mp4') { throw 'Downloaded media is not MP4.' }
    if (-not (Test-Path -LiteralPath $output -PathType Leaf)) { throw 'Downloaded media is missing.' }
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $output).Hash.ToLowerInvariant()
    if ($actualHash -ne [string]$completed.outputSha256) { throw 'Source job output hash mismatch.' }
    if ($actualHash -ne [string]$project.media.sha256) { throw 'Project media hash mismatch.' }
    if ([string]$completed.originalUrl -ne $TestUrl) { throw 'Original URL was not preserved.' }
    if ([string]$completed.siteMediaId -ne [string]$preview.source.siteMediaId) {
        throw 'Site media ID was not preserved.'
    }
    if ([string]$completed.toolVersion -ne '2026.06.09') { throw 'Pinned tool version was not preserved.' }
    if ([string]$completed.projectId -ne [string]$project.id) { throw 'Source job project link mismatch.' }

    [pscustomobject]@{
        status = 'ok'
        attribution = '(c) copyright Blender Foundation | www.sintel.org; CC BY 3.0'
        testUrl = $TestUrl
        sourceJobId = $jobId
        siteMediaId = $completed.siteMediaId
        toolVersion = $completed.toolVersion
        toolSha256 = $completed.toolSha256
        cancelledBeforeProject = ($afterCancel.projects.Count -eq 0)
        resumedAttemptCount = $completed.attemptCount
        observedStatuses = @($script:statuses | Select-Object -Unique)
        maximumObservedProgress = $script:maxProgress
        projectId = $project.id
        outputBytes = (Get-Item -LiteralPath $output).Length
        outputSha256 = $actualHash
        outputDurationSeconds = [double]$project.media.durationSeconds
        projectCount = $after.projects.Count
        temporaryArtifactsKept = [bool]$KeepArtifacts
        temporaryPath = if ($KeepArtifacts) { $work } else { $null }
    } | ConvertTo-Json -Depth 6
} finally {
    $env:SIAOCUT_HOME = $previousHome
    $env:SIAOCUT_DIRECT = $previousDirect
    $env:SIAOCUT_YTDLP = $previousYtDlp
    $env:SIAOCUT_FFMPEG = $previousFfmpeg
    $env:SIAOCUT_FFPROBE = $previousFfprobe
    if (-not $KeepArtifacts -and (Test-Path -LiteralPath $work)) {
        $resolvedWork = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $work).Path)
        if (-not $resolvedWork.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove a test directory outside the system temporary directory.'
        }
        Remove-Item -LiteralPath $resolvedWork -Recurse -Force
    }
}
