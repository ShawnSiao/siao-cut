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
- Use `transcribe <projectId> --model <absolute local model path> --expected-version <currentVersionId> [--language en|zh|auto]` only after the user has selected or installed a local model. Read `currentVersionId` from the latest project response. Existing subtitles additionally require `transcript replacement-preflight <projectId>` and explicit `--confirm-replace`; never infer replacement approval. This command sends neither media nor transcript to a network service.
- Agent tasks receive text and timestamps, never the media path. Do not read media files to answer a task.
- Claim an Agent task with `--payload-output <absolute path outside the repository>`. Read the complete task payload, including its unpredictable `leaseId` and `attemptCount`, from that file; the console response intentionally contains only compact metadata. Do not invent or reuse a lease ID from another attempt.
- If the claim console response is lost or truncated, read `leaseId` from the successfully written payload file, then repeat the targeted claim with `--lease-id <that-lease-id>`. This reissues the same payload without creating a new attempt. Do not call `task fail` merely to recover the claim payload.
- Agent results are proposals. `task submit` creates a pending patch set and never changes project text. Only `task review` or `task review-all` may apply a proposal after an explicit human choice.
- Preserve `before` exactly as supplied in the claimed segment. This enables SiaoCut to show the task baseline, the Agent suggestion, and the current human text side by side.
- If the project changes while an Agent is working, still submit the result. SiaoCut marks affected items as conflicts for review instead of overwriting human edits.
- A soft cut is only a proposal until the user asks to apply it. Always report the spoken text and time range, never internal cut ids.
- `cut dismiss <projectId> <cutId>` records an explicit 「保留原片」 decision without changing the timeline or making translations stale. Use `cut restore` to undo either an applied or dismissed cut decision.
- `auto start` defaults to the `balanced` profile for backward compatibility. Select `draft` or `delivery` only when the user explicitly asks for that workflow outcome. Never add translation, AI execution, or a non-source subtitle mode to `draft`.
- An automatic workflow may stop at `needs_agent` or `needs_review`. Resolve every pending proposal through the existing review commands, then use `auto continue`; this confirmation never authorizes automatic application.
- Before export, run `audit`. A stale translation is a warning: ask whether the user wants to refresh it or export the last reviewed translation.
- Run `media prepare <projectId>` once when the user wants proxy playback, waveform evidence, or thumbnails. Reuse `ready` artifacts while their `sourceSha256` still matches the imported media.
- Final video export is a background Core job. Report its progress from `video status`; use `video cancel` only when the user asks to stop. A cancelled job must not be described as a completed export.
- Before `model install`, report the selected profile's source, size, and license from `model list`. Only install after an explicit user choice. Poll `model status`; do not use a model until `model verify` returns `verified: true`.

## Project flow

```powershell
$imported = siaocut --json import "C:\Videos\talk.mp4" --title "产品发布口播" | ConvertFrom-Json
siaocut --json transcribe $imported.projectId --model "$env:LOCALAPPDATA\SiaoCut\models\ggml-tiny.en.bin" --language en --expected-version $imported.project.history.currentVersionId
siaocut --json workflow create <projectId> --kind translate --lang en
$claimPayload = Join-Path $env:TEMP "siaocut-<taskId>-claim.json"
siaocut --json task claim <taskId> --worker external-agent --payload-output $claimPayload
$claim = Get-Content -LiteralPath $claimPayload -Raw | ConvertFrom-Json
$leaseId = $claim.leaseId
```

After the claim succeeds, verify `payloadFile.sha256`, read the complete JSON object from `$claimPayload`, then produce a response JSON file outside the repository and submit it:

```powershell
siaocut --json task submit <taskId> --worker external-agent --lease-id $leaseId --response "C:\Temp\siaocut-response.json"
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

Copy `baseVersionId` exactly from the payload file. While working, renew the lease and report coarse progress:

```powershell
siaocut --json task heartbeat <taskId> --worker external-agent --lease-id $leaseId --progress 0.5 --message "正在校对译文"
```

If processing cannot continue, use `task fail <taskId> --worker <worker> --lease-id <leaseId> --message <reason>`; do not submit partial content as complete. Failed or interrupted tasks can be returned to the queue with `task retry`. Use `task events <taskId> --after <eventId>` to read progress visible to the App.

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

## Automatic workflow profiles

All profiles require an output path and export MP4. The Core owns the route and recovery state; do not reproduce the stage sequence in an Agent script.

```powershell
# Fast draft: import, transcribe, audit, export. Source subtitles only.
siaocut --json auto start --profile draft --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --output "C:\Exports\draft.mp4" --subtitle-mode source

# Balanced review: the legacy/default path. --profile may be omitted.
siaocut --json auto start --profile balanced --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --translate en --output "C:\Exports\reviewed.mp4" --subtitle-mode bilingual

# Delivery: includes local audio analysis and always pauses for final review confirmation.
siaocut --json auto start --profile delivery --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --output "C:\Exports\delivery.mp4" --subtitle-mode source
```

- `draft`: does not create cut suggestions or Agent tasks. Its result is an unreviewed draft.
- `balanced`: runs cut suggestions and optional translation, and pauses only when a decision is pending.
- `delivery`: checks local FFmpeg capability before starting, reuses one recorded audio-analysis child task, and reaches audit only after `auto continue` confirms the review gate.

Poll with `auto status <workflowId>` and read durable events with `auto events <workflowId> --after <eventId>`. Use `auto cancel` and `auto continue` for explicit cancellation and recovery. A remote failure or child-task failure must be reported as such; do not silently switch execution targets.

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
