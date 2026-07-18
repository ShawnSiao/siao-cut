param(
    [ValidateSet('All', 'GitHub', 'Local')]
    [string]$Mode = 'All',
    [string]$Repository = '',
    [string]$ExpectedVersion = '0.2.0',
    [string]$CertificateThumbprint = '',
    [string]$UpdaterPrivateKeyPath = '',
    [string]$UpdaterPublicKeyPath = '',
    [switch]$RequireWindows11,
    [switch]$AllowIncomplete
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$checks = [Collections.Generic.List[object]]::new()

function Add-ReadinessCheck {
    param(
        [string]$Id,
        [ValidateSet('passed', 'missing', 'failed', 'not_applicable')]
        [string]$Status,
        [string]$Detail
    )
    $checks.Add([pscustomobject]@{ id = $Id; status = $Status; detail = $Detail })
}

function Invoke-NativeCapture {
    param([scriptblock]$Command)
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'SilentlyContinue'
    try {
        $output = @(& $Command 2>$null)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    [pscustomobject]@{
        exitCode = $exitCode
        stdout = ($output -join [Environment]::NewLine)
    }
}

function Get-CargoPackageVersion {
    param([string]$Path)
    $content = Get-Content -LiteralPath $Path -Raw
    $match = [regex]::Match($content, '(?ms)^\[package\]\s*.*?^version\s*=\s*"([^"]+)"')
    if (-not $match.Success) { throw "Cannot read package version from $Path." }
    $match.Groups[1].Value
}

function Get-CargoLockedVersion {
    param([string]$Path, [string]$PackageName)
    $content = Get-Content -LiteralPath $Path -Raw
    $escapedName = [regex]::Escape($PackageName)
    $match = [regex]::Match(
        $content,
        "(?ms)^\[\[package\]\]\r?\nname = `"$escapedName`"\r?\nversion = `"([^`"]+)`""
    )
    if (-not $match.Success) { throw "Cannot read $PackageName from $Path." }
    $match.Groups[1].Value
}

function Get-JsonTopLevelVersion {
    param([string]$Path)
    $content = Get-Content -LiteralPath $Path -Raw
    $match = [regex]::Match($content, '"version"\s*:\s*"([^"]+)"')
    if (-not $match.Success) { throw "Cannot read version from $Path." }
    $match.Groups[1].Value
}

function Get-GitHubRepositoryFromOrigin {
    $gitResult = Invoke-NativeCapture { git -C $root remote get-url origin }
    if ($gitResult.exitCode -ne 0 -or [string]::IsNullOrWhiteSpace($gitResult.stdout)) { return '' }
    $normalized = $gitResult.stdout.Trim() -replace '\.git$', ''
    $match = [regex]::Match($normalized, 'github\.com[:/]([^/]+/[^/]+)$')
    if ($match.Success) { return $match.Groups[1].Value }
    ''
}

$versions = [ordered]@{
    coreCargo = Get-CargoPackageVersion (Join-Path $root 'Cargo.toml')
    coreLock = Get-CargoLockedVersion (Join-Path $root 'Cargo.lock') 'siaocut-core'
    rootPackage = Get-JsonTopLevelVersion (Join-Path $root 'package.json')
    desktopPackage = Get-JsonTopLevelVersion (Join-Path $root 'apps\desktop\package.json')
    desktopPackageLock = Get-JsonTopLevelVersion (Join-Path $root 'apps\desktop\package-lock.json')
    tauriConfig = Get-JsonTopLevelVersion (Join-Path $root 'apps\desktop\src-tauri\tauri.conf.json')
    desktopCargo = Get-CargoPackageVersion (Join-Path $root 'apps\desktop\src-tauri\Cargo.toml')
    desktopCargoLock = Get-CargoLockedVersion (Join-Path $root 'apps\desktop\src-tauri\Cargo.lock') 'siaocut-desktop'
}
$wrongVersions = @($versions.GetEnumerator() | Where-Object { $_.Value -ne $ExpectedVersion })
if ($wrongVersions.Count -eq 0) {
    Add-ReadinessCheck 'version_consistency' 'passed' "All release manifests and lockfiles use $ExpectedVersion."
} else {
    $mismatches = ($wrongVersions | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ', '
    Add-ReadinessCheck 'version_consistency' 'failed' "Expected $ExpectedVersion; mismatches: $mismatches."
}

$requiredFiles = @(
    '.github\workflows\release-windows.yml',
    '.github\workflows\promote-windows-release.yml',
    'tools\build-signed-release.ps1',
    'tools\test-local-updater.ps1'
)
$missingFiles = @($requiredFiles | Where-Object { -not (Test-Path -LiteralPath (Join-Path $root $_) -PathType Leaf) })
if ($missingFiles.Count -eq 0) {
    Add-ReadinessCheck 'release_automation' 'passed' 'Prerelease build, promotion, signing, and local updater verification files exist.'
} else {
    Add-ReadinessCheck 'release_automation' 'missing' ("Missing files: " + ($missingFiles -join ', '))
}

$windowsBuild = [Environment]::OSVersion.Version.Build
$windowsGeneration = if ($windowsBuild -ge 22000) { 'Windows 11' } elseif ($windowsBuild -ge 10240) { 'Windows 10' } else { 'unsupported Windows' }
if ($windowsBuild -ge 10240) {
    Add-ReadinessCheck 'current_windows_environment' 'passed' "$windowsGeneration build $windowsBuild."
} else {
    Add-ReadinessCheck 'current_windows_environment' 'failed' "Unsupported Windows build $windowsBuild."
}
if ($RequireWindows11) {
    if ($windowsBuild -ge 22000) {
        Add-ReadinessCheck 'windows_11_environment' 'passed' "Windows 11 build $windowsBuild is available."
    } else {
        Add-ReadinessCheck 'windows_11_environment' 'missing' 'A Windows 11 machine is required for the second upgrade and sleep/wake run.'
    }
}

$resolvedRepository = $Repository.Trim()
if ($Mode -in @('All', 'GitHub')) {
    $authResult = Invoke-NativeCapture { gh auth status --hostname github.com }
    if ($authResult.exitCode -eq 0) {
        Add-ReadinessCheck 'github_authentication' 'passed' 'GitHub CLI authentication is active.'
    } else {
        Add-ReadinessCheck 'github_authentication' 'missing' 'GitHub CLI authentication is unavailable.'
    }

    if ([string]::IsNullOrWhiteSpace($resolvedRepository)) {
        $resolvedRepository = Get-GitHubRepositoryFromOrigin
    }
    if ([string]::IsNullOrWhiteSpace($resolvedRepository)) {
        Add-ReadinessCheck 'github_repository' 'missing' 'No GitHub repository parameter or GitHub origin remote is configured.'
        Add-ReadinessCheck 'github_actions' 'missing' 'Repository is required before Actions permissions can be checked.'
        Add-ReadinessCheck 'github_release_secrets' 'missing' 'Repository is required before release secret names can be checked.'
    } else {
        $repositoryResult = Invoke-NativeCapture { gh repo view $resolvedRepository --json nameWithOwner,url,visibility }
        if ($repositoryResult.exitCode -ne 0) {
            Add-ReadinessCheck 'github_repository' 'missing' "$resolvedRepository does not exist or is not accessible."
            Add-ReadinessCheck 'github_actions' 'missing' 'Accessible repository is required before Actions permissions can be checked.'
            Add-ReadinessCheck 'github_release_secrets' 'missing' 'Accessible repository is required before release secret names can be checked.'
        } else {
            $repositoryInfo = $repositoryResult.stdout | ConvertFrom-Json
            $resolvedRepository = $repositoryInfo.nameWithOwner
            Add-ReadinessCheck 'github_repository' 'passed' "$resolvedRepository is accessible with $($repositoryInfo.visibility) visibility."

            $actionsResult = Invoke-NativeCapture { gh api "repos/$resolvedRepository/actions/permissions" }
            if ($actionsResult.exitCode -eq 0 -and ($actionsResult.stdout | ConvertFrom-Json).enabled) {
                Add-ReadinessCheck 'github_actions' 'passed' 'GitHub Actions is enabled.'
            } else {
                Add-ReadinessCheck 'github_actions' 'failed' 'GitHub Actions is disabled or its permission state cannot be read.'
            }

            $secretResult = Invoke-NativeCapture { gh secret list --repo $resolvedRepository --json name }
            if ($secretResult.exitCode -ne 0) {
                Add-ReadinessCheck 'github_release_secrets' 'failed' 'Release secret names cannot be read.'
            } else {
                $configuredNames = @($secretResult.stdout | ConvertFrom-Json | ForEach-Object { $_.name })
                $requiredNames = @(
                    'WINDOWS_CERTIFICATE_BASE64',
                    'WINDOWS_CERTIFICATE_PASSWORD',
                    'TAURI_SIGNING_PRIVATE_KEY',
                    'TAURI_SIGNING_PRIVATE_KEY_PASSWORD',
                    'TAURI_SIGNING_PUBLIC_KEY'
                )
                $missingNames = @($requiredNames | Where-Object { $_ -notin $configuredNames })
                if ($missingNames.Count -eq 0) {
                    Add-ReadinessCheck 'github_release_secrets' 'passed' 'All required release secret names are configured; values were not read.'
                } else {
                    Add-ReadinessCheck 'github_release_secrets' 'missing' ("Missing secret names: " + ($missingNames -join ', '))
                }
            }
        }
    }
}

if ($Mode -in @('All', 'Local')) {
    $normalizedThumbprint = ($CertificateThumbprint -replace '\s', '').ToUpperInvariant()
    if ([string]::IsNullOrWhiteSpace($normalizedThumbprint)) {
        Add-ReadinessCheck 'authenticode_certificate' 'missing' 'CertificateThumbprint was not provided.'
    } else {
        $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$normalizedThumbprint" -ErrorAction SilentlyContinue
        $codeSigningOid = '1.3.6.1.5.5.7.3.3'
        if (-not $certificate) {
            Add-ReadinessCheck 'authenticode_certificate' 'missing' 'The selected certificate is not installed in Cert:\CurrentUser\My.'
        } elseif (-not $certificate.HasPrivateKey) {
            Add-ReadinessCheck 'authenticode_certificate' 'failed' 'The selected certificate has no private key.'
        } elseif ($certificate.NotAfter -le (Get-Date)) {
            Add-ReadinessCheck 'authenticode_certificate' 'failed' 'The selected certificate is expired.'
        } elseif ($certificate.EnhancedKeyUsageList.ObjectId.Value -notcontains $codeSigningOid) {
            Add-ReadinessCheck 'authenticode_certificate' 'failed' 'The selected certificate is not valid for code signing.'
        } else {
            Add-ReadinessCheck 'authenticode_certificate' 'passed' "Code-signing certificate $normalizedThumbprint has a private key and is not expired."
        }
    }

    if ([string]::IsNullOrWhiteSpace($UpdaterPrivateKeyPath) -or -not (Test-Path -LiteralPath $UpdaterPrivateKeyPath -PathType Leaf)) {
        Add-ReadinessCheck 'updater_private_key' 'missing' 'UpdaterPrivateKeyPath does not identify an existing file.'
    } elseif ([string]::IsNullOrWhiteSpace([IO.File]::ReadAllText((Resolve-Path -LiteralPath $UpdaterPrivateKeyPath)))) {
        Add-ReadinessCheck 'updater_private_key' 'failed' 'Updater private key file is empty.'
    } else {
        Add-ReadinessCheck 'updater_private_key' 'passed' 'Updater private key file exists and is not empty; its value was not reported.'
    }
    if ([string]::IsNullOrWhiteSpace($UpdaterPublicKeyPath) -or -not (Test-Path -LiteralPath $UpdaterPublicKeyPath -PathType Leaf)) {
        Add-ReadinessCheck 'updater_public_key' 'missing' 'UpdaterPublicKeyPath does not identify an existing file.'
    } elseif ([string]::IsNullOrWhiteSpace([IO.File]::ReadAllText((Resolve-Path -LiteralPath $UpdaterPublicKeyPath)))) {
        Add-ReadinessCheck 'updater_public_key' 'failed' 'Updater public key file is empty.'
    } else {
        Add-ReadinessCheck 'updater_public_key' 'passed' 'Updater public key file exists and is not empty.'
    }
    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
        Add-ReadinessCheck 'updater_key_password' 'missing' 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set in the process environment.'
    } else {
        Add-ReadinessCheck 'updater_key_password' 'passed' 'Updater key password is present; its value was not read or reported.'
    }
}

$blockingChecks = @($checks | Where-Object { $_.status -in @('missing', 'failed') })
$status = if ($blockingChecks.Count -eq 0) { 'ready' } else { 'incomplete' }
$result = [ordered]@{
    status = $status
    mode = $Mode
    expectedVersion = $ExpectedVersion
    versions = $versions
    repository = if ([string]::IsNullOrWhiteSpace($resolvedRepository)) { $null } else { $resolvedRepository }
    checks = $checks
    blockingCheckIds = @($blockingChecks | ForEach-Object { $_.id })
}
$result | ConvertTo-Json -Depth 6
if ($status -ne 'ready' -and -not $AllowIncomplete) { exit 2 }
