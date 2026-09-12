import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

/** Read models and derived media only. Project edits belong to the versioned editing queue. */
export const transcriptEditingClient = {
  transcriptReplacementPreflight: (projectId: string) => desktopQuery({action:"transcript_replacement_preflight",projectId}),
  getSpeakerTrack: (projectId: string) => desktopQuery({action:"speaker_track",projectId}),
  inspectSubtitleFile: (projectId: string, path: string) => desktopQuery({action:"inspect_subtitle",projectId,path}),
  prepareMedia: (projectId: string) => desktopControl({action:"media_prepare",projectId}),
  previewCut: (projectId: string, editId: string) => desktopQuery({action:"preview_cut",projectId,editId}),
};
