---
name: siaocut
description: >-
  Drive a local SiaoCut Windows project through the JSON CLI: create projects,
  transcribe media, run reviewable Agent workflows, submit three-way text patches,
  review reversible cuts, audit, and export subtitles. Use this whenever a local
  SiaoCut project needs transcription, polishing, translation, proofreading,
  semantic editing, or subtitle export.
---

# SiaoCut

Use this Skill when the user asks to 转写、润色字幕、翻译字幕、剪口播、去口癖、导出字幕，and the media belongs in a local SiaoCut project.

## Rules

- Use `siaocut --json` for every command. Treat `status: "error"` as a stop condition.
- Do not edit SQLite, project data, or model metadata directly. The CLI is the only writer.
- Run `health` before transcription. If `health.engines.asr` or `health.engines.ffmpeg` is `not_configured`, explain the missing local dependency; do not invent a transcript.
- Use `transcribe <projectId> --model <absolute local model path> [--language en|zh|auto]` only after the user has selected or installed a local model. This command sends neither media nor transcript to a network service.
- Agent tasks receive text and timestamps, never the media path. Do not read media files to answer a task.
- Agent results are proposals. `task submit` creates a pending patch set and never changes project text. Only `task review` or `task review-all` may apply a proposal after an explicit human choice.
- Preserve `before` exactly as supplied in the claimed segment. This enables SiaoCut to show the task baseline, the Agent suggestion, and the current human text side by side.
- If the project changes while an Agent is working, still submit the result. SiaoCut marks affected items as conflicts for review instead of overwriting human edits.
- A soft cut is only a proposal until the user asks to apply it. Always report the spoken text and time range, never internal cut ids.
- Before export, run `audit`. A stale translation is a warning: ask whether the user wants to refresh it or export the last reviewed translation.
- Run `media prepare <projectId>` once when the user wants proxy playback, waveform evidence, or thumbnails. Reuse `ready` artifacts while their `sourceSha256` still matches the imported media.
- Final video export is a background Core job. Report its progress from `video status`; use `video cancel` only when the user asks to stop. A cancelled job must not be described as a completed export.
- Before `model install`, report the selected profile's source, size, and license from `model list`. Only install after an explicit user choice. Poll `model status`; do not use a model until `model verify` returns `verified: true`.

## Project flow

```powershell
siaocut --json import "C:\Videos\talk.mp4" --title "产品发布口播"
siaocut --json transcribe <projectId> --model "$env:LOCALAPPDATA\SiaoCut\models\ggml-tiny.en.bin" --language en
siaocut --json workflow create <projectId> --kind translate --lang en
siaocut --json task claim <taskId> --worker external-agent
```

When a task is claimed, produce a response JSON file outside the repository and submit it:

```powershell
siaocut --json task submit <taskId> --worker external-agent --response "C:\Temp\siaocut-response.json"
```

For `polish`, `translate`, `proofread`, `edit`, and `cut`, use this response shape:

```json
{
  "baseVersionId": "v-xxxx",
  "patches": [
    {
      "segmentId": "s-xxxx",
      "before": "Exact claimed text",
      "after": "Reviewed text",
      "reason": "Corrected a product name from the glossary",
      "confidence": 0.96
    }
  ]
}
```

For `cut`, keep `before` exact and set `after` to an empty string. Explain why the complete segment can be removed. Do not propose a partial-word boundary.

Copy `baseVersionId` exactly from the claim payload. While working, renew the lease and report coarse progress:

```powershell
siaocut --json task heartbeat <taskId> --worker external-agent --progress 0.5 --message "正在校对译文"
```

If processing cannot continue, use `task fail`; do not submit partial content as complete. Failed or interrupted tasks can be returned to the queue with `task retry`. Use `task events <taskId> --after <eventId>` to read progress visible to the App.

After submission, inspect the pending result without changing the project:

```powershell
siaocut --json task diff <taskId>
siaocut --json workflow status <workflowId>
```

Use `task review <patchItemId> --action apply|keep` for one item. Use `task review-all <taskId> --action apply|keep` only when the user explicitly chooses the same action for every unresolved item. `keep` records the decision and preserves the current project text.

## Workflow recipes

Use one workflow for one review objective. Supported kinds are `polish`, `translate`, `proofread`, `edit`, `cut`, and `summary`.

```powershell
# Correct transcription errors and verbal clutter
siaocut --json workflow create <projectId> --kind polish

# Translate after the source transcript has been reviewed
siaocut --json workflow create <projectId> --kind translate --lang en

# Check spelling, punctuation, names, and terminology
siaocut --json workflow create <projectId> --kind proofread

# Propose semantic removals such as repetition or a failed take
siaocut --json workflow create <projectId> --kind edit

# Propose complete spoken segments as soft cuts
siaocut --json workflow create <projectId> --kind cut
```

Run `workflow continue <workflowId>` after an interruption or when the App asks the Agent to continue. It retries interrupted work, reports pending review, or confirms completion; it does not silently apply patches.

## Cut and export flow

```powershell
siaocut --json cut detect <projectId>
siaocut --json cut apply <projectId> <cutId>
siaocut --json audit <projectId>
siaocut --json transcript export <projectId> --format srt -o "C:\Exports\talk.srt"
```

Use `cut restore <projectId> <cutId>` for one proposal or `cut restore <projectId> --all` to restore the original timeline.

For proxy preview and a final MP4:

```powershell
siaocut --json media prepare <projectId>
siaocut --json media timeline <projectId>
siaocut --json video export <projectId> -o "C:\Exports\talk.mp4" --burn-subtitles
siaocut --json video status <jobId>
```

The final export uses the same kept source ranges returned by `media timeline`. Do not calculate a second timeline in the Agent. Report the manifest path after the job reaches `completed`; the manifest records source and output hashes, applied cuts, encoder, duration map, and subtitle options.

For an explicitly approved local model download:

```powershell
siaocut --json model list
siaocut --json model install base
siaocut --json model status <jobId>
siaocut --json model verify base
```

Use `model cancel <jobId>` to pause; the partial file is retained for a later explicit `model install <profile>` resume. Use `model remove <profile>` only after the user asks to free disk space.
