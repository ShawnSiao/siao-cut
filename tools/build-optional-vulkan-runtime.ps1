param(
    [string]$Destination,
    [string]$SourceDirectory,
    [string]$BuildDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Destination) { $Destination = Join-Path $root 'apps\desktop\src-tauri\runtime\whisper-vulkan' }
if (-not $SourceDirectory) { $SourceDirectory = Join-Path $root 'third_party\whisper.cpp' }

$arguments = @{
    Backend = 'vulkan'
    Destination = $Destination
    SourceDirectory = $SourceDirectory
}
if ($BuildDirectory) { $arguments.BuildDirectory = $BuildDirectory }
& (Join-Path $PSScriptRoot 'build-whisper-runtime.ps1') @arguments
if ($LASTEXITCODE -ne 0) { throw 'Could not build the optional Vulkan runtime.' }
