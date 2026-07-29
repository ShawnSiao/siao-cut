param(
    [string]$SourceDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$contractPath = Join-Path $root 'release\whisper-runtime-source.json'
$contract = [IO.File]::ReadAllText($contractPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
$patchPath = Join-Path $root ($contract.patch.path -replace '/', '\')
$expectedTargets = @($contract.patch.targets | ForEach-Object { $_ -replace '\\', '/' })

if (-not $SourceDirectory) {
    $SourceDirectory = Join-Path $root 'third_party\whisper.cpp'
}

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

if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
    throw "whisper.cpp patch is missing: $patchPath"
}
$patchSha256 = Get-Sha256 $patchPath
if ($patchSha256 -ne $contract.patch.sha256) {
    throw "whisper.cpp patch hash mismatch. Expected $($contract.patch.sha256), got $patchSha256."
}

if (-not (Test-Path -LiteralPath (Join-Path $SourceDirectory '.git'))) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $SourceDirectory) | Out-Null
    & git clone --filter=blob:none "$($contract.source).git" $SourceDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Could not clone whisper.cpp.' }
}

$head = (& git -C $SourceDirectory rev-parse HEAD).Trim()
$changedFiles = @(& git -C $SourceDirectory diff --name-only | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$alreadyPatched = $false
if ($changedFiles.Count -gt 0) {
    $unexpected = @($changedFiles | Where-Object { $_ -notin $expectedTargets })
    if ($unexpected.Count -gt 0 -or $head -ne $contract.sourceCommit) {
        throw "whisper.cpp source contains unrelated changes: $($changedFiles -join ', ')"
    }
    & git -C $SourceDirectory apply --reverse --check $patchPath 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw 'whisper.cpp source is dirty but does not contain the declared SiaoCut patch.'
    }
    $alreadyPatched = $true
} else {
    & git -C $SourceDirectory cat-file -e "$($contract.sourceCommit)^{commit}" 2>$null
    if ($LASTEXITCODE -ne 0) {
        & git -C $SourceDirectory fetch --depth 1 origin $contract.sourceCommit
        if ($LASTEXITCODE -ne 0) { throw 'Could not fetch the pinned whisper.cpp commit.' }
    }
    if ($head -ne $contract.sourceCommit) {
        & git -C $SourceDirectory checkout --detach $contract.sourceCommit
        if ($LASTEXITCODE -ne 0) { throw 'Could not check out the pinned whisper.cpp commit.' }
    }
    & git -C $SourceDirectory apply --check $patchPath
    if ($LASTEXITCODE -ne 0) { throw 'The declared whisper.cpp patch does not apply cleanly.' }
    & git -C $SourceDirectory apply $patchPath
    if ($LASTEXITCODE -ne 0) { throw 'Could not apply the declared whisper.cpp patch.' }
}

$actualHead = (& git -C $SourceDirectory rev-parse HEAD).Trim()
if ($actualHead -ne $contract.sourceCommit) {
    throw "whisper.cpp source commit mismatch. Expected $($contract.sourceCommit), got $actualHead."
}
$actualTargets = @(& git -C $SourceDirectory diff --name-only | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if (($actualTargets -join "`n") -ne ($expectedTargets -join "`n")) {
    throw "whisper.cpp patch changed unexpected files: $($actualTargets -join ', ')"
}
& git -C $SourceDirectory diff --check
if ($LASTEXITCODE -ne 0) { throw 'Patched whisper.cpp source failed git diff --check.' }

[pscustomobject]@{
    source = $contract.source
    sourceCommit = $actualHead
    upstreamTokenMappingFixCommit = $contract.upstreamTokenMappingFix.commit
    patchPath = $contract.patch.path
    patchSha256 = $patchSha256
    alreadyPatched = $alreadyPatched
    changedFiles = $actualTargets
    sourceCapabilities = $contract.sourceCapabilities
} | ConvertTo-Json -Depth 6
