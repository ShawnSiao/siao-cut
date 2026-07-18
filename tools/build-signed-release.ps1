param(
    [Parameter(Mandatory = $true)]
    [string]$CertificateThumbprint,
    [Parameter(Mandatory = $true)]
    [string]$UpdaterPrivateKeyPath,
    [Parameter(Mandatory = $true)]
    [string]$UpdaterPublicKeyPath,
    [Parameter(Mandatory = $true)]
    [string]$UpdateEndpoint,
    [Parameter(Mandatory = $true)]
    [string]$DownloadBaseUrl,
    [string]$ReleaseNotes = '',
    [string]$TimestampUrl = 'http://timestamp.digicert.com'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$thumbprint = ($CertificateThumbprint -replace '\s', '').ToUpperInvariant()
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint" -ErrorAction Stop
if (-not $certificate.HasPrivateKey) { throw 'The selected code-signing certificate has no private key.' }
$codeSigningOid = '1.3.6.1.5.5.7.3.3'
if (-not ($certificate.EnhancedKeyUsageList.ObjectId.Value -contains $codeSigningOid)) {
    throw 'The selected certificate is not valid for code signing.'
}
$privateKey = Get-Item -LiteralPath $UpdaterPrivateKeyPath -ErrorAction Stop
$publicKey = [IO.File]::ReadAllText((Get-Item -LiteralPath $UpdaterPublicKeyPath -ErrorAction Stop).FullName).Trim()
if ([string]::IsNullOrWhiteSpace($publicKey)) { throw 'The updater public key is empty.' }
$updateUri = [uri]$UpdateEndpoint
$downloadBaseUri = [uri]$DownloadBaseUrl
if ($updateUri.Scheme -ne 'https') { throw 'UpdateEndpoint must use HTTPS.' }
if ($downloadBaseUri.Scheme -ne 'https') { throw 'DownloadBaseUrl must use HTTPS.' }
if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
    throw 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD must be set in the process environment.'
}

$configPath = Join-Path ([IO.Path]::GetTempPath()) ('siaocut-signing-' + [guid]::NewGuid().ToString('N') + '.json')
$config = @{
    bundle = @{
        createUpdaterArtifacts = $true
        windows = @{
            certificateThumbprint = $thumbprint
            digestAlgorithm = 'sha256'
            timestampUrl = $TimestampUrl
            tsp = $false
        }
    }
    plugins = @{
        updater = @{
            pubkey = $publicKey
            endpoints = @($UpdateEndpoint)
            windows = @{ installMode = 'passive' }
        }
    }
} | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))

try {
    $previousPrivateKey = $env:TAURI_SIGNING_PRIVATE_KEY
    $previousEndpoint = $env:SIAOCUT_UPDATE_ENDPOINT
    $previousPublicKey = $env:SIAOCUT_UPDATER_PUBKEY
    $previousEnabled = $env:SIAOCUT_UPDATER_ENABLED
    $env:TAURI_SIGNING_PRIVATE_KEY = $privateKey.FullName
    $env:SIAOCUT_UPDATE_ENDPOINT = $UpdateEndpoint
    $env:SIAOCUT_UPDATER_PUBKEY = $publicKey
    $env:SIAOCUT_UPDATER_ENABLED = '1'
    Push-Location (Join-Path $root 'apps\desktop')
    & (Join-Path $root 'apps\desktop\node_modules\.bin\tauri.cmd') build --config $configPath
    if ($LASTEXITCODE -ne 0) { throw "Tauri signed build failed with exit code $LASTEXITCODE." }
    Pop-Location
    $installer = Get-ChildItem (Join-Path $root 'apps\desktop\src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $installer) { throw 'Signed NSIS installer was not produced.' }
    $signature = Get-AuthenticodeSignature -LiteralPath $installer.FullName
    if ($signature.Status -ne 'Valid') { throw "Installer signature is not valid: $($signature.StatusMessage)" }
    $application = Get-Item -LiteralPath (Join-Path $root 'apps\desktop\src-tauri\target\release\siaocut-desktop.exe') -ErrorAction Stop
    $applicationSignature = Get-AuthenticodeSignature -LiteralPath $application.FullName
    if ($applicationSignature.Status -ne 'Valid') { throw "Application signature is not valid: $($applicationSignature.StatusMessage)" }
    $updaterSignature = Get-Item -LiteralPath ($installer.FullName + '.sig') -ErrorAction Stop
    $version = (Get-Content -Raw (Join-Path $root 'apps\desktop\src-tauri\tauri.conf.json') | ConvertFrom-Json).version
    $downloadUrl = $DownloadBaseUrl.TrimEnd('/') + '/' + $installer.Name
    $manifestPath = Join-Path $installer.Directory.FullName 'latest.json'
    $manifestResult = & (Join-Path $root 'tools\new-update-manifest.ps1') `
        -InstallerPath $installer.FullName `
        -SignaturePath $updaterSignature.FullName `
        -Version $version `
        -DownloadUrl $downloadUrl `
        -OutputPath $manifestPath `
        -Notes $ReleaseNotes | ConvertFrom-Json
    [pscustomobject]@{
        installer = $installer.FullName
        application = $application.FullName
        updaterSignature = $updaterSignature.FullName
        updateManifest = $manifestPath
        sha256 = $manifestResult.sha256
        signer = $signature.SignerCertificate.Subject
        thumbprint = $signature.SignerCertificate.Thumbprint
        timestamp = $signature.TimeStamperCertificate.Subject
    } | ConvertTo-Json
} finally {
    if ((Get-Location).Path -ne $root) { Pop-Location -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $configPath) { Remove-Item -LiteralPath $configPath -Force }
    $env:TAURI_SIGNING_PRIVATE_KEY = $previousPrivateKey
    $env:SIAOCUT_UPDATE_ENDPOINT = $previousEndpoint
    $env:SIAOCUT_UPDATER_PUBKEY = $previousPublicKey
    $env:SIAOCUT_UPDATER_ENABLED = $previousEnabled
}
