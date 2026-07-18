$repoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
$release = Join-Path $repoRoot "target\release\siaocut-core.exe"
$debug = Join-Path $repoRoot "target\debug\siaocut-core.exe"

if (Test-Path $release) {
  & $release @args
} elseif (Test-Path $debug) {
  & $debug @args
} else {
  & cargo run --manifest-path (Join-Path $repoRoot "Cargo.toml") -- @args
}
exit $LASTEXITCODE
