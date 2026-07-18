param(
    [string]$SourceTestUrl = 'https://www.youtube.com/watch?v=HOfdboHvshg',
    [string]$FromVersion = '0.1.1',
    [string]$ToVersion = '0.2.0'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$token = [guid]::NewGuid().ToString('N')
$installDir = Join-Path $tempRoot "SiaoCut-Acceptance-$token"
$probeDir = Join-Path $env:LOCALAPPDATA 'SiaoCut\retention-probes'
$probe = Join-Path $probeDir "$token.txt"
$configPath = Join-Path $tempRoot "siaocut-installer-test-$token.json"
if ([version]$ToVersion -le [version]$FromVersion) { throw 'ToVersion must be higher than FromVersion.' }

function Build-AcceptanceInstaller([string]$Version) {
    $config = @{
        productName = 'SiaoCut Acceptance'
        version = $Version
        identifier = 'app.siaocut.desktop.acceptance'
    } | ConvertTo-Json -Depth 3
    [IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))
    Push-Location (Join-Path $root 'apps\desktop')
    & (Join-Path $root 'apps\desktop\node_modules\.bin\tauri.cmd') build --config $configPath | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Acceptance installer build $Version failed." }
    Pop-Location
    $installer = Get-ChildItem (Join-Path $root 'apps\desktop\src-tauri\target\release\bundle\nsis') -Filter 'SiaoCut Acceptance_*-setup.exe' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $installer) { throw "Acceptance installer $Version was not produced." }
    return $installer.FullName
}

function Install-Silent([string]$Installer) {
    $process = Start-Process -FilePath $Installer -ArgumentList '/S', "/D=$installDir" -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer failed with exit code $($process.ExitCode)." }
}

