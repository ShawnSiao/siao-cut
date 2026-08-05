# Windows App-Only Package Acceptance Record

This document records reproducible acceptance requirements for the SiaoCut Windows app-only package. The candidate is for local release preparation only and is not a formal Release.

The current package profile is `app-only`: the installer contains only the desktop app, `siaocut-core`, frontend assets, icons, and static component metadata. FFmpeg, FFprobe, Whisper CPU/Vulkan, VAD, model weights, and `yt-dlp` are managed outside the installer by the shared `Component Store` on demand.

## Candidate

| Item | Result |
| --- | --- |
| Source commit | This change set (see Git commit) |
| File | `SiaoCut_0.2.0_x64-setup.exe` |
| Size | 6,634,877 bytes (about 6.33 MiB) |
| SHA-256 | `bdaaaf31e411d91ab9a6eeb2971f6fe885461724434e24f24643cc1991847677` |
| Build time | 2026-08-02 18:11:51 (Asia/Shanghai) |
| Authenticode | `NotSigned`, as expected for this unsigned candidate |
| Test system | Windows 10 22H2, build 19045 |

The previous 0.2.0 candidate record describes a historical package that contained runtime files; it is not evidence for the current `app-only` package. The current candidate is produced with `npm run desktop:build`, does not read formal signing material, and does not download or compile runtime components before packaging.

## Automated acceptance

| Check | Status | Evidence and boundary |
| --- | --- | --- |
| Release build and NSIS packaging | Passed | Tauri produced one NSIS installer with exit code 0 |
| No console window | Passed | A desktop window was present; console windows and shell child processes were both 0 |
| Core CLI JSON health | Passed | `status=ok`, API version `0.1` |
| Isolated install and desktop startup | Passed | A separate `SiaoCut Acceptance` product was installed to a temporary directory and started |
| Core sidecar and app-only package boundary | Passed | `siaocut-core` and static manifests are present; runtime directories, executables, and model weights are absent |
| Startup without dependencies | Passed | The desktop app starts and Core health reports `not_configured` without treating missing components as an install failure |
| Shared component boundary | Passed | The installer contains no runtime or model assets; formal execution accepts only verified common v2 components, while legacy `SIAOCUT_*` paths are migration input only |
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

## Package size and static-resource boundary

- The clean build measured 6,634,877 bytes, below the SiaoVPlay reference size of about `30 MB`.
- The acceptance script records compressed installer and installed-directory sizes; it blocks above `50 MiB` and lists the largest files.
- `notices/runtime-manifest.json` only exposes canonical catalog, version, and license metadata. It does not mean that the component is installed with the package; runtime archives are published through `ShawnSiao/siao-components`.

## Reproduction

```powershell
npm run desktop:build

$corePath = & .\skills\siaocut\bin\resolve-core-path.ps1 -Profile Release
$tauriMetadata = cargo metadata `
  --manifest-path apps/desktop/src-tauri/Cargo.toml `
  --no-deps `
  --format-version 1 | ConvertFrom-Json
$desktopPath = Join-Path $tauriMetadata.target_directory "release\siaocut-desktop.exe"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-no-console-windows.ps1 `
  -DesktopPath $desktopPath `
  -CorePath $corePath

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-installer-retention.ps1
```
