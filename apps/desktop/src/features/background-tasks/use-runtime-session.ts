import { useEffect,useState } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { localFileAvailable,openLogDirectory,pickModel,selectAsrBackend } from "../../domains/desktop-platform-client";
import { tr } from "../../i18n";
import type { ModelStatus,RuntimeInfo } from "../../types";
import type { useBackgroundSession } from "./use-background-session";
type Inputs = Pick<ReturnType<typeof useBackgroundSession>, "modelJob" | "setModelJob"> & {setNotice:(value:string|null)=>void;withBusy:(label:string, action:()=>Promise<void>)=>Promise<void>};
/** Owns runtime discovery and selected local model; downloads remain persisted background jobs. */
export function useRuntimeSession({modelJob,setModelJob,setNotice,withBusy}:Inputs) {
    const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
    const [models, setModels] = useState<ModelStatus[]>([]);
    const [modelPath, setModelPath] = useState<string | null>(() => localStorage.getItem("siaocut.modelPath"));
    const [modelPathAvailable, setModelPathAvailable] = useState(false);
    useEffect(() => {
        let cancelled = false;
        if (!modelPath) {
            setModelPathAvailable(false);
            return;
        }
        void localFileAvailable(modelPath).then((available) => {
            if (!cancelled)
                setModelPathAvailable(available);
        }).catch(() => {
            if (!cancelled)
                setModelPathAvailable(false);
        });
        return () => {
            cancelled = true;
        };
    }, [modelPath]);
    const changeAsrBackend = (backend: "cpu" | "vulkan") => withBusy(tr("app.s0113"), async () => {
        const next = await selectAsrBackend(backend);
        setRuntime(next);
        setNotice(backend === "vulkan" ? tr("app.s0114") : tr("app.s0115"));
    });
    const openDiagnostics = () => withBusy(tr("app.s0116"), async () => {
        await openLogDirectory();
        setNotice(tr("app.s0117"));
    });
    const chooseModel = () => withBusy(tr("app.s0214"), async () => {
        const path = await pickModel();
        if (!path)
            return;
        if (!await localFileAvailable(path))
            throw new Error(tr("app.capability.modelRequired"));
        localStorage.setItem("siaocut.modelPath", path);
        setModelPath(path);
        setModelPathAvailable(true);
        setNotice(tr("app.s0215"));
    });
    const installModel = (modelId: string) => withBusy(tr("app.s0216"), async () => {
        const envelope = await backgroundTaskClient.installModel(modelId);
        if (!envelope.modelJob)
            throw new Error(tr("app.s0217"));
        setModelJob(envelope.modelJob);
        if (envelope.modelJob.status === "completed") {
            const catalog = await backgroundTaskClient.listModels();
            const available = catalog.models ?? [];
            setModels(available);
            const installed = available.find((item) => item.id === modelId);
            if (installed) {
                localStorage.setItem("siaocut.modelPath", installed.path);
                setModelPath(installed.path);
                setModelPathAvailable(installed.installed && installed.verified === true);
            }
            setNotice(tr("app.s0057"));
            return;
        }
        setNotice(tr("app.s0218"));
    });
    const cancelModel = () => modelJob && withBusy(tr("app.s0219"), async () => {
        const envelope = await backgroundTaskClient.cancelModel(modelJob.id);
        if (envelope.modelJob)
            setModelJob(envelope.modelJob);
    });
    const removeModel = (modelId: string) => withBusy(tr("app.s0220"), async () => {
        await backgroundTaskClient.removeModel(modelId);
        const catalog = await backgroundTaskClient.listModels();
        const available = catalog.models ?? [];
        setModels(available);
        const selected = models.find((item) => item.id === modelId)?.path;
        if (selected && selected === modelPath) {
            localStorage.removeItem("siaocut.modelPath");
            setModelPath(null);
            setModelPathAvailable(false);
        }
        setNotice(tr("app.s0221"));
    });
    return { runtime, setRuntime, models, setModels, modelPath, setModelPath, modelPathAvailable, setModelPathAvailable, changeAsrBackend, openDiagnostics, chooseModel, installModel, cancelModel, removeModel };
}
