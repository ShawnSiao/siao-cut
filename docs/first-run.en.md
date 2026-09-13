# First run with SiaoCut

[简体中文](first-run.md) | [English](first-run.en.md)

The Windows x64 local preview is unsigned and has automatic updates disabled. Start with a short video that is authorized for processing and has a separate original copy.

## 1. Get the preview

Download the installer whose name contains `local-preview` and `SHA256SUMS` from the [preview release page](https://github.com/ShawnSiao/siao-cut/releases/tag/local-preview-0.2.0-20260913). Check the downloaded file in PowerShell:

```powershell
Get-FileHash -LiteralPath '.\SiaoCut_0.2.0_local-preview_20260913_x64-setup.exe' -Algorithm SHA256
```

Compare the result with the matching entry in `SHA256SUMS`. A matching hash establishes file integrity, not code signing. Read the release page's native acceptance limitations before installing; do not disable system protection to install the preview.

The package contains the app and Core, without FFmpeg, Whisper, or models. Missing-component status on first launch is expected and does not mean installation failed.

## 2. Prepare local components

Open runtime settings, check FFmpeg, Whisper, and model status, and prepare the required components from the sources shown by the app. Select an available transcription model. Use a backend supported by the current hardware; a graphics driver alone does not verify GPU execution. Experimental MOSS multispeaker transcription is optional for a first run.

## 3. Complete a local workflow

1. Import the short video and confirm that the preview plays.
2. Select a model, start local transcription, and wait for completion.
3. Play the beginning, middle, and end to check timing and text; correct one clear error.
4. Inspect errors in Quality and saved versions in History.
5. Export SRT or VTT to a separate file and inspect its text and timing. Export a separate MP4 if needed.
6. Close and reopen the project. Confirm that the subtitle edit persists and the original video is unchanged.

Agent assistance is optional. Local transcription, manual correction, and export do not require Codex or a paid model account once the local components are ready.

## 4. Use AI assistance when needed

Choose a configured AI service, an installed and authenticated local Codex CLI, or manual handoff. A usable LLM API can provide text assistance without Codex. With no AI account, the local manual workflow remains available.

Start AI assistance, then check the recipient, model, actual text scope, and cost notice before confirming the send. Review the returned differences before applying them. Local Codex can also call remote models. Manual handoff needs an Agent environment that can execute the local protocol; it does not provide free model access. See [AI service configuration](ai-services.md) (Chinese).

## Troubleshooting

| Symptom | Next step |
| --- | --- |
| Missing components or transcription cannot start | Check runtime status, selected model, and component paths |
| Transcription takes a long time | Check the actual stage and try a short clip; performance depends on hardware and model |
| AI assistance is unavailable | Continue the local manual workflow and check the account or CLI separately |
| Missing or changed source blocks export | Locate the matching original copy; do not bypass integrity checks |
| Recovery or upgrade fails | Preserve the failure state, exit the app, and back up the whole data directory; do not overwrite the only copy |

The default data directory is `%LOCALAPPDATA%\SiaoCut`, unless `SIAOCUT_HOME` selects another location. Original media may be outside that directory and needs its own backup. Feedback should contain only the version, reproduction steps, error code, and sanitized screenshots; do not upload private transcripts, original media, or credentials.

See the [workbench guide](workbench-user-guide.md) (Chinese) for more operations and [release status](release-updates.en.md) for acceptance boundaries. Completing this guide does not establish signing, full Windows installation and upgrade acceptance, or complete real-media and AI regression coverage.
