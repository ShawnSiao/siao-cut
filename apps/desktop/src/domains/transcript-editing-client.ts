import { runCore } from "../core";

/** Read models and derived media only. Project edits belong to the versioned editing queue. */
export const transcriptEditingClient = {
  transcriptReplacementPreflight: (projectId: string) => runCore(["transcript", "replacement-preflight", projectId]),
  getSpeakerTrack: (projectId: string) => runCore(["speaker", "track", projectId]),
  inspectSubtitleFile: (projectId: string, path: string) => runCore(["transcript", "inspect-file", projectId, path]),
  prepareMedia: (projectId: string) => runCore(["media", "prepare", projectId]),
  previewCut: (projectId: string, editId: string) => runCore(["cut", "preview", projectId, editId]),
};
