# MOSS Multispeaker Transcription

[简体中文](multispeaker-transcription.md) | [English](multispeaker-transcription.en.md)

MOSS multispeaker transcription is an advanced experimental capability for interviews, podcasts, meetings, courses, and other long-form material. It produces segment timing, anonymous speaker labels, and transcript text in one inference pass. SiaoCut validates the result before saving it as a recoverable project version.

SiaoCut does not install MOSS, Python, CUDA, model weights, or an inference framework. A compatible service must already be running independently on the same computer.

## Service contract

SiaoCut accepts only these endpoint forms:

- `http://127.0.0.1:<port>`
- `http://localhost:<port>`
- `http://[::1]:<port>`

The endpoint must use HTTP and must not include credentials, query parameters, fragments, or API paths such as `/v1`. The Core rejects remote hosts and HTTPS endpoints.

The service must expose OpenAI-compatible routes:

- `GET /v1/models` for health checks.
- `POST /v1/audio/transcriptions` for a temporary WAV and a `verbose_json` response.

The default model is `OpenMOSS-Team/MOSS-Transcribe-Diarize`. The model team documents compatible SGLang Omni and vLLM serving options. CUDA requirements, framework builds, and launch parameters can change, so follow the [official model card](https://huggingface.co/OpenMOSS-Team/MOSS-Transcribe-Diarize) and [official repository](https://github.com/OpenMOSS/MOSS-Transcribe-Diarize).

## Configure and check the service

After the service starts, open Runtime in the desktop app, find MOSS long-form speaker service, enter the root endpoint and model ID, then select Save and check. Multispeaker transcription remains disabled until the state is Service available.

The same check is available through the CLI:

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription configure `
  --endpoint http://127.0.0.1:8000 `
  --model OpenMOSS-Team/MOSS-Transcribe-Diarize

.\skills\siaocut\bin\siaocut.ps1 --json transcription health
```

`providerHealth.state` must be `healthy`. A top-level `status=ok` means the Core command completed; it does not mean that the external MOSS service is available.

## Start and observe a job

Open a project with linked local media, choose Long-form speakers, and start transcription. The Core then:

1. revalidates the source-media SHA-256;
2. creates a temporary 16 kHz WAV with FFmpeg;
3. sends the temporary WAV to the confirmed loopback service;
4. validates timing, order, text, and speaker labels; and
5. atomically saves the transcript, speaker track, and review items, or preserves a candidate for confirmation.

The temporary WAV is deleted when the job finishes. The raw response remains under `transcription-runs` in the SiaoCut data directory for recovery and result auditing. It does not belong in the public repository.

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription start <projectId> `
  --language en `
  --hotword SiaoCut `
  --hotword MOSS

.\skills\siaocut\bin\siaocut.ps1 --json transcription latest <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription status <jobId>
```

Prompts and hotwords are experimental inputs. The model may ignore them; they are not an enforced dictionary or content policy.

## Confirmation and conflicts

If the project does not change during transcription, the Core saves the transcript and speaker track together as one undoable version.

If the project or source changes, the result does not overwrite current content:

- Changed source hash: the job fails until the byte-identical original is relinked.
- Changed project version: the job enters `awaiting_apply` and the app shows a candidate result.
- Apply candidate: review the impact again and explicitly confirm replacing the current transcript and speaker track.
- Discard candidate: remove the prepared result without modifying the project.

MOSS results currently have no word-level timing, so word-range cuts are disabled. Segment editing, speaker review, subtitle export, and structured export remain available.

## Review and export

Review items cover rapid speaker switches, very short segments, and missing punctuation. Open errors block structured export. Open warnings require explicit confirmation.

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription review <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription resolve <itemId> --action resolved

.\skills\siaocut\bin\siaocut.ps1 --json transcription export <projectId> `
  --format json `
  --output C:\Temp\multispeaker.json `
  --include-speaker-labels
```

Speaker labels are anonymous labels relative to one media item. They are not verified identities. Merging, renaming, or reassigning speakers requires human review of the audio and context.

## Interruption and recovery

- Service unavailable: the job fails and never falls back to Whisper; restore the service, then resume explicitly.
- App restart: an active job becomes interrupted; reopen the project and resume it explicitly.
- User cancellation: incomplete output does not modify the project.
- Apply-stage failure: a prepared raw result remains recoverable after validation, without repeating model inference.
- Project edited during transcription: the candidate stays isolated until it is applied or discarded.

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription resume <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription cancel <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription discard <jobId>
```

## Boundaries

- SiaoCut connects only to a loopback service, stores no API key, and does not support a remote MOSS endpoint.
- SiaoCut does not manage the external service process, GPU driver, CUDA, Python environment, or model download.
- The model's stated language, duration, and hardware coverage is not evidence that SiaoCut has completed equivalent product acceptance.
- Complete product acceptance with a real MOSS service, long-form media, complex noise, and multilingual material is still pending.
- A completed job does not make an unreliable transcript publishable. Segment-by-segment review remains required.
