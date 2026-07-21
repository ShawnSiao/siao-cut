# SiaoCut Signed Updates and Releases

[简体中文](release-updates.md) | [English](release-updates.en.md)

## Release states

As of July 21, 2026, the GitHub repository has no tags or Releases. The source can be built, and an unsigned Windows 10 candidate has partial local acceptance evidence, but neither is a public release.

| State | Meaning | Current status |
| --- | --- | --- |
| Source beta | Built from the repository with the full development toolchain; intended only for invited testing | Available; external Creator Beta acceptance is incomplete |
| Unsigned candidate | Built locally with `NotSigned` for installation and recovery tests | A Windows 10 candidate exists; it is not uploaded publicly |
| GitHub prerelease | Formally signed and uploaded, but still awaiting real upgrade and recovery acceptance | Not created |
| Formal Release | Signing, checksums, SBOM, provenance, and Windows 10/11 acceptance all pass | Not available |

See the [Windows candidate acceptance record](windows-candidate-acceptance.en.md). Missing evidence must not be bypassed by renaming an artifact, uploading it manually, or removing the prerelease flag.

## Signing boundary

Windows updates use both a Tauri updater signature and Authenticode. A release build also records the installer SHA-256 and size in `latest.json`. The desktop app does not run an installer when any verification fails.

- The Tauri updater public key may be committed or injected into build configuration.
- The Tauri updater private key must stay outside the repository and retain an offline backup. Losing it prevents existing installations from verifying future updates.
- The Authenticode certificate must include code-signing usage and a private key under `Cert:\CurrentUser\My`.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is provided only through the process environment, not through `.env`, script arguments, or repository files.

Generate a Tauri key through the official CLI and write the private key outside the repository:

```powershell
npm --prefix apps/desktop run tauri signer generate -- -w C:\secure\siaocut-updater.key
```

## Preflight

The GitHub preflight reads repository state, Actions permissions, and Secret names. It does not read Secret values:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-release-readiness.ps1 `
  -Mode GitHub `
  -Repository '<owner>/<repo>' `
  -RequireWindows11
```

The local signing check requires an explicit certificate thumbprint and key paths outside the repository:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<inject from password manager>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-release-readiness.ps1 `
  -Mode Local `
  -CertificateThumbprint '<certificate thumbprint>' `
  -UpdaterPrivateKeyPath 'C:\secure\siaocut-updater.key' `
  -UpdaterPublicKeyPath 'C:\secure\siaocut-updater.key.pub'
```

The result is JSON. Missing requirements produce exit code `2`. Add `-AllowIncomplete` only for diagnostics so missing items remain visible without a failing exit code.

## Build release artifacts

The release command requires a stable `latest.json` endpoint and a versioned installer URL:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<inject from password manager>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-signed-release.ps1 `
  -CertificateThumbprint '<certificate thumbprint>' `
  -UpdaterPrivateKeyPath 'C:\secure\siaocut-updater.key' `
  -UpdaterPublicKeyPath 'C:\secure\siaocut-updater.key.pub' `
  -UpdateEndpoint 'https://github.com/<owner>/<repo>/releases/latest/download/latest.json' `
  -DownloadBaseUrl 'https://github.com/<owner>/<repo>/releases/download/v0.2.0' `
  -ReleaseNotes 'SiaoCut 0.2.0'
```

The script succeeds only when:

1. the NSIS installer has a `Valid` Authenticode status;
2. Tauri creates the matching `.sig`; and
3. `latest.json` contains an HTTPS URL, inline Tauri signature, file size, and SHA-256.

A tag-triggered workflow uploads the installer, matching `.sig`, and `latest.json` to a prerelease. It does not replace the stable Latest release or activate the client update path.

After real download, upgrade, data-retention, and Windows 10/11 acceptance, promote the prerelease manually:

```powershell
gh workflow run promote-windows-release.yml -f tag=v0.2.0
```

The promotion workflow downloads the three files again and checks prerelease state, Authenticode, Tauri signature, size, SHA-256, version, and download URL before changing the release to Latest.

## Client behavior

- A formally signed build checks for updates at most once every 24 hours and also provides a manual check.
- Development builds, builds without injected updater configuration, and executables without valid Authenticode do not contact the update endpoint.
- Tauri's SemVer comparison rejects equal-version updates and downgrades.
- The app shows the version, release notes, and installer size before download.
- Download and installation require explicit confirmation. Windows installation closes the app; SiaoCut does not automatically restart it.
- After download, the client verifies the Tauri signature, size, SHA-256, and Authenticode in that order.

## Local contract verification

This command uses a temporary Tauri key and a loopback update source. It does not require a production key or certificate and does not run an installer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-local-updater.ps1
```

The contract covers upgrade from 0.1.1 to 0.2.0, equal versions, downgrade, installer tampering, incorrect SHA-256, missing Tauri signature, and Authenticode that is not `Valid`. Loopback HTTP is accepted only by isolated test configuration; production manifests still require HTTPS.

Official references: [Tauri Updater](https://v2.tauri.app/plugin/updater/) and [Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/).
