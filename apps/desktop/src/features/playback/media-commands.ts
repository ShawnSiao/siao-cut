import { getProjectCapabilities } from "../../app-view-model";
import { pickMedia } from "../../domains/desktop-platform-client";
import { transcriptEditingClient } from "../../domains/transcript-editing-client";
import { tr } from "../../i18n";
import type { Project } from "../../types";
import type { EditingSession } from "../editing/editing-session";
type Inputs = {project:Project|null;mediaUrl:string|null;editing:EditingSession;withBusy:(label:string,action:()=>Promise<void>)=>Promise<void>;refreshProject:(id:string,media?:boolean)=>Promise<Project>;setNotice:(value:string|null)=>void};
export function createMediaCommands({project,mediaUrl,editing,withBusy,refreshProject,setNotice}:Inputs) {
    const capabilities = getProjectCapabilities(project,{mediaUrl});
    const relinkMedia = () => project && withBusy(tr("app.s0118"), async () => {
        const path = await pickMedia();
        if (!path)
            return;
        await editing.mutate(project.id, {kind:"relink_media",path});
        await refreshProject(project.id, true);
        setNotice(tr("app.s0119"));
    });
    const preparePreview = () => project && withBusy(tr("app.s0184"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        await transcriptEditingClient.prepareMedia(project.id);
        await refreshProject(project.id, true);
        setNotice(tr("app.s0185"));
    });
    return {relinkMedia,preparePreview};
}
