[CmdletBinding()]
param(
    [string]$VendorDirectory = "vendor/siao-component-store",
    [string]$ProvenancePath = "release/component-store-vendor-provenance.json"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$vendorRoot = if ([IO.Path]::IsPathRooted($VendorDirectory)) {
    [IO.Path]::GetFullPath($VendorDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $VendorDirectory))
}
$provenanceFile = if ([IO.Path]::IsPathRooted($ProvenancePath)) {
    [IO.Path]::GetFullPath($ProvenancePath)
} else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $ProvenancePath))
}

if (-not (Test-Path -LiteralPath $vendorRoot -PathType Container)) {
    throw "audited vendor directory does not exist: $vendorRoot"
}
if (-not (Test-Path -LiteralPath $provenanceFile -PathType Leaf)) {
    throw "vendor provenance does not exist: $provenanceFile"
}

$record = Get-Content -LiteralPath $provenanceFile -Raw | ConvertFrom-Json
$sourceRecord = Get-Content -LiteralPath (Join-Path $repoRoot "release/component-store-provenance.json") -Raw | ConvertFrom-Json
foreach ($property in @("canonicalRepository", "tag", "revision")) {
    if ([string]$record.$property -ne [string]$sourceRecord.canonicalSource.($property -replace "canonicalRepository", "repository")) {
        throw "vendor provenance $property does not match canonical source provenance"
    }
}
if ([string]$record.role -ne "offline-disaster-recovery-only") {
    throw "vendor provenance role must remain offline-disaster-recovery-only"
}

$expected = @($record.files)
$actual = @(Get-ChildItem -LiteralPath $vendorRoot -Recurse -File | ForEach-Object {
    $relative = [IO.Path]::GetRelativePath($vendorRoot, $_.FullName) -replace '\\', '/'
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    [pscustomobject]@{ path = $relative; sha256 = $hash; sizeBytes = $_.Length }
})
if ($actual.Count -ne $expected.Count) {
    throw "vendor file count mismatch: expected $($expected.Count), actual $($actual.Count)"
}
$actualByPath = @{}
foreach ($file in $actual) { $actualByPath[$file.path] = $file }
foreach ($file in $expected) {
    $found = $actualByPath[[string]$file.path]
    if (-not $found -or [int64]$found.sizeBytes -ne [int64]$file.sizeBytes -or $found.sha256 -ne [string]$file.sha256) {
        throw "vendor file mismatch: $($file.path)"
    }
}

Write-Output "component-store-vendor-provenance-ok: $($record.tag) ($($record.revision)); $($expected.Count) files"
