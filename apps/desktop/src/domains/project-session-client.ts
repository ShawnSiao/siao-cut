import { listProjects, loadProject, runCore } from "../core";

export const projectSessionClient = {
  listProjects,
  loadProject,
  importMedia: (path: string) => runCore(["import", path]),
  deletePreflight: (projectId: string) => runCore(["project", "delete-preflight", projectId]),
  deleteProject: (projectId: string) => runCore(["project", "delete", projectId]),
  relinkMedia: (projectId: string, path: string) => runCore(["project", "relink", projectId, path]),
  restoreVersion: (projectId: string, versionId: string) => runCore(["project", "restore", projectId, versionId]),
  navigateHistory: (projectId: string, action: "undo" | "redo") => runCore(["project", action, projectId]),
};
