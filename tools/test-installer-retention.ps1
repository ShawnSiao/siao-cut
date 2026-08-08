param(
    [string]$SourceTestUrl = 'https://www.youtube.com/watch?v=HOfdboHvshg',
    [string]$FromVersion = '0.1.1',
    [string]$ToVersion = '0.2.0',
    [string]$FromInstallerPath = '',
    [string]$ExternalRuntimeRoot = '',
    [string]$ValidationRoot = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Get-CargoTargetDirectory {
    param([string]$ManifestPath)

    $metadataOutput = & cargo metadata --manifest-path $ManifestPath --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve the Cargo target directory.' }
    $metadata = ($metadataOutput -join "`n") | ConvertFrom-Json
    return [IO.Path]::GetFullPath($metadata.target_directory)
}

$tauriTargetDirectory = Get-CargoTargetDirectory -ManifestPath (Join-Path $root 'apps\desktop\src-tauri\Cargo.toml')
$systemTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
if ([string]::IsNullOrWhiteSpace($ValidationRoot)) {
    $tempRoot = $systemTempRoot
} else {
    New-Item -ItemType Directory -Force -Path $ValidationRoot | Out-Null
    $tempRoot = (Resolve-Path -LiteralPath $ValidationRoot -ErrorAction Stop).Path
}

function Get-NormalizedWindowsPath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    if ($full.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) {
        return '\\' + $full.Substring(8)
    }
    if ($full.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) {
        return $full.Substring(4)
    }
    return $full
}
$token = [guid]::NewGuid().ToString('N')
$installDir = Join-Path $tempRoot "SiaoCut-Acceptance-$token"
$managedResourceRoot = Join-Path $tempRoot "SiaoCut-Resources-$token"
$resourceConfigHome = Join-Path $tempRoot "SiaoCut-Resource-Config-$token"
$resourceSentinel = Join-Path $managedResourceRoot 'retention-sentinel.txt'
$probeDir = Join-Path $env:LOCALAPPDATA 'SiaoCut\retention-probes'
$probe = Join-Path $probeDir "$token.txt"
$configPath = Join-Path $tempRoot "siaocut-installer-test-$token.json"
if ([version]$ToVersion -le [version]$FromVersion) { throw 'ToVersion must be higher than FromVersion.' }
if (-not [string]::IsNullOrWhiteSpace($FromInstallerPath)) {
    $FromInstallerPath = (Resolve-Path -LiteralPath $FromInstallerPath -ErrorAction Stop).Path
}

$externalRuntime = $null
if (-not [string]::IsNullOrWhiteSpace($ExternalRuntimeRoot)) {
    $ExternalRuntimeRoot = (Resolve-Path -LiteralPath $ExternalRuntimeRoot -ErrorAction Stop).Path
    $externalRuntime = [ordered]@{
        ffmpeg = Join-Path $ExternalRuntimeRoot 'ffmpeg\ffmpeg.exe'
        ffprobe = Join-Path $ExternalRuntimeRoot 'ffmpeg\ffprobe.exe'
        whisper = Join-Path $ExternalRuntimeRoot 'whisper\whisper-cli.exe'
        vad = Join-Path $ExternalRuntimeRoot 'whisper\ggml-silero-v6.2.0.bin'
        ytDlp = Join-Path $ExternalRuntimeRoot 'yt-dlp\yt-dlp.exe'
        whisperVulkan = Join-Path $ExternalRuntimeRoot 'whisper-vulkan\whisper-cli.exe'
    }
    foreach ($required in @('ffmpeg', 'ffprobe', 'whisper', 'vad', 'ytDlp')) {
        if (-not (Test-Path -LiteralPath $externalRuntime[$required] -PathType Leaf)) {
            throw "External runtime root is missing ${required}: $($externalRuntime[$required])"
        }
    }
}

