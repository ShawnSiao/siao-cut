param(
    [string]$Core,
    [string]$Model = (Join-Path $env:LOCALAPPDATA 'SiaoCut\models\ggml-base.bin'),
    [switch]$InstallSpeakerPackage,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$root = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
if (-not $Core) { $Core = Join-Path $root 'target\debug\siaocut-core.exe' }
$runtime = Join-Path $root 'apps\desktop\src-tauri\runtime'
$ffmpeg = Join-Path $runtime 'ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $runtime 'ffmpeg\ffprobe.exe'
$whisper = Join-Path $runtime 'whisper\whisper-cli.exe'
$vadModel = Join-Path $runtime 'whisper\ggml-silero-v6.2.0.bin'

foreach ($path in $Core, $Model, $ffmpeg, $ffprobe, $whisper, $vadModel) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required voice-intelligence dependency is missing: $path"
    }
}
$Core = (Resolve-Path -LiteralPath $Core).Path
$Model = (Resolve-Path -LiteralPath $Model).Path
$ffmpeg = (Resolve-Path -LiteralPath $ffmpeg).Path
$ffprobe = (Resolve-Path -LiteralPath $ffprobe).Path
$whisper = (Resolve-Path -LiteralPath $whisper).Path
$vadModel = (Resolve-Path -LiteralPath $vadModel).Path

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = [IO.Path]::GetFullPath((Join-Path $temporaryRoot ('siaocut-voice-intelligence-' + [guid]::NewGuid().ToString('N'))))
if (-not $work.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Resolved benchmark directory is outside the system temporary directory.'
}
New-Item -ItemType Directory -Force -Path $work | Out-Null

$previousEnvironment = @{
    SIAOCUT_HOME = $env:SIAOCUT_HOME
    SIAOCUT_DIRECT = $env:SIAOCUT_DIRECT
    SIAOCUT_FFMPEG = $env:SIAOCUT_FFMPEG
    SIAOCUT_FFPROBE = $env:SIAOCUT_FFPROBE
    SIAOCUT_WHISPER_CLI = $env:SIAOCUT_WHISPER_CLI
    SIAOCUT_WHISPER_VAD_MODEL = $env:SIAOCUT_WHISPER_VAD_MODEL
}

function Invoke-Core {
    param([string[]]$Arguments)
    $raw = & $Core --json @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "SiaoCut command failed: $($Arguments -join ' ')`n$($raw | Out-String)"
    }
    $response = ($raw | Out-String) | ConvertFrom-Json
    if ([string]$response.status -ne 'ok') {
        throw "SiaoCut returned $($response.status): $($raw | Out-String)"
    }
    return $response
}

function Invoke-CoreExpectedError {
    param([string[]]$Arguments, [string]$Code)
    $token = [guid]::NewGuid().ToString('N')
    $stdout = Join-Path $work "expected-$token.out"
    $stderr = Join-Path $work "expected-$token.err"
    $process = Start-Process -FilePath $Core -ArgumentList (@('--json') + $Arguments) -RedirectStandardOutput $stdout -RedirectStandardError $stderr -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -eq 0) {
        throw "Expected $Code, but the command succeeded: $($Arguments -join ' ')"
    }
    $raw = Get-Content -LiteralPath $stderr -Raw -Encoding UTF8
    $response = $raw | ConvertFrom-Json
    if ([string]$response.code -ne $Code) {
        throw "Expected $Code, received $($response.code): $raw"
    }
    return $response
}

function Wait-AudioJob {
    param([string]$JobId, [int]$Seconds = 90)
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $job = (Invoke-Core -Arguments @('speech', 'audio-status', $JobId)).audioAnalysisJob
        if ([string]$job.status -eq 'completed') { return $job }
        if ([string]$job.status -in @('failed', 'cancelled', 'interrupted')) {
            throw "Audio analysis ended as $($job.status): $($job.errorMessage)"
        }
        Start-Sleep -Milliseconds 150
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for audio analysis: $JobId"
}

function Wait-SpeakerJob {
    param([string]$JobId, [int]$Seconds = 180)
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $job = (Invoke-Core -Arguments @('speaker', 'job-status', $JobId)).speakerJob
        if ([string]$job.status -eq 'completed') { return $job }
        if ([string]$job.status -in @('failed', 'cancelled', 'interrupted')) {
            throw "Speaker job ended as $($job.status): $($job.errorMessage)"
        }
        Start-Sleep -Milliseconds 200
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for speaker job: $JobId"
}

