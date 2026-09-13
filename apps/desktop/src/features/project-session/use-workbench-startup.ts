import { useCallback,useEffect,useRef } from "react";
import { agentReviewClient } from "../../domains/agent-review-client";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { localFileAvailable,runtimeInfo,updaterPolicy } from "../../domains/desktop-platform-client";
import { localResourceClient } from "../../domains/local-resource-client";
import { projectSessionClient } from "../../domains/project-session-client";
import type { ProjectPage } from "../../generated/core-contract";
import { tr } from "../../i18n";
import type { AutoWorkflow,CodexHealth,ModelStatus,RuntimeInfo,UpdatePolicy } from "../../types";
import { ACTIONABLE_AUTO_WORKFLOW_STATUSES } from "../background-tasks/auto-workflow-snapshots";
import { RESOURCE_SETUP_DEFERRED_KEY } from "../background-tasks/resource-messages";
import type { useAutoWorkflowSession } from "../background-tasks/use-auto-workflow-session";
import type { useBackgroundSession } from "../background-tasks/use-background-session";
import type { useResourceSession } from "../background-tasks/use-resource-session";
import type { useSpeakerSession } from "../background-tasks/use-speaker-session";
import type { useTranscriptionReviewSession } from "../background-tasks/use-transcription-review-session";
type Inputs = Pick<ReturnType<typeof useBackgroundSession>, "setAutoWorkflows" | "setAutoWorkflow" | "setModelJob" | "setSpeakerJobs" | "setSpeakerJob" | "setResourceJob" | "setSourceJob">
  & Pick<ReturnType<typeof useResourceSession>, "setLocalResources" | "setResourceProfile" | "setResourceCapability" | "setResourceSetupReason" | "setResourcePlan" | "setShowResourceSetup">
  & Pick<ReturnType<typeof useSpeakerSession>, "setSpeakerPackage">
  & Pick<ReturnType<typeof useTranscriptionReviewSession>, "setTranscriptionHealth" | "setTranscriptionConfig">
  & Pick<ReturnType<typeof useAutoWorkflowSession>, "setTrackedAutoWorkflowIds">
  & {setBusy:(value:string|null)=>void; setError:(value:string|null)=>void; setUpdatePolicy:(value:UpdatePolicy)=>void;
    setModels:(value:ModelStatus[])=>void; setRuntime:(value:RuntimeInfo)=>void; setCodexHealth:(value:CodexHealth|null)=>void;
    setModelPath:(value:string|null)=>void; setModelPathAvailable:(value:boolean)=>void;
    replaceProjectPage:(page:ProjectPage)=>void; restoreProject:(page:ProjectPage)=>Promise<void>; flush:()=>Promise<void>};
