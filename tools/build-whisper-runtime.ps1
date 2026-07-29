param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('cpu', 'vulkan', 'cuda')]
    [string]$Backend,
    [string]$Destination,
    [string]$SourceDirectory,
    [string]$BuildDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$contractPath = Join-Path $root 'release\whisper-runtime-source.json'
$contract = [IO.File]::ReadAllText($contractPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
if (-not $SourceDirectory) {
    $SourceDirectory = Join-Path $root 'third_party\whisper.cpp'
}
if (-not $BuildDirectory) {
    # Vulkan's nested shader-generator build can exceed legacy MSBuild path limits
    # when the intermediate directory lives under a deep repository checkout.
    $rootBytes = [Text.Encoding]::UTF8.GetBytes([IO.Path]::GetFullPath($root).ToLowerInvariant())
    $rootSha = [Security.Cryptography.SHA256]::Create()
    try {
        $rootKey = ([BitConverter]::ToString($rootSha.ComputeHash($rootBytes))).Replace('-', '').Substring(0, 8).ToLowerInvariant()
    } finally {
        $rootSha.Dispose()
    }
    $BuildDirectory = Join-Path ([IO.Path]::GetTempPath()) "siaocut-w-$rootKey-$Backend"
}
if (-not $Destination) {
    $runtimeName = if ($Backend -eq 'cpu') { 'whisper' } else { "whisper-$Backend" }
    $Destination = Join-Path $root "apps\desktop\src-tauri\runtime\$runtimeName"
}

$prepared = & (Join-Path $PSScriptRoot 'prepare-whisper-source.ps1') -SourceDirectory $SourceDirectory | ConvertFrom-Json
if ($prepared.sourceCommit -ne $contract.sourceCommit -or $prepared.patchSha256 -ne $contract.patch.sha256) {
    throw 'Prepared whisper.cpp source identity does not match the runtime source contract.'
}

$cmakeOptions = @(
    '-DGGML_NATIVE=OFF',
    '-DGGML_BACKEND_DL=ON',
    '-DGGML_CPU_ALL_VARIANTS=ON',
    '-DWHISPER_BUILD_EXAMPLES=ON',
    '-DWHISPER_BUILD_TESTS=OFF',
    '-DWHISPER_BUILD_SERVER=OFF'
)
$backendDll = $null
$backendRequirement = $null
switch ($Backend) {
    'vulkan' {
        $sdk = if ($env:VULKAN_SDK -and (Test-Path -LiteralPath $env:VULKAN_SDK -PathType Container)) {
            Get-Item -LiteralPath $env:VULKAN_SDK
        } else {
            Get-ChildItem -LiteralPath 'C:\VulkanSDK' -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending |
                Select-Object -First 1
        }
        if (-not $sdk) {
            throw 'Vulkan SDK not found. Install KhronosGroup.VulkanSDK before building the Vulkan runtime.'
        }
        $env:VULKAN_SDK = $sdk.FullName
        $env:PATH = "$(Join-Path $sdk.FullName 'Bin');$env:PATH"
        $cmakeOptions += '-DGGML_VULKAN=ON'
        $backendDll = 'ggml-vulkan.dll'
        $backendRequirement = "Vulkan SDK $($sdk.Name)"
    }
    'cuda' {
        $cuda = if ($env:CUDA_PATH -and (Test-Path -LiteralPath $env:CUDA_PATH -PathType Container)) {
            Get-Item -LiteralPath $env:CUDA_PATH
        } else {
            Get-ChildItem -LiteralPath 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA' -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending |
                Select-Object -First 1
        }
        if (-not $cuda -or -not (Test-Path -LiteralPath (Join-Path $cuda.FullName 'bin\nvcc.exe') -PathType Leaf)) {
            throw 'CUDA Toolkit with nvcc was not found. A source-built CUDA runtime cannot be declared from the driver-only environment.'
        }
        $env:CUDA_PATH = $cuda.FullName
        $env:PATH = "$(Join-Path $cuda.FullName 'bin');$env:PATH"
        $cmakeOptions += '-DGGML_CUDA=ON'
        $backendDll = 'ggml-cuda.dll'
        $backendRequirement = "CUDA Toolkit $($cuda.Name)"
    }
}

New-Item -ItemType Directory -Force -Path $BuildDirectory | Out-Null
$configureArgs = @('-S', $SourceDirectory, '-B', $BuildDirectory, '-A', 'x64') + $cmakeOptions
& cmake @configureArgs | ForEach-Object { Write-Host $_ }
if ($LASTEXITCODE -ne 0) { throw "whisper.cpp $Backend configuration failed." }
& cmake --build $BuildDirectory --config Release --target whisper-cli --parallel | ForEach-Object { Write-Host $_ }
if ($LASTEXITCODE -ne 0) { throw "whisper.cpp $Backend build failed." }

$binaryDirectory = Join-Path $BuildDirectory 'bin\Release'
$cli = Join-Path $binaryDirectory 'whisper-cli.exe'
if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
    throw "$Backend build did not produce whisper-cli.exe."
}
if ($backendDll -and -not (Test-Path -LiteralPath (Join-Path $binaryDirectory $backendDll) -PathType Leaf)) {
    throw "$Backend build did not produce $backendDll."
}

