import { useRef,useState,type RefObject } from "react";
import { isHttpsSourceUrl } from "../../app-view-model";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { tr } from "../../i18n";
import type { LocalResourceStatus,RuntimeInfo,SourceBrowser,SourceImportJob,SourcePreview } from "../../types";
type Options = {
  localResources: LocalResourceStatus | null;
  runtime: RuntimeInfo | null;
  activeProjectIdRef: RefObject<string | null>;
  getJob: () => SourceImportJob | null;
  setJob: (job: SourceImportJob | null) => void;
  setNotice: (notice: string | null) => void;
  prepareResources: () => Promise<void>;
};
/** Owns import consent and dialog state; persisted jobs remain in the background session. */
export function useSourceImportSession(options: Options) {
    const busyRef = useRef(false);
    const previewRevision = useRef(0);
    const [sourcePreview, setSourcePreview] = useState<SourcePreview | null>(null);
    const [sourceUrl, updateSourceUrl] = useState("");
    const [sourceAuthorized, setSourceAuthorized] = useState(false);
    const [sourceAuthMode, updateSourceAuthMode] = useState<"anonymous" | "browser">("anonymous");
    const [sourceBrowser, updateSourceBrowser] = useState<SourceBrowser>("chrome");
    const [sourceBrowserAuthorized, updateSourceBrowserAuthorized] = useState(false);
    const [sourceBusy, setSourceBusy] = useState<string | null>(null);
    const [sourceError, setSourceError] = useState<string | null>(null);
    const [showSourceImport, setShowSourceImport] = useState(false);
    const sourceJobOriginProjectIdsRef = useRef(new Map<string, string | null>());
    const invalidatePreview = () => {
        previewRevision.current += 1;
        setSourcePreview(null);
        setSourceAuthorized(false);
    };
    const setSourceUrl = (value: string) => { invalidatePreview(); updateSourceUrl(value); };
    const setSourceAuthMode = (value: "anonymous" | "browser") => { invalidatePreview(); updateSourceAuthMode(value); };
    const setSourceBrowser = (value: SourceBrowser) => { invalidatePreview(); updateSourceBrowser(value); };
    const setSourceBrowserAuthorized = (value: boolean) => { invalidatePreview(); updateSourceBrowserAuthorized(value); };
    const withSourceBusy = async (label: string, action: () => Promise<void>) => {
        if (busyRef.current) return;
        busyRef.current = true;
        setSourceBusy(label);
        setSourceError(null);
        try {
            await action();
        }
        catch (cause) {
            setSourceError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            busyRef.current = false;
            setSourceBusy(null);
        }
    };
    const inspectSource = () => withSourceBusy(tr("app.s0083"), async () => {
        const revision = previewRevision.current;
        const url = sourceUrl.trim();
        if (!isHttpsSourceUrl(url))
            throw new Error(tr("app.s0085"));
        const urlCapability = options.localResources?.capabilities.find((capability) => capability.id === "url_import");
        if (!["ready", "update_available"].includes(urlCapability?.state ?? "not_ready") || !options.runtime?.ytDlpConfigured) {
            await options.prepareResources();
            return;
        }
        if (sourceAuthMode === "browser" && !sourceBrowserAuthorized)
            throw new Error("source_browser_consent_required");
        const envelope = await backgroundTaskClient.inspectSource(url, sourceAuthMode === "browser" ? sourceBrowser : undefined);
        if (revision !== previewRevision.current) return;
        if (!envelope.source)
            throw new Error(tr("app.s0086"));
        setSourcePreview(envelope.source);
        options.setJob(null);
        setSourceAuthorized(false);
    });
    const startSourceImport = () => sourcePreview && withSourceBusy(tr("app.s0087"), async () => {
        if (!sourceAuthorized)
            throw new Error(tr("app.s0088"));
        const originProjectId = options.activeProjectIdRef.current;
        const envelope = await backgroundTaskClient.startSourceImport(sourcePreview.originalUrl, sourcePreview.siteMediaId, sourcePreview.browser ?? undefined);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0089"));
        sourceJobOriginProjectIdsRef.current.set(envelope.sourceJob.id, originProjectId);
        options.setJob(envelope.sourceJob);
        options.setNotice(tr("app.s0090"));
    });
    const cancelSourceImport = () => options.getJob() && withSourceBusy(tr("app.s0091"), async () => {
        const envelope = await backgroundTaskClient.cancelSourceImport(options.getJob()!.id);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0092"));
        options.setJob(envelope.sourceJob);
    });
    const resumeSourceImport = () => options.getJob() && withSourceBusy(tr("app.s0093"), async () => {
        const originProjectId = options.activeProjectIdRef.current;
        const envelope = await backgroundTaskClient.resumeSourceImport(options.getJob()!.id);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0094"));
        sourceJobOriginProjectIdsRef.current.set(envelope.sourceJob.id, originProjectId);
        options.setJob(envelope.sourceJob);
        options.setNotice(tr("app.s0095", { "0": envelope.sourceJob.attemptCount }));
    });
    const resetSourceImport = () => {
        if (options.getJob() && ["queued", "running", "finalizing"].includes(options.getJob()!.status))
            return;
        setSourcePreview(null);
        options.setJob(null);
        setSourceUrl("");
        setSourceAuthorized(false);
        setSourceBrowserAuthorized(false);
        setSourceError(null);
    };
    return { sourcePreview, setSourcePreview, sourceUrl, setSourceUrl, sourceAuthorized, setSourceAuthorized, sourceAuthMode, setSourceAuthMode, sourceBrowser, setSourceBrowser, sourceBrowserAuthorized, setSourceBrowserAuthorized, sourceBusy, setSourceBusy, sourceError, setSourceError, showSourceImport, setShowSourceImport, sourceJobOriginProjectIdsRef, inspectSource, startSourceImport, cancelSourceImport, resumeSourceImport, resetSourceImport };
}
