import { useRef,useState } from "react";
import { pickSubtitleFile } from "../../domains/desktop-platform-client";
import { transcriptEditingClient } from "../../domains/transcript-editing-client";
import { tr } from "../../i18n";
import type { Project,SubtitleImportPreview } from "../../types";
import type { EditingSession } from "./editing-session";
type Inputs = {project:Project|null; editing:EditingSession; onApplied:(project:Project)=>Promise<void>;setNotice:(value:string|null)=>void};
export function useSubtitleImportSession({project, editing, onApplied, setNotice}:Inputs) {
    const command = useRef(false);
    const [showSubtitleImport, setShowSubtitleImport] = useState(false);
    const [subtitleImportPath, setSubtitleImportPath] = useState("");
    const [subtitleImportPreview, setSubtitleImportPreview] = useState<SubtitleImportPreview | null>(null);
    const [subtitleImportBusy, setSubtitleImportBusy] = useState<string | null>(null);
    const [subtitleImportError, setSubtitleImportError] = useState<string | null>(null);
    const [subtitleReplaceConfirmed, setSubtitleReplaceConfirmed] = useState(false);
    const openSubtitleImport = () => {
        setSubtitleImportPath("");
        setSubtitleImportPreview(null);
        setSubtitleImportError(null);
        setSubtitleReplaceConfirmed(false);
        setShowSubtitleImport(true);
    };
    const inspectSubtitleFile = async () => {
        if (!project || command.current) return;
        command.current = true;
        setSubtitleImportBusy(tr("app.s0167"));
        setSubtitleImportError(null);
        try {
            const path = await pickSubtitleFile();
            if (!path)
                return;
            setSubtitleImportPath(path);
            setSubtitleReplaceConfirmed(false);
            const envelope = await transcriptEditingClient.inspectSubtitleFile(project.id, path);
            if (!envelope.subtitleImportPreview)
                throw new Error(tr("app.s0168"));
            setSubtitleImportPreview(envelope.subtitleImportPreview);
        }
        catch (cause) {
            setSubtitleImportPreview(null);
            setSubtitleImportError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            command.current = false;
            setSubtitleImportBusy(null);
        }
    };
    const confirmSubtitleImport = async () => {
        if (!project || !subtitleImportPreview || !subtitleReplaceConfirmed || command.current)
            return;
        command.current = true;
        setSubtitleImportBusy(tr("app.s0169"));
        setSubtitleImportError(null);
        try {
            const envelope = await editing.mutate(project.id,{kind:"import_subtitle",path:subtitleImportPath,sha256:subtitleImportPreview.sha256,previewVersionId:subtitleImportPreview.expectedVersionId});
            if (!envelope.project)
                throw new Error(tr("app.s0170"));
            await onApplied(envelope.project);
            setShowSubtitleImport(false);
            setNotice(tr("app.s0171", { "0": envelope.project.transcript.segments.length }));
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            const versionMismatch = message.includes("subtitle_import_version_mismatch") || ["subtitle_import_version_mismatch","editing_version_conflict"].includes((cause as {code?:string}).code ?? "");
            setSubtitleImportError(versionMismatch ? tr("app.subtitleImport.versionMismatch") : message);
            if (versionMismatch) {
                setSubtitleImportPreview(null);
                setSubtitleReplaceConfirmed(false);
            }
        }
        finally {
            command.current = false;
            setSubtitleImportBusy(null);
        }
    };
    return { showSubtitleImport, setShowSubtitleImport, subtitleImportPath, setSubtitleImportPath, subtitleImportPreview, setSubtitleImportPreview, subtitleImportBusy, setSubtitleImportBusy, subtitleImportError, setSubtitleImportError, subtitleReplaceConfirmed, setSubtitleReplaceConfirmed, openSubtitleImport, inspectSubtitleFile, confirmSubtitleImport };
}
