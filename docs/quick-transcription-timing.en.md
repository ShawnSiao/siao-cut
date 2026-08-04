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

When the capability gate passes and VAD is used, a successful response includes:

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

For a project with linked media and an existing transcript, open More commands and select Regenerate quick subtitles. The desktop app first reads the replacement preflight:

```powershell
siaocut-core --json transcript replacement-preflight <projectId>
```

Replacement can continue only when no edits, Agent suggestions, or task baselines reference the current transcript. After confirmation, the app binds the request to the preflight version:

```powershell
siaocut-core --json transcribe <projectId> `
  --model <modelPath> `
  --language auto `
  --expected-version <currentVersionId> `
  --confirm-replace
```

Core rejects the request if the project changes after confirmation. A successful replacement creates an undoable version without modifying source media or existing exports.

## Runtime capability gate

Core does not treat an installed VAD model as sufficient proof. Before enabling VAD, it rechecks that:

- the common v2 catalog binds the CPU/Vulkan runtime to shared release assets, source, patch, and the installed file manifest; [`release/whisper-runtime-source.json`](../release/whisper-runtime-source.json) is retained only as migration evidence for the old SiaoCut identity;
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

- This safety mode does not run forced alignment.
- MOSS long-form speaker transcription keeps its separate candidate and apply workflow.
- A verified runtime addresses whisper.cpp VAD mapping to the original-media time domain. It does not replace manual review, complex-noise testing, or forced alignment.
- Manually selected, older, or unknown whisper.cpp builds can still use the no-VAD path, but cannot claim verified VAD timing.
