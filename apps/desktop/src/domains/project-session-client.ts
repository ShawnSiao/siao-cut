import { listProjects,loadProject,runCoreStructured } from "../core";
import { desktopQuery } from "./desktop-query-client";

export const projectSessionClient = {
  listProjects,
  loadProject,
  review: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"review",projectId}}),
  history: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"history",projectId}}),
  insights: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"insights",projectId}}),
  importMedia: importMedia,
  deletePreflight: (projectId: string) => desktopQuery({action:"delete_preflight",projectId}),
  deleteProject: (projectId: string, expectedVersionId: string) => runCoreStructured({kind: "project_command", request: {action: "delete", projectId, expectedVersionId, mutationId: `delete:${projectId}:${expectedVersionId}`}}),
};

const pendingImports = new Map<string, Promise<Awaited<ReturnType<typeof runCoreStructured>>>>();
function importMedia(path: string) {
  const pending = pendingImports.get(path);
  if (pending) return pending;
  const key = `siaocut.importMutation:${path}`;
  // Retain the command ID across an uncertain response and a renderer reload.
  const mutationId = sessionStorage.getItem(key) ?? crypto.randomUUID();
  sessionStorage.setItem(key, mutationId);
  const request = runCoreStructured({kind: "project_command", request: {action: "import", path, mutationId}})
    .then(result => { sessionStorage.removeItem(key); return result; })
    .finally(() => pendingImports.delete(path));
  pendingImports.set(path, request);
  return request;
}
