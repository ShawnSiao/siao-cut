// Core canonicalizes Windows paths; compare equivalent spellings before reading.
export const handoffPathCheck = String.raw`function Normalize-SiaoCutPayloadPath([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { throw "SiaoCut payload path missing" }
  $path = $path.Replace('/', '\')
  if ($path.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) {
    $path = '\\' + $path.Substring(8)
  } elseif ($path.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) {
    $path = $path.Substring(4)
  }
  if ($path -notmatch '^(?:[A-Za-z]:\\|\\\\[^\\]+\\[^\\]+\\)') { throw "SiaoCut payload path must be absolute" }
  return [IO.Path]::GetFullPath($path)
}
$expectedPath = Normalize-SiaoCutPayloadPath $payloadPath
$returnedPath = Normalize-SiaoCutPayloadPath ([string]$claim.payloadFile.path)
if (-not [string]::Equals($expectedPath, $returnedPath, [StringComparison]::OrdinalIgnoreCase)) {
  throw "SiaoCut payload path mismatch"
}`;
