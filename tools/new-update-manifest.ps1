param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,
    [Parameter(Mandatory = $true)]
    [string]$SignaturePath,
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [string]$DownloadUrl,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$Notes = '',
    [datetime]$PublishedAt = [datetime]::UtcNow
)

$ErrorActionPreference = 'Stop'
$installer = Get-Item -LiteralPath $InstallerPath -ErrorAction Stop
$signatureFile = Get-Item -LiteralPath $SignaturePath -ErrorAction Stop
if ($Version -notmatch '^v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') {
    throw 'Version must be a valid SemVer release version.'
}
$downloadUri = [uri]$DownloadUrl
if ($downloadUri.Scheme -ne 'https') { throw 'DownloadUrl must use HTTPS.' }
$signature = [IO.File]::ReadAllText($signatureFile.FullName).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) { throw 'The Tauri signature file is empty.' }
$sha256 = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$platform = [ordered]@{
    signature = $signature
    url = $downloadUri.AbsoluteUri
    size = [long]$installer.Length
    sha256 = $sha256
}
$manifest = [ordered]@{
    version = $Version.TrimStart('v')
    notes = $Notes
    pub_date = $PublishedAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64-nsis' = $platform
    }
}
$resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
$outputDirectory = Split-Path -Parent $resolvedOutput
if (-not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
}
[IO.File]::WriteAllText(
    $resolvedOutput,
    ($manifest | ConvertTo-Json -Depth 6),
    [Text.UTF8Encoding]::new($false)
)
[pscustomobject]@{
    manifest = $resolvedOutput
    installer = $installer.FullName
    size = [long]$installer.Length
    sha256 = $sha256
    platform = 'windows-x86_64-nsis'
} | ConvertTo-Json
