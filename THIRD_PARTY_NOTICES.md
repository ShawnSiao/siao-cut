# Third-party runtime notices

SiaoCut source is Apache-2.0. Runtime dependencies remain separately licensed and are not included in the app-only installer. The installed application exposes the pinned source, size, SHA-256 and license metadata before an external component is selected.

| Component | Current local use | License / release requirement |
| --- | --- | --- |
| SQLite | Bundled into Rust Core through `rusqlite` / `libsqlite3-sys` | Public domain. Retain upstream notices in packaged dependency report. |
| whisper.cpp | CPU x64 1.9.1, Vulkan and CUDA profiles are installed externally after explicit selection | MIT. Source: `ggml-org/whisper.cpp`. The release manifest pins archive size, SHA-256 and source commit. |
| Silero VAD | 6.2.0 GGML model is installed externally when VAD is selected | MIT. Source: `snakers4/silero-vad`; converted model: `ggml-org/whisper-vad`. The release manifest pins size and SHA-256. |
| Whisper model | Tiny / Base / Small are downloaded only after explicit selection | MIT. Source: `ggerganov/whisper.cpp`, converted from OpenAI Whisper weights. Every profile shows source, size and SHA-256 before download. |
| FFmpeg | BtbN FFmpeg 8.1 LGPL shared build is installed externally when media processing is selected | LGPL-2.1-or-later. The release manifest pins the archive SHA-256 and upstream license source. |
| yt-dlp | Windows x64 2026.08.19 is installed externally before explicitly confirmed public URL imports | The combined PyInstaller executable is GPL-3.0-or-later and includes components under additional licenses; yt-dlp source is Unlicense. The release manifest pins the executable and official license-file SHA-256 values, and self-update is disabled. |
| LobeHub AI service logos | Provider marks displayed in the bundled environment settings interface | MIT. Copyright (c) 2023 LobeHub. The license text is bundled under `notices/licenses/`. |

The installer includes the repository-tracked license texts under `notices/licenses/` and the machine-readable component manifest under `notices/runtime-manifest.json`. No runtime executable or model weight is included. No media is uploaded by Rust Core. Model downloads and user-confirmed public URL imports remain the only built-in network operations.
