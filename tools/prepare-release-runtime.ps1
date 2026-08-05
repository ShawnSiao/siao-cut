param(
    [string]$CacheDirectory,
    [switch]$Refresh,
    [switch]$IncludeVulkan
)

$ErrorActionPreference = 'Stop'
throw 'legacy_runtime_packaging_removed: formal SiaoCut packages are app-only; install and verify components through the shared Component Store.'
$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $root 'release\runtime-manifest.json'
$manifest = [IO.File]::ReadAllText($manifestPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
$target = Join-Path $root 'apps\desktop\src-tauri\runtime'
if (-not $CacheDirectory) {
    $CacheDirectory = if ($env:SIAOCUT_DOWNLOAD_CACHE_ROOT) {
        Join-Path $env:SIAOCUT_DOWNLOAD_CACHE_ROOT 'siaocut-runtime'
    } else {
        Join-Path $root '.release-cache'
    }
}

New-Item -ItemType Directory -Force -Path $CacheDirectory, $target | Out-Null

function Get-FileSha256 {
    param([string]$Path)
    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        } finally {
            $sha.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

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
    $actual = Get-FileSha256 $archive
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

$whisperTarget = Join-Path $target 'whisper'
& (Join-Path $PSScriptRoot 'build-whisper-runtime.ps1') -Backend cpu -Destination $whisperTarget
if ($LASTEXITCODE -ne 0) { throw 'The source-built CPU runtime failed.' }
$whisperExecutable = Join-Path $whisperTarget 'whisper-cli.exe'
$whisperMetadata = Join-Path $whisperTarget 'runtime-metadata.json'
if (-not (Test-Path -LiteralPath $whisperExecutable) -or -not (Test-Path -LiteralPath $whisperMetadata)) {
    throw 'The source-built CPU runtime is incomplete.'
}
$vad = $manifest.components | Where-Object id -eq 'whisper-vad-silero-6.2'
$vadFile = Get-VerifiedArchive $vad
$installedVad = Join-Path $whisperTarget 'ggml-silero-v6.2.0.bin'
Copy-Item -LiteralPath $vadFile -Destination $installedVad -Force
$timelineModel = Get-VerifiedArchive ($manifest.models | Where-Object id -eq 'tiny')
$cpuEvidence = Join-Path $whisperTarget 'vad-timeline-evidence.json'
& (Join-Path $PSScriptRoot 'test-whisper-vad-timeline.ps1') `
    -PatchedWhisper $whisperExecutable `
    -ExpectedPatchedBackend cpu `
    -Model $timelineModel `
    -VadModel $installedVad `
    -EvidenceOutput $cpuEvidence `
    -RuntimeMetadata $whisperMetadata | Write-Host
if ($LASTEXITCODE -ne 0) { throw 'The source-built CPU runtime failed VAD timeline verification.' }
$cpuRuntimeMetadata = [IO.File]::ReadAllText($whisperMetadata, [Text.Encoding]::UTF8) | ConvertFrom-Json
if ($cpuRuntimeMetadata.vadTimelineVerification.status -ne 'verified') {
    throw 'The source-built CPU runtime was not certified for original-media VAD timestamps.'
}
$whisperComponent = $manifest.components | Where-Object id -eq 'whisper-cpu'
$whisperComponent | Add-Member -NotePropertyName executableSha256 -NotePropertyValue $cpuRuntimeMetadata.executableSha256 -Force
$whisperComponent | Add-Member -NotePropertyName runtimeMetadataSha256 -NotePropertyValue (Get-FileSha256 $whisperMetadata) -Force
$whisperComponent.vadTimelineVerification = 'verified'

$vulkanTarget = Join-Path $target 'whisper-vulkan'
if ($IncludeVulkan) {
    & (Join-Path $PSScriptRoot 'build-optional-vulkan-runtime.ps1') -Destination $vulkanTarget
    $vulkanExecutable = Join-Path $vulkanTarget 'whisper-cli.exe'
    if (-not (Test-Path -LiteralPath $vulkanExecutable)) {
        throw 'The bundled Vulkan runtime did not produce whisper-cli.exe.'
    }
    $vulkanMetadata = Join-Path $vulkanTarget 'runtime-metadata.json'
    $vulkanEvidence = Join-Path $vulkanTarget 'vad-timeline-evidence.json'
    & (Join-Path $PSScriptRoot 'test-whisper-vad-timeline.ps1') `
        -PatchedWhisper $vulkanExecutable `
        -ExpectedPatchedBackend vulkan `
        -Model $timelineModel `
        -VadModel $installedVad `
        -EvidenceOutput $vulkanEvidence `
        -RuntimeMetadata $vulkanMetadata | Write-Host
    if ($LASTEXITCODE -ne 0) { throw 'The source-built Vulkan runtime failed VAD timeline verification.' }
    $vulkanRuntimeMetadata = [IO.File]::ReadAllText($vulkanMetadata, [Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($vulkanRuntimeMetadata.vadTimelineVerification.status -ne 'verified') {
        throw 'The source-built Vulkan runtime was not certified for original-media VAD timestamps.'
    }
    $vulkanComponent = $manifest.components | Where-Object id -eq 'whisper-vulkan'
    $vulkanExecutableSha256 = Get-FileSha256 $vulkanExecutable
    $vulkanComponent | Add-Member -NotePropertyName executableSha256 -NotePropertyValue $vulkanExecutableSha256 -Force
    $vulkanComponent | Add-Member -NotePropertyName runtimeMetadataSha256 -NotePropertyValue (Get-FileSha256 $vulkanMetadata) -Force
    $vulkanComponent.vadTimelineVerification = 'verified'
} elseif (Test-Path -LiteralPath $vulkanTarget) {
    Remove-Item -LiteralPath $vulkanTarget -Recurse -Force
}

$manifest.packageProfile = 'runtime-enabled-fixture'
foreach ($componentId in @('ffmpeg-cpu', 'yt-dlp', 'whisper-cpu', 'whisper-vad-silero-6.2')) {
    $component = $manifest.components | Where-Object id -eq $componentId
    if ($component) { $component.bundled = $true }
}
if ($IncludeVulkan) {
    $vulkanComponent = $manifest.components | Where-Object id -eq 'whisper-vulkan'
    if ($vulkanComponent) { $vulkanComponent.bundled = $true }
}
$generatedManifest = $manifest | ConvertTo-Json -Depth 12
$utf8WithoutBom = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $target 'runtime-manifest.json'), $generatedManifest, $utf8WithoutBom)
Write-Host "Prepared verified release runtime in $target"
