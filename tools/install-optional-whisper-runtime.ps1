param(
    [ValidateSet('whisper-cuda-11')]
    [string]$Runtime = 'whisper-cuda-11',
    [string]$Core = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifest = [IO.File]::ReadAllText((Join-Path $root 'release\runtime-manifest.json'), [Text.Encoding]::UTF8) | ConvertFrom-Json
$component = $manifest.components | Where-Object id -eq $Runtime
if (-not $component -or $component.kind -ne 'optional-runtime') { throw "Unknown optional runtime: $Runtime" }
if (-not $Core) { $Core = Join-Path $root 'target\release\siaocut-core.exe' }
$Core = (Resolve-Path -LiteralPath $Core).Path
$downloadDir = Join-Path $env:LOCALAPPDATA 'SiaoCut\downloads'
$runtimeDir = Join-Path $env:LOCALAPPDATA "SiaoCut\runtimes\$Runtime"
$archive = Join-Path $downloadDir ([IO.Path]::GetFileName([uri]$component.url))
$extract = Join-Path $downloadDir "$Runtime.extracting"
New-Item -ItemType Directory -Force -Path $downloadDir | Out-Null

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
        finally { $sha.Dispose() }
    } finally { $stream.Dispose() }
}

if (-not (Test-Path -LiteralPath $archive)) {
    Write-Host "Downloading $($component.name) ($([math]::Round($component.size / 1MB, 1)) MB) from $($component.source)..."
    Invoke-WebRequest -UseBasicParsing -Uri $component.url -OutFile $archive
}
$actual = Get-Sha256 $archive
if ($actual -ne $component.sha256) { throw "Runtime archive hash mismatch. Expected $($component.sha256), got $actual." }

if (Test-Path -LiteralPath $extract) { Remove-Item -LiteralPath $extract -Recurse -Force }
New-Item -ItemType Directory -Force -Path $extract | Out-Null
Expand-Archive -LiteralPath $archive -DestinationPath $extract -Force
$whisper = Get-ChildItem -LiteralPath $extract -Recurse -Filter 'whisper-cli.exe' | Select-Object -First 1
if (-not $whisper) { throw 'Verified runtime archive does not contain whisper-cli.exe.' }
if (Test-Path -LiteralPath $runtimeDir) { Remove-Item -LiteralPath $runtimeDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
Copy-Item -LiteralPath $whisper.FullName -Destination $runtimeDir -Force
Copy-Item -Path (Join-Path $whisper.DirectoryName '*.dll') -Destination $runtimeDir -Force
$installedWhisper = Join-Path $runtimeDir 'whisper-cli.exe'

$selectionRaw = & $Core --json runtime select $component.backend --whisper $installedWhisper --source $component.source --version $component.version --archive-sha256 $component.sha256 | Out-String
$selection = $selectionRaw | ConvertFrom-Json
if ($selection.status -ne 'ok') { throw $selection.message }
if (Test-Path -LiteralPath $extract) { Remove-Item -LiteralPath $extract -Recurse -Force }

[pscustomobject]@{
    backend = $selection.runtime.backend
    path = $selection.selection.whisperPath
    source = $component.source
    version = $component.version
    archiveSha256 = $component.sha256
    license = $component.license
    requires = $component.requires
} | ConvertTo-Json