/** Serial startup/refresh with draft flushing and a cancellation fence; domains retain state ownership. */
export function useWorkbenchStartup(inputs: Inputs) {
  const latest = useRef(inputs); latest.current = inputs;
  const mounted = useRef(false);
  const pending = useRef<Promise<void> | null>(null);
  const initialize = useCallback(() => {
    if (pending.current) return pending.current;
    const run = hydrate(latest.current, () => mounted.current).catch(error => {if(mounted.current) latest.current.setError(String(error));})
      .finally(() => {pending.current=null;if(mounted.current) latest.current.setBusy(null);});
    pending.current=run; return run;
  }, []);
  useEffect(() => {mounted.current=true;void initialize();return () => {mounted.current=false;};}, [initialize]);
  return initialize;
}
async function hydrate(inputs:Inputs, current:()=>boolean) {
  const {setBusy, setError, setUpdatePolicy, setAutoWorkflows, setAutoWorkflow, setTrackedAutoWorkflowIds, setModels, setModelJob, setSpeakerPackage, setSpeakerJobs, setSpeakerJob, setTranscriptionHealth, setTranscriptionConfig, setCodexHealth, setLocalResources, setResourceProfile, setResourceCapability, setResourceSetupReason, setResourcePlan, setShowResourceSetup, setResourceJob, setSourceJob, setRuntime, setModelPath, setModelPathAvailable, replaceProjectPage, restoreProject, flush} = inputs;
        setBusy(tr("app.s0039"));
        setError(null);
        await flush();
        if (!current()) return;
        const [projectsResult, runtimeResult, modelsResult, modelJobsResult, sourceJobsResult, autoWorkflowsResult, updatePolicyResult, speakerPackageResult, speakerJobsResult, transcriptionHealthResult, codexHealthResult, localResourcesResult, resourceJobsResult, recommendedPlanResult] = await Promise.allSettled([
            projectSessionClient.listProjects(),
            runtimeInfo(),
            backgroundTaskClient.listModels(true),
            backgroundTaskClient.listModelJobs(),
            backgroundTaskClient.listSourceJobs(),
            backgroundTaskClient.listAutoWorkflows(),
            updaterPolicy(),
            backgroundTaskClient.getSpeakerPackage(),
            backgroundTaskClient.listSpeakerJobs(),
            backgroundTaskClient.getTranscriptionHealth(),
            agentReviewClient.getCodexHealth(),
            localResourceClient.status(),
            localResourceClient.listJobs(),
            localResourceClient.plan("basic_media"),
        ]);
        if (!current()) return;
        const errors: string[] = [];
        let activeAutoWorkflow: AutoWorkflow | null = null;
        const autoWorkflowSourceIds = new Set<string>();
        if (updatePolicyResult.status === "fulfilled")
            setUpdatePolicy(updatePolicyResult.value);
        if (autoWorkflowsResult.status === "fulfilled") {
            const workflows = autoWorkflowsResult.value.workflows ?? [];
            activeAutoWorkflow = workflows.find((item) => ["queued", "running", "needs_agent", "awaiting_authorization", "needs_review", "failed", "interrupted"].includes(item.status)) ?? null;
            workflows.forEach((workflow) => {
                if (workflow.sourceImportId)
                    autoWorkflowSourceIds.add(workflow.sourceImportId);
            });
            setAutoWorkflows(workflows);
            setAutoWorkflow(activeAutoWorkflow);
            setTrackedAutoWorkflowIds((current) => Array.from(new Set([
                ...current,
                ...workflows.filter((workflow) => ACTIONABLE_AUTO_WORKFLOW_STATUSES.has(workflow.status)).map((workflow) => workflow.id),
            ])));
        }
        else {
            errors.push(tr("app.s0040", { "0": autoWorkflowsResult.reason instanceof Error ? autoWorkflowsResult.reason.message : String(autoWorkflowsResult.reason) }));
        }
        let managedModelPath: string | null = null;
        if (modelsResult.status === "fulfilled") {
            const available = modelsResult.value.models ?? [];
            setModels(available);
            managedModelPath = available.find((item) => item.installed && item.verified === true && item.recommended)?.path
                ?? available.find((item) => item.installed && item.verified === true)?.path
                ?? null;
        }
        else {
            errors.push(tr("app.s0041", { "0": modelsResult.reason instanceof Error ? modelsResult.reason.message : String(modelsResult.reason) }));
        }
        if (modelJobsResult.status === "fulfilled") {
            setModelJob(modelJobsResult.value.modelJobs?.find((item) => ["queued", "running"].includes(item.status)) ?? null);
        }
        if (speakerPackageResult.status === "fulfilled") {
            setSpeakerPackage(speakerPackageResult.value.speakerPackage ?? null);
        }
        else {
            errors.push(tr("app.s0042", { "0": speakerPackageResult.reason instanceof Error ? speakerPackageResult.reason.message : String(speakerPackageResult.reason) }));
        }
        if (speakerJobsResult.status === "fulfilled") {
            const jobs = speakerJobsResult.value.speakerJobs ?? [];
            setSpeakerJobs(jobs);
            setSpeakerJob(jobs.find((item) => ["queued", "running"].includes(item.status)) ?? jobs[0] ?? null);
        }
        if (transcriptionHealthResult.status === "fulfilled" && transcriptionHealthResult.value.providerHealth) {
            const next = transcriptionHealthResult.value.providerHealth;
            setTranscriptionHealth(next);
            setTranscriptionConfig({ providerId: next.providerId, endpoint: next.endpoint, modelId: next.modelId, updatedAt: next.checkedAt });
        }
        setCodexHealth(codexHealthResult.status === "fulfilled" ? codexHealthResult.value.codex ?? null : null);
        if (localResourcesResult.status === "fulfilled" && localResourcesResult.value.localResources) {
            const next = localResourcesResult.value.localResources;
            setLocalResources(next);
            setResourceProfile(next.transcriptionProfile);
            if (!next.configured && localStorage.getItem(RESOURCE_SETUP_DEFERRED_KEY) !== "1") {
                setResourceCapability("basic_media");
                setResourceSetupReason("first_run");
                setResourcePlan(recommendedPlanResult.status === "fulfilled" ? recommendedPlanResult.value.resourcePlan ?? null : null);
                setShowResourceSetup(true);
            }
        }
        else {
            errors.push(tr("app.resources.error.generic"));
        }
        if (resourceJobsResult.status === "fulfilled") {
            const jobs = resourceJobsResult.value.resourceJobs ?? [];
            setResourceJob(jobs.find((item) => ["queued", "running"].includes(item.status)) ?? null);
        }
        if (sourceJobsResult.status === "fulfilled") {
            const jobs = (sourceJobsResult.value.sourceJobs ?? []).filter((item) => !autoWorkflowSourceIds.has(item.id));
            setSourceJob(jobs.find((item) => ["queued", "running", "finalizing"].includes(item.status)) ?? jobs[0] ?? null);
        }
        else {
            errors.push(tr("app.s0043", { "0": sourceJobsResult.reason instanceof Error ? sourceJobsResult.reason.message : String(sourceJobsResult.reason) }));
        }
        if (runtimeResult.status === "fulfilled") {
            setRuntime(runtimeResult.value);
            const stored = localStorage.getItem("siaocut.modelPath");
            const candidates = Array.from(new Set([
                stored,
                managedModelPath,
                runtimeResult.value.defaultModelAvailable ? runtimeResult.value.defaultModelPath : null,
            ].filter((value): value is string => Boolean(value))));
            let nextModelPath: string | null = null;
            for (const candidate of candidates) {
                const managedCandidate = modelsResult.status === "fulfilled"
                    ? (modelsResult.value.models ?? []).find((model) => model.path === candidate)
                    : undefined;
                const available = managedCandidate
                    ? managedCandidate.installed && managedCandidate.verified === true
                    : await localFileAvailable(candidate);
                if (available) {
                    nextModelPath = candidate;
                    break;
                }
            }
            if (!current()) return;
            setModelPath(nextModelPath);
            setModelPathAvailable(Boolean(nextModelPath));
            if (nextModelPath)
                localStorage.setItem("siaocut.modelPath", nextModelPath);
            else
                localStorage.removeItem("siaocut.modelPath");
        }
        else {
            errors.push(tr("app.s0044", { "0": runtimeResult.reason instanceof Error ? runtimeResult.reason.message : String(runtimeResult.reason) }));
        }
        if (projectsResult.status === "fulfilled") {
            replaceProjectPage(projectsResult.value);
            try { await restoreProject(projectsResult.value); }
            catch (cause) { errors.push(tr("app.s0045", { "0": String(cause) })); }
        }
        else {
            errors.push(tr("app.s0046", { "0": projectsResult.reason instanceof Error ? projectsResult.reason.message : String(projectsResult.reason) }));
        }
        setError(errors.length ? errors.join(" ") : null);
}
