param(
    [string]$Core = (Join-Path $PSScriptRoot "..\..\..\target\debug\siaocut-core.exe")
)

$ErrorActionPreference = "Stop"
$Core = (Resolve-Path -LiteralPath $Core).Path
$testHome = Join-Path ([System.IO.Path]::GetTempPath()) ("siaocut-skill-" + [guid]::NewGuid().ToString("N"))
$previousHome = $env:SIAOCUT_HOME
$previousIdle = $env:SIAOCUT_SERVICE_IDLE_MS

function Invoke-SiaoCut([string[]]$Arguments) {
    $raw = & $Core --json @Arguments | Out-String
    if ($LASTEXITCODE -ne 0) { throw "SiaoCut command failed: $raw" }
    $result = $raw | ConvertFrom-Json
    if ($result.status -ne "ok") { throw "SiaoCut returned $($result.status): $raw" }
    return $result
}

try {
    New-Item -ItemType Directory -Path $testHome | Out-Null
    $env:SIAOCUT_HOME = $testHome
    $env:SIAOCUT_SERVICE_IDLE_MS = "100"
    $media = Join-Path $testHome "talk.wav"
    [System.IO.File]::WriteAllBytes($media, [byte[]](1, 2, 3, 4))

    $project = Invoke-SiaoCut @("import", $media, "--title", "Skill workflow test")
    $segment = Invoke-SiaoCut @("transcript", "add", $project.projectId, "--start", "0", "--end", "2", "--text", "hello world")
    $workflow = Invoke-SiaoCut @("workflow", "create", $project.projectId, "--kind", "polish")
    $claim = Invoke-SiaoCut @("task", "claim", "--worker", "skill-e2e")

    $responsePath = Join-Path $testHome "response.json"
    $responseJson = @{
        baseVersionId = $claim.payload.baseVersionId
        patches = @(@{
            segmentId = $segment.segment.id
            before = "hello world"
            after = "Hello, world."
            reason = "Add punctuation"
            confidence = 0.99
        })
    } | ConvertTo-Json -Depth 6
    [System.IO.File]::WriteAllText($responsePath, $responseJson, (New-Object System.Text.UTF8Encoding($false)))

    $submitted = Invoke-SiaoCut @("task", "submit", $claim.taskId, "--worker", "skill-e2e", "--response", $responsePath)
    if ($submitted.task.status -ne "review") { throw "Task did not enter review state" }
    $beforeReview = Invoke-SiaoCut @("project", "show", $project.projectId)
    if ($beforeReview.project.transcript.segments[0].text -ne "hello world") { throw "Agent result changed the project before review" }

    $diff = Invoke-SiaoCut @("task", "diff", $claim.taskId)
    if ($diff.patchSet.items[0].afterText -ne "Hello, world.") { throw "Patch diff is missing the proposed text" }
    $reviewed = Invoke-SiaoCut @("task", "review-all", $claim.taskId, "--action", "apply")
    if ($reviewed.project.transcript.segments[0].text -ne "Hello, world.") { throw "Reviewed patch was not applied" }
    $status = Invoke-SiaoCut @("workflow", "status", $workflow.workflowId)
    if ($status.workflow.status -ne "completed") { throw "Workflow did not complete" }

    # A human edit made after claim must win. The Agent result becomes a
    # reviewable conflict and keeping it must leave the human text untouched.
    $conflictWorkflow = Invoke-SiaoCut @("workflow", "create", $project.projectId, "--kind", "proofread")
    $conflictClaim = Invoke-SiaoCut @("task", "claim", "--worker", "skill-e2e-conflict")
    Invoke-SiaoCut @("transcript", "edit", $project.projectId, $segment.segment.id, "--text", "Human edit wins.") | Out-Null

    $conflictResponsePath = Join-Path $testHome "conflict-response.json"
    $conflictResponseJson = @{
        baseVersionId = $conflictClaim.payload.baseVersionId
        patches = @(@{
            segmentId = $segment.segment.id
            before = "Hello, world."
            after = "Agent edit."
            reason = "Normalize wording"
            confidence = 0.85
        })
    } | ConvertTo-Json -Depth 6
    [System.IO.File]::WriteAllText($conflictResponsePath, $conflictResponseJson, (New-Object System.Text.UTF8Encoding($false)))

    $conflictSubmitted = Invoke-SiaoCut @("task", "submit", $conflictClaim.taskId, "--worker", "skill-e2e-conflict", "--response", $conflictResponsePath)
    if ($conflictSubmitted.patchSet.items[0].status -ne "conflict") { throw "Human edit did not produce a review conflict" }
    $conflictDiff = Invoke-SiaoCut @("task", "diff", $conflictClaim.taskId)
    if ($conflictDiff.patchSet.items[0].currentText -ne "Human edit wins.") { throw "Conflict diff is missing the current human text" }
    $kept = Invoke-SiaoCut @("task", "review-all", $conflictClaim.taskId, "--action", "keep")
    if ($kept.project.transcript.segments[0].text -ne "Human edit wins.") { throw "Keeping a conflict overwrote the human edit" }
    $conflictStatus = Invoke-SiaoCut @("workflow", "status", $conflictWorkflow.workflowId)
    if ($conflictStatus.workflow.status -ne "completed") { throw "Conflict workflow did not complete" }

    [pscustomobject]@{
        status = "ok"
        projectId = $project.projectId
        workflowId = $workflow.workflowId
        taskId = $claim.taskId
        conflictWorkflowId = $conflictWorkflow.workflowId
        conflictTaskId = $conflictClaim.taskId
    } | ConvertTo-Json
}
finally {
    $env:SIAOCUT_HOME = $previousHome
    $env:SIAOCUT_SERVICE_IDLE_MS = $previousIdle
    if (Test-Path -LiteralPath $testHome) { Remove-Item -LiteralPath $testHome -Recurse -Force }
}
