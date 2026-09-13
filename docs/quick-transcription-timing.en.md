# Quick-transcription timing safety

[简体中文](quick-transcription-timing.md) | [English](quick-transcription-timing.en.md)

Quick transcription normalizes audio to a mono 16 kHz WAV. Core enables whisper.cpp's internal VAD only when the active runtime has passed the pinned original-media timeline verification. Missing metadata, identity mismatches, damaged evidence, and unverified runtimes automatically use the no-VAD safe path. Both paths must pass segment and word-timing validation before any project write.

## Write rules

Core validates the complete result before modifying the project:

- segment and word timestamps must be finite and non-negative, with each end later than its start;
- timestamps must stay ordered and within the normalized audio duration;
- each word must remain within `0.5 seconds` of its parent segment;
- every non-empty segment must contain trusted word timing; character-proportional fallback timing is rejected;
- whitespace, special markers, and zero-duration punctuation are not stored as standalone words; punctuation is attached to a neighboring word; and
- any failure returns `transcription_timing_invalid` while preserving the current transcript, translations, edits, and version history.

Desktop background jobs and the CLI share runtime selection, VAD gating, and timing validation. When the gate passes and VAD is used, the CLI transcription response and the persisted background candidate include the following timing information. Starting a background job only acknowledges registration:

```json
{
  "timingValidation": {
    "status": "verified",
    "timeDomain": "original_media",
    "mode": "whisper_verified_vad",
    "vadUsed": true,
    "segmentCount": 12,
    "wordCount": 87
  }
}
```

On safe fallback, `mode` is `whisper_no_vad` and `vadUsed` is `false`. Both modes require `timeDomain` to be `original_media`.

## Regenerating an existing transcript

Existing projects are not migrated or retimed automatically. Once VAD-compressed raw word timing has been stored, it does not provide a reliable inverse map back to the source timeline.

For a project with linked media and an existing transcript, open More commands and select Regenerate quick subtitles. The app checks the current version and replacement impact. Starting is allowed only when no edits, Agent suggestions, or task baselines reference the current transcript.

Confirmation registers a background job, available from Tasks. Once computation finishes, existing subtitles or a changed project version keep the result as a candidate for review. Inspect the actual subtitles and replacement count, then confirm application. Applying rechecks the version and replacement conditions and creates an undoable version without modifying source media or existing exports.

The synchronous CLI replacement entry point remains available. First read the preflight:

```powershell
siaocut-core --json transcript replacement-preflight <projectId>
```

After confirming the replacement scope, explicitly bind the request to the preflight version:

```powershell
siaocut-core --json transcribe <projectId> `
  --model <modelPath> `
  --language auto `
  --expected-version <currentVersionId> `
  --confirm-replace
```

The synchronous CLI attempts to write the approved replacement after computation and rejects it if the project version or source media changes. It shares transcription execution rules with the desktop but does not provide the desktop candidate-review interface.

## Runtime capability gate

Core does not treat an installed VAD model as sufficient proof. Before enabling VAD, it rechecks that:

A missing selected executable or changed SHA-256 stops transcription and requires verification again; Core does not silently switch executables. No-VAD fallback applies when an available runtime lacks valid VAD evidence or the VAD model.

- the runtime uses the source commit and patch pinned in [`release/whisper-runtime-source.json`](../release/whisper-runtime-source.json);
- the actual `whisper-cli.exe` SHA-256 matches runtime metadata, the file manifest, and acceptance evidence;
- the evidence uses the pinned `speech-silence-speech` fixture and the active backend;
- the evidence passed, reports `original_media`, and contains no lexical token outside its parent segment.

Release preparation builds and verifies CPU and optional Vulkan runtimes independently. CUDA uses the same source-build and acceptance entry point, but it cannot be selected until a CUDA Toolkit build completes a real CUDA run and produces bound evidence. Without that evidence, transcription remains on the no-VAD safe path.

The `engines.vad` field in `health` distinguishes:

- `verified`: the VAD model exists and the active runtime passed timeline verification;
- `safe_fallback`: the model exists, but the active runtime did not pass the capability gate; and
- `not_configured`: no usable VAD model is present.

The desktop UI likewise displays either “VAD timeline verified” or “No-VAD safe fallback”; model presence is never reported as runtime verification.

## Boundaries

- The shared desktop/CLI execution fix is currently [Unreleased](../CHANGELOG.md#unreleased) and is not included in the existing preview installer.
- This safety mode does not run forced alignment.
- MOSS long-form speaker transcription keeps its separate candidate and apply workflow.
- A verified runtime addresses whisper.cpp VAD mapping to the original-media time domain. It does not replace manual review, complex-noise testing, or forced alignment.
- Manually selected, older, or unknown whisper.cpp builds can still use the no-VAD path, but cannot claim verified VAD timing.

## Regression verification

To reproduce the previous mismatch, configure a verified runtime and VAD model, then transcribe through the CLI and a desktop background job. The old background path omitted VAD and always recorded `vadUsed: false`. The shared executor must use the same gate and retain the actual mode and source timing.

With local dependencies prepared, run:

```powershell
node tools/test-whisper-background.mjs `
  --core <CoreExecutable> `
  --whisper <VerifiedWhisperExecutable> `
  --model <LocalWhisperModel> `
  --vad-model <LocalVadModel> `
  --sample <EnglishSpeechWav> `
  --backend cpu
```

The script downloads nothing and uses isolated projects in an ignored `.tmp-whisper-background-*` directory, retaining its fixture and report there. It checks CLI/background parity with and without VAD, source timing after silence, replacement review, and rejection of a changed runtime hash. Test Vulkan or CUDA separately with the corresponding verified runtime and `--backend vulkan` or `--backend cuda`; CPU success does not validate those backends.
