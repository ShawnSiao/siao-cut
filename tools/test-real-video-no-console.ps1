param(
    [Parameter(Mandatory = $true)]
    [string]$InstallDirectory,
    [Parameter(Mandatory = $true)]
    [string]$VideoPath,
    [Parameter(Mandatory = $true)]
    [string]$ModelPath,
    [string]$Language = 'zh'
)

$ErrorActionPreference = 'Stop'
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testHome = Join-Path $tempRoot ("SiaoCut-RealVideo-" + [guid]::NewGuid().ToString('N'))
$install = (Resolve-Path -LiteralPath $InstallDirectory).Path
$video = (Resolve-Path -LiteralPath $VideoPath).Path
$model = (Resolve-Path -LiteralPath $ModelPath).Path
$core = Join-Path $install 'siaocut-core.exe'
$consoleEvidence = [Collections.Generic.List[string]]::new()
$observedProcesses = [Collections.Generic.HashSet[string]]::new()

Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class SiaoCutRealVideoWindowProbe {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder className, int maxCount);

    public static string[] ConsoleWindowsFor(HashSet<uint> processIds) {
        var windows = new List<string>();
        EnumWindows((window, _) => {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processIds.Contains(processId)) {
                var name = new StringBuilder(256);
                GetClassName(window, name, name.Capacity);
                if (name.ToString() == "ConsoleWindowClass") windows.Add(processId.ToString());
            }
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
}
'@

function Invoke-CoreObserved([string[]]$Arguments) {
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $core
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.EnvironmentVariables['SIAOCUT_HOME'] = $testHome
    $startInfo.EnvironmentVariables['SIAOCUT_SERVICE_IDLE_MS'] = '1000'
    $startInfo.EnvironmentVariables['SIAOCUT_FFMPEG'] = Join-Path $install 'runtime\ffmpeg\ffmpeg.exe'
    $startInfo.EnvironmentVariables['SIAOCUT_FFPROBE'] = Join-Path $install 'runtime\ffmpeg\ffprobe.exe'
    $startInfo.EnvironmentVariables['SIAOCUT_WHISPER_CLI'] = Join-Path $install 'runtime\whisper\whisper-cli.exe'
    $startInfo.EnvironmentVariables['SIAOCUT_WHISPER_VAD_MODEL'] = Join-Path $install 'runtime\whisper\ggml-silero-v6.2.0.bin'
    $allArguments = @('--json') + $Arguments
    if ($null -ne $startInfo.ArgumentList) {
        foreach ($argument in $allArguments) { [void]$startInfo.ArgumentList.Add($argument) }
    } else {
        $startInfo.Arguments = ($allArguments | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join ' '
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw 'Unable to start installed Core.' }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    while (-not $process.HasExited) {
        $targets = @(Get-Process -Name 'siaocut-core', 'ffmpeg', 'ffprobe', 'whisper-cli' -ErrorAction SilentlyContinue)
        $ids = [Collections.Generic.HashSet[uint32]]::new()
        foreach ($target in $targets) {
            [void]$ids.Add([uint32]$target.Id)
            [void]$observedProcesses.Add($target.ProcessName)
        }
        foreach ($window in [SiaoCutRealVideoWindowProbe]::ConsoleWindowsFor($ids)) {
            [void]$consoleEvidence.Add($window)
        }
        Start-Sleep -Milliseconds 100
    }
    $process.WaitForExit()
    $output = $stdout.GetAwaiter().GetResult()
    $errorOutput = $stderr.GetAwaiter().GetResult()
    if ($process.ExitCode -ne 0) {
        throw "Core exited with code $($process.ExitCode): $errorOutput"
    }
    $response = $output | ConvertFrom-Json
    if ($response.status -ne 'ok') {
        throw ($response | ConvertTo-Json -Depth 6)
    }
    return $response
}

function ConvertTo-CommandLineArgument([string]$Value) {
    if ($Value -notmatch '[\s"]') { return $Value }
    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * ($slashes * 2 + 1)))
            [void]$builder.Append('"')
        } else {
            [void]$builder.Append(('\' * $slashes))
            [void]$builder.Append($character)
        }
        $slashes = 0
    }
    [void]$builder.Append(('\' * ($slashes * 2)))
    [void]$builder.Append('"')
    return $builder.ToString()
}

try {
    New-Item -ItemType Directory -Force -Path $testHome | Out-Null
    $import = Invoke-CoreObserved @('import', $video, '--title', 'SiaoCut 真实视频无控制台验收')
    $projectId = $import.projectId
    if (-not $projectId) { $projectId = $import.project.id }
    if (-not $projectId) { throw 'Import response did not contain a project id.' }

    $transcription = Invoke-CoreObserved @('transcribe', $projectId, '--model', $model, '--language', $Language)
    if ($consoleEvidence.Count -gt 0) {
        throw "Console windows appeared during media processing: $($consoleEvidence -join ', ')"
    }

    [pscustomobject]@{
        status = 'ok'
        video = $video
        projectId = $projectId
        segments = [int]$transcription.segments
        observedProcesses = @($observedProcesses | Sort-Object)
        consoleWindows = 0
        isolatedProjectRemoved = $true
    } | ConvertTo-Json -Depth 4
} finally {
    Start-Sleep -Milliseconds 1500
    $resolved = [IO.Path]::GetFullPath($testHome)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
