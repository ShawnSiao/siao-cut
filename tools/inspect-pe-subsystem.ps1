param(
    [Parameter(Mandatory = $true)]
    [string]$DesktopPath,
    [Parameter(Mandatory = $true)]
    [string]$CorePath
)

$ErrorActionPreference = 'Stop'

function Get-PeSubsystem([string]$Path) {
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $bytes = [IO.File]::ReadAllBytes($resolved)
    if ($bytes.Length -lt 256 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "Not a valid PE file: $resolved"
    }

    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 96 -gt $bytes.Length) {
        throw "Invalid PE header offset: $resolved"
    }
    if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45) {
        throw "PE signature is missing: $resolved"
    }

    $optionalHeader = $peOffset + 24
    $magic = [BitConverter]::ToUInt16($bytes, $optionalHeader)
    if ($magic -notin @(0x10b, 0x20b)) {
        throw "Unsupported PE optional header: $resolved"
    }
    $subsystem = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
    [pscustomobject]@{
        path = $resolved
        subsystem = $subsystem
        subsystemName = switch ($subsystem) {
            2 { 'Windows GUI' }
            3 { 'Windows CUI' }
            default { "Other ($subsystem)" }
        }
    }
}

$desktop = Get-PeSubsystem $DesktopPath
$core = Get-PeSubsystem $CorePath
if ($desktop.subsystem -ne 2) {
    throw "Desktop executable must use Windows GUI, found $($desktop.subsystemName)."
}
if ($core.subsystem -ne 3) {
    throw "Core executable must use Windows CUI, found $($core.subsystemName)."
}

[pscustomobject]@{
    status = 'ok'
    desktop = $desktop
    core = $core
} | ConvertTo-Json -Depth 4