function Assert-DesktopStarts {
    $application = Join-Path $installDir 'siaocut-desktop.exe'
    if (-not (Test-Path -LiteralPath $application -PathType Leaf)) { throw 'Installed desktop application is missing.' }
    $process = Start-Process -FilePath $application -PassThru
    try {
        Start-Sleep -Seconds 3
        if ($process.HasExited) { throw "Installed desktop application exited during startup with code $($process.ExitCode)." }
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}

try {
    $v1 = Build-AcceptanceInstaller $FromVersion
    Install-Silent $v1
    Assert-DesktopStarts
    if (-not (Test-Path -LiteralPath (Join-Path $installDir 'siaocut-core.exe'))) { throw 'Core sidecar is missing after install.' }
    $ffmpeg = Join-Path $installDir 'runtime\ffmpeg\ffmpeg.exe'
    $whisper = Join-Path $installDir 'runtime\whisper\whisper-cli.exe'
    $vad = Join-Path $installDir 'runtime\whisper\ggml-silero-v6.2.0.bin'
    $vulkan = Join-Path $installDir 'runtime\whisper-vulkan\whisper-cli.exe'
    $ytDlp = Join-Path $installDir 'runtime\yt-dlp\yt-dlp.exe'
    $runtimeManifest = Join-Path $installDir 'runtime\runtime-manifest.json'
    $noticeManifest = Join-Path $installDir 'notices\runtime-manifest.json'
    $notices = Join-Path $installDir 'notices\THIRD_PARTY_NOTICES.md'
    $ytDlpSourceLicense = Join-Path $installDir 'notices\licenses\yt-dlp-Unlicense.txt'
    $ytDlpCombinedLicense = Join-Path $installDir 'notices\licenses\yt-dlp-GPL-3.0-and-third-party.txt'
    foreach ($required in @($ffmpeg, $whisper, $vad, $vulkan, $ytDlp, $runtimeManifest, $noticeManifest, $notices, $ytDlpSourceLicense, $ytDlpCombinedLicense)) {
        if (-not (Test-Path -LiteralPath $required)) { throw "Packaged runtime is missing: $required" }
    }
    $packagedManifest = [IO.File]::ReadAllText($runtimeManifest, [Text.Encoding]::UTF8) | ConvertFrom-Json
    $sourceComponent = $packagedManifest.components | Where-Object id -eq 'yt-dlp'
    if (-not $sourceComponent -or $sourceComponent.selfUpdate -ne $false) { throw 'Packaged manifest does not pin yt-dlp with self-update disabled.' }
    if ((Get-Item -LiteralPath $ytDlp).Length -ne [long]$sourceComponent.size) { throw 'Packaged yt-dlp size does not match the release manifest.' }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $ytDlp).Hash.ToLowerInvariant() -ne $sourceComponent.sha256) { throw 'Packaged yt-dlp hash does not match the release manifest.' }
    foreach ($license in $sourceComponent.licenseFiles) {
        $licensePath = Join-Path $installDir "notices\licenses\$($license.target)"
        if ((Get-Item -LiteralPath $licensePath).Length -ne [long]$license.size) { throw "Packaged license size mismatch: $($license.target)" }
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $licensePath).Hash.ToLowerInvariant() -ne $license.sha256) { throw "Packaged license hash mismatch: $($license.target)" }
    }
    $ytDlpVersion = (& $ytDlp --version | Out-String).Trim()
    if ($ytDlpVersion -ne $sourceComponent.version) { throw "Packaged yt-dlp version mismatch: $ytDlpVersion" }
    $env:SIAOCUT_HOME = Join-Path $installDir 'acceptance-home'
    $env:SIAOCUT_DIRECT = '1'
    $env:SIAOCUT_FFMPEG = $ffmpeg
    $env:SIAOCUT_FFPROBE = Join-Path $installDir 'runtime\ffmpeg\ffprobe.exe'
    $env:SIAOCUT_WHISPER_CLI = $whisper
    $env:SIAOCUT_WHISPER_VAD_MODEL = $vad
    $env:SIAOCUT_YTDLP = $ytDlp
    $env:SIAOCUT_SERVICE_IDLE_MS = '100'
    $health = & (Join-Path $installDir 'siaocut-core.exe') --json health | Out-String | ConvertFrom-Json
    if ($health.status -ne 'ok' -or $health.engines.ffmpeg -ne 'configured' -or $health.engines.asr -ne 'configured' -or $health.engines.sourceImport -ne 'configured') {
        throw 'Installed Core did not pass the packaged runtime health check.'
    }
    $projectsBefore = & (Join-Path $installDir 'siaocut-core.exe') --json project list | Out-String | ConvertFrom-Json
    $sourceInspection = & (Join-Path $installDir 'siaocut-core.exe') --json source inspect $SourceTestUrl | Out-String | ConvertFrom-Json
    $projectsAfter = & (Join-Path $installDir 'siaocut-core.exe') --json project list | Out-String | ConvertFrom-Json
    if ($sourceInspection.status -ne 'ok' -or -not $sourceInspection.source.siteMediaId) { throw 'Installed Core could not inspect the authorized public source.' }
    if (@($projectsBefore.projects).Count -ne @($projectsAfter.projects).Count) { throw 'Installed source inspection created a project before confirmation.' }
    Start-Sleep -Milliseconds 500
    New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
    [IO.File]::WriteAllText($probe, 'must survive install, upgrade, and uninstall', [Text.UTF8Encoding]::new($false))

    $v2 = Build-AcceptanceInstaller $ToVersion
    Install-Silent $v2
    Assert-DesktopStarts
    if (-not (Test-Path -LiteralPath $probe)) { throw 'User data probe was deleted during upgrade.' }
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller)) { throw 'Uninstaller is missing after upgrade.' }
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw "Uninstaller failed with exit code $($uninstall.ExitCode)." }
    if (-not (Test-Path -LiteralPath $probe)) { throw 'User data probe was deleted during uninstall.' }

    [pscustomobject]@{
        installed = $FromVersion
        upgraded = $ToVersion
        sidecarPresent = $true
        userDataAfterUpgrade = $true
        userDataAfterUninstall = $true
        installedCoreHealth = 'ok'
        packagedCpuRuntime = $true
        packagedVulkanRuntime = $true
        packagedYtDlpVersion = $ytDlpVersion
        packagedYtDlpHash = $sourceComponent.sha256
        packagedYtDlpLicenses = @($sourceComponent.licenseFiles).Count
        installedSourceInspection = 'ok'
        installedDesktopStartup = 'ok'
        sourceInspectionCreatedProject = $false
        installerSignature = (Get-AuthenticodeSignature -LiteralPath $v2).Status.ToString()
        testProduct = 'SiaoCut Acceptance'
    } | ConvertTo-Json
} finally {
    if ((Get-Location).Path -ne $root) { Pop-Location -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $configPath) { Remove-Item -LiteralPath $configPath -Force }
    if (Test-Path -LiteralPath $probe) { Remove-Item -LiteralPath $probe -Force }
    if ((Test-Path -LiteralPath $probeDir) -and -not (Get-ChildItem -LiteralPath $probeDir -Force | Select-Object -First 1)) { Remove-Item -LiteralPath $probeDir -Force }
    $resolved = [IO.Path]::GetFullPath($installDir)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Start-Sleep -Milliseconds 500
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
