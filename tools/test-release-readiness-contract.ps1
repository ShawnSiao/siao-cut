$ErrorActionPreference = 'Stop'
$preflight = Join-Path $PSScriptRoot 'test-release-readiness.ps1'
[void][scriptblock]::Create((Get-Content -LiteralPath $preflight -Raw))

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('siaocut-release-readiness-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$previousPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
try {
    $privateKey = Join-Path $testRoot 'updater.key'
    $publicKey = Join-Path $testRoot 'updater.key.pub'
    [IO.File]::WriteAllText($privateKey, 'contract-private-key-placeholder')
    [IO.File]::WriteAllText($publicKey, 'contract-public-key-placeholder')
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'contract-secret-value'

    $positiveOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $preflight `
        -Mode Local `
        -UpdaterPrivateKeyPath $privateKey `
        -UpdaterPublicKeyPath $publicKey `
        -AllowIncomplete | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'AllowIncomplete preflight returned a nonzero exit code.' }
    $positive = $positiveOutput | ConvertFrom-Json
    $positiveChecks = @{}
    foreach ($check in $positive.checks) { $positiveChecks[$check.id] = $check.status }
    if ($positiveChecks.version_consistency -ne 'passed') { throw 'Version consistency did not pass.' }
    if ($positiveChecks.release_automation -ne 'passed') { throw 'Release automation did not pass.' }
    if ($positiveChecks.updater_private_key -ne 'passed') { throw 'Updater private key presence did not pass.' }
    if ($positiveChecks.updater_public_key -ne 'passed') { throw 'Updater public key presence did not pass.' }
    if ($positiveChecks.updater_key_password -ne 'passed') { throw 'Updater key password presence did not pass.' }
    if ($positiveChecks.authenticode_certificate -ne 'missing') { throw 'Missing certificate was not reported.' }
    if ($positiveOutput.Contains('contract-secret-value')) { throw 'Updater key password leaked into the report.' }
    if ($positiveOutput.Contains('contract-private-key-placeholder')) { throw 'Updater private key leaked into the report.' }

    $wrongVersionOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $preflight `
        -Mode Local `
        -ExpectedVersion '9.9.9' `
        -AllowIncomplete | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Wrong-version diagnostic returned a nonzero exit code.' }
    $wrongVersion = $wrongVersionOutput | ConvertFrom-Json
    $versionCheck = @($wrongVersion.checks | Where-Object { $_.id -eq 'version_consistency' })
    if ($versionCheck.Count -ne 1 -or $versionCheck[0].status -ne 'failed') {
        throw 'Wrong expected version was not rejected.'
    }

    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $null
    & powershell -NoProfile -ExecutionPolicy Bypass -File $preflight -Mode Local *> $null
    if ($LASTEXITCODE -ne 2) { throw "Strict incomplete preflight returned $LASTEXITCODE instead of 2." }

    [pscustomobject]@{
        status = 'passed'
        consistentVersion = $positive.expectedVersion
        localInputsAccepted = $true
        missingCertificateReported = $true
        wrongVersionRejected = $true
        strictIncompleteExitCode = 2
        secretValuesReported = $false
    } | ConvertTo-Json
} finally {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $previousPassword
    $resolvedTestRoot = [IO.Path]::GetFullPath($testRoot)
    if ($resolvedTestRoot.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
