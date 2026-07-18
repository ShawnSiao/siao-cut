param(
    [string]$CorePath = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
if (-not $CorePath) { $CorePath = Join-Path $repoRoot "target\debug\siaocut-core.exe" }
if (-not (Test-Path -LiteralPath $CorePath)) {
    & cargo build --manifest-path (Join-Path $repoRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "Core build failed" }
}
$CorePath = (Resolve-Path -LiteralPath $CorePath).Path
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$runRoot = Join-Path $tempRoot ("siaocut-subtitle-mode-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

function Invoke-Core {
    param([string[]]$Arguments)
    $raw = & $CorePath --json @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Core command failed: $($Arguments -join ' ')" }
    return ($raw | ConvertFrom-Json)
}

function Invoke-CoreFailure {
    param([string[]]$Arguments)
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $raw = & $CorePath --json @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -eq 0) { throw "Command unexpectedly succeeded: $($Arguments -join ' ')" }
    $text = (($raw | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)
    $jsonStart = $text.IndexOf("{")
    if ($jsonStart -lt 0) { throw "Core failure did not return JSON: $text" }
    return ($text.Substring($jsonStart) | ConvertFrom-Json)
}

function Wait-Export {
    param([string]$JobId)
    $deadline = [DateTime]::UtcNow.AddMinutes(2)
    do {
        $status = Invoke-Core -Arguments @("video", "status", $JobId)
        if ($status.job.status -eq "completed") { return $status.job }
        if ($status.job.status -in @("failed", "cancelled", "interrupted")) {
            throw "Video export did not complete: $($status.job.status) $($status.job.errorMessage)"
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Video export timed out: $JobId"
}

$previousHome = $env:SIAOCUT_HOME
$previousFfmpeg = $env:SIAOCUT_FFMPEG
$previousFfprobe = $env:SIAOCUT_FFPROBE
$previousIdle = $env:SIAOCUT_SERVICE_IDLE_MS

try {
    $env:SIAOCUT_HOME = Join-Path $runRoot "home"
    $env:SIAOCUT_FFMPEG = $ffmpeg
    $env:SIAOCUT_FFPROBE = $ffprobe
    $env:SIAOCUT_SERVICE_IDLE_MS = "100"

    $sourcePath = Join-Path $runRoot "source.mp4"
    & $ffmpeg -y -hide_banner -loglevel error -f lavfi -i "testsrc2=size=640x360:rate=30" -f lavfi -i "sine=frequency=660:sample_rate=48000" -t 1.5 -c:v mpeg4 -q:v 3 -pix_fmt yuv420p -c:a aac -shortest $sourcePath
    if ($LASTEXITCODE -ne 0) { throw "Fixture generation failed" }

    $imported = Invoke-Core -Arguments @("import", $sourcePath, "--title", "subtitle-mode")
    $projectId = [string]$imported.projectId
    $added = Invoke-Core -Arguments @("transcript", "add", $projectId, "--start", "0", "--end", "1.5", "--text", "Source caption")
    $segmentId = [string]$added.segment.id

    $missingOutput = Join-Path $runRoot "missing.srt"
    $missing = Invoke-CoreFailure -Arguments @("transcript", "export", $projectId, "--format", "srt", "--output", $missingOutput, "--subtitle-mode", "translated", "--lang", "en")
    if ($missing.code -ne "translation_missing") { throw "Unexpected missing translation code: $($missing.code)" }

    $task = Invoke-Core -Arguments @("task", "create", $projectId, "--kind", "translate", "--lang", "en")
    $claim = Invoke-Core -Arguments @("task", "claim", "--worker", "subtitle-mode-e2e")
    $responsePath = Join-Path $runRoot "translation-response.json"
    $response = @{
        baseVersionId = [string]$claim.payload.baseVersionId
        patches = @(@{
            segmentId = $segmentId
            before = "Source caption"
            after = "Translated caption"
            reason = "English translation"
            confidence = 1.0
        })
    } | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText($responsePath, $response, [Text.UTF8Encoding]::new($false))
    Invoke-Core -Arguments @("task", "submit", ([string]$task.taskId), "--worker", "subtitle-mode-e2e", "--response", $responsePath) | Out-Null
    Invoke-Core -Arguments @("task", "review-all", ([string]$task.taskId), "--action", "apply") | Out-Null

    $formats = @("srt", "vtt", "ass", "markdown")
    $modes = @("source", "translated", "bilingual")
    $fileCases = 0
    foreach ($format in $formats) {
        foreach ($mode in $modes) {
            $extension = if ($format -eq "markdown") { "md" } else { $format }
            $output = Join-Path $runRoot ("$format-$mode.$extension")
            $arguments = @("transcript", "export", $projectId, "--format", $format, "--output", $output, "--subtitle-mode", $mode)
            if ($mode -ne "source") { $arguments += @("--lang", "en") }
            Invoke-Core -Arguments $arguments | Out-Null
            $content = [IO.File]::ReadAllText($output)
            if ($mode -eq "source" -and ($content -notmatch "Source caption" -or $content -match "Translated caption")) {
                throw "$format source content mismatch"
            }
            if ($mode -eq "translated" -and ($content -notmatch "Translated caption" -or $content -match "Source caption")) {
                throw "$format translated content mismatch"
            }
            if ($mode -eq "bilingual" -and ($content -notmatch "Source caption" -or $content -notmatch "Translated caption")) {
                throw "$format bilingual content mismatch"
            }
            $fileCases += 1
        }
    }

    $videoHashes = @{}
    foreach ($mode in $modes) {
        $output = Join-Path $runRoot ("video-$mode.mp4")
        $arguments = @("video", "export", $projectId, "--output", $output, "--burn-subtitles", "--subtitle-mode", $mode)
        if ($mode -ne "source") { $arguments += @("--lang", "en") }
        $started = Invoke-Core -Arguments $arguments
        $job = Wait-Export -JobId ([string]$started.jobId)
        $manifest = [IO.File]::ReadAllText([string]$job.manifestPath) | ConvertFrom-Json
        if ($manifest.subtitleMode -ne $mode) { throw "MP4 manifest subtitle mode mismatch: $mode" }
        $videoHashes[$mode] = (Get-FileHash -Algorithm SHA256 -LiteralPath $output).Hash
    }
    if (($videoHashes.Values | Select-Object -Unique).Count -ne 3) {
        throw "Subtitle modes produced identical MP4 files"
    }

    Invoke-Core -Arguments @("transcript", "edit", $projectId, $segmentId, "--text", "Changed source caption") | Out-Null
    $staleOutput = Join-Path $runRoot "stale.srt"
    $stale = Invoke-CoreFailure -Arguments @("transcript", "export", $projectId, "--format", "srt", "--output", $staleOutput, "--subtitle-mode", "translated", "--lang", "en")
    if ($stale.code -ne "translation_stale") { throw "Unexpected stale translation code: $($stale.code)" }

    [pscustomobject]@{
        status = "passed"
        fileCases = $fileCases
        videoCases = $videoHashes.Count
        missingTranslationCode = [string]$missing.code
        staleTranslationCode = [string]$stale.code
        videoHashesAreDistinct = $true
    } | ConvertTo-Json -Depth 4
}
finally {
    $env:SIAOCUT_HOME = $previousHome
    $env:SIAOCUT_FFMPEG = $previousFfmpeg
    $env:SIAOCUT_FFPROBE = $previousFfprobe
    $env:SIAOCUT_SERVICE_IDLE_MS = $previousIdle
    Start-Sleep -Milliseconds 500
    $resolvedRunRoot = [IO.Path]::GetFullPath($runRoot)
    if ($resolvedRunRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedRunRoot)) {
        Remove-Item -LiteralPath $resolvedRunRoot -Recurse -Force
    }
}
