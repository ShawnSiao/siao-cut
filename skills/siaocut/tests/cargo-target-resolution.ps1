$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$resolver = Join-Path $repoRoot "skills\siaocut\bin\resolve-core-path.ps1"
$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testTarget = Join-Path $temporaryRoot ("siaocut-target-resolution-" + [guid]::NewGuid().ToString("N"))
$previousTarget = $env:CARGO_TARGET_DIR

try {
    $debugDirectory = Join-Path $testTarget "debug"
    $releaseDirectory = Join-Path $testTarget "release"
    New-Item -ItemType Directory -Path $debugDirectory, $releaseDirectory -Force | Out-Null
    $debugCore = Join-Path $debugDirectory "siaocut-core.exe"
    $releaseCore = Join-Path $releaseDirectory "siaocut-core.exe"
    [System.IO.File]::WriteAllBytes($debugCore, [byte[]](1))
    [System.IO.File]::WriteAllBytes($releaseCore, [byte[]](2))
    $env:CARGO_TARGET_DIR = $testTarget

    $resolvedDebug = & $resolver -Profile Debug
    if ($resolvedDebug -ne $debugCore) {
        throw "Debug Core resolved to an unexpected path: $resolvedDebug"
    }
    $resolvedRelease = & $resolver -Profile Release
    if ($resolvedRelease -ne $releaseCore) {
        throw "Release Core resolved to an unexpected path: $resolvedRelease"
    }
    $resolvedAuto = & $resolver -Profile Auto
    if ($resolvedAuto -ne $releaseCore) {
        throw "Auto resolution did not prefer Release Core: $resolvedAuto"
    }

    Remove-Item -LiteralPath $releaseCore -Force
    $resolvedFallback = & $resolver -Profile Auto
    if ($resolvedFallback -ne $debugCore) {
        throw "Auto resolution did not fall back to Debug Core: $resolvedFallback"
    }

    Remove-Item -LiteralPath $debugCore -Force
    $optionalResult = & $resolver -Profile Auto -Optional
    if ($null -ne $optionalResult) {
        throw "Optional resolution returned a path for a missing Core: $optionalResult"
    }
    $missingFailed = $false
    try {
        & $resolver -Profile Debug | Out-Null
    }
    catch {
        $missingFailed = $true
    }
    if (-not $missingFailed) {
        throw "Required resolution did not fail for a missing Core."
    }

    Write-Host "Cargo target resolution checks passed."
}
finally {
    if ($null -eq $previousTarget) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_TARGET_DIR = $previousTarget
    }
    if (Test-Path -LiteralPath $testTarget) {
        $resolvedTarget = [System.IO.Path]::GetFullPath($testTarget)
        if (-not $resolvedTarget.StartsWith($temporaryRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove test target outside the system temporary directory: $resolvedTarget"
        }
        Remove-Item -LiteralPath $resolvedTarget -Recurse -Force
    }
}
