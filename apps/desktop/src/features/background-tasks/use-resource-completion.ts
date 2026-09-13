import { useEffect } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { runtimeInfo } from "../../domains/desktop-platform-client";
import { localResourceClient } from "../../domains/local-resource-client";
import { tr } from "../../i18n";
import type { ModelStatus,RuntimeInfo } from "../../types";
import { localCapabilityLabel,localResourceError,RESOURCE_SETUP_DEFERRED_KEY } from "./resource-messages";
import type { useBackgroundSession } from "./use-background-session";
import type { useResourceSession } from "./use-resource-session";
import type { useSpeakerSession } from "./use-speaker-session";
type Inputs = Pick<ReturnType<typeof useResourceSession>, "handledResourceJobRef" | "setResourceError" | "setLocalResources" | "setShowResourceSetup" | "setResourceSelectedRoot" | "pendingResourceAction" | "setPendingResourceAction" | "resourceSetupReason">
 & Pick<ReturnType<typeof useBackgroundSession>, "resourceJob" | "setResourceJob">
 & Pick<ReturnType<typeof useSpeakerSession>, "setSpeakerPackage">
 & {models:ModelStatus[]; setModels:(value:ModelStatus[])=>void; setRuntime:(value:RuntimeInfo)=>void;
 setModelPath:(value:string|null)=>void; setModelPathAvailable:(value:boolean)=>void; setNotice:(value:string|null)=>void;
 setShowSourceImport:(value:boolean)=>void; setResumeSourceInspection:(value:boolean)=>void; setResumeLocalTranscription:(value:boolean)=>void; setShowRuntime:(value:boolean)=>void;};
/** A completed resource job refreshes its capabilities, then resumes the original user action. */
export function useResourceCompletion({resourceJob, handledResourceJobRef, setResourceError, setLocalResources, setRuntime, models, setModels, setSpeakerPackage, setModelPath, setModelPathAvailable, setResourceJob, setShowResourceSetup, setResourceSelectedRoot, setNotice, pendingResourceAction, setPendingResourceAction, setShowSourceImport, setResumeSourceInspection, setResumeLocalTranscription, resourceSetupReason, setShowRuntime}:Inputs) {
    useEffect(() => {
        if (!resourceJob || handledResourceJobRef.current === resourceJob.id)
            return;
        if (["failed", "interrupted"].includes(resourceJob.status)) {
            setResourceError(localResourceError(new Error(`${resourceJob.errorCode ?? "resource_job_state_changed"}: resource preparation failed`)));
            return;
        }
        if (resourceJob.status !== "completed")
            return;
        handledResourceJobRef.current = resourceJob.id;
        let stale = false;
        void Promise.allSettled([
            localResourceClient.status(),
            runtimeInfo(),
            backgroundTaskClient.listModels(true),
            backgroundTaskClient.getSpeakerPackage(),
        ]).then(([resourceResult, runtimeResult, modelsResult, speakerResult]) => {
            if (stale) return;
            if (resourceResult.status === "rejected")
                throw resourceResult.reason;
            if (runtimeResult.status === "rejected")
                throw runtimeResult.reason;
            const resourceEnvelope = resourceResult.value;
            const nextRuntime = runtimeResult.value;
            if (resourceEnvelope.localResources)
                setLocalResources(resourceEnvelope.localResources);
            setRuntime(nextRuntime);
            const nextModels = modelsResult.status === "fulfilled"
                ? modelsResult.value.models ?? []
                : models;
            if (modelsResult.status === "fulfilled")
                setModels(nextModels);
            if (speakerResult.status === "fulfilled")
                setSpeakerPackage(speakerResult.value.speakerPackage ?? null);
            const nextModelPath = nextRuntime.defaultModelAvailable
                ? nextRuntime.defaultModelPath
                : nextModels.find((model) => model.installed && model.verified === true)?.path ?? null;
            setModelPath(nextModelPath);
            setModelPathAvailable(Boolean(nextModelPath));
            if (nextModelPath)
                localStorage.setItem("siaocut.modelPath", nextModelPath);
            else
                localStorage.removeItem("siaocut.modelPath");
            setResourceJob(null);
            setShowResourceSetup(false);
            setResourceSelectedRoot("");
            setResourceError(null);
            localStorage.removeItem(RESOURCE_SETUP_DEFERRED_KEY);
            setNotice(tr("app.resources.readyNotice", { capability: localCapabilityLabel(resourceJob.capabilityId) }));
            if (pendingResourceAction === "inspect_url") {
                setPendingResourceAction(null);
                setShowSourceImport(true);
                setResumeSourceInspection(true);
            }
            else if (pendingResourceAction === "transcribe") {
                setPendingResourceAction(null);
                setResumeLocalTranscription(true);
            }
            else if (resourceSetupReason === "manage") {
                setShowRuntime(true);
            }
        }).catch((cause) => { if (!stale) setResourceError(localResourceError(cause)); });
        return () => { stale = true; };
    }, [resourceJob?.id, resourceJob?.status]);
}
