# Quick-transcription timing safety

[简体中文](quick-transcription-timing.md) | [English](quick-transcription-timing.en.md)

Quick transcription normalizes audio to a mono 16 kHz WAV and asks whisper.cpp for word timing with its internal VAD disabled. Segment and word timestamps therefore remain in the original-media time domain instead of advancing when silence is removed.

## Write rules

Core validates the complete result before modifying the project:

- segment and word timestamps must be finite and non-negative, with each end later than its start;
- timestamps must stay ordered and within the normalized audio duration;
- each word must remain within `0.5 seconds` of its parent segment;
- every non-empty segment must contain trusted word timing; character-proportional fallback timing is rejected;
- whitespace, special markers, and zero-duration punctuation are not stored as standalone words; punctuation is attached to a neighboring word; and
- any failure returns `transcription_timing_invalid` while preserving the current transcript, translations, edits, and version history.

A successful response includes:

```json
{
  "timingValidation": {
    "status": "verified",
    "timeDomain": "original_media",
    "mode": "whisper_no_vad",
    "vadUsed": false,
    "segmentCount": 12,
    "wordCount": 87
  }
}
```

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

## Boundaries

- This safety mode does not run forced alignment.
- MOSS long-form speaker transcription keeps its separate candidate and apply workflow.
- An installed VAD model remains part of runtime integrity checks, but quick transcription does not use it.
- Re-enabling VAD requires an explicit, verified runtime capability stating that word-level JSON timestamps use the original-media timeline.
