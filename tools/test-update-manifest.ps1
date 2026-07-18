$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$scripts = @(
    (Join-Path $PSScriptRoot 'new-update-manifest.ps1'),
    (Join-Path $PSScriptRoot 'build-signed-release.ps1')
)
foreach ($script in $scripts) {
    [void][scriptblock]::Create((Get-Content -Raw $script))
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('siaocut-manifest-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    $installer = Join-Path $testRoot 'SiaoCut_0.2.0_x64-setup.exe'
    $signature = "$installer.sig"
    [IO.File]::WriteAllBytes($installer, [byte[]](1, 2, 3, 4))
    [IO.File]::WriteAllText($signature, 'trusted-tauri-signature')
    $output = Join-Path $testRoot 'latest.json'
    $result = & (Join-Path $PSScriptRoot 'new-update-manifest.ps1') `
        -InstallerPath $installer `
        -SignaturePath $signature `
        -Version '0.2.0' `
        -DownloadUrl 'https://github.com/example/siaocut/releases/download/v0.2.0/SiaoCut_0.2.0_x64-setup.exe' `
        -OutputPath $output `
        -Notes 'Release' | ConvertFrom-Json
    $manifest = Get-Content -Raw $output | ConvertFrom-Json
    $entry = $manifest.platforms.'windows-x86_64-nsis'
    if ($entry.size -ne 4) { throw 'Manifest size does not match the installer.' }
    if ($entry.sha256 -ne '9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a') {
        throw 'Manifest SHA-256 does not match the installer.'
    }
    if ($entry.signature -ne 'trusted-tauri-signature') { throw 'Manifest signature is not inline.' }
    if ($result.manifest -ne $output) { throw 'Manifest output path was not reported.' }

    $httpRejected = $false
    try {
        & (Join-Path $PSScriptRoot 'new-update-manifest.ps1') `
            -InstallerPath $installer `
            -SignaturePath $signature `
            -Version '0.2.0' `
            -DownloadUrl 'http://example.invalid/update.exe' `
            -OutputPath (Join-Path $testRoot 'insecure.json') | Out-Null
    } catch {
        $httpRejected = $true
    }
    if (-not $httpRejected) { throw 'An insecure update URL was accepted.' }

    [pscustomobject]@{
        status = 'passed'
        platform = 'windows-x86_64-nsis'
        size = [long]$entry.size
        sha256 = $entry.sha256
        insecureUrlRejected = $httpRejected
    } | ConvertTo-Json
} finally {
    $resolvedTestRoot = [IO.Path]::GetFullPath($testRoot)
    if ($resolvedTestRoot.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
