# SiaoCut

[简体中文](README.md) | [English](README.en.md)

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D4)
![Status: Development](https://img.shields.io/badge/status-development-orange)

SiaoCut is a Windows-local-first editing workbench for AI talking-head creators. It uses the transcript and subtitles as the primary editing surface, keeping media import, transcription, subtitle review, soft cuts, and video export on the local machine.

> **Project status: in development.** This repository does not currently publish installers or a publicly trusted, code-signed Windows release. The source can be built and run, but it should not be treated as a production release.

## Workflow

1. Import local media, or import a public single-video URL after confirming that you have permission to process it.
2. Select a local Whisper model and generate a timed transcript with FFmpeg and whisper.cpp.
3. Edit subtitles and review Agent suggestions, speech evidence, and soft cuts before applying changes.
4. Export subtitles or an MP4. Video export and subtitle retiming use the same timeline mapping.

## Current capabilities

| Area | Current implementation |
| --- | --- |
| Local transcription | Normalizes audio with FFmpeg and transcribes through whisper.cpp on CPU or a compatible Vulkan GPU. The model is always selected explicitly. |
| Transcript editing | Provides positioned subtitle editing, translation review, soft cuts, undo, redo, and version restore. Source media is never overwritten. |
| Speech evidence | Flags pace, pauses, filler words, low confidence, loudness, silence, and possible clipping. An optional local model can create a speaker track for review. |
| Agent review | Agents receive only text, timestamps, and structural constraints. Results remain reviewable three-way patches and do not modify the project directly. |
| Export | Exports SRT, VTT, ASS, Markdown, and MP4, with optional burned-in subtitles and source or 9:16 canvas layouts. |
| Project integrity | The Rust Core is the only writer. SQLite stores project versions, and media SHA-256 audits block export if source files are missing or changed. |

## Design boundaries

- Windows 10 and Windows 11 are the only supported platforms today.
- Media processing stays local. Models, runtimes, and URL media are downloaded from disclosed sources only after an explicit action.
- The desktop app, CLI, and Skill modify projects through the Rust Core instead of writing SQLite directly.
- Speech analysis and Agent output are evidence or suggestions. Applying text changes or cuts requires human review.
- Real-world coverage still needs to expand across dialects, overlapping speech, complex noise, and additional hardware.

## Run from source

### Requirements

- Windows 10 or Windows 11
- Git
- Rust stable and Visual Studio 2022 C++ Build Tools
- Node.js 22 or later
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

The default data directory is `%LOCALAPPDATA%\SiaoCut`. Development and tests can override it with `SIAOCUT_HOME`. Use `SIAOCUT_FFMPEG`, `SIAOCUT_FFPROBE`, and `SIAOCUT_WHISPER_CLI` to select audited local binaries.

See [`skills/siaocut/SKILL.md`](skills/siaocut/SKILL.md) for the complete CLI workflow.

Invitation-only English creators should follow the [English Creator Source Beta guide](docs/english-creator-beta.md), including its Codex Worker, recovery, privacy, and feedback requirements.

## Development and verification

```powershell
# Rust Core and Node.js contract tests
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

- [Architecture](ARCHITECTURE.md)
- [Voice intelligence 0.3](docs/voice-intelligence-0.3.md)
- [English Creator Source Beta](docs/english-creator-beta.md)
- [Release and updates](docs/release-updates.md)
- [Repository artifact policy](docs/repository-artifact-policy.md)
- [Contributing](CONTRIBUTING.md)
- [Third-party notices](THIRD_PARTY_NOTICES.md)

Use [GitHub Issues](https://github.com/ShawnSiao/siao-cut/issues) for bugs and feature requests. Remove media content, local paths, and personal information before attaching logs, screenshots, or sample projects.

## License

SiaoCut is licensed under the [Apache License 2.0](LICENSE). Third-party components included in release builds retain their respective licenses; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