$testEnvironmentNames = @(
    'SIAOCUT_HOME',
    'SIAOCUT_RESOURCE_CONFIG_HOME',
    'SIAOCUT_DIRECT',
    'SIAOCUT_FFMPEG',
    'SIAOCUT_FFPROBE',
    'SIAOCUT_WHISPER_CLI',
    'SIAOCUT_WHISPER_VULKAN_CLI',
    'SIAOCUT_WHISPER_VAD_MODEL',
    'SIAOCUT_YTDLP',
    'SIAOCUT_SERVICE_IDLE_MS'
)
$previousEnvironment = @{}
foreach ($name in $testEnvironmentNames) {
    $previousEnvironment[$name] = [pscustomobject]@{
        exists = Test-Path -LiteralPath "Env:$name"
        value = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
}

function Build-AcceptanceInstaller([string]$Version) {
    $config = @{
        productName = 'SiaoCut Acceptance'
        version = $Version
        identifier = 'app.siaocut.desktop.acceptance'
    } | ConvertTo-Json -Depth 3
    [IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))
    Push-Location (Join-Path $root 'apps\desktop')
    try {
        & (Join-Path $root 'apps\desktop\node_modules\.bin\tauri.cmd') build --config $configPath | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "Acceptance installer build $Version failed." }
    } finally {
        Pop-Location
    }
    $installer = Get-ChildItem (Join-Path $tauriTargetDirectory 'release\bundle\nsis') -Filter 'SiaoCut Acceptance_*-setup.exe' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
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

function Get-DirectorySizeBytes([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return [long]0 }
    return [long](Get-ChildItem -LiteralPath $Path -Recurse -File -Force | Measure-Object -Property Length -Sum).Sum
}

