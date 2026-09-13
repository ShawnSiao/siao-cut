# Third-party runtime notices

SiaoCut source is Apache-2.0. Third-party dependencies retain their own licenses. SQLite is compiled into Rust Core; media runtime executables and model weights are external to the app-only installer. The installed application exposes the pinned source, size, SHA-256 and license metadata before an external component is selected.

| Component | Current local use | License / release requirement |
| --- | --- | --- |
| SQLite | Bundled into Rust Core through `rusqlite` / `libsqlite3-sys` | Public domain. Retain upstream notices in packaged dependency report. |
| whisper.cpp | CPU x64 1.9.1, Vulkan and CUDA profiles are installed externally after explicit selection | MIT. Source: `ggml-org/whisper.cpp`. The release manifest pins archive size, SHA-256 and source commit. |
| Silero VAD | 6.2.0 GGML model is installed externally when VAD is selected | MIT. Source: `snakers4/silero-vad`; converted model: `ggml-org/whisper-vad`. The release manifest pins size and SHA-256. |
| Whisper model | Tiny / Base / Small are downloaded only after explicit selection | MIT. Source: `ggerganov/whisper.cpp`, converted from OpenAI Whisper weights. Every profile shows source, size and SHA-256 before download. |
| FFmpeg | BtbN FFmpeg 8.1 LGPL shared build is installed externally when media processing is selected | LGPL-2.1-or-later. The release manifest pins the archive SHA-256 and upstream license source. |
| yt-dlp | Windows x64 2026.08.19 is installed externally before explicitly confirmed public URL imports | The combined PyInstaller executable is GPL-3.0-or-later and includes components under additional licenses; yt-dlp source is Unlicense. The release manifest pins the executable and official license-file SHA-256 values, and self-update is disabled. |
| LobeHub AI service logos | Provider marks displayed in the bundled environment settings interface | MIT. Copyright (c) 2023 LobeHub. The license text is bundled under `notices/licenses/`. |

The installer includes the repository-tracked license texts under `notices/licenses/` and the machine-readable component manifest under `notices/runtime-manifest.json`. No FFmpeg, Whisper, VAD, yt-dlp executable, or model weight is included.

Network operations depend on the selected feature:

- Models and media runtimes are downloaded from disclosed sources after an explicit selection; component update checks can contact their configured sources.
- Public URL imports contact the selected media site and download source after confirmation.
- Configured AI services can contact their endpoints for model discovery, connection tests, and approved text tasks. Local Codex may also use a remote model. These tasks contain text and structural context, not media bytes or media paths; service credentials are used for authentication and are not part of the task text.
- Experimental MOSS transcription sends a temporary WAV only to the explicitly configured loopback HTTP service on the same computer. It does not support remote MOSS endpoints.
- Eligible signed builds can check for application updates; installation requires confirmation and signature checks. The current unsigned preview has automatic updates disabled.

See [AI service configuration](docs/ai-services.md) (Chinese), [MOSS transcription](docs/multispeaker-transcription.en.md), and [release behavior](docs/release-updates.en.md) for the corresponding data and acceptance boundaries.
