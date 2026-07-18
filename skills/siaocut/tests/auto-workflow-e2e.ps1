param(
    [string]$TestUrl = 'https://www.youtube.com/watch?v=HOfdboHvshg',
    [string]$Model = (Join-Path $env:LOCALAPPDATA 'SiaoCut\models\ggml-base.bin'),
    [ValidateRange(3, 10)]
    [int]$LocalRuns = 3,
    [switch]$SkipUrl,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$core = Join-Path $root 'target\debug\siaocut-core.exe'
$runtime = Join-Path $root 'apps\desktop\src-tauri\runtime'
$ffmpeg = Join-Path $runtime 'ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $runtime 'ffmpeg\ffprobe.exe'
$whisper = Join-Path $runtime 'whisper\whisper-cli.exe'
$vadModel = Join-Path $runtime 'whisper\ggml-silero-v6.2.0.bin'
$ytDlp = Join-Path $runtime 'yt-dlp\yt-dlp.exe'
foreach ($path in $core, $ffmpeg, $ffprobe, $whisper, $vadModel, $ytDlp, $Model) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required automatic-workflow dependency is missing: $path"
    }
}
$core = (Resolve-Path -LiteralPath $core).Path
$Model = (Resolve-Path -LiteralPath $Model).Path

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = [IO.Path]::GetFullPath((Join-Path $temporaryRoot ('siaocut-auto-e2e-' + [guid]::NewGuid().ToString('N'))))
if (-not $work.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Resolved test directory is outside the system temporary directory.'
}
New-Item -ItemType Directory -Force -Path $work | Out-Null

$previousEnvironment = @{
    SIAOCUT_HOME = $env:SIAOCUT_HOME
    SIAOCUT_DIRECT = $env:SIAOCUT_DIRECT
    SIAOCUT_FFMPEG = $env:SIAOCUT_FFMPEG
    SIAOCUT_FFPROBE = $env:SIAOCUT_FFPROBE
    SIAOCUT_WHISPER_CLI = $env:SIAOCUT_WHISPER_CLI
    SIAOCUT_WHISPER_VAD_MODEL = $env:SIAOCUT_WHISPER_VAD_MODEL
    SIAOCUT_YTDLP = $env:SIAOCUT_YTDLP
}
$observedStages = [Collections.Generic.List[string]]::new()
$observedStatuses = [Collections.Generic.List[string]]::new()
$agentGateObserved = $false
$reviewGateObserved = $false
$reviewBlockedBeforeResolution = $false

function Invoke-Core {
    param([string[]]$Arguments)
    $raw = & $core --json @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "SiaoCut command failed: $($Arguments -join ' ')`n$($raw | Out-String)"
    }
    $response = ($raw | Out-String) | ConvertFrom-Json
    if ($response.status -ne 'ok') {
        throw "SiaoCut returned $($response.status): $($raw | Out-String)"
    }
    return $response
}

function Invoke-CoreExpectedError {
    param([string[]]$Arguments, [string]$Code)
    $token = [guid]::NewGuid().ToString('N')
    $stdout = Join-Path $work ("expected-error-$token.out")
    $stderr = Join-Path $work ("expected-error-$token.err")
    $process = Start-Process -FilePath $core -ArgumentList (@('--json') + $Arguments) -RedirectStandardOutput $stdout -RedirectStandardError $stderr -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -eq 0) {
        throw "Expected $Code, but the command succeeded: $($Arguments -join ' ')"
    }
    $raw = Get-Content -LiteralPath $stderr -Raw -Encoding UTF8
    $response = $raw | ConvertFrom-Json
    if ([string]$response.code -ne $Code) {
        throw "Expected $Code, received $($response.code): $($raw | Out-String)"
    }
    return $response
}

function Get-Workflow {
    param([string]$WorkflowId)
    $response = Invoke-Core -Arguments @('auto', 'status', $WorkflowId)
    $script:observedStages.Add([string]$response.workflow.currentStage) | Out-Null
    $script:observedStatuses.Add([string]$response.workflow.status) | Out-Null
    return $response.workflow
}

function Wait-WorkflowState {
    param(
        [string]$WorkflowId,
        [string[]]$Wanted,
        [int]$Seconds = 180
    )
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $workflow = Get-Workflow -WorkflowId $WorkflowId
        if ($Wanted -contains [string]$workflow.status) { return $workflow }
        if ([string]$workflow.status -in @('failed', 'cancelled')) {
            throw "Automatic workflow ended as $($workflow.status): $($workflow.errorMessage)"
        }
        Start-Sleep -Milliseconds 150
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for automatic workflow state: $($Wanted -join ', ')"
}

