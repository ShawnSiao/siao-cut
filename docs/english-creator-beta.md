# English Creator Source Beta

This invitation-only beta is for English-language talking-head videos, YouTube explainers, courses, and video podcasts. It runs from source on Windows 10 or 11. It is not an installer or a production release.

## Who this beta is for

Join only if you can run PowerShell commands and already have:

- Git;
- Node.js 22 or later;
- Rust stable with `rustfmt` and `clippy`;
- Visual Studio 2022 Build Tools with the Desktop development with C++ workload;
- Microsoft Edge WebView2 Runtime;
- an external Agent environment that can run local SiaoCut commands for optional Agent workflows; and
- English media you own or are authorized to process.

The beta does not include an installer, code signing, SmartScreen reputation, telemetry, billing, automatic updates, publishing integrations, or live developer setup support.

## 1. Clone and start the app

Open PowerShell:

```powershell
git clone https://github.com/ShawnSiao/siao-cut.git
cd siao-cut
npm ci --prefix apps/desktop
cargo build --release
npm run desktop:dev
```

The first launch uses English unless the Windows UI language starts with `zh`. Change the interface at any time from the language switcher. Interface language is independent from the source-media and translation languages.

SiaoCut stores its local database and managed files under `%LOCALAPPDATA%\SiaoCut` by default. Do not point `SIAOCUT_HOME` at the repository.

## 2. Check the shared component Store and choose a model

Run the health check before importing media:

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json health
.\skills\siaocut\bin\siaocut.ps1 --json component-store health
.\skills\siaocut\bin\siaocut.ps1 --json component-store list-installations
```

The Base multilingual Whisper profile is the default beta recommendation because one local model can handle English and Chinese projects. The Tiny profile is faster and smaller but trades away recognition accuracy. Installation and verification are performed by the shared Store; product settings retain only the selected `ComponentKey`.

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json component-store install base
.\skills\siaocut\bin\siaocut.ps1 --json component-store operations
.\skills\siaocut\bin\siaocut.ps1 --json component-store verify base
```

Do not transcribe until the Store reports the selected model and runtime as `verified`. If common v2 is incomplete or a component is unavailable, follow the structured status in `health` and install or register it through the Store; do not substitute an unverified executable or a legacy path.

## 3. Complete the English creator workflow

1. Import one local MP4, MOV, WAV, or other supported media file. The original file is referenced, never overwritten.
2. Set Source language to **Auto** for unknown or mixed input, or **English** when the recording is known to be English.
3. Start transcription and inspect the timed transcript.
4. Correct names and recognition errors. Review filler, repetition, false-start, timing, reading-speed, and line-length suggestions. These checks never edit text or apply cuts automatically.
5. Optionally select one Agent workflow: **Clean up transcript**, **Proofread transcript**, **Improve concision**, or **Translate subtitles**.
6. Review every proposed Agent change in the three-way diff. Apply or keep each item explicitly.
7. Export SRT, VTT, ASS, or MP4. English captions use the beta limits of 8 seconds, 42 visible characters per line, 2 lines, 20 characters per second, and a 0.12-second minimum gap. Warnings do not silently rewrite captions.

For bilingual output, SiaoCut wraps and checks the source and translated tracks independently. The existing subtitle safe area is retained.

## 4. Hand off to an external Agent

An external Agent is separate from SiaoCut. It receives transcript text, timestamps, task instructions, and structural constraints. It never receives media bytes or media paths.

The app displays **Waiting for an external Agent to claim** until an Agent claims the task. Copy the full handoff instructions from the app into an Agent environment that can run SiaoCut commands on this computer. For a direct local invocation, use:

```powershell
$claimPayload = Join-Path $env:TEMP "siaocut-<taskId>-claim.json"
.\skills\siaocut\bin\siaocut.ps1 --json task claim <taskId> --worker external-agent --payload-output $claimPayload
$claim = Get-Content -LiteralPath $claimPayload -Raw | ConvertFrom-Json
```

The payload file includes `instructionLocale`, `language`, `contentLanguage`, and an unpredictable `leaseId` for this exact attempt. Follow its `instructions` and `responseSchema`, write the response JSON outside the repository, then submit it with that lease ID:

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json task submit <taskId> --worker external-agent --lease-id $claim.leaseId --response "C:\Temp\siaocut-response.json"
.\skills\siaocut\bin\siaocut.ps1 --json task diff <taskId>
```

If a workflow is interrupted, use the copyable command shown in the app or run:

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json workflow continue <workflowId>
```

Continuing a workflow can retry work or return to pending review. It does not apply Agent output.

## 5. Recovery and troubleshooting

- **Interrupted transcription or workflow:** reopen the project and use the visible Resume action. SiaoCut reuses safe persisted work and does not replace the source file.
- **Interrupted export:** select Retry. A new export job uses the current project version; partial output is not reported as complete.
- **Missing or changed source:** relink only the byte-identical original. The SHA-256 audit blocks export when the source does not match.
- **Model download stopped:** resume the same model install. The partial download is retained and the finished file must pass verification.
- **External Agent not connected:** leave the task queued, then copy the app's handoff instructions into an Agent environment that can access the local SiaoCut Core.
- **Unknown error:** use the localized summary first, then expand or copy the original technical detail. Remove local paths, media text, and personal data before reporting it.

Run the local verification commands if the app does not start:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
npm --prefix apps/desktop run test:e2e
```

## 6. Privacy and feedback rules

- Keep original media, exported media, screenshots containing content, logs, response JSON, and test notes outside the repository.
- Record only an anonymous tester ID, clip ID, source-media SHA-256, duration, accent category, audio condition, result, and GitHub issue number.
- Confirm the source SHA-256 before and after the test. The two hashes must match.
- Do not attach media, transcripts, absolute paths, account names, or personal information to a public issue.
- Agent suggestions must remain pending until the tester explicitly reviews them.

Copy the [beta evidence template](english-creator-beta-feedback-template.md) to a private folder outside the repository before recording results.

## Invitation text

> You are invited to test the SiaoCut English Creator source beta on Windows 10/11. This beta requires Git, Node.js 22, Rust, Visual Studio Build Tools, and an external Agent environment that can run local SiaoCut commands. Please use only media you are authorized to process. SiaoCut keeps media processing local and sends only transcript text, timestamps, and structural constraints to an external Agent. No installer or real-time setup support is provided. Feedback must be anonymized and must not include media, transcripts, local paths, or personal data.
