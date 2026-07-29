param(
    [string]$BaselineWhisper,
    [Parameter(Mandatory = $true)]
    [string]$PatchedWhisper,
    [Parameter(Mandatory = $true)]
    [string]$Model,
    [Parameter(Mandatory = $true)]
    [string]$VadModel,
    [ValidateSet('cpu', 'vulkan', 'cuda')]
    [string]$ExpectedPatchedBackend = 'cpu',
    [string]$EvidenceOutput,
    [string]$RuntimeMetadata,
    [string]$TempDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$sample = Join-Path $root 'third_party\whisper.cpp\samples\jfk.wav'
$requiredFiles = @($PatchedWhisper, $Model, $VadModel, $sample)
if ($BaselineWhisper) { $requiredFiles += $BaselineWhisper }
foreach ($required in $requiredFiles) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required VAD timeline fixture is missing: $required"
    }
}
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source

$tempBase = if ($TempDirectory) {
    [IO.Path]::GetFullPath($TempDirectory)
} else {
    [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
}
New-Item -ItemType Directory -Force -Path $tempBase | Out-Null
$runDirectory = [IO.Path]::GetFullPath((Join-Path $tempBase "siaocut-vad-timeline-$([guid]::NewGuid().ToString('N'))"))
if (-not $runDirectory.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a VAD timeline directory outside the declared temp root: $runDirectory"
}
New-Item -ItemType Directory -Path $runDirectory | Out-Null

function Get-Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Invoke-WhisperJson([string]$Whisper, [string]$Name, [string]$Fixture) {
    $outputBase = Join-Path $runDirectory $Name
    $previousErrorPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $Whisper `
        -m $Model `
        -f $Fixture `
        -l en `
        -ojf `
        -sow `
        --vad `
        -vm $VadModel `
        --vad-min-silence-duration-ms 250 `
        --vad-speech-pad-ms 80 `
        -of $outputBase *> (Join-Path $runDirectory "$Name.log")
    $whisperExitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorPreference
    if ($whisperExitCode -ne 0) {
        throw "$Name whisper.cpp VAD transcription failed with exit code $whisperExitCode."
    }
    $jsonPath = "$outputBase.json"
    if (-not (Test-Path -LiteralPath $jsonPath -PathType Leaf)) {
        throw "$Name whisper.cpp did not produce full JSON."
    }
    return [IO.File]::ReadAllText($jsonPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
}

function Get-SpokenTokens([object]$Document) {
    $rows = @()
    for ($segmentIndex = 0; $segmentIndex -lt $Document.transcription.Count; $segmentIndex++) {
        $segment = $Document.transcription[$segmentIndex]
        for ($tokenIndex = 0; $tokenIndex -lt $segment.tokens.Count; $tokenIndex++) {
            $token = $segment.tokens[$tokenIndex]
            $hasLexicalCharacter = $token.text -and @(
                $token.text.ToCharArray() | Where-Object { [char]::IsLetterOrDigit($_) }
            ).Count -gt 0
            if (-not $token.offsets -or -not $hasLexicalCharacter -or $token.text -match '^\[_') {
                continue
            }
            $rows += [pscustomobject]@{
                segmentIndex = $segmentIndex
                tokenIndex = $tokenIndex
                text = $token.text
                segmentFrom = [long]$segment.offsets.from
                segmentTo = [long]$segment.offsets.to
                tokenFrom = [long]$token.offsets.from
                tokenTo = [long]$token.offsets.to
            }
        }
    }
    return $rows
}

try {
    $fixture = Join-Path $runDirectory 'speech-silence-speech.wav'
    & $ffmpeg `
        -y `
        -hide_banner `
        -loglevel error `
        -i $sample `
        -f lavfi `
        -t 4 `
        -i 'anullsrc=r=16000:cl=mono' `
        -filter_complex '[0:a][1:a][0:a]concat=n=3:v=0:a=1[out]' `
        -map '[out]' `
        -ac 1 `
        -ar 16000 `
        -c:a pcm_s16le `
        $fixture
    if ($LASTEXITCODE -ne 0) { throw 'Could not generate the speech-silence-speech VAD fixture.' }
    $durationSeconds = [double]((& $ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 $fixture).Trim())

    $baseline = if ($BaselineWhisper) {
        Invoke-WhisperJson $BaselineWhisper 'baseline' $fixture
    } else {
        $null
    }
    $patched = Invoke-WhisperJson $PatchedWhisper 'patched' $fixture
    $patchedLog = [IO.File]::ReadAllText((Join-Path $runDirectory 'patched.log'))
    switch ($ExpectedPatchedBackend) {
        'cpu' {
            if (-not $patchedLog.Contains('load_backend: loaded CPU backend')) {
                throw 'The patched CPU acceptance run did not load the CPU backend.'
            }
        }
        'vulkan' {
            if (-not $patchedLog.Contains('load_backend: loaded Vulkan backend') -or
                $patchedLog.Contains('ggml_vulkan: Found 0 Vulkan devices')) {
                throw 'The patched Vulkan acceptance run did not load an available Vulkan device.'
            }
        }
        'cuda' {
            if (-not $patchedLog.Contains('load_backend: loaded CUDA backend')) {
                throw 'The patched CUDA acceptance run did not load the CUDA backend.'
            }
        }
    }
    if (($baseline -and $baseline.transcription.Count -lt 2) -or $patched.transcription.Count -lt 2) {
        throw 'The VAD fixture did not produce the expected repeated speech segments.'
    }

    $patchedSegments = @($patched.transcription | ForEach-Object {
        "$($_.offsets.from)|$($_.offsets.to)|$($_.text)"
    })

    $patchedTokens = @(Get-SpokenTokens $patched)
    if ($patchedTokens.Count -eq 0) { throw 'The patched CLI output does not contain spoken tokens.' }
    $patchedOutsideParent = @($patchedTokens | Where-Object {
        $_.tokenFrom -lt ($_.segmentFrom - 500) -or $_.tokenTo -gt ($_.segmentTo + 500)
    })
    if ($patchedOutsideParent.Count -gt 0) {
        throw "The patched CLI left $($patchedOutsideParent.Count) tokens outside their original-media segment."
    }

    for ($index = 0; $index -lt $patchedTokens.Count; $index++) {
        $patchedToken = $patchedTokens[$index]
        if ($patchedToken.tokenTo -le $patchedToken.tokenFrom) {
            throw "Patched token has a non-positive duration at index $index."
        }
        if ($index -gt 0 -and $patchedToken.tokenFrom -lt $patchedTokens[$index - 1].tokenFrom) {
            throw "Patched token timeline is not monotonic at index $index."
        }
    }

    $baselineEvidence = $null
    $maxMappedDeltaMs = $null
    $textAndSegmentTimelineUnchanged = $null
    if ($baseline) {
        $baselineSegments = @($baseline.transcription | ForEach-Object {
            "$($_.offsets.from)|$($_.offsets.to)|$($_.text)"
        })
        if (($baselineSegments -join "`n") -ne ($patchedSegments -join "`n")) {
            throw 'The CLI patch changed segment text or segment timestamps.'
        }
        $baselineTokens = @(Get-SpokenTokens $baseline)
        if ($baselineTokens.Count -eq 0 -or $baselineTokens.Count -ne $patchedTokens.Count) {
            throw 'Baseline and patched CLI outputs do not contain the same spoken tokens.'
        }
        $baselineOutsideParent = @($baselineTokens | Where-Object {
            $_.tokenFrom -lt ($_.segmentFrom - 500) -or $_.tokenTo -gt ($_.segmentTo + 500)
        })
        if ($baselineOutsideParent.Count -eq 0) {
            throw 'The unpatched CLI did not reproduce VAD-processed token timestamps.'
        }
        $maxMappedDeltaMs = 0L
        for ($index = 0; $index -lt $baselineTokens.Count; $index++) {
            if ($baselineTokens[$index].text -ne $patchedTokens[$index].text) {
                throw "Token text changed at index $index."
            }
            $delta = [math]::Abs($patchedTokens[$index].tokenFrom - $baselineTokens[$index].tokenFrom)
            if ($delta -gt $maxMappedDeltaMs) { $maxMappedDeltaMs = $delta }
        }
        if ($maxMappedDeltaMs -lt 1000) {
            throw "The fixture did not prove a material VAD time-domain mapping; maximum delta was $maxMappedDeltaMs ms."
        }
        $baselineEvidence = @{
            executableSha256 = Get-Sha256 $BaselineWhisper
            outsideParentTokenCount = $baselineOutsideParent.Count
            timeDomain = 'vad_processed'
        }
        $textAndSegmentTimelineUnchanged = $true
    }

    $backendProbe = switch ($ExpectedPatchedBackend) {
        'vulkan' {
            $deviceLine = $patchedLog -split "\r?\n" |
                Where-Object { $_ -match '^ggml_vulkan: 0 = ' } |
                Select-Object -First 1
            if ($deviceLine -match '^ggml_vulkan: 0 = (?<device>.+?) \(') {
                "Vulkan device: $($Matches.device)"
            } else {
                'Vulkan device loaded'
            }
        }
        'cuda' {
            $deviceLine = $patchedLog -split "\r?\n" |
                Where-Object { $_ -match 'cuda.*device|device.*cuda' } |
                Select-Object -First 1
            if ($deviceLine) { $deviceLine.Trim() } else { 'CUDA device loaded' }
        }
        default { 'CPU backend loaded' }
    }
    $result = [ordered]@{
        schemaVersion = 1
        status = 'passed'
        fixture = @{
            source = 'whisper.cpp/samples/jfk.wav + 4 seconds synthetic silence + repeated sample'
            durationSeconds = $durationSeconds
            sha256 = Get-Sha256 $fixture
        }
        baseline = $baselineEvidence
        patched = @{
            backend = $ExpectedPatchedBackend
            backendProbe = $backendProbe
            executableSha256 = Get-Sha256 $PatchedWhisper
            outsideParentTokenCount = $patchedOutsideParent.Count
            timeDomain = 'original_media'
        }
        segmentCount = $patched.transcription.Count
        spokenTokenCount = $patchedTokens.Count
        maximumMappedDeltaMs = $maxMappedDeltaMs
        textAndSegmentTimelineUnchanged = $textAndSegmentTimelineUnchanged
    }
    $resultJson = $result | ConvertTo-Json -Depth 7

    if ($EvidenceOutput) {
        $resolvedEvidence = [IO.Path]::GetFullPath($EvidenceOutput)
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $resolvedEvidence) | Out-Null
        [IO.File]::WriteAllText($resolvedEvidence, $resultJson, [Text.UTF8Encoding]::new($false))
        $evidenceSha256 = Get-Sha256 $resolvedEvidence

        if ($RuntimeMetadata) {
            $resolvedMetadata = [IO.Path]::GetFullPath($RuntimeMetadata)
            if (-not (Test-Path -LiteralPath $resolvedMetadata -PathType Leaf)) {
                throw "Runtime metadata is missing: $resolvedMetadata"
            }
            $metadata = [IO.File]::ReadAllText($resolvedMetadata, [Text.Encoding]::UTF8) | ConvertFrom-Json
            if ($metadata.backend -ne $ExpectedPatchedBackend) {
                throw "Runtime metadata backend is $($metadata.backend), expected $ExpectedPatchedBackend."
            }
            if ($metadata.executableSha256 -ne $result.patched.executableSha256) {
                throw 'Runtime metadata executable hash does not match the accepted whisper-cli.'
            }
            if ($metadata.patchSha256 -ne (
                [IO.File]::ReadAllText((Join-Path $root 'release\whisper-runtime-source.json'), [Text.Encoding]::UTF8) |
                    ConvertFrom-Json
            ).patch.sha256) {
                throw 'Runtime metadata does not contain the declared VAD timeline patch.'
            }
            $metadata.vadTimelineVerification.status = 'verified'
            $metadata.vadTimelineVerification.timeDomain = 'original_media'
            $metadata.vadTimelineVerification.evidenceSha256 = $evidenceSha256
            $metadata.vadTimelineVerification | Add-Member -NotePropertyName fixtureSha256 -NotePropertyValue $result.fixture.sha256 -Force
            $metadata.vadTimelineVerification | Add-Member -NotePropertyName verifier -NotePropertyValue 'tools/test-whisper-vad-timeline.ps1' -Force
            $metadata.files = @($metadata.files | Where-Object { $_.name -ne (Split-Path -Leaf $resolvedEvidence) })
            $metadata.files += [pscustomobject]@{
                name = Split-Path -Leaf $resolvedEvidence
                size = (Get-Item -LiteralPath $resolvedEvidence).Length
                sha256 = $evidenceSha256
            }
            [IO.File]::WriteAllText(
                $resolvedMetadata,
                ($metadata | ConvertTo-Json -Depth 9),
                [Text.UTF8Encoding]::new($false)
            )
        }
    } elseif ($RuntimeMetadata) {
        throw 'RuntimeMetadata requires EvidenceOutput so the verification remains auditable.'
    }

    $resultJson
} finally {
    if (Test-Path -LiteralPath $runDirectory) {
        $resolvedRunDirectory = [IO.Path]::GetFullPath($runDirectory)
        if (-not $resolvedRunDirectory.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove a directory outside the declared temp root: $resolvedRunDirectory"
        }
        Remove-Item -LiteralPath $resolvedRunDirectory -Recurse -Force
    }
}
