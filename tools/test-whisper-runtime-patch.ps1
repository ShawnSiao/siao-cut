param(
    [string]$SourceRepository
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$contractPath = Join-Path $root 'release\whisper-runtime-source.json'
$contract = [IO.File]::ReadAllText($contractPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
$patchPath = Join-Path $root ($contract.patch.path -replace '/', '\')
if (-not $SourceRepository) {
    $SourceRepository = Join-Path $root 'third_party\whisper.cpp'
}
if (-not (Test-Path -LiteralPath (Join-Path $SourceRepository '.git'))) {
    throw "A local whisper.cpp Git repository is required: $SourceRepository"
}

$tempRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) "siaocut-whisper-patch-$([guid]::NewGuid().ToString('N'))"))
$systemTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
if (-not $tempRoot.StartsWith($systemTemp, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a temporary directory outside the system temp root: $tempRoot"
}

try {
    & git clone --no-hardlinks --no-checkout $SourceRepository $tempRoot
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the isolated whisper.cpp patch fixture.' }
    & git -C $tempRoot checkout --detach $contract.sourceCommit
    if ($LASTEXITCODE -ne 0) { throw 'Could not check out the pinned whisper.cpp source fixture.' }

    $cliPath = Join-Path $tempRoot 'examples\cli\cli.cpp'
    $baseline = [IO.File]::ReadAllText($cliPath, [Text.Encoding]::UTF8)
    if (-not $baseline.Contains('times_o(mt.data.t0, mt.t1, false);')) {
        throw 'The unpatched fixture no longer exposes raw VAD token times in CLI JSON.'
    }
    if ($baseline.Contains('whisper_full_get_token_t0(ctx, i, j)')) {
        throw 'The pinned fixture unexpectedly contains the SiaoCut CLI mapping patch.'
    }
    & git -C $tempRoot apply --check $patchPath
    if ($LASTEXITCODE -ne 0) { throw 'The patch does not apply to the pinned clean source.' }

    $prepared = & (Join-Path $root 'tools\prepare-whisper-source.ps1') -SourceDirectory $tempRoot | ConvertFrom-Json
    if ($prepared.sourceCommit -ne $contract.sourceCommit) {
        throw 'The prepared source does not match the pinned commit.'
    }
    if ($prepared.patchSha256 -ne $contract.patch.sha256) {
        throw 'The prepared source does not match the declared patch.'
    }
    $patched = [IO.File]::ReadAllText($cliPath, [Text.Encoding]::UTF8)
    foreach ($required in @(
        'whisper_full_get_token_t0(ctx, i, j)',
        'whisper_full_get_token_t1(ctx, i, j)',
        'times_o(mt.t0, mt.t1, false);'
    )) {
        if (-not $patched.Contains($required)) {
            throw "Patched CLI JSON writer is missing: $required"
        }
    }
    if ($patched.Contains('times_o(mt.data.t0, mt.t1, false);')) {
        throw 'Patched CLI JSON writer still exports raw VAD token timestamps.'
    }
    $previousErrorPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & git -C $tempRoot apply --check $patchPath *> $null
    $repeatApplyExitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorPreference
    if ($repeatApplyExitCode -eq 0) {
        throw 'The patch can be applied twice; repeated application must fail safely.'
    }
    & git -C $tempRoot apply --reverse --check $patchPath
    if ($LASTEXITCODE -ne 0) {
        throw 'The patched source cannot prove the declared patch was applied.'
    }

    [pscustomobject]@{
        status = 'passed'
        sourceCommit = $prepared.sourceCommit
        patchSha256 = $prepared.patchSha256
        unpatchedCliJsonTimeDomain = 'vad_processed'
        patchedCliJsonTimeDomain = 'original_media'
        changedFiles = @($prepared.changedFiles)
    } | ConvertTo-Json -Depth 4
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        $resolvedTemp = [IO.Path]::GetFullPath($tempRoot)
        if (-not $resolvedTemp.StartsWith($systemTemp, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove a directory outside the system temp root: $resolvedTemp"
        }
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}
