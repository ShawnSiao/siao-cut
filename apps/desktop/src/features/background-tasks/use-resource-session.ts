import { useRef,useState } from "react";
import { pickResourceDirectory,runtimeInfo } from "../../domains/desktop-platform-client";
import { localResourceClient } from "../../domains/local-resource-client";
import { tr } from "../../i18n";
import type { LocalCapabilityId,LocalResourceJob,LocalResourcePlan,LocalResourceStatus,LocalTranscriptionProfile,RuntimeInfo } from "../../types";
import { localCapabilityLabel,localResourceError,RESOURCE_SETUP_DEFERRED_KEY } from "./resource-messages";
type Options = {
  getJob: () => LocalResourceJob | null;
  setJob: (job: LocalResourceJob) => void;
  setRuntime: (runtime: RuntimeInfo) => void;
  setShowRuntime: (show: boolean) => void;
  setShowSourceImport: (show: boolean) => void;
  setNotice: (notice: string | null) => void;
};
/** Resource setup owns configuration and intent; execution snapshots belong to background tasks. */
export function useResourceSession(options: Options) {
    const planRevision = useRef(0);
    const [localResources, setLocalResources] = useState<LocalResourceStatus | null>(null);
    const [resourcePlan, setResourcePlan] = useState<LocalResourcePlan | null>(null);
    const [resourceCapability, setResourceCapability] = useState<LocalCapabilityId>("basic_media");
    const [resourceProfile, setResourceProfile] = useState<LocalTranscriptionProfile>("standard");
    const [resourceSetupReason, setResourceSetupReason] = useState<"first_run" | "on_demand" | "manage">("first_run");
    const [resourceSelectedRoot, setResourceSelectedRoot] = useState("");
    const [resourceBusy, setResourceBusy] = useState(false);
    const [resourceError, setResourceError] = useState<string | null>(null);
    const [showResourceSetup, setShowResourceSetup] = useState(false);
    const [pendingResourceAction, setPendingResourceAction] = useState<"inspect_url" | "transcribe" | null>(null);
    const handledResourceJobRef = useRef<string | null>(null);
    const openResourcePreparation = async (capability: LocalCapabilityId, reason: "first_run" | "on_demand" | "manage") => {
        const revision = ++planRevision.current;
        setResourcePlan(null);
        setResourceBusy(true);
        const profile = capability === "local_transcription" ? localResources?.transcriptionProfile ?? resourceProfile : undefined;
        setResourceCapability(capability);
        if (profile)
            setResourceProfile(profile);
        setResourceSetupReason(reason);
        setResourceSelectedRoot("");
        setResourceError(null);
        if (reason === "manage")
            options.setShowRuntime(false);
        if (reason === "on_demand")
            options.setShowSourceImport(false);
        setShowResourceSetup(true);
        try {
            const envelope = await localResourceClient.plan(capability, profile);
            if (revision === planRevision.current) setResourcePlan(envelope.resourcePlan ?? null);
        }
        catch (cause) {
            if (revision === planRevision.current) setResourceError(localResourceError(cause));
        }
        finally { if (revision === planRevision.current) setResourceBusy(false); }
    };
    const changeResourceProfile = async (profile: LocalTranscriptionProfile) => {
        const revision = ++planRevision.current;
        setResourcePlan(null);
        setResourceProfile(profile);
        setResourceBusy(true);
        setResourceError(null);
        try {
            const envelope = await localResourceClient.plan("local_transcription", profile);
            if (revision === planRevision.current) setResourcePlan(envelope.resourcePlan ?? null);
        }
        catch (cause) {
            if (revision === planRevision.current) setResourceError(localResourceError(cause));
        }
        finally {
            if (revision === planRevision.current) setResourceBusy(false);
        }
    };
    const chooseResourceLocation = async () => {
        const path = await pickResourceDirectory();
        if (path) {
            setResourceSelectedRoot(path);
            setResourceError(null);
        }
    };
    const confirmResourceLocation = async () => {
        if (!resourceSelectedRoot)
            return;
        setResourceBusy(true);
        setResourceError(null);
        try {
            const changingLocation = Boolean(localResources?.configured);
            const envelope = changingLocation
                ? await localResourceClient.migrate(resourceSelectedRoot)
                : await localResourceClient.configure(resourceSelectedRoot);
            if (!envelope.localResources)
                throw new Error("resource_setup_required");
            setLocalResources(envelope.localResources);
            setResourceSelectedRoot("");
            options.setRuntime(await runtimeInfo());
            localStorage.removeItem(RESOURCE_SETUP_DEFERRED_KEY);
            options.setNotice(tr(changingLocation ? "app.resources.locationMoved" : "app.resources.locationConfirmed"));
            if (changingLocation && resourceSetupReason === "manage") {
                setShowResourceSetup(false);
                options.setShowRuntime(true);
            }
        }
        catch (cause) {
            setResourceError(localResourceError(cause));
        }
        finally {
            setResourceBusy(false);
        }
    };
    const startResourcePreparation = async () => {
        if (!localResources?.configured || resourceSelectedRoot)
            return;
        setResourceBusy(true);
        setResourceError(null);
        handledResourceJobRef.current = null;
        try {
            const isUpdate = localResources.capabilities.some((capability) => capability.id === resourceCapability && capability.state === "update_available");
            const envelope = isUpdate
                ? await localResourceClient.update(resourceCapability, resourceCapability === "local_transcription" ? resourceProfile : undefined)
                : await localResourceClient.install(resourceCapability, resourceCapability === "local_transcription" ? resourceProfile : undefined);
            if (!envelope.resourceJob)
                throw new Error("resource_job_not_found");
            options.setJob(envelope.resourceJob);
            options.setNotice(tr("app.resources.preparingNotice", { capability: localCapabilityLabel(resourceCapability) }));
        }
        catch (cause) {
            setResourceError(localResourceError(cause));
        }
        finally {
            setResourceBusy(false);
        }
    };
    const cancelResourcePreparation = async () => {
        if (!options.getJob())
            return;
        setResourceBusy(true);
        try {
            const envelope = await localResourceClient.cancel(options.getJob()!.id);
            if (envelope.resourceJob)
                options.setJob(envelope.resourceJob);
        }
        catch (cause) {
            setResourceError(localResourceError(cause));
        }
        finally {
            setResourceBusy(false);
        }
    };
    const resumeResourcePreparation = async () => {
        if (!options.getJob())
            return;
        setResourceBusy(true);
        setResourceError(null);
        handledResourceJobRef.current = null;
        try {
            const envelope = await localResourceClient.resume(options.getJob()!.id);
            if (!envelope.resourceJob)
                throw new Error("resource_job_not_found");
            options.setJob(envelope.resourceJob);
        }
        catch (cause) {
            setResourceError(localResourceError(cause));
        }
        finally {
            setResourceBusy(false);
        }
    };
    const closeResourcePreparation = () => {
        if (options.getJob() && ["queued", "running"].includes(options.getJob()!.status))
            return;
        planRevision.current += 1;
        setResourceBusy(false);
        setShowResourceSetup(false);
        setResourceSelectedRoot("");
        setResourceError(null);
        if (resourceSetupReason === "first_run")
            localStorage.setItem(RESOURCE_SETUP_DEFERRED_KEY, "1");
        if (resourceSetupReason === "manage")
            options.setShowRuntime(true);
        if (pendingResourceAction === "inspect_url") {
            setPendingResourceAction(null);
            options.setShowSourceImport(true);
        }
        else if (pendingResourceAction === "transcribe") {
            setPendingResourceAction(null);
        }
    };
    return { localResources, setLocalResources, resourcePlan, setResourcePlan, resourceCapability, setResourceCapability, resourceProfile, setResourceProfile, resourceSetupReason, setResourceSetupReason, resourceSelectedRoot, setResourceSelectedRoot, resourceBusy, setResourceBusy, resourceError, setResourceError, showResourceSetup, setShowResourceSetup, pendingResourceAction, setPendingResourceAction, handledResourceJobRef, openResourcePreparation, changeResourceProfile, chooseResourceLocation, confirmResourceLocation, startResourcePreparation, cancelResourcePreparation, resumeResourcePreparation, closeResourcePreparation };
}