function Resolve-AgentGate {
    param([pscustomobject]$Workflow)
    $script:agentGateObserved = $true
    if (-not $Workflow.agentTaskId) { throw 'needs_agent workflow is missing agentTaskId.' }
    $before = Invoke-Core -Arguments @('project', 'show', [string]$Workflow.projectId)
    if ($before.project.translations.PSObject.Properties.Name -contains 'es') {
        throw 'Translation existed before the Agent result was reviewed.'
    }
    $claim = Invoke-Core -Arguments @('task', 'claim', '--worker', 'auto-e2e-agent')
    if ([string]$claim.taskId -ne [string]$Workflow.agentTaskId) {
        throw 'Agent claimed a task outside the active automatic workflow.'
    }
    $patches = @($before.project.transcript.segments | ForEach-Object {
        [ordered]@{
            segmentId = [string]$_.id
            before = [string]$_.text
            after = "ES: $([string]$_.text)"
            reason = 'Deterministic acceptance translation'
            confidence = 0.99
        }
    })
    if ($patches.Count -eq 0) { throw 'Agent gate project has no transcript segments.' }
    $responsePath = Join-Path $work ("agent-$($Workflow.id).json")
    $responseJson = [ordered]@{
        baseVersionId = [string]$claim.payload.baseVersionId
        patches = $patches
    } | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($responsePath, $responseJson, [Text.UTF8Encoding]::new($false))
    Invoke-Core -Arguments @('task', 'submit', [string]$claim.taskId, '--worker', 'auto-e2e-agent', '--response', $responsePath) | Out-Null
    $staged = Invoke-Core -Arguments @('project', 'show', [string]$Workflow.projectId)
    if ($staged.project.translations.PSObject.Properties.Name -contains 'es') {
        throw 'Agent submission changed the project before human review.'
    }
}

function Resolve-ReviewGate {
    param([pscustomobject]$Workflow)
    $script:reviewGateObserved = $true
    $projectResponse = Invoke-Core -Arguments @('project', 'show', [string]$Workflow.projectId)
    $project = $projectResponse.project
    $proposed = @($project.edits | Where-Object { $_.status -eq 'proposed' })
    $pendingPatchCount = @($project.patchSets | ForEach-Object { $_.items } | Where-Object { $_.status -in @('pending', 'conflict') }).Count
    if (($proposed.Count + $pendingPatchCount) -eq 0) {
        throw 'needs_review workflow has no reviewable evidence.'
    }
    if ([Math]::Abs([double]$project.timeline.outputDuration - [double]$project.timeline.sourceDuration) -gt 0.001) {
        throw 'A proposed cut changed the timeline before human review.'
    }
    $blocked = Invoke-CoreExpectedError -Arguments @('auto', 'continue', [string]$Workflow.id) -Code 'auto_workflow_review_pending'
    if (-not $blocked.message) { throw 'Review gate rejection did not include a message.' }
    $script:reviewBlockedBeforeResolution = $true
    foreach ($edit in $proposed) {
        Invoke-Core -Arguments @('cut', 'restore', [string]$Workflow.projectId, [string]$edit.id) | Out-Null
    }
    if ($Workflow.agentTaskId -and $pendingPatchCount -gt 0) {
        Invoke-Core -Arguments @('task', 'review-all', [string]$Workflow.agentTaskId, '--action', 'apply') | Out-Null
    }
    $continued = Invoke-Core -Arguments @('auto', 'continue', [string]$Workflow.id)
    if ([string]$continued.workflow.status -notin @('queued', 'running')) {
        throw "Reviewed workflow did not resume: $($continued.workflow.status)"
    }
}

function Complete-Workflow {
    param([string]$WorkflowId, [int]$Seconds = 240)
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $workflow = Get-Workflow -WorkflowId $WorkflowId
        switch ([string]$workflow.status) {
            'needs_agent' { Resolve-AgentGate -Workflow $workflow }
            'needs_review' { Resolve-ReviewGate -Workflow $workflow }
            'completed' { return $workflow }
            'failed' { throw "Automatic workflow failed: $($workflow.errorMessage)" }
            'cancelled' { throw 'Automatic workflow was cancelled unexpectedly.' }
            'interrupted' { throw "Automatic workflow was interrupted unexpectedly: $($workflow.errorMessage)" }
        }
        Start-Sleep -Milliseconds 150
    } while ((Get-Date) -lt $deadline)
    throw "Timed out completing automatic workflow: $WorkflowId"
}

