# Windows Candidate Acceptance Record

This document records reproducible results for the unsigned 0.2.0 candidate. The candidate is for local release preparation only and is not a formal Release.

## Candidate

| Item | Result |
| --- | --- |
| Source commit | `f24cfca6121b1df10e1fc58ccfee50ab0db29c30` |
| File | `SiaoCut_0.2.0_x64-setup.exe` |
| Size | 77,116,423 bytes |
| SHA-256 | `7c2a0bd3820a248215294a9f2baa4d4c9bb8ac236613b12c9b12c4db9aa0488b` |
| Build time | 2026-07-21 12:00:45 UTC |
| Authenticode | `NotSigned`, as expected for this unsigned candidate |
| Test system | Windows 10 22H2, build 19045 |

The candidate was produced with `npm run desktop:build`. It includes pinned FFmpeg, whisper.cpp CPU/Vulkan runtimes, a Silero VAD model, and yt-dlp. No formal signing material was read.

## Automated acceptance

| Check | Status | Evidence and boundary |
| --- | --- | --- |
| Release build and NSIS packaging | Passed | Tauri produced one NSIS installer with exit code 0 |
| No console window | Passed | A desktop window was present; console windows and shell child processes were both 0 |
| Core CLI JSON health | Passed | `status=ok`, API version `0.1` |
| Isolated install and desktop startup | Passed | A separate `SiaoCut Acceptance` product was installed to a temporary directory and started |
| Core sidecar and runtime integrity | Passed | CPU, Vulkan, VAD, yt-dlp, manifests, and licenses were present; pinned file hashes matched |
| URL source inspection | Passed | The installed Core inspected an authorized public URL without creating a project before confirmation |
| Over-install contract | Passed | The same source was packaged as 0.1.1 and 0.2.0 to test NSIS replacement behavior |
| Data after over-install | Passed | The isolated retention probe remained under `%LOCALAPPDATA%\SiaoCut\retention-probes` |
| Data after uninstall | Passed | The isolated retention probe remained after uninstalling the test product |
| Acceptance cleanup | Passed | No temporary install directories, configs, processes, or uninstall entries remained |

The over-install evidence is `same-source-installer-contract` with `historicalBinaryUpgrade=false`. It proves installer replacement and retention behavior, not compatibility from a previously released binary. `tools/test-installer-retention.ps1` accepts a historical `SiaoCut Acceptance` installer through `-FromInstallerPath` when one is available.

## Remaining acceptance

| Check | Status | Requirement |
| --- | --- | --- |
| Historical binary upgrade | Blocked | Requires a historical installer with the same acceptance product identifier; relabeling the current source is not sufficient |
| Formal-product installer replacement | Not run | The current machine may contain a daily installation; run this in an isolated Windows account or virtual machine |
| Windows 11 install, upgrade, and uninstall | Blocked | Requires an independent Windows 11 build 22000 or newer environment |
| Job recovery after sleep and wake | Not run | Requires a dedicated machine so the active automation session is not interrupted |
| Formal Authenticode and Tauri updater signing | Not applicable | Formal signing is outside this acceptance round |

Until these gaps are closed, 0.2.0 is a "Windows 10 unsigned candidate," not a formal release with complete Windows 10/11 upgrade acceptance.

## Reproduction

```powershell
npm run desktop:build

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-no-console-windows.ps1 `
  -DesktopPath apps/desktop/src-tauri/target/release/siaocut-desktop.exe `
  -CorePath target/release/siaocut-core.exe

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-installer-retention.ps1
```
