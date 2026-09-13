import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { describe, expect, it } from "vitest";
import { handoffPathCheck } from "./handoff-path-check";

describe.skipIf(process.platform !== "win32")("manual handoff Windows path check", () => {
  const execute = promisify(execFile);
  // Windows CI cold-starts PowerShell. Bound the process separately from UI tests.
  const processTimeout = 15_000;
  const testTimeout = 20_000;
  async function verify(expected: string, returned: string) {
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const script = `$ErrorActionPreference = 'Stop'
try {
$payloadPath = ${quote(expected)}
$claim = @{ payloadFile = @{ path = ${quote(returned)} } }
${handoffPathCheck}
Write-Output 'verified'
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}`;
    const { stdout } = await execute("powershell.exe", ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], { encoding: "utf8", windowsHide: true, timeout: processTimeout });
    return stdout;
  }
  it.each([
    [String.raw`C:\Temp\claim.json`, String.raw`\\?\C:\Temp\claim.json`],
    [String.raw`\\server\share\Temp\claim.json`, String.raw`\\?\UNC\server\share\Temp\claim.json`],
    [String.raw`C:\Temp\claim.json`, String.raw`c:\temp\claim.json`],
    [String.raw`C:\Temp\claim.json`, "C:/Temp/./claim.json"],
  ])("accepts equivalent paths %s", async (expected, returned) => {
    await expect(verify(expected, returned)).resolves.toContain("verified");
  }, testTimeout);
  it.each([
    [String.raw`C:\Other\claim.json`, "mismatch"],
    [String.raw`C:\Temp\other.json`, "mismatch"],
    ["", "missing"],
    ["claim.json", "must be absolute"],
    [String.raw`C:claim.json`, "must be absolute"],
    [String.raw`\\?\C:\Temp\..\Other\claim.json`, "mismatch"],
  ])("rejects mismatched or invalid path %s", async (returned, reason) => {
    await expect(verify(String.raw`C:\Temp\claim.json`, returned)).rejects.toThrow(`SiaoCut payload path ${reason}`);
  }, testTimeout);
});
