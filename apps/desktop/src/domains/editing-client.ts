import { runCoreStructured } from "../core";
import { projectSessionClient } from "./project-session-client";
import type { EditingTransport } from "../features/editing/editing-session";

export const editingClient: EditingTransport = {
  mutate: async (mutation) => runCoreStructured({ kind: "editing", request: { action: "mutate", mutation } }),
  load: projectSessionClient.loadProject,
  journal: async (draft) => { await runCoreStructured({ kind: "editing", request: { action: "journal", draft } }); },
  list: async (projectId) => (await runCoreStructured({ kind: "editing", request: { action: "list", projectId } })).drafts ?? [],
  discard: async (draft) => { await runCoreStructured({ kind: "editing", request: { action: "discard", draft } }); },
  save: async (edit) => {
    const result = await runCoreStructured({ kind: "editing", request: { action: "save", edit } });
    if (!result.editReceipt) throw new Error("Core did not acknowledge the edit");
    return result.editReceipt;
  },
};