function Invoke-Ffmpeg {
    param([string[]]$Arguments, [string]$Failure)
    & $ffmpeg -y -hide_banner -loglevel error @Arguments
    if ($LASTEXITCODE -ne 0) { throw $Failure }
}

function ConvertFrom-Utf8Base64 {
    param([string]$Value)
    return [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Value))
}

function New-TtsWave {
    param(
        $Synthesizer,
        [string]$Voice,
        [string]$Text,
        [string]$Path,
        [int]$Rate = -1
    )
    $raw = "$Path.raw.wav"
    $Synthesizer.SelectVoice($Voice)
    $Synthesizer.Rate = $Rate
    $Synthesizer.SetOutputToWaveFile($raw)
    try { $Synthesizer.Speak($Text) } finally { $Synthesizer.SetOutputToNull() }
    Invoke-Ffmpeg -Arguments @('-i', $raw, '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $Path) -Failure "Could not normalize TTS fixture: $Path"
    Remove-Item -LiteralPath $raw -Force
}

function New-PitchShiftedTtsWave {
    param(
        $Synthesizer,
        [string]$Voice,
        [string]$Text,
        [string]$Path
    )
    $base = "$Path.base.wav"
    try {
        New-TtsWave -Synthesizer $Synthesizer -Voice $Voice -Text $Text -Path $base -Rate -2
        Invoke-Ffmpeg -Arguments @('-i', $base, '-af', 'asetrate=13120,aresample=16000,atempo=1.219512', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $Path) -Failure "Could not create the pitch-shifted voice fixture: $Path"
    } finally {
        if (Test-Path -LiteralPath $base) { Remove-Item -LiteralPath $base -Force }
    }
}

function New-ConcatenatedWave {
    param([string[]]$Inputs, [string]$Path)
    $list = "$Path.concat.txt"
    $lines = @($Inputs | ForEach-Object { "file '$($_.Replace("'", "''"))'" })
    [IO.File]::WriteAllLines($list, $lines, [Text.UTF8Encoding]::new($false))
    Invoke-Ffmpeg -Arguments @('-f', 'concat', '-safe', '0', '-i', $list, '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $Path) -Failure "Could not concatenate fixture: $Path"
    Remove-Item -LiteralPath $list -Force
}

function Get-Duration {
    param([string]$Path)
    $value = & $ffprobe -v error -show_entries 'format=duration' -of 'default=noprint_wrappers=1:nokey=1' $Path
    if ($LASTEXITCODE -ne 0) { throw "Could not inspect duration: $Path" }
    return [Math]::Round([double]$value, 3)
}

function Get-ProjectInvariant {
    param([string]$ProjectId)
    $project = (Invoke-Core -Arguments @('project', 'show', $ProjectId)).project
    $segments = @($project.transcript.segments | ForEach-Object {
        [ordered]@{ id = [string]$_.id; start = [double]$_.start; end = [double]$_.end; text = [string]$_.text }
    })
    $edits = @($project.edits | ForEach-Object {
        [ordered]@{ id = [string]$_.id; start = [double]$_.start; end = [double]$_.end; status = [string]$_.status }
    })
    return [ordered]@{
        transcript = ($segments | ConvertTo-Json -Depth 6 -Compress)
        edits = ($edits | ConvertTo-Json -Depth 6 -Compress)
        sourceDuration = [Math]::Round([double]$project.timeline.sourceDuration, 3)
        outputDuration = [Math]::Round([double]$project.timeline.outputDuration, 3)
        segmentCount = $segments.Count
        wordCount = @($project.transcript.words).Count
        recognizedText = ($segments | ForEach-Object { $_.text }) -join ''
    }
}

function Compare-ProjectInvariant {
    param([System.Collections.IDictionary]$Before, [System.Collections.IDictionary]$After)
    return (
        $Before.transcript -eq $After.transcript -and
        $Before.edits -eq $After.edits -and
        $Before.sourceDuration -eq $After.sourceDuration -and
        $Before.outputDuration -eq $After.outputDuration
    )
}

try {
    $env:SIAOCUT_HOME = Join-Path $work 'home'
    $env:SIAOCUT_DIRECT = '1'
    $env:SIAOCUT_FFMPEG = $ffmpeg
    $env:SIAOCUT_FFPROBE = $ffprobe
    $env:SIAOCUT_WHISPER_CLI = $whisper
    $env:SIAOCUT_WHISPER_VAD_MODEL = $vadModel

    Add-Type -AssemblyName System.Speech
    $synthesizer = [System.Speech.Synthesis.SpeechSynthesizer]::new()
    try {
        $voices = @($synthesizer.GetInstalledVoices() | Where-Object Enabled | ForEach-Object { $_.VoiceInfo })
        $zhFemale = @($voices | Where-Object { $_.Culture.Name -eq 'zh-CN' -and $_.Gender -eq 'Female' })[0]
        $zhMale = @($voices | Where-Object { $_.Culture.Name -eq 'zh-CN' -and $_.Gender -eq 'Male' })[0]
        $enVoice = @($voices | Where-Object { $_.Culture.Name -eq 'en-US' })[0]
        if (-not $zhFemale -or -not $enVoice) {
            throw 'The benchmark requires enabled zh-CN and en-US Windows voices.'
        }
        $usesPitchShiftedZhVoice = -not [bool]$zhMale
        if ($usesPitchShiftedZhVoice) { $zhMale = $zhFemale }

        $singleText = ConvertFrom-Utf8Base64 '6L+Z5piv5LiA5q615Y2V5Lq65pys5Zyw5Ymq6L6R5rWL6K+V44CC6K+t6Z+z5YiG5p6Q5Y+q5o+Q5L6b6K+B5o2u77yM5LiN5Lya6Ieq5Yqo5Yig6Zmk5YaF5a6544CC5oiR5Lus5Lya5YWI5qOA5p+l57uT5p6c77yM5YaN5Yaz5a6a5LiL5LiA5q2l5pON5L2c44CC'
        $dualFemaleText = ConvertFrom-Utf8Base64 '56ys5LiA5L2N6K+06K+d5Lq65q2j5Zyo5LuL57uN5pys5Zyw5LyY5YWI55qE5Ymq6L6R5rWB56iL44CC5omA5pyJ5aqS5L2T6YO95L+d5a2Y5Zyo55S16ISR5LiK44CC'
        $dualMaleText = ConvertFrom-Utf8Base64 '56ys5LqM5L2N6K+06K+d5Lq66LSf6LSj5a6h6ZiF57uT5p6c44CC5Lu75L2V5L+u5pS56YO96ZyA6KaB5Lq65bel56Gu6K6k77yM5bm25LiU5Y+v5Lul5pKk6ZSA44CC'
        $mixedChineseText = ConvertFrom-Utf8Base64 '546w5Zyo5byA5aeL5Lit6Iux5paH5re35ZCI5rWL6K+V77yM5pys5Zyw5bel5L2c5Y+w5Lya5L+d55WZ5q+P5LiA5qyh5Lq65bel5Yaz5a6a44CC'
        $mixedEnglishText = 'This English passage verifies bilingual timing and local speaker evidence without cloud upload.'

        $single = Join-Path $work 'single.wav'
        $femaleA = Join-Path $work 'dual-female-a.wav'
        $femaleB = Join-Path $work 'dual-female-b.wav'
        $maleA = Join-Path $work 'dual-male-a.wav'
        $maleB = Join-Path $work 'dual-male-b.wav'
        $mixedZhA = Join-Path $work 'mixed-zh-a.wav'
        $mixedZhB = Join-Path $work 'mixed-zh-b.wav'
        $mixedEnA = Join-Path $work 'mixed-en-a.wav'
        $mixedEnB = Join-Path $work 'mixed-en-b.wav'
        $pause = Join-Path $work 'pause.wav'

        New-TtsWave -Synthesizer $synthesizer -Voice $zhFemale.Name -Text $singleText -Path $single
        New-TtsWave -Synthesizer $synthesizer -Voice $zhFemale.Name -Text $dualFemaleText -Path $femaleA
        New-TtsWave -Synthesizer $synthesizer -Voice $zhFemale.Name -Text (ConvertFrom-Utf8Base64 '56ys5LiA5L2N6K+06K+d5Lq65YaN5qyh56Gu6K6k77yM5YiG5p6Q57uT5p6c5LuN54S25LiN5Lya55u05o6l5pS55YaZ5a2X5bmV44CC') -Path $femaleB
        if ($usesPitchShiftedZhVoice) {
            New-PitchShiftedTtsWave -Synthesizer $synthesizer -Voice $zhMale.Name -Text $dualMaleText -Path $maleA
            New-PitchShiftedTtsWave -Synthesizer $synthesizer -Voice $zhMale.Name -Text (ConvertFrom-Utf8Base64 '56ys5LqM5L2N6K+06K+d5Lq65pyA5ZCO56Gu6K6k77yM5Y6f5aeL5aqS5L2T5Zyo5rWL6K+V5YmN5ZCO5b+F6aG75L+d5oyB5LiN5Y+Y44CC') -Path $maleB
        } else {
            New-TtsWave -Synthesizer $synthesizer -Voice $zhMale.Name -Text $dualMaleText -Path $maleA
            New-TtsWave -Synthesizer $synthesizer -Voice $zhMale.Name -Text (ConvertFrom-Utf8Base64 '56ys5LqM5L2N6K+06K+d5Lq65pyA5ZCO56Gu6K6k77yM5Y6f5aeL5aqS5L2T5Zyo5rWL6K+V5YmN5ZCO5b+F6aG75L+d5oyB5LiN5Y+Y44CC') -Path $maleB
        }
        New-TtsWave -Synthesizer $synthesizer -Voice $zhFemale.Name -Text $mixedChineseText -Path $mixedZhA
        New-TtsWave -Synthesizer $synthesizer -Voice $zhFemale.Name -Text (ConvertFrom-Utf8Base64 '5Lit5paH5q616JC957uT5p2f5ZCO77yM57O757uf5LuN54S25Y+q5L+d5a2Y5Y+v5Lul5a6h6ZiF55qE6K+B5o2u44CC') -Path $mixedZhB
        New-TtsWave -Synthesizer $synthesizer -Voice $enVoice.Name -Text $mixedEnglishText -Path $mixedEnA
        New-TtsWave -Synthesizer $synthesizer -Voice $enVoice.Name -Text 'The final English passage confirms that transcript changes always remain under human control.' -Path $mixedEnB
        Invoke-Ffmpeg -Arguments @('-f', 'lavfi', '-i', 'anullsrc=r=16000:cl=mono', '-t', '0.7', '-c:a', 'pcm_s16le', $pause) -Failure 'Could not generate the pause fixture.'
    } finally {
        $synthesizer.Dispose()
    }

    $dual = Join-Path $work 'dual.wav'
    $mixed = Join-Path $work 'mixed-zh-en.wav'
    $silence = Join-Path $work 'silence.wav'
    $noise = Join-Path $work 'speech-with-pink-noise.wav'
    $music = Join-Path $work 'speech-with-music-bed.wav'
    New-ConcatenatedWave -Inputs @($femaleA, $pause, $maleA, $pause, $femaleB, $pause, $maleB) -Path $dual
    New-ConcatenatedWave -Inputs @($mixedZhA, $pause, $mixedEnA, $pause, $mixedZhB, $pause, $mixedEnB) -Path $mixed
    Invoke-Ffmpeg -Arguments @('-f', 'lavfi', '-i', 'anullsrc=r=16000:cl=mono', '-t', '8', '-c:a', 'pcm_s16le', $silence) -Failure 'Could not generate the silence fixture.'
    Invoke-Ffmpeg -Arguments @('-i', $single, '-f', 'lavfi', '-i', 'anoisesrc=color=pink:amplitude=0.06:sample_rate=16000:seed=12345', '-filter_complex', '[0:a][1:a]amix=inputs=2:duration=first:normalize=0', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $noise) -Failure 'Could not generate the noise fixture.'
    Invoke-Ffmpeg -Arguments @('-i', $single, '-f', 'lavfi', '-i', 'sine=frequency=220:sample_rate=16000', '-f', 'lavfi', '-i', 'sine=frequency=330:sample_rate=16000', '-filter_complex', '[1:a]volume=0.035[t1];[2:a]volume=0.025[t2];[0:a][t1][t2]amix=inputs=3:duration=first:normalize=0', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $music) -Failure 'Could not generate the music-bed fixture.'

    $fixtures = @(
        [ordered]@{ id = 'single'; path = $single; language = 'zh'; expectedSpeakers = 1; interference = 'none'; groundTruth = $singleText },
        [ordered]@{ id = 'dual'; path = $dual; language = 'zh'; expectedSpeakers = 2; interference = 'none'; groundTruth = "$dualFemaleText $dualMaleText" },
        [ordered]@{ id = 'mixed-zh-en'; path = $mixed; language = $null; expectedSpeakers = 2; interference = 'language-switch'; groundTruth = "$mixedChineseText $mixedEnglishText" },
        [ordered]@{ id = 'silence'; path = $silence; language = 'zh'; expectedSpeakers = 0; interference = 'silence'; groundTruth = '' },
        [ordered]@{ id = 'pink-noise'; path = $noise; language = 'zh'; expectedSpeakers = 1; interference = 'pink-noise-overlay'; groundTruth = $singleText },
        [ordered]@{ id = 'music-bed'; path = $music; language = 'zh'; expectedSpeakers = 1; interference = 'two-tone-music-proxy'; groundTruth = $singleText }
    )

    $results = [Collections.Generic.List[object]]::new()
    $projectState = @{}
    foreach ($fixture in $fixtures) {
        $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixture.path).Hash.ToLowerInvariant()
        $imported = Invoke-Core -Arguments @('import', $fixture.path, '--title', "Voice benchmark $($fixture.id)")
        $transcribeArguments = @('transcribe', [string]$imported.projectId, '--model', $Model)
        if ($fixture.language) { $transcribeArguments += @('--language', [string]$fixture.language) }
        Invoke-Core -Arguments $transcribeArguments | Out-Null
        $before = Get-ProjectInvariant -ProjectId ([string]$imported.projectId)
        $rhythm = (Invoke-Core -Arguments @('speech', 'analyze', [string]$imported.projectId)).speechInsights
        $audioStarted = Invoke-Core -Arguments @('speech', 'audio-start', [string]$imported.projectId)
        $audio = Wait-AudioJob -JobId ([string]$audioStarted.audioAnalysisJob.id)
        $afterDeterministic = Get-ProjectInvariant -ProjectId ([string]$imported.projectId)
        $unchanged = Compare-ProjectInvariant -Before $before -After $afterDeterministic
        if (-not $unchanged) { throw "Deterministic voice analysis changed project content for $($fixture.id)." }
        if ($sourceHash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $fixture.path).Hash.ToLowerInvariant()) {
            throw "Source media changed during analysis for $($fixture.id)."
        }
        $entry = [ordered]@{
            id = [string]$fixture.id
            mediaSha256 = $sourceHash
            durationSeconds = Get-Duration -Path $fixture.path
            languageHint = $fixture.language
            expectedSpeakers = [int]$fixture.expectedSpeakers
            interference = [string]$fixture.interference
            groundTruth = [string]$fixture.groundTruth
            projectId = [string]$imported.projectId
            transcriptSegments = [int]$afterDeterministic.segmentCount
            transcriptWords = [int]$afterDeterministic.wordCount
            recognizedText = [string]$afterDeterministic.recognizedText
            rhythm = $rhythm
            audio = $audio.report
            deterministicAnalysisPreservedProject = $unchanged
            sourceHashPreserved = $true
            speaker = $null
        }
        $results.Add($entry) | Out-Null
        $projectState[[string]$fixture.id] = [ordered]@{ projectId = [string]$imported.projectId; invariant = $afterDeterministic; sourceHash = $sourceHash; path = $fixture.path }
    }

    $packageBefore = (Invoke-Core -Arguments @('speaker', 'package')).speakerPackage
    if ($packageBefore.installed) { throw 'The isolated benchmark home unexpectedly contains the speaker package.' }
    $missingProject = $projectState['single']
    $missingError = Invoke-CoreExpectedError -Arguments @('speaker', 'analyze', [string]$missingProject.projectId) -Code 'speaker_package_missing'
    $missingAfter = Get-ProjectInvariant -ProjectId ([string]$missingProject.projectId)
    $missingPackageDidNotBlock = Compare-ProjectInvariant -Before $missingProject.invariant -After $missingAfter
    if (-not $missingPackageDidNotBlock) { throw 'Missing speaker package changed or blocked the single-speaker project.' }

    $installResult = $null
    if ($InstallSpeakerPackage) {
        $installStarted = Invoke-Core -Arguments @('speaker', 'install')
        $installJob = Wait-SpeakerJob -JobId ([string]$installStarted.speakerJob.id) -Seconds 240
        $packageAfter = (Invoke-Core -Arguments @('speaker', 'package', '--verify')).speakerPackage
        if (-not $packageAfter.installed -or $packageAfter.verified -ne $true) {
            throw 'Speaker package did not pass explicit post-install verification.'
        }
        $installResult = [ordered]@{
            explicitSwitch = $true
            jobId = [string]$installJob.id
            status = [string]$installJob.status
            downloadSize = [int64]$packageAfter.downloadSize
            installedSize = [int64]$packageAfter.installedSize
            license = [string]$packageAfter.license
            source = [string]$packageAfter.source
            verified = [bool]$packageAfter.verified
            assets = @($packageAfter.assets | ForEach-Object { [ordered]@{ id = $_.id; source = $_.source; license = $_.license; size = $_.size; sha256 = $_.sha256; verified = $_.verified } })
        }
        foreach ($entry in $results) {
            $state = $projectState[[string]$entry.id]
            $speakerStarted = Invoke-Core -Arguments @('speaker', 'analyze', [string]$state.projectId)
            $speakerJob = Wait-SpeakerJob -JobId ([string]$speakerStarted.speakerJob.id)
            $track = (Invoke-Core -Arguments @('speaker', 'track', [string]$state.projectId)).speakerTrack
            $afterSpeaker = Get-ProjectInvariant -ProjectId ([string]$state.projectId)
            $preserved = Compare-ProjectInvariant -Before $state.invariant -After $afterSpeaker
            if (-not $preserved) { throw "Speaker analysis changed project content for $($entry.id)." }
            if ($state.sourceHash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $state.path).Hash.ToLowerInvariant()) {
                throw "Source media changed during speaker analysis for $($entry.id)."
            }
            $entry['speaker'] = [ordered]@{
                jobId = [string]$speakerJob.id
                status = [string]$track.status
                detectedSpeakers = @($track.speakers).Count
                expectedSpeakers = [int]$entry.expectedSpeakers
                speakerCountAbsoluteError = [Math]::Abs(@($track.speakers).Count - [int]$entry.expectedSpeakers)
                turns = @($track.turns).Count
                associations = @($track.associations).Count
                runtimeVersion = [string]$track.runtimeVersion
                segmentationModel = [string]$track.segmentationModel
                embeddingModel = [string]$track.embeddingModel
                preservedProject = $preserved
            }
        }
    }

    $silenceResult = @($results | Where-Object { $_.id -eq 'silence' })[0]
    if (-not $silenceResult -or $silenceResult.transcriptSegments -ne 0 -or $silenceResult.transcriptWords -ne 0 -or [string]$silenceResult.rhythm.status -ne 'insufficient_evidence') {
        throw 'The silence benchmark produced transcript content or fabricated rhythm evidence.'
    }
    $speechResults = @($results | Where-Object { $_.id -ne 'silence' })
    if (@($speechResults | Where-Object { [string]$_.rhythm.status -ne 'ready' }).Count -ne 0) {
        throw 'At least one audible benchmark did not produce deterministic rhythm evidence.'
    }
    $speakerExactCount = $null
    $speakerMaximumError = $null
    if ($InstallSpeakerPackage) {
        $speakerExactCount = @($results | Where-Object { [int]$_.speaker.speakerCountAbsoluteError -eq 0 }).Count
        $speakerMaximumError = ($results | ForEach-Object { [int]$_.speaker.speakerCountAbsoluteError } | Measure-Object -Maximum).Maximum
        if ($speakerExactCount -lt 5 -or $speakerMaximumError -gt 1 -or [string]$silenceResult.speaker.status -ne 'no_speech') {
            throw "Speaker benchmark exceeded the acceptance boundary: exact=$speakerExactCount/6, maximum error=$speakerMaximumError."
        }
    }

    $os = Get-CimInstance Win32_OperatingSystem
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    $ffmpegVersion = (& $ffmpeg -version | Select-Object -First 1)
    $previousErrorPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $whisperVersionOutput = & $whisper --version 2>&1
    } finally {
        $ErrorActionPreference = $previousErrorPreference
    }
    $whisperVersion = ($whisperVersionOutput | Where-Object { $_ -like 'whisper.cpp version:*' } | Select-Object -First 1)
    $cargoVersionLine = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"' } | Select-Object -First 1
    $coreVersion = if ($cargoVersionLine -match '^version\s*=\s*"([^"]+)"') { $Matches[1] } else { 'unknown' }
    $report = [ordered]@{
        schemaVersion = 1
        evaluatedAt = (Get-Date).ToUniversalTime().ToString('o')
        platform = [ordered]@{
            os = [string]$os.Caption
            version = [string]$os.Version
            architecture = [string]$os.OSArchitecture
            cpu = [string]$cpu.Name
            logicalProcessors = [int]$cpu.NumberOfLogicalProcessors
        }
        tools = [ordered]@{
            core = $coreVersion
            ffmpeg = [string]$ffmpegVersion
            whisper = [string]$whisperVersion
            whisperModelSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Model).Hash.ToLowerInvariant()
        }
        materialPolicy = [ordered]@{
            redistributed = $false
            scripts = 'Self-authored SiaoCut acceptance phrases.'
            speech = 'Generated locally through enabled Windows system voices; temporary WAV files are not committed.'
            silenceNoiseAndMusic = 'Generated locally with FFmpeg lavfi; the music case is a deterministic two-tone interference proxy.'
            voices = @($voices | ForEach-Object { [ordered]@{ name = $_.Name; culture = $_.Culture.Name; gender = [string]$_.Gender } })
            pitchShiftedZhVoiceFallback = [bool]$usesPitchShiftedZhVoice
        }
        speakerPackageBeforeInstall = [ordered]@{
            installed = [bool]$packageBefore.installed
            id = [string]$packageBefore.id
            source = [string]$packageBefore.source
            license = [string]$packageBefore.license
            downloadSize = [int64]$packageBefore.downloadSize
        }
        missingSpeakerPackage = [ordered]@{
            expectedErrorCode = [string]$missingError.code
            singleSpeakerProjectRemainedUsable = $missingPackageDidNotBlock
        }
        speakerInstall = $installResult
        benchmarks = @($results)
        invariants = [ordered]@{
            fixtureCount = $results.Count
            allDeterministicAnalysesPreservedProjects = (@($results | Where-Object { -not $_.deterministicAnalysisPreservedProject }).Count -eq 0)
            allSourceHashesPreserved = (@($results | Where-Object { -not $_.sourceHashPreserved }).Count -eq 0)
            allSpeakerAnalysesPreservedProjects = if ($InstallSpeakerPackage) { @($results | Where-Object { -not $_.speaker.preservedProject }).Count -eq 0 } else { $null }
            noVoiceAnalysisAppliedCuts = ((@($results | Where-Object { $_.deterministicAnalysisPreservedProject -ne $true }).Count -eq 0) -and (-not $InstallSpeakerPackage -or @($results | Where-Object { -not $_.speaker.preservedProject }).Count -eq 0))
            silenceProducedNoTranscript = ($silenceResult.transcriptSegments -eq 0 -and $silenceResult.transcriptWords -eq 0)
            audibleFixturesProducedRhythmEvidence = (@($speechResults | Where-Object { [string]$_.rhythm.status -ne 'ready' }).Count -eq 0)
            speakerExactCountFixtures = $speakerExactCount
            speakerMaximumAbsoluteError = $speakerMaximumError
        }
        limitations = @(
            'The audio analyzer measures loudness, true peak, silence and clipping; it does not classify arbitrary noise or music.',
            'Speaker-count error is recorded per fixture instead of being hidden behind a combined AI score.',
            'Synthetic system voices are repeatable smoke benchmarks, not a substitute for consented creator, dialect or overlapping-speech evaluation.'
        )
    }
    $report | ConvertTo-Json -Depth 24
} finally {
    foreach ($name in $previousEnvironment.Keys) {
        $value = $previousEnvironment[$name]
        if ($null -eq $value) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue } else { Set-Item "Env:$name" $value }
    }
    if (-not $KeepArtifacts -and (Test-Path -LiteralPath $work)) {
        $resolved = (Resolve-Path -LiteralPath $work).Path
        if (-not $resolved.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -notlike 'siaocut-voice-intelligence-*') {
            throw "Refusing to remove unexpected benchmark directory: $resolved"
        }
        [IO.Directory]::Delete($resolved, $true)
    }
}
