param(
    [ValidateSet('whisper-cuda-11')]
    [string]$Runtime = 'whisper-cuda-11',
    [string]$Core = '',
    [string]$SourceDirectory = '',
    [string]$BuildDirectory = ''
)

$ErrorActionPreference = 'Stop'
throw 'legacy_runtime_selection_removed: formal SiaoCut packages use component-store register-existing and resolve_and_acquire; old source-built runtime installation is migration evidence only.'
$root = Split-Path -Parent $PSScriptRoot
$manifest = [IO.File]::ReadAllText((Join-Path $root 'release\runtime-manifest.json'), [Text.Encoding]::UTF8) | ConvertFrom-Json
$component = $manifest.components | Where-Object id -eq $Runtime
if (-not $component -or $component.kind -ne 'optional-source-built-runtime') {
    throw "Unknown source-built optional runtime: $Runtime"
}
if (-not $Core) {
    $metadataOutput = & cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve the Cargo target directory.' }
    $metadata = ($metadataOutput -join "`n") | ConvertFrom-Json
    $Core = Join-Path $metadata.target_directory 'release\siaocut-core.exe'
}
$Core = (Resolve-Path -LiteralPath $Core).Path
if (-not $SourceDirectory) { $SourceDirectory = Join-Path $root 'third_party\whisper.cpp' }

$downloadDir = if ($env:SIAOCUT_DOWNLOAD_CACHE_ROOT) {
    Join-Path $env:SIAOCUT_DOWNLOAD_CACHE_ROOT 'optional-runtime'
} else {
    Join-Path $env:LOCALAPPDATA 'SiaoCut\downloads'
}
$runtimeDir = Join-Path $env:LOCALAPPDATA "SiaoCut\runtimes\$Runtime"
New-Item -ItemType Directory -Force -Path $downloadDir | Out-Null

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
        finally { $sha.Dispose() }
    } finally { $stream.Dispose() }
}

function Get-VerifiedDownload([object]$Asset) {
    $target = Join-Path $downloadDir ([IO.Path]::GetFileName([uri]$Asset.url))
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
        Write-Host "Downloading $($Asset.name) ($([math]::Round($Asset.size / 1MB, 1)) MB) from $($Asset.source)..."
        Invoke-WebRequest -UseBasicParsing -Uri $Asset.url -OutFile $target
    }
    $actual = Get-Sha256 $target
    if ($actual -ne $Asset.sha256) {
        throw "Asset hash mismatch. Expected $($Asset.sha256), got $actual."
    }
    return $target
}

$buildArguments = @{
    Backend = 'cuda'
    Destination = $runtimeDir
    SourceDirectory = $SourceDirectory
}
if ($BuildDirectory) { $buildArguments.BuildDirectory = $BuildDirectory }
& (Join-Path $PSScriptRoot 'build-whisper-runtime.ps1') @buildArguments
if ($LASTEXITCODE -ne 0) { throw 'The source-built CUDA runtime failed.' }

$model = Get-VerifiedDownload ($manifest.models | Where-Object id -eq 'tiny')
$vad = Get-VerifiedDownload ($manifest.components | Where-Object id -eq 'whisper-vad-silero-6.2')
$installedWhisper = Join-Path $runtimeDir 'whisper-cli.exe'
$runtimeMetadata = Join-Path $runtimeDir 'runtime-metadata.json'
$evidence = Join-Path $runtimeDir 'vad-timeline-evidence.json'
& (Join-Path $PSScriptRoot 'test-whisper-vad-timeline.ps1') `
    -PatchedWhisper $installedWhisper `
    -ExpectedPatchedBackend cuda `
    -Model $model `
    -VadModel $vad `
    -EvidenceOutput $evidence `
    -RuntimeMetadata $runtimeMetadata | Write-Host
if ($LASTEXITCODE -ne 0) {
    throw 'The source-built CUDA runtime failed original-media VAD timeline verification and was not selected.'
}

$verifiedMetadata = [IO.File]::ReadAllText($runtimeMetadata, [Text.Encoding]::UTF8) | ConvertFrom-Json
if ($verifiedMetadata.vadTimelineVerification.status -ne 'verified') {
    throw 'The source-built CUDA runtime is not certified for original-media VAD timestamps.'
}
$selectionRaw = & $Core --json runtime select $component.backend `
    --whisper $installedWhisper `
    --source $component.source `
    --version $component.version | Out-String
$selection = $selectionRaw | ConvertFrom-Json
if ($selection.status -ne 'ok') { throw $selection.message }

[pscustomobject]@{
    backend = $selection.runtime.backend
    path = $selection.selection.whisperPath
    source = $component.source
    sourceCommit = $verifiedMetadata.sourceCommit
    version = $component.version
    patchSha256 = $verifiedMetadata.patchSha256
    executableSha256 = $verifiedMetadata.executableSha256
    vadTimelineVerification = $verifiedMetadata.vadTimelineVerification.status
    evidenceSha256 = $verifiedMetadata.vadTimelineVerification.evidenceSha256
    license = $component.license
    requires = $component.requires
} | ConvertTo-Json
