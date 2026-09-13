import { useRef,useState,type Dispatch,type RefObject,type SetStateAction } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { localFileAvailable,pickMedia,pickVideoPath } from "../../domains/desktop-platform-client";
import { tr,type UiLocale } from "../../i18n";
import type { AutoWorkflow,RuntimeInfo,SourcePreview,TranscriptionLanguage,WorkflowProfile } from "../../types";
import type { AiExecutionSelection } from "../ai-assistance/types";
import { AUTO_WORKFLOW_DISMISSED_STORAGE_KEY,parseDismissedAutoWorkflowIds,selectAutoWorkflowSnapshot,upsertAutoWorkflowSnapshot } from "./auto-workflow-snapshots";
type Options = {
  runtime: RuntimeInfo | null; modelPath: string | null; modelPathAvailable: boolean;
  setModelPathAvailable: (available: boolean) => void;
  transcriptionLanguage: TranscriptionLanguage; uiLocale: UiLocale;
  activeProjectIdRef: RefObject<string | null>;
  getWorkflow: () => AutoWorkflow | null;
  setWorkflow: Dispatch<SetStateAction<AutoWorkflow | null>>;
  setWorkflows: Dispatch<SetStateAction<AutoWorkflow[]>>;
  openProject: (id: string) => Promise<void>;
  setNotice: (notice: string | null) => void;
};
/** Owns setup intent and workflow actions; persisted execution snapshots stay in the task session. */
export function useAutoWorkflowSession(options: Options) {
    const busyRef = useRef(false);
    const sourceRevision = useRef(0);
    const [showAutoWorkflow, setShowAutoWorkflow] = useState(false);
    const [trackedAutoWorkflowIds, setTrackedAutoWorkflowIds] = useState<string[]>([]);
    const [dismissedAutoWorkflowIds, setDismissedAutoWorkflowIds] = useState<string[]>(() => parseDismissedAutoWorkflowIds(localStorage.getItem(AUTO_WORKFLOW_DISMISSED_STORAGE_KEY)));
    const [autoInputKind, setAutoInputKind] = useState<"local" | "url">("local");
    const [autoMediaPath, setAutoMediaPath] = useState("");
    const [autoUrl, updateAutoUrl] = useState("");
    const [autoSourcePreview, setAutoSourcePreview] = useState<SourcePreview | null>(null);
    const [autoAuthorized, setAutoAuthorized] = useState(false);
    const [autoTranslate, setAutoTranslate] = useState(false); const [autoProfile, setAutoProfile] = useState<WorkflowProfile>("balanced");
    const [autoAiSelection, setAutoAiSelection] = useState<AiExecutionSelection | null>(null);
    const [autoTranslationLanguage, setAutoTranslationLanguage] = useState("en");
    const [autoBurnSubtitles, setAutoBurnSubtitles] = useState(true);
    const [autoSubtitleMode, setAutoSubtitleMode] = useState<"source" | "translated" | "bilingual">("source");
    const [autoBusy, setAutoBusy] = useState<string | null>(null);
    const [autoError, setAutoError] = useState<string | null>(null);
    const [autoWorkflowErrors, setAutoWorkflowErrors] = useState<Record<string, string>>({});
    const autoWorkflowOriginProjectIdsRef = useRef(new Map<string, string | null>());
    const setAutoUrl = (url: string) => {
        sourceRevision.current += 1;
        updateAutoUrl(url);
        setAutoSourcePreview(null);
        setAutoAuthorized(false);
    };
    const withAutoBusy = async (
        label: string,
        action: () => Promise<void>,
        workflowId?: string,
    ) => {
        if (busyRef.current) return;
        busyRef.current = true;
        setAutoBusy(label);
        if (workflowId) {
            setAutoWorkflowErrors((current) => {
                if (!(workflowId in current))
                    return current;
                const next = { ...current };
                delete next[workflowId];
                return next;
            });
        }
        else {
            setAutoError(null);
        }
        try {
            await action();
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            if (workflowId)
                setAutoWorkflowErrors((current) => ({ ...current, [workflowId]: message }));
            else
                setAutoError(message);
        }
        finally {
            busyRef.current = false;
            setAutoBusy(null);
        }
    };
    const chooseAutoMedia = () => withAutoBusy(tr("app.s0096"), async () => {
        const path = await pickMedia();
        if (path)
            setAutoMediaPath(path);
    });
    const inspectAutoSource = () => withAutoBusy(tr("app.s0083"), async () => {
        const revision = sourceRevision.current;
        if (!options.runtime?.ytDlpConfigured)
            throw new Error(tr("app.s0084"));
        if (!autoUrl.trim())
            throw new Error(tr("app.s0085"));
        const envelope = await backgroundTaskClient.inspectSource(autoUrl.trim());
        if (revision !== sourceRevision.current) return;
        if (!envelope.source)
            throw new Error(tr("app.s0086"));
        setAutoSourcePreview(envelope.source);
        setAutoAuthorized(false);
    });
    const showAutoWorkflowStatus = (target: AutoWorkflow) => {
        setTrackedAutoWorkflowIds((current) => current.includes(target.id) ? current : [...current, target.id]);
        setDismissedAutoWorkflowIds((current) => current.filter((id) => id !== target.id));
        options.setWorkflow({ ...target });
    };
    const dismissAutoWorkflowStatus = (target: AutoWorkflow) => {
        setTrackedAutoWorkflowIds((current) => current.filter((id) => id !== target.id));
        setDismissedAutoWorkflowIds((current) => current.includes(target.id) ? current : [...current, target.id]);
    };
    const startAutoWorkflow = () => withAutoBusy(tr("app.s0097"), async () => {
        const originProjectId = options.activeProjectIdRef.current;
        if (!options.modelPath || !options.modelPathAvailable || !await localFileAvailable(options.modelPath)) {
            options.setModelPathAvailable(false);
            throw new Error(tr("app.s0098"));
        }
        if (autoTranslate && !autoTranslationLanguage.trim())
            throw new Error(tr("app.s0099"));
        if (autoInputKind === "local" && !autoMediaPath)
            throw new Error(tr("app.s0100"));
        if (autoInputKind === "url" && (!autoSourcePreview || !autoAuthorized))
            throw new Error(tr("app.s0101"));
        const output = await pickVideoPath(autoSourcePreview?.title ?? tr("app.s0102"));
        if (!output)
            return;
        const input = autoInputKind === "local"
            ? { kind: "local" as const, mediaPath: autoMediaPath, title: tr("app.s0103") }
            : { kind: "url" as const, url: autoSourcePreview!.originalUrl, confirmedMediaId: autoSourcePreview!.siteMediaId };
        const envelope = await backgroundTaskClient.startAutoWorkflow({
            input,
            modelPath: options.modelPath,
            language: options.transcriptionLanguage,
            locale: options.uiLocale,
            output,
            subtitleMode: autoTranslate ? autoSubtitleMode : "source", profile: autoProfile,
            translationLanguage: autoTranslate ? autoTranslationLanguage : undefined,
            burnSubtitles: autoBurnSubtitles,
            aiExecution: autoTranslate ? autoAiSelection ?? undefined : undefined,
        });
        if (!envelope.workflow)
            throw new Error(tr("app.s0104"));
        autoWorkflowOriginProjectIdsRef.current.set(envelope.workflow.id, originProjectId);
        options.setWorkflows((current) => upsertAutoWorkflowSnapshot(current, { ...envelope.workflow! }));
        showAutoWorkflowStatus(envelope.workflow);
        options.setWorkflow({ ...envelope.workflow });
        setShowAutoWorkflow(false);
        options.setNotice(tr("app.s0105"));
    });
    const cancelAutoWorkflow = (target: AutoWorkflow | null = options.getWorkflow()) => {
        if (!target)
            return;
        options.setWorkflow({ ...target });
        return withAutoBusy(tr("app.s0106"), async () => {
        const envelope = await backgroundTaskClient.cancelAutoWorkflow(target.id);
        if (!envelope.workflow)
            throw new Error(tr("app.s0107"));
        options.setWorkflows((current) => upsertAutoWorkflowSnapshot(current, { ...envelope.workflow! }));
        options.setWorkflow((current) => selectAutoWorkflowSnapshot(current, envelope.workflow!));
        options.setNotice(tr("app.s0108"));
        }, target.id);
    };
    const continueAutoWorkflow = (target: AutoWorkflow | null = options.getWorkflow()) => {
        if (!target)
            return;
        showAutoWorkflowStatus(target);
        return withAutoBusy(tr("app.s0109"), async () => {
        const envelope = await backgroundTaskClient.continueAutoWorkflow(target.id);
        if (!envelope.workflow)
            throw new Error(tr("app.s0110"));
        options.setWorkflows((current) => upsertAutoWorkflowSnapshot(current, { ...envelope.workflow! }));
        showAutoWorkflowStatus(envelope.workflow);
        options.setWorkflow((current) => selectAutoWorkflowSnapshot(current, envelope.workflow!));
        options.setNotice(tr("app.s0111", { "0": envelope.workflow.attemptCount }));
        }, target.id);
    };
    const openAutoProject = (target: AutoWorkflow | null = options.getWorkflow()) => {
        if (!target?.projectId)
            return;
        options.setWorkflow({ ...target });
        return withAutoBusy(tr("app.s0112"), async () => {
            await options.openProject(target.projectId!);
        }, target.id);
    };
    return { showAutoWorkflow, setShowAutoWorkflow, trackedAutoWorkflowIds, setTrackedAutoWorkflowIds, dismissedAutoWorkflowIds, setDismissedAutoWorkflowIds, autoInputKind, setAutoInputKind, autoMediaPath, setAutoMediaPath, autoUrl, setAutoUrl, autoSourcePreview, setAutoSourcePreview, autoAuthorized, setAutoAuthorized, autoTranslate, setAutoTranslate, autoProfile, setAutoProfile, autoAiSelection, setAutoAiSelection, autoTranslationLanguage, setAutoTranslationLanguage, autoBurnSubtitles, setAutoBurnSubtitles, autoSubtitleMode, setAutoSubtitleMode, autoBusy, setAutoBusy, autoError, setAutoError, autoWorkflowErrors, setAutoWorkflowErrors, autoWorkflowOriginProjectIdsRef, chooseAutoMedia, inspectAutoSource, showAutoWorkflowStatus, dismissAutoWorkflowStatus, startAutoWorkflow, cancelAutoWorkflow, continueAutoWorkflow, openAutoProject };
}
