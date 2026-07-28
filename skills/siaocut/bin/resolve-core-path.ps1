[CmdletBinding()]
param(
    [ValidateSet("Auto", "Debug", "Release")]
    [string]$Profile = "Auto",
    [string]$RepoRoot,
    [switch]$Optional
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
}
$RepoRoot = [System.IO.Path]::GetFullPath($RepoRoot)
$manifestPath = Join-Path $RepoRoot "Cargo.toml"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "SiaoCut Cargo manifest not found: $manifestPath"
}

$metadataOutput = & cargo metadata `
    --manifest-path $manifestPath `
    --no-deps `
    --format-version 1
if ($LASTEXITCODE -ne 0) {
    throw "Unable to resolve the SiaoCut Cargo target directory."
}
$metadata = ($metadataOutput -join "`n") | ConvertFrom-Json
$targetDirectory = [System.IO.Path]::GetFullPath($metadata.target_directory)
$profiles = if ($Profile -eq "Auto") {
    @("release", "debug")
}
else {
    @($Profile.ToLowerInvariant())
}

foreach ($candidateProfile in $profiles) {
    $candidate = Join-Path $targetDirectory "$candidateProfile\siaocut-core.exe"
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        return (Get-Item -LiteralPath $candidate).FullName
    }
}

if ($Optional) {
    return
}

$expected = $profiles |
    ForEach-Object { Join-Path $targetDirectory "$_\siaocut-core.exe" }
throw "SiaoCut Core not found. Expected one of: $($expected -join ', ')"
