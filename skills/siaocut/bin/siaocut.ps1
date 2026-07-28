$repoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
$resolver = Join-Path $PSScriptRoot "resolve-core-path.ps1"
$core = & $resolver -Profile Auto -RepoRoot $repoRoot -Optional

if ($core) {
  & $core @args
} else {
  & cargo run --manifest-path (Join-Path $repoRoot "Cargo.toml") -- @args
}
exit $LASTEXITCODE
