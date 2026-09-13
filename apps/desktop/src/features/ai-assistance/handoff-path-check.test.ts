import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import { handoffPathCheck } from "./handoff-path-check";

describe.skipIf(process.platform !== "win32")("manual handoff Windows path check", () => {
  function verify(expected: string, returned: string) {
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const script = `$ErrorActionPreference = 'Stop'
$payloadPath = ${quote(expected)}
$claim = @{ payloadFile = @{ path = ${quote(returned)} } }
${handoffPathCheck}
Write-Output 'verified'`;
    return execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], { encoding: "utf8", stdio: "pipe" });
  }
  it.each([
    [String.raw`C:\Temp\claim.json`, String.raw`\\?\C:\Temp\claim.json`],
    [String.raw`\\server\share\Temp\claim.json`, String.raw`\\?\UNC\server\share\Temp\claim.json`],
    [String.raw`C:\Temp\claim.json`, String.raw`c:\temp\claim.json`],
    [String.raw`C:\Temp\claim.json`, "C:/Temp/./claim.json"],
  ])("accepts equivalent paths %s", (expected, returned) => {
    expect(verify(expected, returned)).toContain("verified");
  });
  it.each([String.raw`C:\Other\claim.json`, String.raw`C:\Temp\other.json`, "", "claim.json", String.raw`C:claim.json`, String.raw`\\?\C:\Temp\..\Other\claim.json`])("rejects mismatched or invalid path %s", (returned) => {
    expect(() => verify(String.raw`C:\Temp\claim.json`, returned)).toThrow();
  });
});
