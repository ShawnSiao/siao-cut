import type { Dispatch,SetStateAction } from "react";
import { tr } from "../../i18n";
import type { CanvasSettings,Project } from "../../types";
import type { EditingSession } from "../editing/editing-session";
import { resolveCanvasMedia } from "../playback/use-playback-session";
type Inputs = {project:Project|null;setProject:Dispatch<SetStateAction<Project|null>>;updateProjectSummary:(project:Project)=>void;
 setMediaUrl:(value:string|null)=>void;withBusy:(label:string,action:()=>Promise<void>)=>Promise<void>;setNotice:(value:string|null)=>void;editing:EditingSession};
export function createPresentationCommands({project, setProject, updateProjectSummary, setMediaUrl, withBusy, setNotice, editing}:Inputs) {
    const changeCanvas = (settings: CanvasSettings) => {
        if (!project)
            return Promise.resolve();
        const projectId = project.id;
        const previousSettings = project.canvasSettings;
        const updateCanvasState = (canvasSettings: CanvasSettings) => {
            setProject((current) => current?.id === projectId ? { ...current, canvasSettings } : current);
        };
        updateCanvasState(settings);
        return withBusy(tr("app.s0178"), async () => {
            try {
                const envelope = await editing.mutate(projectId, { kind: "canvas", aspectRatio: settings.aspectRatio, framing: settings.framing });
                if (!envelope.project)
                    throw new Error(tr("app.canvas.projectMissing"));
                setProject(envelope.project);
                updateProjectSummary(envelope.project!);
                const authorization = await resolveCanvasMedia(projectId);
                setMediaUrl(authorization.mediaUrl);
                const savedNotice = settings.aspectRatio === "9:16" ? tr("app.s0179") : tr("app.s0180");
                setNotice(authorization.warning ? `${savedNotice} ${tr("app.canvas.previewUnavailable")}` : savedNotice);
            }
            catch (cause) {
                updateCanvasState(previousSettings);
                throw cause;
            }
        });
    };
    const changeSubtitleStyle = (preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"], sourceFontSize?: number, translationFontSize?: number, boxWidthPercent?: number, boxHeightLines?: number) => project && withBusy(tr("app.s0181"), async () => {
        const envelope = await editing.mutate(project.id, { kind: "style", preset, position, sourceFontSize: sourceFontSize ?? null, translationFontSize: translationFontSize ?? null, boxWidthPercent: boxWidthPercent ?? null, boxHeightLines: boxHeightLines ?? null });
        if (!envelope.project)
            throw new Error(tr("app.s0182"));
        setProject(envelope.project);
        updateProjectSummary(envelope.project!);
        setNotice(tr("app.s0183"));
    });
    return {changeCanvas,changeSubtitleStyle};
}
