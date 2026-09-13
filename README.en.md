# SiaoCut

[简体中文](README.md) | [English](README.en.md)

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D4)
![Status: Development](https://img.shields.io/badge/status-development-orange)

SiaoCut is a Windows-local-first editing workbench for AI talking-head creators. It uses the transcript and subtitles as the primary editing surface, keeping media import, transcription, subtitle review, soft cuts, and video export on the local machine.

> **Project status: in development.** An [unsigned local preview installer](https://github.com/ShawnSiao/siao-cut/releases/tag/local-preview-0.2.0-20260913-r2) is available for testing and feedback, with automatic updates disabled. Release acceptance is incomplete; this is not a stable release.

See the [first-run guide](docs/first-run.en.md) for download verification, component setup, transcription, and subtitle export. The local workflow does not require Codex.

## Workflow

1. Import local media, or import a public single-video URL after confirming that you have permission to process it.
2. Select a local Whisper model for quick transcription, or explicitly connect a local MOSS service for long-form segments and anonymous speaker labels.
3. Edit subtitles and review Agent suggestions, speech evidence, and soft cuts before applying changes.
4. Export subtitles, MP4, or MKV. Video export and subtitle retiming use the same timeline mapping.

## Current capabilities

These descriptions refer to the current source. See the [changelog](CHANGELOG.md#unreleased) for unreleased fixes; the existing preview installer does not automatically include them.

| Area | Current implementation |
| --- | --- |
| Local transcription | Normalizes audio with an externally configured FFmpeg and transcribes through same-source whisper.cpp builds on CPU or a compatible Vulkan GPU. VAD is enabled only for runtimes that passed original-media timeline verification; unverified VAD capability uses the no-VAD path. A missing or hash-mismatched selected executable stops transcription. The model is always selected explicitly. |
| Multispeaker long-form (experimental) | Explicitly connects to a loopback MOSS service for segments, anonymous speaker labels, and review items. SiaoCut does not install the service, CUDA, Python, or model weights. |
| Transcript editing | Provides positioned subtitle editing, translation review, soft cuts, undo, redo, and version restore. Source media is never overwritten. |
| Speech evidence | Flags pace, pauses, filler words, low confidence, loudness, silence, and possible clipping. An optional local model can create a speaker track for review. |
| AI assistance and review | Uses a configured LLM API, local Codex, or manual handoff. Confirm the recipient, model, and text scope before sending. Results remain reviewable three-way patches and do not modify the project directly. |
| One-click workflows | Provides fixed Draft, Balanced, and Delivery routes with recoverable background stages; suggestions and translations still require human decisions. |
| Export | Exports SRT, VTT, ASS, Markdown, MP4, and MKV. Subtitles can be burned in, embedded as a text track, or written beside the video as UTF-8 SRT/VTT; video can use the source ratio or a `9:16` canvas. |
| Project integrity | The Rust Core is the only writer. SQLite stores project versions, and media SHA-256 audits block export if source files are missing or changed. |

## Design boundaries

- Windows 10 and Windows 11 are the only supported platforms today.
- Media processing stays local. Models, runtimes, and URL media are downloaded from disclosed sources only after an explicit action.
- AI assistance can send transcript text and task context to the confirmed service and may incur charges; local Codex can also use a remote model. MOSS transfers temporary audio only to the configured loopback service on the same computer.
- The installer contains only the desktop app, Rust Core, and component metadata. FFmpeg, Whisper, VAD, models, and `yt-dlp` are external components; the app starts and reports them as not configured when they are absent.
- The desktop app, CLI, and Skill modify projects through the Rust Core instead of writing SQLite directly.
- Speech analysis and Agent output are evidence or suggestions. Applying text changes or cuts requires human review.
- Real-world coverage still needs to expand across dialects, overlapping speech, complex noise, and additional hardware.
- CPU and Vulkan have independent VAD timeline acceptance entry points. CUDA cannot be selected until a local source build completes a real backend acceptance run; driver presence alone is not verification.
- MOSS accepts only a loopback service, not a remote endpoint or API key. An unavailable service never triggers a silent Whisper fallback.

## Run from source

### Requirements

- Windows 10 or Windows 11
- Git
- Rust stable and Visual Studio 2022 C++ Build Tools
- Node.js 22.13+ (22.x) or 24+; CI uses Node.js 24
- Microsoft Edge WebView2 Runtime

### Start the desktop app

```powershell
git clone https://github.com/ShawnSiao/siao-cut.git
cd siao-cut
npm ci --prefix apps/desktop
cargo build --release
npm run desktop:dev
```

Development mode starts the local UI. Before transcription or export, check the FFmpeg, whisper.cpp, and model configuration:

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json health
```

The default data directory is `%LOCALAPPDATA%\SiaoCut`. Development and tests can override it with `SIAOCUT_HOME`. Use `SIAOCUT_FFMPEG`, `SIAOCUT_FFPROBE`, `SIAOCUT_WHISPER_CLI`, `SIAOCUT_WHISPER_VAD_MODEL`, and `SIAOCUT_YTDLP` to select audited local components. Component metadata and verification details are included in `notices/runtime-manifest.json`.

See [`skills/siaocut/SKILL.md`](skills/siaocut/SKILL.md) for the complete CLI workflow and [`docs/auto-workflow-profiles.md`](docs/auto-workflow-profiles.md) for the profile contract.

Invitation-only English creators should follow the [English Creator Source Beta guide](docs/english-creator-beta.md), including its external Agent, recovery, privacy, and feedback requirements.

## Development and verification

```powershell
# Rust Core tests
npm test

# Desktop build, component tests, and browser end-to-end tests
npm --prefix apps/desktop run build
npm run test:ui
npm run test:e2e

# Repository artifact policy
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the complete environment, branch, commit, and pull request requirements.

## Repository layout

```text
src/                  Rust Core, SQLite, CLI, and local media adapters
apps/desktop/         Tauri 2 and React desktop application
skills/siaocut/       Agent Skill, PowerShell entry point, and end-to-end tests
docs/                 Focused documentation and repository policies
release/              Pinned runtime sources, hashes, and third-party licenses
tools/                Build, release, and repository-checking tools
```

## Documentation

- [Workbench user guide (Chinese)](docs/workbench-user-guide.md)
- [AI service configuration and data boundaries (Chinese)](docs/ai-services.md)
- [One-click workflow profiles (Chinese)](docs/auto-workflow-profiles.md)
- [Voice intelligence (Chinese)](docs/voice-intelligence.md)
- [Quick-transcription timing safety](docs/quick-transcription-timing.en.md)
- [MOSS multispeaker transcription](docs/multispeaker-transcription.en.md)
- [English Creator Source Beta](docs/english-creator-beta.md)
- [Release and updates](docs/release-updates.en.md)
- [Windows candidate acceptance record](docs/windows-candidate-acceptance.en.md)
- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)
- [Support](SUPPORT.md)
- [Repository artifact policy](docs/repository-artifact-policy.md)
- [Contributing](CONTRIBUTING.md)
- [Third-party notices](THIRD_PARTY_NOTICES.md)

Use [GitHub Issues](https://github.com/ShawnSiao/siao-cut/issues) for bugs and feature requests. Remove media content, local paths, and personal information before attaching logs, screenshots, or sample projects.

## License

SiaoCut is licensed under the [Apache License 2.0](LICENSE). Third-party components included in release builds retain their respective licenses; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
