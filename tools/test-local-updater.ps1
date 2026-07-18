param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $PSScriptRoot "updater-contract\Cargo.toml"

Push-Location $repositoryRoot
try {
    cargo run --locked --manifest-path $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "Local updater contract test failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
