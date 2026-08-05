[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$root = (git rev-parse --show-toplevel).Trim()
if (-not $root) {
    throw 'The current directory is not inside a Git repository.'
}

$provenancePath = Join-Path $root 'release\component-store-provenance.json'
$provenance = Get-Content -LiteralPath $provenancePath -Raw | ConvertFrom-Json
$source = $provenance.canonicalSource
$revision = [string]$source.revision
$repository = [string]$source.repository
$tag = [string]$source.tag

if ($revision -notmatch '^[0-9a-f]{40}$') {
    throw "Canonical component-store revision is not a 40-character commit: $revision"
}
if ($repository -ne 'https://github.com/ShawnSiao/siao-component-store') {
    throw "Unexpected component-store canonical repository: $repository"
}
if ($tag -notmatch '^v\d+\.\d+\.\d+$') {
    throw "Canonical component-store tag is not immutable-looking: $tag"
}
$remoteRefs = @(& git ls-remote --tags $repository "refs/tags/$tag" "refs/tags/$tag^{}" 2>$null)
$remoteExitCode = $LASTEXITCODE
if ($remoteExitCode -ne 0 -or -not ($remoteRefs | Where-Object { $_ -match "^$revision\s" })) {
    throw "Canonical tag $tag does not resolve to revision $revision on $repository."
}

$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
foreach ($package in @('siao-component-store-core', 'siao-component-store-catalogs')) {
    $repositoryPattern = [regex]::Escape($repository) + '(?:\.git)?'
    $pattern = [regex]::Escape($package) + '\s*=\s*\{[^}]*git\s*=\s*"' + $repositoryPattern + '"[^}]*rev\s*=\s*"([0-9a-f]{40})"'
    $match = [regex]::Match($manifest, $pattern)
    if (-not $match.Success) {
        throw "Cargo.toml does not pin $package to the canonical Git source."
    }
    if ($match.Groups[1].Value -ne $revision) {
        throw "$package pin does not match canonical revision $revision."
    }
}

$lockPath = Join-Path $root 'Cargo.lock'
$lock = Get-Content -LiteralPath $lockPath -Raw
$lockSource = "git+https://github.com/ShawnSiao/siao-component-store.git?rev=$revision#$revision"
if ($lock -notmatch [regex]::Escape($lockSource)) {
    throw "Cargo.lock does not record the canonical component-store revision $revision."
}

Write-Host "Component-store provenance pin passed: $tag ($revision)."
