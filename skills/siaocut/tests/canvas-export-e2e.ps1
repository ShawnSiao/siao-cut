param(
    [string]$CorePath = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
if (-not $CorePath) {
    $CorePath = Join-Path $repoRoot "target\debug\siaocut-core.exe"
}
if (-not (Test-Path -LiteralPath $CorePath)) {
    & cargo build --manifest-path (Join-Path $repoRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "Core 构建失败" }
}
$CorePath = (Resolve-Path -LiteralPath $CorePath).Path
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$runRoot = Join-Path $tempRoot ("siaocut-canvas-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

function Invoke-Core {
    param([string[]]$Arguments)
    $raw = & $CorePath --json @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Core 命令失败：$($Arguments -join ' ')" }
    return ($raw | ConvertFrom-Json)
}

function Get-VideoEvidence {
    param([string]$Path)
    $raw = & $ffprobe -v error -select_streams v:0 -show_entries stream=width,height,pix_fmt -show_entries format=duration -of json $Path
    if ($LASTEXITCODE -ne 0) { throw "FFprobe 读取失败：$Path" }
    $probe = $raw | ConvertFrom-Json
    return [pscustomobject]@{
        width = [int]$probe.streams[0].width
        height = [int]$probe.streams[0].height
        pixelFormat = [string]$probe.streams[0].pix_fmt
        duration = [double]$probe.format.duration
    }
}

function Wait-Export {
    param([string]$JobId)
    $deadline = [DateTime]::UtcNow.AddMinutes(2)
    do {
        $status = Invoke-Core -Arguments @("video", "status", $JobId)
        if ($status.job.status -eq "completed") { return $status.job }
        if ($status.job.status -in @("failed", "cancelled", "interrupted")) {
            throw "视频导出未完成：$($status.job.status) $($status.job.errorMessage)"
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "视频导出超时：$JobId"
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

    $sources = @(
        @{ name = "landscape"; size = "1280x720" },
        @{ name = "square"; size = "720x720" },
        @{ name = "portrait"; size = "720x1280" }
    )
    $framings = @("contain-blur", "cover-center")
    $evidence = @()
    $landscapePath = ""

    foreach ($source in $sources) {
        $sourcePath = Join-Path $runRoot ("$($source.name).mp4")
        & $ffmpeg -y -hide_banner -loglevel error -f lavfi -i "testsrc2=size=$($source.size):rate=30" -f lavfi -i "sine=frequency=880:sample_rate=48000" -t 1.5 -c:v mpeg4 -q:v 3 -pix_fmt yuv420p -c:a aac -shortest $sourcePath
        if ($LASTEXITCODE -ne 0) { throw "测试素材生成失败：$($source.name)" }
        if ($source.name -eq "landscape") { $landscapePath = $sourcePath }

        if ($source.name -eq "landscape") {
            $sourceImported = Invoke-Core -Arguments @("import", $sourcePath, "--title", "landscape-source")
            $sourceProjectId = [string]$sourceImported.projectId
            Invoke-Core -Arguments @("transcript", "add", $sourceProjectId, "--start", "0", "--end", "1.5", "--text", "source canvas test") | Out-Null
            $sourcePrepared = Invoke-Core -Arguments @("media", "prepare", $sourceProjectId)
            $sourceProxyEvidence = Get-VideoEvidence -Path ([string]$sourcePrepared.artifacts.proxyPath)
            $sourceOutput = Join-Path $runRoot "landscape-source-output.mp4"
            $sourceStarted = Invoke-Core -Arguments @("video", "export", $sourceProjectId, "--output", $sourceOutput)
            Wait-Export -JobId ([string]$sourceStarted.jobId) | Out-Null
            $sourceOutputEvidence = Get-VideoEvidence -Path $sourceOutput
            foreach ($item in @($sourceProxyEvidence, $sourceOutputEvidence)) {
                if ($item.width -ne 1280 -or $item.height -ne 720) {
                    throw "source 画布改变了原始比例：$($item.width)x$($item.height)"
                }
            }
            $evidence += [pscustomobject]@{
                source = "landscape"
                framing = "source"
                proxy = $sourceProxyEvidence
                output = $sourceOutputEvidence
            }
        }

        foreach ($framing in $framings) {
            $imported = Invoke-Core -Arguments @("import", $sourcePath, "--title", "$($source.name)-$framing")
            $projectId = [string]$imported.projectId
            Invoke-Core -Arguments @("transcript", "add", $projectId, "--start", "0", "--end", "1.5", "--text", "canvas test") | Out-Null
            Invoke-Core -Arguments @("canvas", "set", $projectId, "--aspect-ratio", "9:16", "--framing", $framing) | Out-Null

            $prepared = Invoke-Core -Arguments @("media", "prepare", $projectId)
            $proxyEvidence = Get-VideoEvidence -Path ([string]$prepared.artifacts.proxyPath)
            $output = Join-Path $runRoot ("$($source.name)-$framing-output.mp4")
            $started = Invoke-Core -Arguments @("video", "export", $projectId, "--output", $output)
            Wait-Export -JobId ([string]$started.jobId) | Out-Null
            $outputEvidence = Get-VideoEvidence -Path $output

            foreach ($item in @($proxyEvidence, $outputEvidence)) {
                if ($item.width -ne 1080 -or $item.height -ne 1920) {
                    throw "竖屏尺寸不正确：$($source.name) $framing $($item.width)x$($item.height)"
                }
                if ($item.pixelFormat -ne "yuv420p") {
                    throw "像素格式不正确：$($source.name) $framing $($item.pixelFormat)"
                }
                if ([Math]::Abs($item.duration - 1.5) -gt 0.12) {
                    throw "时长偏差过大：$($source.name) $framing $($item.duration)"
                }
            }
            $evidence += [pscustomobject]@{
                source = $source.name
                framing = $framing
                proxy = $proxyEvidence
                output = $outputEvidence
            }
        }
    }

    $snapshotImported = Invoke-Core -Arguments @("import", $landscapePath, "--title", "canvas-snapshot")
    $snapshotProjectId = [string]$snapshotImported.projectId
    Invoke-Core -Arguments @("transcript", "add", $snapshotProjectId, "--start", "0", "--end", "1.5", "--text", "snapshot test") | Out-Null
    Invoke-Core -Arguments @("canvas", "set", $snapshotProjectId, "--aspect-ratio", "9:16", "--framing", "contain-blur") | Out-Null
    $snapshotOutput = Join-Path $runRoot "canvas-snapshot-output.mp4"
    $snapshotStarted = Invoke-Core -Arguments @("video", "export", $snapshotProjectId, "--output", $snapshotOutput, "--start-delay-ms", "1000")
    Invoke-Core -Arguments @("canvas", "set", $snapshotProjectId, "--aspect-ratio", "source", "--framing", "contain-blur") | Out-Null
    Wait-Export -JobId ([string]$snapshotStarted.jobId) | Out-Null
    $snapshotEvidence = Get-VideoEvidence -Path $snapshotOutput
    if ($snapshotEvidence.width -ne 1080 -or $snapshotEvidence.height -ne 1920) {
        throw "导出任务未保留创建时的画布快照：$($snapshotEvidence.width)x$($snapshotEvidence.height)"
    }
    $evidence += [pscustomobject]@{
        source = "landscape"
        framing = "snapshot-contain-blur"
        proxy = $null
        output = $snapshotEvidence
    }

    [pscustomobject]@{
        status = "passed"
        cases = $evidence.Count
        evidence = $evidence
    } | ConvertTo-Json -Depth 6
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
