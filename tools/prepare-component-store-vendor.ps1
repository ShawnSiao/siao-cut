param(
    [string]$OutputDirectory = "vendor/siao-component-store",
    [string]$ProvenancePath = "release/component-store-vendor-provenance.json"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repoRoot

$provenance = Get-Content (Join-Path $repoRoot "release/component-store-provenance.json") -Raw | ConvertFrom-Json
$expectedCommit = [string]$provenance.canonicalSource.revision
if ($expectedCommit -notmatch '^[0-9a-f]{40}$') {
    throw "release provenance 必须包含 40 位 canonical revision"
}

$cargoManifest = Get-Content (Join-Path $repoRoot "Cargo.toml") -Raw
$gitPin = 'git = "https://github.com/ShawnSiao/siao-component-store.git"'
$revisionPin = 'rev = "' + $expectedCommit + '"'
foreach ($package in @("siao-component-store-core", "siao-component-store-catalogs")) {
    if ($cargoManifest -notmatch [regex]::Escape($gitPin) -or
        $cargoManifest -notmatch [regex]::Escape($revisionPin)) {
        throw "$package 未固定到 canonical revision $expectedCommit"
    }
}

$output = if ([IO.Path]::IsPathRooted($OutputDirectory)) { [IO.Path]::GetFullPath($OutputDirectory) } else { [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory)) }
$provenanceOutput = if ([IO.Path]::IsPathRooted($ProvenancePath)) { [IO.Path]::GetFullPath($ProvenancePath) } else { [IO.Path]::GetFullPath((Join-Path $repoRoot $ProvenancePath)) }
if ((Test-Path $output) -and (Get-ChildItem -LiteralPath $output -Force | Select-Object -First 1)) {
    throw "vendor 输出目录不是空目录：$output；请先人工清理或指定新的灾备目录"
}
New-Item -ItemType Directory -Force -Path $output | Out-Null

$config = & cargo vendor --locked --versioned-dirs $output
if ($LASTEXITCODE -ne 0) {
    throw "cargo vendor 失败，未生成可审计灾备快照"
}
$configPath = Join-Path $output "config.toml"
$config | Set-Content -LiteralPath $configPath -Encoding utf8

$files = @(Get-ChildItem -LiteralPath $output -Recurse -File | Sort-Object FullName | ForEach-Object {
    $relative = [IO.Path]::GetRelativePath($output, $_.FullName) -replace '\\', '/'
    [ordered]@{
        path = $relative
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        sizeBytes = $_.Length
    }
})
$record = [ordered]@{
    schemaVersion = 1
    role = "offline-disaster-recovery-only"
    canonicalRepository = $provenance.canonicalSource.repository
    tag = $provenance.canonicalSource.tag
    revision = $expectedCommit
    generatedAt = [DateTime]::UtcNow.ToString("o")
    defaultBuildPath = "git-pin"
    files = $files
}
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($provenanceOutput)) | Out-Null
$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $provenanceOutput -Encoding utf8
Write-Output "component-store-vendor-ready: $output"
