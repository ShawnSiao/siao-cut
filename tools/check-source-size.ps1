[CmdletBinding()]
param([string]$BaseRef = 'origin/main')

$ErrorActionPreference = 'Stop'
$root = (git rev-parse --show-toplevel).Trim()
if (-not $root) { throw 'The current directory is not inside a Git repository.' }

$changed = @(
    git -C $root -c core.quotepath=false diff --name-only --diff-filter=AM $BaseRef --
    git -C $root -c core.quotepath=false ls-files --others --exclude-standard
) | ForEach-Object { $_.Replace('\', '/') } | Sort-Object -Unique
$sourceFiles = @($changed | Where-Object { $_ -match '\.(rs|ts|tsx)$' -and $_ -notmatch '(^|/)generated/' })
$baseFiles = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
git -C $root -c core.quotepath=false ls-tree -r --name-only $BaseRef | ForEach-Object { [void]$baseFiles.Add($_.Replace('\', '/')) }
$numstat = @{}
git -C $root -c core.quotepath=false diff --numstat $BaseRef -- | ForEach-Object {
    $parts = $_ -split "`t", 3
    if ($parts.Count -eq 3 -and $parts[0] -ne '-') {
        $numstat[$parts[2].Replace('\', '/')] = [pscustomobject]@{ Added = [int]$parts[0]; Deleted = [int]$parts[1] }
    }
}

$errors = [Collections.Generic.List[string]]::new()
$warnings = [Collections.Generic.List[string]]::new()
foreach ($path in $sourceFiles) {
    $fullPath = Join-Path $root $path
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { continue }
    $lines = @(Get-Content -LiteralPath $fullPath).Count
    $isTest = $path -match '(^|/)(tests?|__tests__)/' -or $path -match '(\.test\.|_test\.rs$)'
    $limit = if ($isTest) { 600 }
        elseif ($path -match '/providers/.*\.rs$') { 300 }
        elseif ($path.EndsWith('.tsx')) { 220 }
        elseif ($path -match '/(use-[^/]+|[^/]*gateway|[^/]*controller)\.ts$') { 250 }
        else { 450 }
    $existed = $baseFiles.Contains($path)
    if (-not $existed) {
        if ($lines -gt $limit) { $errors.Add("new source exceeds $limit lines: $path ($lines)") }
        elseif ($path.EndsWith('.rs') -and -not $isTest -and $lines -gt 300) { $warnings.Add("new Rust source exceeds the 300-line recommendation: $path ($lines)") }
        continue
    }
    $change = $numstat[$path]
    if ($change -and $lines -gt $limit -and ($change.Added - $change.Deleted) -gt 120) {
        $errors.Add("existing oversized source grew by more than 120 lines: $path (+$($change.Added - $change.Deleted), $lines total)")
    }
}

foreach ($warning in $warnings) { Write-Warning $warning }
if ($errors.Count -gt 0) {
    throw "Source size check failed:$([Environment]::NewLine)$(($errors | ForEach-Object { "- $_" }) -join [Environment]::NewLine)"
}
Write-Host "Source size check passed for $($sourceFiles.Count) changed source files."