$resolvedDestination = [IO.Path]::GetFullPath($Destination)
$resolvedRoot = [IO.Path]::GetFullPath($root)
if ($resolvedDestination -eq $resolvedRoot -or [IO.Path]::GetPathRoot($resolvedDestination) -eq $resolvedDestination) {
    throw "Refusing to use a broad runtime destination: $resolvedDestination"
}
New-Item -ItemType Directory -Force -Path $resolvedDestination | Out-Null
Get-ChildItem -LiteralPath $resolvedDestination -File -ErrorAction SilentlyContinue |
    Where-Object {
        $_.Name -eq 'whisper-cli.exe' -or
        $_.Name -eq 'whisper.dll' -or
        $_.Name -eq 'runtime-metadata.json' -or
        $_.Name -like 'ggml*.dll'
    } |
    Remove-Item -Force
Copy-Item -LiteralPath $cli -Destination $resolvedDestination -Force
Copy-Item -Path (Join-Path $binaryDirectory '*.dll') -Destination $resolvedDestination -Force

function Get-Sha256([string]$Path) {
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

$installedCli = Join-Path $resolvedDestination 'whisper-cli.exe'
$files = @(
    Get-ChildItem -LiteralPath $resolvedDestination -File |
        Where-Object { $_.Name -eq 'whisper-cli.exe' -or $_.Name -like '*.dll' } |
        Sort-Object Name |
        ForEach-Object {
            [ordered]@{
                name = $_.Name
                size = $_.Length
                sha256 = Get-Sha256 $_.FullName
            }
        }
)
$metadata = [ordered]@{
    schemaVersion = 1
    runtimeId = "siaocut-whisper-$Backend"
    version = $contract.version
    backend = $Backend
    source = $contract.source
    sourceCommit = $contract.sourceCommit
    upstreamTokenMappingFixCommit = $contract.upstreamTokenMappingFix.commit
    patchPath = $contract.patch.path
    patchSha256 = $contract.patch.sha256
    sourceCapabilities = $contract.sourceCapabilities
    build = [ordered]@{
        generator = 'Visual Studio 17 2022'
        architecture = 'x64'
        configuration = 'Release'
        options = $cmakeOptions
        requirement = $backendRequirement
    }
    executableSha256 = Get-Sha256 $installedCli
    vadTimelineVerification = [ordered]@{
        status = 'not_run'
        timeDomain = 'original_media'
        evidenceSha256 = $null
    }
    files = $files
}
$metadataPath = Join-Path $resolvedDestination 'runtime-metadata.json'
[IO.File]::WriteAllText(
    $metadataPath,
    ($metadata | ConvertTo-Json -Depth 8),
    [Text.UTF8Encoding]::new($false)
)

[pscustomobject]@{
    backend = $Backend
    destination = $resolvedDestination
    executable = $installedCli
    executableSha256 = $metadata.executableSha256
    sourceCommit = $contract.sourceCommit
    patchSha256 = $contract.patch.sha256
    metadata = $metadataPath
    vadTimelineVerification = 'not_run'
    files = $files.Count
} | ConvertTo-Json -Depth 5
