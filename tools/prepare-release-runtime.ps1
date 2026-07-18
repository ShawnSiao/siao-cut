param(
    [string]$CacheDirectory,
    [switch]$Refresh,
    [switch]$IncludeVulkan
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $root 'release\runtime-manifest.json'
$manifest = [IO.File]::ReadAllText($manifestPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
$target = Join-Path $root 'apps\desktop\src-tauri\runtime'
if (-not $CacheDirectory) {
    $CacheDirectory = Join-Path $root '.release-cache'
}

New-Item -ItemType Directory -Force -Path $CacheDirectory, $target | Out-Null

function Get-VerifiedArchive {
    param([object]$Component)
    $archive = Join-Path $CacheDirectory ([IO.Path]::GetFileName([uri]$Component.url))
    if ($Refresh -and (Test-Path -LiteralPath $archive)) {
        Remove-Item -LiteralPath $archive -Force
    }
    if (-not (Test-Path -LiteralPath $archive)) {
        Write-Host "Downloading $($Component.name) ($([math]::Round($Component.size / 1MB, 1)) MB)..."
        Invoke-WebRequest -UseBasicParsing -Uri $Component.url -OutFile $archive
    }
    $stream = [IO.File]::OpenRead($archive)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            $actual = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        } finally {
            $sha.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
    if ($actual -ne $Component.sha256) {
        throw "Hash mismatch for $($Component.id). Expected $($Component.sha256), got $actual. The pinned release manifest must be reviewed before updating."
    }
    return $archive
}

function Expand-CleanArchive {
    param([string]$Archive, [string]$Destination)
    if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Expand-Archive -LiteralPath $Archive -DestinationPath $Destination -Force
}

$ffmpeg = $manifest.components | Where-Object id -eq 'ffmpeg-cpu'
$ffmpegArchive = Get-VerifiedArchive $ffmpeg
$ffmpegExtract = Join-Path $CacheDirectory 'ffmpeg-cpu'
Expand-CleanArchive $ffmpegArchive $ffmpegExtract
$ffmpegBin = Get-ChildItem -LiteralPath $ffmpegExtract -Recurse -Filter 'ffmpeg.exe' | Select-Object -First 1 -ExpandProperty DirectoryName
if (-not $ffmpegBin) { throw 'ffmpeg.exe was not found in the verified archive.' }
$ffmpegTarget = Join-Path $target 'ffmpeg'
if (Test-Path -LiteralPath $ffmpegTarget) { Remove-Item -LiteralPath $ffmpegTarget -Recurse -Force }
New-Item -ItemType Directory -Force -Path $ffmpegTarget | Out-Null
Copy-Item -LiteralPath (Join-Path $ffmpegBin 'ffmpeg.exe'), (Join-Path $ffmpegBin 'ffprobe.exe') -Destination $ffmpegTarget -Force
Copy-Item -Path (Join-Path $ffmpegBin '*.dll') -Destination $ffmpegTarget -Force
$licenseTarget = Join-Path $target 'licenses'
if (Test-Path -LiteralPath $licenseTarget) { Remove-Item -LiteralPath $licenseTarget -Recurse -Force }
New-Item -ItemType Directory -Force -Path $licenseTarget | Out-Null
$ffmpegLicense = Get-ChildItem -LiteralPath $ffmpegExtract -Recurse -Filter 'LICENSE.txt' | Select-Object -First 1
if (-not $ffmpegLicense) { throw 'FFmpeg license was not found in the verified archive.' }
Copy-Item -LiteralPath $ffmpegLicense.FullName -Destination (Join-Path $licenseTarget 'FFmpeg-LGPL-2.1.txt') -Force
Copy-Item -Path (Join-Path $root 'release\licenses\*.txt') -Destination $licenseTarget -Force

$ytDlp = $manifest.components | Where-Object id -eq 'yt-dlp'
$ytDlpBinary = Get-VerifiedArchive $ytDlp
$ytDlpTarget = Join-Path $target 'yt-dlp'
if (Test-Path -LiteralPath $ytDlpTarget) { Remove-Item -LiteralPath $ytDlpTarget -Recurse -Force }
New-Item -ItemType Directory -Force -Path $ytDlpTarget | Out-Null
$ytDlpInstalled = Join-Path $ytDlpTarget 'yt-dlp.exe'
Copy-Item -LiteralPath $ytDlpBinary -Destination $ytDlpInstalled -Force
$ytDlpVersion = (& $ytDlpInstalled --version | Out-String).Trim()
if ($ytDlpVersion -ne $ytDlp.version) {
    throw "yt-dlp version mismatch. Expected $($ytDlp.version), got $ytDlpVersion."
}
foreach ($license in $ytDlp.licenseFiles) {
    $licenseSource = Get-VerifiedArchive $license
    Copy-Item -LiteralPath $licenseSource -Destination (Join-Path $licenseTarget $license.target) -Force
}

$whisper = $manifest.components | Where-Object id -eq 'whisper-cpu'
$whisperArchive = Get-VerifiedArchive $whisper
$whisperExtract = Join-Path $CacheDirectory 'whisper-cpu'
Expand-CleanArchive $whisperArchive $whisperExtract
$whisperExe = Get-ChildItem -LiteralPath $whisperExtract -Recurse -Filter 'whisper-cli.exe' | Select-Object -First 1
if (-not $whisperExe) { throw 'whisper-cli.exe was not found in the verified archive.' }
$whisperTarget = Join-Path $target 'whisper'
if (Test-Path -LiteralPath $whisperTarget) { Remove-Item -LiteralPath $whisperTarget -Recurse -Force }
New-Item -ItemType Directory -Force -Path $whisperTarget | Out-Null
Copy-Item -LiteralPath $whisperExe.FullName -Destination $whisperTarget -Force
Copy-Item -Path (Join-Path $whisperExe.DirectoryName 'ggml*.dll') -Destination $whisperTarget -Force
Copy-Item -LiteralPath (Join-Path $whisperExe.DirectoryName 'whisper.dll') -Destination $whisperTarget -Force
$vad = $manifest.components | Where-Object id -eq 'whisper-vad-silero-6.2'
$vadFile = Get-VerifiedArchive $vad
Copy-Item -LiteralPath $vadFile -Destination (Join-Path $whisperTarget 'ggml-silero-v6.2.0.bin') -Force

$vulkanTarget = Join-Path $target 'whisper-vulkan'
if ($IncludeVulkan) {
    & (Join-Path $PSScriptRoot 'build-optional-vulkan-runtime.ps1') -Destination $vulkanTarget
} elseif (Test-Path -LiteralPath $vulkanTarget) {
    Remove-Item -LiteralPath $vulkanTarget -Recurse -Force
}

Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $target 'runtime-manifest.json') -Force
Write-Host "Prepared verified release runtime in $target"
