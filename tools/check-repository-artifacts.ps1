[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw 'Git is required to check repository artifacts.'
}

$repositoryRoot = (git rev-parse --show-toplevel).Trim()
if (-not $repositoryRoot) {
    throw 'The current directory is not inside a Git repository.'
}

$deletedFiles = @(
    git -C $repositoryRoot ls-files --deleted |
        ForEach-Object { $_.Replace('\', '/') }
)
$trackedFiles = @(
    git -C $repositoryRoot ls-files --cached --others --exclude-standard |
        Where-Object { $deletedFiles -notcontains $_.Replace('\', '/') } |
        Sort-Object -Unique
)
if ($LASTEXITCODE -ne 0) {
    throw 'Unable to list repository files.'
}

$errors = [Collections.Generic.List[string]]::new()
$maximumFileBytes = 5MB
$forbiddenDirectories = @(
    '(^|/)(node_modules|target|dist|test-results|playwright-report|coverage|output)(/|$)',
    '(^|/)docs/goal(/|$)',
    '(^|/)\.codex-remote-attachments(/|$)',
    '(^|/)\.playwright-cli(/|$)',
    '(^|/)\.siaocut(/|$)',
    '(^|/)\.tmp-[^/]*(/|$)'
)
$forbiddenExtensions = @(
    '.7z', '.bin', '.db', '.db-shm', '.db-wal', '.dll', '.docx', '.dmp',
    '.exe', '.gguf', '.log', '.msi', '.msix', '.onnx', '.p12', '.pfx', '.zip'
)
$textExtensions = @(
    '', '.css', '.csv', '.html', '.js', '.json', '.jsx', '.md', '.mjs',
    '.ps1', '.rs', '.svg', '.toml', '.ts', '.tsx', '.txt', '.yaml', '.yml'
)
$sensitivePatterns = [ordered]@{
    'private key marker' = '-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----'
    'GitHub token'       = '(?<![A-Za-z0-9_])(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}'
    'AWS access key'     = '(?<![A-Z0-9])AKIA[0-9A-Z]{16}(?![A-Z0-9])'
    'Slack token'        = '(?<![A-Za-z0-9-])xox[baprs]-[A-Za-z0-9-]{10,}'
}
$localPathPatterns = [ordered]@{
    'Windows user or workspace path' = '(?i)(?<![A-Za-z0-9_])[A-Z]:\\(?:Users|Documents and Settings|githubProjects|projects|workspace)\\'
    'Unix home path'                 = '(?<![A-Za-z0-9_])/(?:home|Users)/[^/\s]+/'
}

foreach ($relativePath in $trackedFiles) {
    $normalizedPath = $relativePath.Replace('\', '/')
    $fullPath = Join-Path $repositoryRoot $relativePath

    foreach ($pattern in $forbiddenDirectories) {
        if ($normalizedPath -match $pattern) {
            $errors.Add("forbidden directory: $normalizedPath")
            break
        }
    }

    $extension = [IO.Path]::GetExtension($relativePath).ToLowerInvariant()
    if ($forbiddenExtensions -contains $extension) {
        $errors.Add("forbidden extension: $normalizedPath")
    }

    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        $errors.Add("repository file is missing from the working tree: $normalizedPath")
        continue
    }

    $file = Get-Item -LiteralPath $fullPath
    if ($file.Length -gt $maximumFileBytes) {
        $errors.Add("file exceeds 5 MiB: $normalizedPath ($($file.Length) bytes)")
    }

    if ($textExtensions -notcontains $extension) {
        continue
    }

    $content = [IO.File]::ReadAllText($fullPath)
    foreach ($entry in $sensitivePatterns.GetEnumerator()) {
        if ($content -match $entry.Value) {
            $errors.Add("$($entry.Key): $normalizedPath")
        }
    }
    foreach ($entry in $localPathPatterns.GetEnumerator()) {
        if ($content -match $entry.Value) {
            $errors.Add("$($entry.Key): $normalizedPath")
        }
    }
}

if ($errors.Count -gt 0) {
    $details = ($errors | Sort-Object -Unique | ForEach-Object { "- $_" }) -join [Environment]::NewLine
    throw "Repository artifact policy check failed:$([Environment]::NewLine)$details"
}

Write-Host "Repository artifact policy check passed for $($trackedFiles.Count) files."
