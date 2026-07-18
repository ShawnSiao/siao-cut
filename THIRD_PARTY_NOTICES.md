# Third-party runtime notices

SiaoCut source is Apache-2.0. Runtime dependencies remain separately licensed and must be shown by a release installer before installation.

| Component | Current local use | License / release requirement |
| --- | --- | --- |
| SQLite | Bundled into Rust Core through `rusqlite` / `libsqlite3-sys` | Public domain. Retain upstream notices in packaged dependency report. |
| whisper.cpp | CPU x64 1.9.1 and a Vulkan build from pinned commit `080bbbe` are bundled; CUDA 11.8 remains an optional, unbundled profile | MIT. Source: `ggml-org/whisper.cpp`. The release manifest pins archive size, SHA-256, and Vulkan source commit. |
| Silero VAD | 6.2.0 GGML model is bundled to reject non-speech before transcription | MIT. Source: `snakers4/silero-vad`; converted model: `ggml-org/whisper-vad`. The release manifest pins size and SHA-256. |
| Whisper model | Tiny / Base / Small are downloaded only after explicit selection | MIT. Source: `ggerganov/whisper.cpp`, converted from OpenAI Whisper weights. Every profile shows source, size and SHA-256 before download. |
| FFmpeg | BtbN FFmpeg 8.1 LGPL shared build is bundled | LGPL-2.1-or-later. The release manifest pins the archive SHA-256; the installer contains the upstream `LICENSE.txt`. |
| yt-dlp | Windows x64 2026.06.09 is bundled only for explicitly confirmed public URL imports | The combined PyInstaller executable is GPL-3.0-or-later and includes components under additional licenses; yt-dlp source is Unlicense. The release manifest pins the executable and official license-file SHA-256 values, and self-update is disabled. |

The installer includes full license texts under `notices/licenses/` and the machine-readable source manifest under `notices/runtime-manifest.json`. No media is uploaded by Rust Core. Model downloads and user-confirmed public URL imports are the only built-in network operations.
