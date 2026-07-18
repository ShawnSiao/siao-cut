param(
    [string]$Core = ""
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
if (-not $Core) {
    $Core = Join-Path $root 'target\release\siaocut-core.exe'
}
if (-not (Test-Path -LiteralPath $Core)) {
    throw "Release Core not found: $Core"
}

$tempRoot = [IO.Path]::GetFullPath('D:\Temp')
$testHome = Join-Path $tempRoot ('siaocut-model-e2e-' + [guid]::NewGuid().ToString('N'))
$env:SIAOCUT_HOME = $testHome
$env:SIAOCUT_SERVICE_IDLE_MS = '500'
$env:SIAOCUT_MODEL_CHUNK_DELAY_MS = '120'

function Invoke-CoreJson {
    param([string[]]$Arguments)
    $raw = & $Core --json @Arguments | Out-String
    $value = $raw | ConvertFrom-Json
    if ($value.status -ne 'ok') {
        throw $value.message
    }
    return $value
}

try {
    $catalog = Invoke-CoreJson @('model', 'list')
    if ($catalog.models.Count -ne 3) { throw 'Expected three model profiles.' }
    if (($catalog.models | Where-Object recommended).id -ne 'base') { throw 'Expected base to be recommended.' }

    $first = Invoke-CoreJson @('model', 'install', 'tiny')
    Start-Sleep -Milliseconds 850
    Invoke-CoreJson @('model', 'cancel', $first.jobId) | Out-Null
    $deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        $cancelled = (Invoke-CoreJson @('model', 'status', $first.jobId)).modelJob
    } while ($cancelled.status -in @('queued', 'running') -and (Get-Date) -lt $deadline)
    $partial = Join-Path $testHome 'models\ggml-tiny.bin.part'
    if ($cancelled.status -ne 'cancelled') { throw "Expected cancelled, got $($cancelled.status)." }
    if (-not (Test-Path -LiteralPath $partial)) { throw 'Cancelled download did not retain a resumable partial file.' }
    $partialBytes = (Get-Item -LiteralPath $partial).Length
    if ($partialBytes -le 0 -or $partialBytes -ge 77691713) { throw "Unexpected partial size: $partialBytes" }

    Remove-Item Env:SIAOCUT_MODEL_CHUNK_DELAY_MS
    Start-Sleep -Milliseconds 900
    $resumed = Invoke-CoreJson @('model', 'install', 'tiny')
    if ($resumed.modelJob.bytesDownloaded -lt $partialBytes) { throw 'Resume job did not preserve partial progress.' }
    $deadline = (Get-Date).AddMinutes(3)
    do {
        Start-Sleep -Milliseconds 350
        $completed = (Invoke-CoreJson @('model', 'status', $resumed.jobId)).modelJob
        if ($completed.status -eq 'failed') { throw $completed.errorMessage }
    } while ($completed.status -in @('queued', 'running') -and (Get-Date) -lt $deadline)
    if ($completed.status -ne 'completed') { throw "Expected completed, got $($completed.status)." }
    $verified = (Invoke-CoreJson @('model', 'verify', 'tiny')).model
    if (-not $verified.verified) { throw 'Downloaded model did not pass SHA-256 verification.' }

    [pscustomobject]@{
        catalogCount = $catalog.models.Count
        cancelled = $cancelled.status
        partialBytes = $partialBytes
        resumedFrom = $resumed.modelJob.bytesDownloaded
        completed = $completed.status
        sha256 = $verified.sha256
    } | ConvertTo-Json
} finally {
    Remove-Item Env:SIAOCUT_MODEL_CHUNK_DELAY_MS -ErrorAction SilentlyContinue
    Remove-Item Env:SIAOCUT_SERVICE_IDLE_MS -ErrorAction SilentlyContinue
    Remove-Item Env:SIAOCUT_HOME -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
    $resolved = [IO.Path]::GetFullPath($testHome)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