function Get-CanonicalProject {
    param([string]$ProjectId)
    $response = Invoke-Core -Arguments @('project', 'show', $ProjectId)
    $project = $response.project
    $segments = @($project.transcript.segments | ForEach-Object {
        [ordered]@{ start = [Math]::Round([double]$_.start, 3); end = [Math]::Round([double]$_.end, 3); text = [string]$_.text }
    })
    $words = @($project.transcript.words | ForEach-Object {
        [ordered]@{ start = [Math]::Round([double]$_.start, 3); end = [Math]::Round([double]$_.end, 3); text = [string]$_.text }
    })
    $transcriptionVersions = @($project.versions | Where-Object { [string]$_.reason -like 'whisper.cpp*' })
    return [ordered]@{
        transcript = ([ordered]@{ language = [string]$project.transcript.sourceLanguage; segments = $segments; words = $words } | ConvertTo-Json -Depth 8 -Compress)
        sourceDuration = [Math]::Round([double]$project.timeline.sourceDuration, 3)
        outputDuration = [Math]::Round([double]$project.timeline.outputDuration, 3)
        transcriptionVersions = [int]$transcriptionVersions.Count
        versionReasons = @($project.versions | ForEach-Object { [string]$_.reason })
        proposedCuts = @($project.edits | Where-Object { $_.status -eq 'proposed' }).Count
        project = $project
    }
}

function Wait-VideoExport {
    param([string]$JobId, [int]$Seconds = 120)
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $job = (Invoke-Core -Arguments @('video', 'status', $JobId)).job
        if ($job.status -eq 'completed') { return $job }
        if ($job.status -in @('failed', 'cancelled', 'interrupted')) {
            throw "Manual video export ended as $($job.status): $($job.errorMessage)"
        }
        Start-Sleep -Milliseconds 150
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for video export: $JobId"
}

