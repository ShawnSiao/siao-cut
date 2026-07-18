param(
    [Parameter(Mandatory = $true)]
    [string]$Source,
    [string]$Core,
    [string]$Ffmpeg = "ffmpeg",
    [string]$Ffprobe = "ffprobe"
)

$ErrorActionPreference = "Stop"
if (-not $Core) { $Core = Join-Path $PSScriptRoot "..\..\..\target\debug\siaocut-core.exe" }
$Core = (Resolve-Path -LiteralPath $Core).Path
$Source = (Resolve-Path -LiteralPath $Source).Path
$testRoot = Join-Path "D:\Temp" ("siaocut-duration-" + [guid]::NewGuid().ToString("N"))
$previousHome = $env:SIAOCUT_HOME
$previousIdle = $env:SIAOCUT_SERVICE_IDLE_MS

function Invoke-SiaoCut([string[]]$Arguments) {
    $raw = & $Core --json @Arguments | Out-String
    if ($LASTEXITCODE -ne 0) { throw "SiaoCut command failed: $($Arguments -join ' ') $raw" }
    $result = $raw | ConvertFrom-Json
    if ($result.status -ne "ok") { throw "SiaoCut returned $($result.status): $raw" }
    return $result
}

try {
    New-Item -ItemType Directory -Path $testRoot | Out-Null
    $env:SIAOCUT_HOME = Join-Path $testRoot "home"
    $env:SIAOCUT_SERVICE_IDLE_MS = "100"
    $results = @()
    foreach ($minutes in @(5, 15, 30)) {
        $requestedSeconds = $minutes * 60
        $fixture = Join-Path $testRoot "$minutes-min-source.mp4"
        & $Ffmpeg -y -hide_banner -loglevel error -stream_loop -1 -i $Source -t $requestedSeconds -map 0 -c copy -avoid_negative_ts make_zero $fixture
        if ($LASTEXITCODE -ne 0) { throw "Could not create the $minutes minute fixture" }
        $sourceDuration = [double](& $Ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 $fixture)

        $project = Invoke-SiaoCut @("import", $fixture, "--title", "$minutes minute duration test")
        Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "0", "--end", "1", "--text", "um") | Out-Null
        Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "1", "--end", $sourceDuration.ToString([Globalization.CultureInfo]::InvariantCulture), "--text", "Long form content") | Out-Null
        $detected = Invoke-SiaoCut @("cut", "detect", $project.projectId)
        Invoke-SiaoCut @("cut", "apply", $project.projectId, $detected.suggestions[0].id) | Out-Null

        $output = Join-Path $testRoot "$minutes-min-output.mp4"
        $stopwatch = [Diagnostics.Stopwatch]::StartNew()
        $export = Invoke-SiaoCut @("video", "export", $project.projectId, "--output", $output)
        $job = $export.job
        $deadline = (Get-Date).AddMinutes(10)
        while ($job.status -in @("queued", "running") -and (Get-Date) -lt $deadline) {
            Start-Sleep -Milliseconds 500
            $job = (Invoke-SiaoCut @("video", "status", $export.jobId)).job
        }
        $stopwatch.Stop()
        if ($job.status -ne "completed") { throw "$minutes minute export failed: $($job.errorMessage)" }
        $outputDuration = [double](& $Ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 $output)
        $expected = $sourceDuration - 1.0
        if ([Math]::Abs($outputDuration - $expected) -gt 0.1) { throw "$minutes minute output duration mismatch: expected $expected, got $outputDuration" }
        $results += [pscustomobject]@{
            minutes = $minutes
            sourceDuration = [Math]::Round($sourceDuration, 3)
            outputDuration = [Math]::Round($outputDuration, 3)
            expectedDuration = [Math]::Round($expected, 3)
            elapsedSeconds = [Math]::Round($stopwatch.Elapsed.TotalSeconds, 2)
            status = $job.status
            manifest = Test-Path -LiteralPath $job.manifestPath
        }
    }
    [pscustomobject]@{
        status = "ok"
        source = $Source
        results = $results
    } | ConvertTo-Json -Depth 5
}
finally {
    $env:SIAOCUT_HOME = $previousHome
    $env:SIAOCUT_SERVICE_IDLE_MS = $previousIdle
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
