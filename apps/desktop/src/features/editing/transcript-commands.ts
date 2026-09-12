import type { RefObject } from "react";
import { transcriptEditingClient } from "../../domains/transcript-editing-client";
import { tr } from "../../i18n";
import type { CutPreview,Project,Segment } from "../../types";
import { fieldKey,type EditingSession } from "./editing-session";
type Inputs = {
 project:Project|null; selected:Segment|null; selectedWords:Project["transcript"]["words"];activeWordRange:{start:number;end:number}|null;cutPadding:number;
 selectedSubtitleLanguage:string;search:string;replacement:string;emptyReplacementConfirmed:boolean;setEmptyReplacementConfirmed:(value:boolean)=>void;
 setWordRange:(value:null)=>void;videoRef:RefObject<HTMLVideoElement|null>;setCutPreview:(value:CutPreview|null)=>void;editing:EditingSession;
 withBusy:(label:string, action:()=>Promise<void>)=>Promise<void>;setNotice:(value:string|null)=>void;refreshProject:(id:string, media?:boolean)=>Promise<Project>;
 refreshSpeakerTrack:(id:string)=>Promise<void>;refreshTranscription:(id:string)=>Promise<void>;
};
/** Transcript, cut and history commands share the project editing queue and explicit refresh ports. */
export function createTranscriptCommands({project, selected, selectedWords, activeWordRange, cutPadding, selectedSubtitleLanguage, search, replacement, emptyReplacementConfirmed, setEmptyReplacementConfirmed, setWordRange, videoRef, setCutPreview, withBusy, setNotice, refreshProject, refreshSpeakerTrack, refreshTranscription, editing}:Inputs) {
    const editSegment = async (segment: Segment, text: string) => {
        if (!project) return;
        const key = fieldKey(project.id, segment.id, "source");
        if (editing.state(key)?.draft.text !== text) editing.change(key, text);
        await editing.save(key, true);
    };
    const editTranslationSegment = async (segment: Segment, text: string) => {
        if (!project || !selectedSubtitleLanguage) return;
        const key = fieldKey(project.id, segment.id, `translation:${selectedSubtitleLanguage}`);
        if (editing.state(key)?.draft.text !== text) editing.change(key, text);
        await editing.save(key, true);
    };
    const replaceAll = () => project && search && (replacement || emptyReplacementConfirmed) && withBusy(tr("app.s0155"), async () => {
        const result = await editing.mutate(project.id, { kind: "replace", search, replacement });
        await refreshProject(project.id);
        setEmptyReplacementConfirmed(false);
        setNotice(Number(result.changedSegments ?? 0) === 0 ? tr("app.s0156") : tr("app.s0157", { "0": result.changedSegments }));
    });
    const updateCut = (editId: string, action: "apply" | "restore" | "dismiss") => project && withBusy(action === "apply" ? tr("app.s0193") : action === "dismiss" ? tr("app.cut.dismissing") : tr("app.s0194"), async () => {
        await editing.mutate(project.id, { kind: "set_cut_status", editId, action });
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0195") : action === "dismiss" ? tr("app.cut.dismissed") : tr("app.s0196"));
    });
    const detectSuggestions = () => project && withBusy(tr("app.s0197"), async () => {
        const envelope = await editing.mutate(project.id, { kind: "detect_cuts" });
        const count = envelope.suggestions?.length ?? 0;
        await refreshProject(project.id);
        setNotice(count ? tr("app.s0198", { "0": count }) : tr("app.s0199"));
    });
    const startCutPreview = async (editId: string) => {
        if (!project)
            return;
        const envelope = await transcriptEditingClient.previewCut(project.id, editId);
        if (!envelope.preview)
            throw new Error(tr("app.s0200"));
        setCutPreview(envelope.preview);
        const video = videoRef.current;
        if (!video) {
            setNotice(tr("app.s0201"));
            return;
        }
        video.currentTime = envelope.preview.previewStart;
        await video.play();
        setNotice(tr("app.s0202"));
    };
    const previewCut = (editId: string) => withBusy(tr("app.s0203"), async () => {
        await startCutPreview(editId);
    });
    const createWordCut = () => project && selected && activeWordRange && withBusy(tr("app.s0204"), async () => {
        const from = selectedWords[activeWordRange.start];
        const to = selectedWords[activeWordRange.end];
        if (!from || !to)
            throw new Error(tr("app.s0205"));
        const envelope = await editing.mutate(project.id, { kind: "create_word_cut", segmentId: selected.id, fromWordId: from.id, toWordId: to.id, paddingMs: cutPadding });
        if (!envelope.cut)
            throw new Error(tr("app.s0206"));
        await refreshProject(project.id);
        setWordRange(null);
        await startCutPreview(envelope.cut.id);
    });
    const restoreVersion = (versionId: string) => project && withBusy(tr("app.s0207"), async () => {
        await editing.mutate(project.id, { kind: "restore", versionId });
        await Promise.all([refreshProject(project.id, true), refreshSpeakerTrack(project.id), refreshTranscription(project.id)]);
        setNotice(tr("app.s0208"));
    });
    const navigateHistory = (action: "undo" | "redo") => project && withBusy(action === "undo" ? tr("app.s0209") : tr("app.s0210"), async () => {
        const envelope = await editing.mutate(project.id, { kind: action });
        if (!envelope.project)
            throw new Error(tr("app.s0211"));
        await Promise.all([refreshProject(project.id, true), refreshSpeakerTrack(project.id), refreshTranscription(project.id)]);
        setNotice(action === "undo" ? tr("app.s0212") : tr("app.s0213"));
    });
    return { editSegment, editTranslationSegment, replaceAll, updateCut, detectSuggestions, previewCut, createWordCut, restoreVersion, navigateHistory };
}
