param(
    [Parameter(Mandatory = $true)]
    [string]$DesktopPath,
    [Parameter(Mandatory = $true)]
    [string]$CorePath,
    [int]$StartupWaitSeconds = 6
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testHome = Join-Path $tempRoot ("SiaoCut-NoConsole-" + [guid]::NewGuid().ToString('N'))
$desktop = (Resolve-Path -LiteralPath $DesktopPath).Path
$core = (Resolve-Path -LiteralPath $CorePath).Path
$process = $null
$previousHome = $env:SIAOCUT_HOME

Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class SiaoCutWindowProbe {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder className, int maxCount);

    public static string[] ClassesFor(HashSet<uint> processIds) {
        var classes = new List<string>();
        EnumWindows((window, _) => {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processIds.Contains(processId)) {
                var name = new StringBuilder(256);
                GetClassName(window, name, name.Capacity);
                classes.Add(processId + ":" + name);
            }
            return true;
        }, IntPtr.Zero);
        return classes.ToArray();
    }
}
'@

function Get-DescendantProcessIds([int]$RootId) {
    $processes = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
    $ids = [Collections.Generic.HashSet[uint32]]::new()
    [void]$ids.Add([uint32]$RootId)
    do {
        $changed = $false
        foreach ($item in $processes) {
            if ($ids.Contains([uint32]$item.ParentProcessId) -and $ids.Add([uint32]$item.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return $ids
}

try {
    & (Join-Path $root 'tools\inspect-pe-subsystem.ps1') -DesktopPath $desktop -CorePath $core | Out-Null

    $health = & $core --json health | Out-String | ConvertFrom-Json
    if ($health.status -ne 'ok' -or $health.apiVersion -ne '0.1') {
        throw 'Core CLI JSON contract failed.'
    }

    New-Item -ItemType Directory -Force -Path $testHome | Out-Null
    $env:SIAOCUT_HOME = $testHome
    $process = Start-Process -FilePath $desktop -PassThru
    Start-Sleep -Seconds $StartupWaitSeconds
    if ($process.HasExited) {
        throw "Desktop exited during startup with code $($process.ExitCode)."
    }

    $processIds = Get-DescendantProcessIds $process.Id
    $windowClasses = [SiaoCutWindowProbe]::ClassesFor($processIds)
    $consoleWindows = @($windowClasses | Where-Object { $_ -match ':ConsoleWindowClass$' })
    if ($consoleWindows.Count -gt 0) {
        throw "Console window detected: $($consoleWindows -join ', ')"
    }

    $processNames = Get-CimInstance Win32_Process | Where-Object { $processIds.Contains([uint32]$_.ProcessId) } | Select-Object ProcessId, Name
    $shellChildren = @($processNames | Where-Object { $_.Name -in @('powershell.exe', 'pwsh.exe', 'cmd.exe') })
    if ($shellChildren.Count -gt 0) {
        throw "Unexpected shell child process detected: $($shellChildren.Name -join ', ')"
    }

    $hasDesktopWindow = @($windowClasses | Where-Object { $_ -notmatch ':ConsoleWindowClass$' }).Count -gt 0
    if (-not $hasDesktopWindow) {
        throw 'Desktop process did not expose a top-level application window.'
    }

    [pscustomobject]@{
        status = 'ok'
        desktopPid = $process.Id
        consoleWindows = 0
        shellChildren = 0
        desktopWindowDetected = $true
        coreCliJson = 'ok'
    } | ConvertTo-Json
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        Wait-Process -Id $process.Id -Timeout 5 -ErrorAction SilentlyContinue
    }
    $env:SIAOCUT_HOME = $previousHome
    $resolved = [IO.Path]::GetFullPath($testHome)
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