function Assert-AppOnlyPackage([string]$InstallerPath) {
    $noticeManifest = Join-Path $installDir 'notices\runtime-manifest.json'
    $notices = Join-Path $installDir 'notices\THIRD_PARTY_NOTICES.md'
    $licenseDirectory = Join-Path $installDir 'notices\licenses'
    foreach ($required in @(
        (Join-Path $installDir 'siaocut-desktop.exe'),
        (Join-Path $installDir 'siaocut-core.exe'),
        $noticeManifest,
        $notices,
        $licenseDirectory
    )) {
        if (-not (Test-Path -LiteralPath $required)) { throw "App-only package is missing: $required" }
    }
    $manifest = [IO.File]::ReadAllText($noticeManifest, [Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($manifest.packageProfile -ne 'app-only') { throw 'Packaged component manifest is not marked app-only.' }
    $bundledComponents = @($manifest.components | Where-Object { $_.bundled -eq $true })
    if ($bundledComponents.Count -gt 0) {
        throw "Packaged component manifest still marks components as bundled: $($bundledComponents.id -join ', ')"
    }
    $legacyRuntimeDirectory = Join-Path $installDir 'runtime'
    if (Test-Path -LiteralPath $legacyRuntimeDirectory) { throw "App-only package contains a runtime directory: $legacyRuntimeDirectory" }
    $forbidden = Get-ChildItem -LiteralPath $installDir -Recurse -File -Force | Where-Object {
        $_.Name -match '^(ffmpeg|ffprobe|whisper-cli|yt-dlp)\.exe$' -or
        $_.Name -match '^ggml-.*\.bin$' -or
        $_.Name -match '\.(onnx|gguf)$' -or
        $_.Name -match '^(av(codec|device|filter|format|util|resample|swresample|scale)-.*|onnxruntime.*|vulkan-.*|ggml.*)\.dll$'
    }
    if (@($forbidden).Count -gt 0) {
        throw "App-only package contains runtime files: $($forbidden.FullName -join ', ')"
    }
    $installerSize = (Get-Item -LiteralPath $InstallerPath).Length
    $installedSize = Get-DirectorySizeBytes $installDir
    if ($installerSize -gt 50MB) {
        $largest = Get-ChildItem -LiteralPath $installDir -Recurse -File -Force |
            Sort-Object Length -Descending |
            Select-Object -First 5 -Property FullName, Length
        throw "App-only installer is larger than 50 MiB ($installerSize bytes): $($largest | ConvertTo-Json -Compress)"
    }
    return [pscustomobject]@{
        installerSizeBytes = $installerSize
        installedDirectorySizeBytes = $installedSize
        manifestPath = $noticeManifest
    }
}

function Stop-InstalledProcesses([string]$Root) {
    $rootPrefix = ([IO.Path]::GetFullPath($Root)).TrimEnd('\') + '\'
    foreach ($processName in @('siaocut-core', 'siaocut-desktop')) {
        Get-Process -Name $processName -ErrorAction SilentlyContinue | ForEach-Object {
            try {
                $processPath = $_.Path
                if ($processPath -and $processPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
                    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
                }
            } catch {
                # The process may have exited between enumeration and inspection.
            }
        }
    }
}

try {
    $usesHistoricalInstaller = -not [string]::IsNullOrWhiteSpace($FromInstallerPath)
    $v1 = if ($usesHistoricalInstaller) { $FromInstallerPath } else { Build-AcceptanceInstaller $FromVersion }
    Install-Silent $v1
    $v1Package = if ($usesHistoricalInstaller) { $null } else { Assert-AppOnlyPackage -InstallerPath $v1 }
    Assert-DesktopStarts
    $env:SIAOCUT_HOME = Join-Path $installDir 'acceptance-home'
    $env:SIAOCUT_RESOURCE_CONFIG_HOME = $resourceConfigHome
    $env:SIAOCUT_DIRECT = '1'
    $env:SIAOCUT_SERVICE_IDLE_MS = '100'
    if ($null -ne $externalRuntime) {
        $env:SIAOCUT_FFMPEG = $externalRuntime.ffmpeg
        $env:SIAOCUT_FFPROBE = $externalRuntime.ffprobe
        $env:SIAOCUT_WHISPER_CLI = $externalRuntime.whisper
        $env:SIAOCUT_WHISPER_VAD_MODEL = $externalRuntime.vad
        $env:SIAOCUT_YTDLP = $externalRuntime.ytDlp
        if (Test-Path -LiteralPath $externalRuntime.whisperVulkan -PathType Leaf) {
            $env:SIAOCUT_WHISPER_VULKAN_CLI = $externalRuntime.whisperVulkan
        } else {
            Remove-Item -LiteralPath Env:SIAOCUT_WHISPER_VULKAN_CLI -ErrorAction SilentlyContinue
        }
    } else {
        $missingRuntimeRoot = Join-Path $installDir 'missing-runtime'
        $env:SIAOCUT_FFMPEG = Join-Path $missingRuntimeRoot 'ffmpeg.exe'
        $env:SIAOCUT_FFPROBE = Join-Path $missingRuntimeRoot 'ffprobe.exe'
        $env:SIAOCUT_WHISPER_CLI = Join-Path $missingRuntimeRoot 'whisper-cli.exe'
        $env:SIAOCUT_WHISPER_VULKAN_CLI = Join-Path $missingRuntimeRoot 'whisper-vulkan-cli.exe'
        $env:SIAOCUT_WHISPER_VAD_MODEL = Join-Path $missingRuntimeRoot 'ggml-silero-v6.2.0.bin'
        $env:SIAOCUT_YTDLP = Join-Path $missingRuntimeRoot 'yt-dlp.exe'
    }
    $health = & (Join-Path $installDir 'siaocut-core.exe') --json health | Out-String | ConvertFrom-Json
    if ($health.status -ne 'ok') {
        throw 'Installed Core health did not return status=ok.'
    }
    $resourceSetup = & (Join-Path $installDir 'siaocut-core.exe') --json resources configure --root $managedResourceRoot | Out-String | ConvertFrom-Json
    if ($resourceSetup.status -ne 'ok' -or -not $resourceSetup.localResources.configured) {
        throw 'Installed Core could not configure an explicitly selected resource location.'
    }
    $configuredRoot = Get-NormalizedWindowsPath ([string]$resourceSetup.localResources.root)
    if (-not $configuredRoot.Equals((Get-NormalizedWindowsPath $managedResourceRoot), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Installed Core changed the selected resource location: $configuredRoot"
    }
    [IO.File]::WriteAllText($resourceSentinel, 'must survive install, upgrade, and uninstall', [Text.UTF8Encoding]::new($false))
    $sourceInspectionStatus = 'not_run_without_external_runtime'
    if ($null -eq $externalRuntime) {
        if ($health.engines.ffmpeg -ne 'not_configured' -or $health.engines.asr -ne 'not_configured' -or $health.engines.sourceImport -ne 'not_configured') {
            throw 'App-only package unexpectedly reported a bundled runtime.'
        }
    } else {
        if ($health.engines.ffmpeg -ne 'configured' -or $health.engines.asr -ne 'configured' -or $health.engines.sourceImport -ne 'configured') {
            throw 'External runtime health check did not configure all required engines.'
        }
        $projectsBefore = & (Join-Path $installDir 'siaocut-core.exe') --json project list | Out-String | ConvertFrom-Json
        $sourceInspection = & (Join-Path $installDir 'siaocut-core.exe') --json source inspect $SourceTestUrl | Out-String | ConvertFrom-Json
        $projectsAfter = & (Join-Path $installDir 'siaocut-core.exe') --json project list | Out-String | ConvertFrom-Json
        if ($sourceInspection.status -ne 'ok' -or -not $sourceInspection.source.siteMediaId) { throw 'Installed Core could not inspect the authorized public source with the external runtime.' }
        if (@($projectsBefore.projects).Count -ne @($projectsAfter.projects).Count) { throw 'Installed source inspection created a project before confirmation.' }
        $sourceInspectionStatus = 'ok'
    }
    Start-Sleep -Milliseconds 500
    New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
    [IO.File]::WriteAllText($probe, 'must survive install, upgrade, and uninstall', [Text.UTF8Encoding]::new($false))

    $v2 = Build-AcceptanceInstaller $ToVersion
    Install-Silent $v2
    $v2Package = if ($usesHistoricalInstaller) { $null } else { Assert-AppOnlyPackage -InstallerPath $v2 }
    Assert-DesktopStarts
    if (-not (Test-Path -LiteralPath $probe)) { throw 'User data probe was deleted during upgrade.' }
    $resourcesAfterUpgrade = & (Join-Path $installDir 'siaocut-core.exe') --json resources status | Out-String | ConvertFrom-Json
    if ($resourcesAfterUpgrade.status -ne 'ok' -or -not $resourcesAfterUpgrade.localResources.configured) {
        throw 'Resource configuration did not survive the application upgrade.'
    }
    if (-not (Test-Path -LiteralPath $resourceSentinel -PathType Leaf)) { throw 'Selected resource directory was deleted during upgrade.' }
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller)) { throw 'Uninstaller is missing after upgrade.' }
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw "Uninstaller failed with exit code $($uninstall.ExitCode)." }
    if (-not (Test-Path -LiteralPath $probe)) { throw 'User data probe was deleted during uninstall.' }
    if (-not (Test-Path -LiteralPath (Join-Path $resourceConfigHome 'local-resources.json') -PathType Leaf)) { throw 'Resource configuration was deleted during uninstall.' }
    if (-not (Test-Path -LiteralPath $resourceSentinel -PathType Leaf)) { throw 'Selected resource directory was deleted during uninstall.' }
    Install-Silent $v2
    Assert-DesktopStarts
    $resourcesAfterReinstall = & (Join-Path $installDir 'siaocut-core.exe') --json resources status | Out-String | ConvertFrom-Json
    if ($resourcesAfterReinstall.status -ne 'ok' -or -not $resourcesAfterReinstall.localResources.configured) {
        throw 'Resource configuration was not restored after reinstall.'
    }
    if (-not (Test-Path -LiteralPath $resourceSentinel -PathType Leaf)) { throw 'Selected resource directory was not retained after reinstall.' }
    $reinstalledUninstaller = Join-Path $installDir 'uninstall.exe'
    $reinstalledUninstall = Start-Process -FilePath $reinstalledUninstaller -ArgumentList '/S' -Wait -PassThru
    if ($reinstalledUninstall.ExitCode -ne 0) { throw "Reinstalled application uninstall failed with exit code $($reinstalledUninstall.ExitCode)." }
    if (-not (Test-Path -LiteralPath $resourceSentinel -PathType Leaf)) { throw 'Selected resource directory was deleted after reinstall.' }

    [pscustomobject]@{
        installed = $FromVersion
        upgraded = $ToVersion
        sidecarPresent = $true
        userDataAfterUpgrade = $true
        userDataAfterUninstall = $true
        resourceConfigAfterUpgrade = $true
        resourceConfigAfterUninstall = $true
        selectedResourceDirectoryAfterUninstall = $true
        resourceConfigAfterReinstall = $true
        selectedResourceDirectoryAfterReinstall = $true
        reinstallCompleted = $true
        selectedResourceRoot = $managedResourceRoot
        installedCoreHealth = 'ok'
        packageProfile = 'app-only'
        externalRuntimeConfigured = $null -ne $externalRuntime
        runtimeFilesInPackage = $false
        installerSizeBytes = if ($null -ne $v2Package) { $v2Package.installerSizeBytes } else { $null }
        installedDirectorySizeBytes = if ($null -ne $v2Package) { $v2Package.installedDirectorySizeBytes } else { $null }
        installedSourceInspection = $sourceInspectionStatus
        installedDesktopStartup = 'ok'
        sourceInspectionCreatedProject = $false
        installerSignature = (Get-AuthenticodeSignature -LiteralPath $v2).Status.ToString()
        testProduct = 'SiaoCut Acceptance'
        upgradeEvidence = if ($usesHistoricalInstaller) { 'historical-acceptance-installer' } else { 'same-source-installer-contract' }
        historicalBinaryUpgrade = $usesHistoricalInstaller
    } | ConvertTo-Json
} finally {
    foreach ($name in $testEnvironmentNames) {
        $previous = $previousEnvironment[$name]
        if ($previous.exists) {
            [Environment]::SetEnvironmentVariable($name, $previous.value, 'Process')
        } else {
            Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
        }
    }
    if (Test-Path -LiteralPath $configPath) { Remove-Item -LiteralPath $configPath -Force }
    if (Test-Path -LiteralPath $probe) { Remove-Item -LiteralPath $probe -Force }
    if ((Test-Path -LiteralPath $probeDir) -and -not (Get-ChildItem -LiteralPath $probeDir -Force | Select-Object -First 1)) { Remove-Item -LiteralPath $probeDir -Force }
    foreach ($generated in @($managedResourceRoot, $resourceConfigHome)) {
        $resolvedGenerated = [IO.Path]::GetFullPath($generated)
        $tempPrefix = $tempRoot.TrimEnd('\') + '\'
        if ($resolvedGenerated.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolvedGenerated) -match '^SiaoCut-(Resources|Resource-Config)-[0-9a-f]{32}$' -and (Test-Path -LiteralPath $resolvedGenerated)) {
            Remove-Item -LiteralPath $resolvedGenerated -Recurse -Force
        }
    }
    $resolved = [IO.Path]::GetFullPath($installDir)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Stop-InstalledProcesses -Root $resolved
        for ($attempt = 0; $attempt -lt 6 -and (Test-Path -LiteralPath $resolved); $attempt++) {
            try {
                Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop
            } catch {
                if ($attempt -eq 5) {
                    Write-Warning "Unable to remove temporary acceptance directory after stopping its processes: $resolved"
                } else {
                    Start-Sleep -Milliseconds 500
                    Stop-InstalledProcesses -Root $resolved
                }
            }
        }
    }
}