function Assert-Video {
    param(
        [string]$Path,
        [double]$ExpectedDuration,
        [int]$ExpectedWidth = 0,
        [int]$ExpectedHeight = 0
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Video output is missing: $Path" }
    $probe = & $ffprobe -v error -select_streams v:0 -show_entries 'stream=width,height:format=duration' -of json $Path | ConvertFrom-Json
    $duration = [double]$probe.format.duration
    if ([Math]::Abs($duration - $ExpectedDuration) -gt 0.15) {
        throw "Video duration $duration does not match timeline $ExpectedDuration."
    }
    if ($ExpectedWidth -gt 0 -and ([int]$probe.streams[0].width -ne $ExpectedWidth -or [int]$probe.streams[0].height -ne $ExpectedHeight)) {
        throw "Unexpected source-canvas dimensions: $($probe.streams[0].width)x$($probe.streams[0].height)"
    }
    return [ordered]@{ duration = $duration; width = [int]$probe.streams[0].width; height = [int]$probe.streams[0].height; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant() }
}

try {
    $env:SIAOCUT_HOME = Join-Path $work 'home'
    $env:SIAOCUT_DIRECT = '1'
    $env:SIAOCUT_FFMPEG = $ffmpeg
    $env:SIAOCUT_FFPROBE = $ffprobe
    $env:SIAOCUT_WHISPER_CLI = $whisper
    $env:SIAOCUT_WHISPER_VAD_MODEL = $vadModel
    $env:SIAOCUT_YTDLP = $ytDlp

    Add-Type -AssemblyName System.Speech
    $wav = Join-Path $work 'speech.wav'
    $speaker = [System.Speech.Synthesis.SpeechSynthesizer]::new()
    try {
        $voice = @($speaker.GetInstalledVoices() | Where-Object { $_.Enabled -and $_.VoiceInfo.Culture.Name -eq 'en-US' })[0]
        if (-not $voice) { throw 'No enabled en-US Windows speech voice is installed.' }
        $speaker.SelectVoice($voice.VoiceInfo.Name)
        $speaker.Rate = -1
        $speaker.SetOutputToWaveFile($wav)
        $speaker.Speak('This is a real workflow test. We can review and export the final video.')
    } finally {
        $speaker.Dispose()
    }
    $media = Join-Path $work 'speech.mp4'
    & $ffmpeg -y -hide_banner -loglevel error -f lavfi -i 'color=c=0x203040:s=640x360:r=30' -i $wav -shortest -c:v mpeg4 -q:v 5 -pix_fmt yuv420p -c:a aac $media
    if ($LASTEXITCODE -ne 0) { throw 'Could not generate the real speech video fixture.' }
    $sourceHashBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $media).Hash.ToLowerInvariant()

    $localResults = [Collections.Generic.List[object]]::new()
    for ($run = 1; $run -le $LocalRuns; $run++) {
        $projectsBeforeRun = @((Invoke-Core -Arguments @('project', 'list')).projects).Count
        $output = Join-Path $work ("auto-local-$run.mp4")
        $arguments = @('auto', 'start', '--media', $media, '--title', "Automatic local run $run", '--model', $Model, '--language', 'en', '--output', $output, '--burn-subtitles', '--subtitle-mode', 'source')
        if ($run -eq 1) { $arguments += @('--start-delay-ms', '5000') }
        if ($run -eq 2) { $arguments += @('--translate', 'es') }
        $started = Invoke-Core -Arguments $arguments
        $duplicate = Invoke-Core -Arguments $arguments
        if ([string]$duplicate.workflowId -ne [string]$started.workflowId) {
            throw "Active automatic workflow was duplicated on run $run."
        }
        $callerExitedBeforeCompletion = [string]$started.workflow.status -ne 'completed'
        $recovered = $false
        if ($run -eq 1) {
            $running = Wait-WorkflowState -WorkflowId ([string]$started.workflowId) -Wanted @('running') -Seconds 10
            $pidValue = [int]$running.workerPid
            $process = Get-CimInstance Win32_Process -Filter "ProcessId=$pidValue"
            if (-not $process -or [IO.Path]::GetFullPath([string]$process.ExecutablePath) -ne [IO.Path]::GetFullPath($core) -or [string]$process.CommandLine -notlike "*__auto_worker*$($started.workflowId)*") {
                throw 'Refusing to stop a process that is not the expected automatic worker.'
            }
            Stop-Process -Id $pidValue -Force
            Start-Sleep -Seconds 6
            $interrupted = Get-Workflow -WorkflowId ([string]$started.workflowId)
            if ([string]$interrupted.status -ne 'interrupted') {
                throw "Stopped worker was not reconciled as interrupted: $($interrupted.status)"
            }
            $continued = Invoke-Core -Arguments @('auto', 'continue', [string]$started.workflowId)
            if ([int]$continued.workflow.attemptCount -ne 2) {
                throw 'Interrupted automatic workflow did not increment its attempt count.'
            }
            $recovered = $true
        }
        $completed = Complete-Workflow -WorkflowId ([string]$started.workflowId)
        $projectsAfterRun = @((Invoke-Core -Arguments @('project', 'list')).projects).Count
        if (($projectsAfterRun - $projectsBeforeRun) -ne 1) {
            throw "Run $run created an unexpected number of projects: $($projectsAfterRun - $projectsBeforeRun)."
        }
        $canonical = Get-CanonicalProject -ProjectId ([string]$completed.projectId)
        if ($canonical.transcriptionVersions -ne 1) { throw "Run $run repeated or missed transcription; found $($canonical.transcriptionVersions), reasons: $($canonical.versionReasons -join ', ')." }
        if ($canonical.proposedCuts -ne 0) { throw "Run $run retained unresolved cut proposals." }
        if (-not $completed.transcriptVersionId -or -not $completed.exportJobId) { throw "Run $run is missing persisted child evidence." }
        $video = Assert-Video -Path $output -ExpectedDuration $canonical.outputDuration -ExpectedWidth 640 -ExpectedHeight 360
        $events = (Invoke-Core -Arguments @('auto', 'events', [string]$started.workflowId, '--after', '0')).events
        foreach ($event in @($events)) {
            $observedStages.Add([string]$event.stage) | Out-Null
            $observedStatuses.Add([string]$event.status) | Out-Null
        }
        $localResults.Add([ordered]@{
            run = $run
            workflowId = [string]$started.workflowId
            projectId = [string]$completed.projectId
            attemptCount = [int]$completed.attemptCount
            recoveredAfterWorkerTermination = $recovered
            duplicateStartReusedWorkflow = ([string]$duplicate.workflowId -eq [string]$started.workflowId)
            projectsCreated = ($projectsAfterRun - $projectsBeforeRun)
            callerExitedBeforeCompletion = $callerExitedBeforeCompletion
            transcriptVersionId = [string]$completed.transcriptVersionId
            transcriptionVersions = $canonical.transcriptionVersions
            eventCount = @($events).Count
            transcript = $canonical.transcript
            sourceDuration = $canonical.sourceDuration
            outputDuration = $canonical.outputDuration
            output = $video
        }) | Out-Null
    }

    $manualImport = Invoke-Core -Arguments @('import', $media, '--title', 'Manual parity run')
    Invoke-Core -Arguments @('transcribe', [string]$manualImport.projectId, '--model', $Model, '--language', 'en') | Out-Null
    $manualDetected = Invoke-Core -Arguments @('cut', 'detect', [string]$manualImport.projectId)
    foreach ($edit in @($manualDetected.suggestions)) {
        Invoke-Core -Arguments @('cut', 'restore', [string]$manualImport.projectId, [string]$edit.id) | Out-Null
    }
    $manualCanonical = Get-CanonicalProject -ProjectId ([string]$manualImport.projectId)
    $manualAudit = Invoke-Core -Arguments @('audit', [string]$manualImport.projectId)
    if ($manualAudit.audit.ready -ne $true) { throw 'Manual parity project did not pass audit.' }
    $manualOutput = Join-Path $work 'manual.mp4'
    $manualStarted = Invoke-Core -Arguments @('video', 'export', [string]$manualImport.projectId, '--output', $manualOutput, '--burn-subtitles', '--subtitle-mode', 'source')
    $manualJob = Wait-VideoExport -JobId ([string]$manualStarted.jobId)
    $manualVideo = Assert-Video -Path $manualOutput -ExpectedDuration $manualCanonical.outputDuration -ExpectedWidth 640 -ExpectedHeight 360
    foreach ($result in $localResults) {
        if ([string]$result.transcript -ne [string]$manualCanonical.transcript) {
            throw "Automatic run $($result.run) transcript differs from the manual workflow."
        }
        if ([double]$result.outputDuration -ne [double]$manualCanonical.outputDuration) {
            throw "Automatic run $($result.run) timeline differs from the manual workflow."
        }
    }

    $urlResult = $null
    if (-not $SkipUrl) {
        $preview = Invoke-Core -Arguments @('source', 'inspect', $TestUrl)
        $urlOutput = Join-Path $work 'auto-url.mp4'
        $urlArguments = @('auto', 'start', '--url', $TestUrl, '--confirm-media-id', [string]$preview.source.siteMediaId, '--model', $Model, '--language', 'en', '--output', $urlOutput, '--burn-subtitles', '--subtitle-mode', 'source')
        $urlStarted = Invoke-Core -Arguments $urlArguments
        $urlDuplicate = Invoke-Core -Arguments $urlArguments
        if ([string]$urlDuplicate.workflowId -ne [string]$urlStarted.workflowId) { throw 'URL automatic workflow was duplicated.' }
        $urlCompleted = Complete-Workflow -WorkflowId ([string]$urlStarted.workflowId) -Seconds 360
        $urlCanonical = Get-CanonicalProject -ProjectId ([string]$urlCompleted.projectId)
        $urlVideo = Assert-Video -Path $urlOutput -ExpectedDuration $urlCanonical.outputDuration
        $urlEvents = (Invoke-Core -Arguments @('auto', 'events', [string]$urlStarted.workflowId, '--after', '0')).events
        foreach ($event in @($urlEvents)) {
            $observedStages.Add([string]$event.stage) | Out-Null
            $observedStatuses.Add([string]$event.status) | Out-Null
        }
        $allSourceJobs = (Invoke-Core -Arguments @('source', 'jobs')).sourceJobs
        $sourceJobs = @($allSourceJobs | Where-Object { $_.id -eq $urlCompleted.sourceImportId })
        if (@($sourceJobs).Count -ne 1) { throw 'URL workflow did not preserve exactly one source import job.' }
        if ([string]$sourceJobs[0].siteMediaId -ne [string]$preview.source.siteMediaId) { throw 'URL workflow lost the confirmed media ID.' }
        $urlResult = [ordered]@{
            attribution = '(c) copyright Blender Foundation | www.sintel.org; CC BY 3.0'
            testUrl = $TestUrl
            siteMediaId = [string]$preview.source.siteMediaId
            workflowId = [string]$urlCompleted.id
            projectId = [string]$urlCompleted.projectId
            sourceImportId = [string]$urlCompleted.sourceImportId
            transcriptVersionId = [string]$urlCompleted.transcriptVersionId
            exportJobId = [string]$urlCompleted.exportJobId
            duplicateStartReusedWorkflow = ([string]$urlDuplicate.workflowId -eq [string]$urlStarted.workflowId)
            sourceImportJobs = @($sourceJobs).Count
            sourceToolVersion = [string]$sourceJobs[0].toolVersion
            sourceToolSha256 = [string]$sourceJobs[0].toolSha256
            downloadedMediaBytes = (Get-Item -LiteralPath ([string]$sourceJobs[0].outputPath)).Length
            downloadedMediaSha256 = [string]$sourceJobs[0].outputSha256
            eventCount = @($urlEvents).Count
            output = $urlVideo
        }
    }

    $sourceHashAfter = (Get-FileHash -Algorithm SHA256 -LiteralPath $media).Hash.ToLowerInvariant()
    if ($sourceHashAfter -ne $sourceHashBefore) { throw 'Automatic or manual workflow changed the source media.' }
    if (-not $agentGateObserved -or -not $reviewGateObserved -or -not $reviewBlockedBeforeResolution) {
        throw 'The real workflow did not prove Agent and human review gates.'
    }
    $active = @((Invoke-Core -Arguments @('auto', 'list')).workflows | Where-Object { $_.status -in @('queued', 'running', 'needs_agent', 'needs_review') })
    if ($active.Count -ne 0) { throw 'Automatic workflows remained active after acceptance.' }

    [ordered]@{
        status = 'ok'
        localRuns = $localResults
        manualParity = [ordered]@{
            projectId = [string]$manualImport.projectId
            exportJobId = [string]$manualJob.id
            transcript = $manualCanonical.transcript
            sourceDuration = $manualCanonical.sourceDuration
            outputDuration = $manualCanonical.outputDuration
            output = $manualVideo
            allAutomaticTranscriptsMatch = $true
            allAutomaticTimelinesMatch = $true
        }
        urlRun = $urlResult
        gates = [ordered]@{
            agentGateObserved = $agentGateObserved
            reviewGateObserved = $reviewGateObserved
            continueBlockedBeforeResolution = $reviewBlockedBeforeResolution
            noAutomaticContentApplication = $true
        }
        recovery = [ordered]@{
            interruptedWorkerReconciled = $localResults[0].recoveredAfterWorkerTermination
            attemptCountAfterContinue = $localResults[0].attemptCount
            oneTranscriptionVersionPerRun = $true
            duplicateStartsReusedActiveWorkflow = $true
            activeWorkflowsAfterTest = $active.Count
        }
        sourceMediaSha256 = $sourceHashAfter
        observedStages = @($observedStages | Select-Object -Unique)
        observedStatuses = @($observedStatuses | Select-Object -Unique)
        temporaryArtifactsKept = [bool]$KeepArtifacts
        temporaryPath = if ($KeepArtifacts) { $work } else { $null }
    } | ConvertTo-Json -Depth 12
} finally {
    $env:SIAOCUT_HOME = $previousEnvironment.SIAOCUT_HOME
    $env:SIAOCUT_DIRECT = $previousEnvironment.SIAOCUT_DIRECT
    $env:SIAOCUT_FFMPEG = $previousEnvironment.SIAOCUT_FFMPEG
    $env:SIAOCUT_FFPROBE = $previousEnvironment.SIAOCUT_FFPROBE
    $env:SIAOCUT_WHISPER_CLI = $previousEnvironment.SIAOCUT_WHISPER_CLI
    $env:SIAOCUT_WHISPER_VAD_MODEL = $previousEnvironment.SIAOCUT_WHISPER_VAD_MODEL
    $env:SIAOCUT_YTDLP = $previousEnvironment.SIAOCUT_YTDLP
    if (-not $KeepArtifacts -and (Test-Path -LiteralPath $work)) {
        $resolvedWork = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $work).Path)
        if (-not $resolvedWork.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove a test directory outside the system temporary directory.'
        }
        Remove-Item -LiteralPath $resolvedWork -Recurse -Force
    }
}
