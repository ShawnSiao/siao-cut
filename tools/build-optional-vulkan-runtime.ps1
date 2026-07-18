param(
    [string]$Destination,
    [string]$SourceDirectory,
    [string]$BuildDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$commit = '080bbbe85230f624f0b52127f1ae1218247989f9'
if (-not $Destination) { $Destination = Join-Path $root 'apps\desktop\src-tauri\runtime\whisper-vulkan' }
if (-not $SourceDirectory) { $SourceDirectory = Join-Path $root 'third_party\whisper.cpp' }
if (-not $BuildDirectory) { $BuildDirectory = Join-Path ([IO.Path]::GetTempPath()) 'siaocut-whisper-vulkan' }

$sdk = Get-ChildItem 'C:\VulkanSDK' -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Vulkan SDK not found. Install KhronosGroup.VulkanSDK before building the optional runtime.' }
$env:VULKAN_SDK = $sdk.FullName
$env:PATH = "$(Join-Path $sdk.FullName 'Bin');$env:PATH"

if (-not (Test-Path -LiteralPath (Join-Path $SourceDirectory '.git'))) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $SourceDirectory) | Out-Null
    & git clone --filter=blob:none https://github.com/ggml-org/whisper.cpp.git $SourceDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Could not clone whisper.cpp.' }
}
$current = (& git -C $SourceDirectory rev-parse HEAD).Trim()
if ($current -ne $commit) {
    & git -C $SourceDirectory fetch --depth 1 origin $commit
    if ($LASTEXITCODE -ne 0) { throw 'Could not fetch the pinned whisper.cpp commit.' }
    & git -C $SourceDirectory checkout --detach $commit
    if ($LASTEXITCODE -ne 0) { throw 'Could not check out the pinned whisper.cpp commit.' }
}

New-Item -ItemType Directory -Force -Path $BuildDirectory | Out-Null
& cmake -S $SourceDirectory -B $BuildDirectory -A x64 -DGGML_VULKAN=ON -DWHISPER_BUILD_EXAMPLES=ON -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF
if ($LASTEXITCODE -ne 0) { throw 'whisper.cpp Vulkan configuration failed.' }
& cmake --build $BuildDirectory --config Release --target whisper-cli --parallel
if ($LASTEXITCODE -ne 0) { throw 'whisper.cpp Vulkan build failed.' }

$binaryDirectory = Join-Path $BuildDirectory 'bin\Release'
$cli = Join-Path $binaryDirectory 'whisper-cli.exe'
$vulkan = Join-Path $binaryDirectory 'ggml-vulkan.dll'
if (-not (Test-Path -LiteralPath $cli) -or -not (Test-Path -LiteralPath $vulkan)) {
    throw 'Vulkan build did not produce the complete runtime.'
}
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Get-ChildItem -LiteralPath $Destination -File -ErrorAction SilentlyContinue | Remove-Item -Force
Copy-Item -LiteralPath $cli -Destination $Destination -Force
Copy-Item -Path (Join-Path $binaryDirectory '*.dll') -Destination $Destination -Force

[pscustomobject]@{
    backend = 'vulkan'
    source = 'https://github.com/ggml-org/whisper.cpp'
    sourceCommit = $commit
    sdk = $sdk.Name
    destination = (Resolve-Path -LiteralPath $Destination).Path
} | ConvertTo-Json
