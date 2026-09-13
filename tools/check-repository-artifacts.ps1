[CmdletBinding()]
param([switch]$Staged)

$ErrorActionPreference = 'Stop'

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw 'Git is required to check repository artifacts.'
}

$repositoryRoot = (git rev-parse --show-toplevel).Trim()
if (-not $repositoryRoot) {
    throw 'The current directory is not inside a Git repository.'
}

# Explicit UTF-8 and NUL-separated paths also work in Windows PowerShell 5.1.
# Callers pass only fixed Git options and validated object IDs, never file content.
function Read-GitText {
    param([string]$Arguments)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = 'git'
    $start.Arguments = '-C "{0}" {1}' -f $repositoryRoot, $Arguments
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.Encoding]::UTF8
    $start.StandardErrorEncoding = [Text.Encoding]::UTF8
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        [void]$process.Start()
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "Git artifact query failed: $($stderr.Result.Trim())" }
        return $stdout.Result
    }
    finally { $process.Dispose() }
}

$errors = [Collections.Generic.List[string]]::new()
$maximumFileBytes = 5MB
$localPaths = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'repository-local-paths.json')) | ConvertFrom-Json
$forbiddenDirectories = @(
    '(^|/)(node_modules|target|dist|test-results|playwright-report|coverage|output|__pycache__)(/|$)',
    '(^|/)docs/goal(/|$)',
    '(^|/)\.codex-remote-attachments(/|$)',
    '(^|/)\.playwright-cli(/|$)',
    '(^|/)\.siaocut(/|$)',
    '(^|/)\.tmp-[^/]*(/|$)'
)
$forbiddenExtensions = @(
    '.7z', '.bin', '.db', '.db-shm', '.db-wal', '.dll', '.docx', '.dmp',
    '.exe', '.gguf', '.log', '.msi', '.msix', '.onnx', '.p12', '.pfx', '.pyc', '.pyo', '.zip'
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

function Test-ArtifactPath {
    param([string]$Path, [string]$Source)
    $name = [IO.Path]::GetFileName($Path)
    if ($name -match '^\.env($|\.)' -and $name -ne '.env.example') {
        $errors.Add("local environment file ($Source): $Path")
    }
    if ($localPaths.files -contains $Path) { $errors.Add("local-only file ($Source): $Path") }
    foreach ($directory in $localPaths.directories) {
        if ($Path.StartsWith("$directory/", [StringComparison]::OrdinalIgnoreCase)) {
            $errors.Add("local-only directory ($Source): $Path")
            break
        }
    }
    foreach ($pattern in $forbiddenDirectories) {
        if ($Path -match $pattern) {
            $errors.Add("forbidden directory ($Source): $Path")
            break
        }
    }
    $extension = [IO.Path]::GetExtension($Path).ToLowerInvariant()
    if ($forbiddenExtensions -contains $extension) {
        $errors.Add("forbidden extension ($Source): $Path")
    }
}

function Test-ArtifactText {
    param([string]$Path, [string]$Source, [string]$Content)
    foreach ($entry in $sensitivePatterns.GetEnumerator()) {
        if ($Content -match $entry.Value) {
            $errors.Add("$($entry.Key) ($Source): $Path")
        }
    }
    foreach ($entry in $localPathPatterns.GetEnumerator()) {
        if ($Content -match $entry.Value) {
            $errors.Add("$($entry.Key) ($Source): $Path")
        }
    }
}

$indexEntries = (Read-GitText 'ls-files --stage -z').Split([char[]]@([char]0), [StringSplitOptions]::RemoveEmptyEntries)
foreach ($entry in $indexEntries) {
    if ($entry -notmatch '(?s)^(\d+) ([0-9a-f]+) (\d)\t(.+)$') { throw 'Invalid Git index entry.' }
    $mode, $objectId, $stage, $relativePath = $Matches[1], $Matches[2], $Matches[3], $Matches[4]
    Test-ArtifactPath $relativePath 'index'
    if ($stage -ne '0' -or $mode -notin @('100644', '100755')) {
        $errors.Add("unmerged or unsupported index entry: $relativePath")
        continue
    }
    $size = [long](Read-GitText "cat-file -s $objectId").Trim()
    if ($size -gt $maximumFileBytes) {
        $errors.Add("file exceeds 5 MiB (index): $relativePath ($size bytes)")
        continue
    }
    if ($textExtensions -contains [IO.Path]::GetExtension($relativePath).ToLowerInvariant()) {
        Test-ArtifactText $relativePath 'index' (Read-GitText "cat-file blob $objectId")
    }
}

$workingFiles = @()
if (-not $Staged) {
    $workingFiles = @((Read-GitText 'ls-files --cached --others --exclude-standard -z').Split([char[]]@([char]0), [StringSplitOptions]::RemoveEmptyEntries) | Sort-Object -Unique)
    foreach ($relativePath in $workingFiles) {
        $fullPath = Join-Path $repositoryRoot $relativePath
        # Unstaged deletions are still checked above, from the index blob.
        if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { continue }
        Test-ArtifactPath $relativePath 'working tree'
        $file = Get-Item -LiteralPath $fullPath
        if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            $errors.Add("unsupported working-tree link: $relativePath")
            continue
        }
        if ($file.Length -gt $maximumFileBytes) {
            $errors.Add("file exceeds 5 MiB (working tree): $relativePath ($($file.Length) bytes)")
            continue
        }
        if ($textExtensions -contains [IO.Path]::GetExtension($relativePath).ToLowerInvariant()) {
            Test-ArtifactText $relativePath 'working tree' ([IO.File]::ReadAllText($fullPath))
        }
    }
}

if ($errors.Count -gt 0) {
    $details = ($errors | Sort-Object -Unique | ForEach-Object { "- $_" }) -join [Environment]::NewLine
    throw "Repository artifact policy check failed:$([Environment]::NewLine)$details"
}

Write-Host "Repository artifact policy check passed (index: $($indexEntries.Count), working tree: $($workingFiles.Count))."
if (-not $Staged) {
    & (Join-Path $PSScriptRoot 'check-source-size.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Source size Git query failed.' }
}
