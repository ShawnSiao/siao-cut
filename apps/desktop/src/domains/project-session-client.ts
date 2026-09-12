import { listProjects, loadProject, runCore, runCoreStructured } from "../core";

export const projectSessionClient = {
  listProjects,
  loadProject,
  review: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"review",projectId}}),
  history: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"history",projectId}}),
  insights: (projectId: string) => runCoreStructured({kind:"project_query",request:{action:"insights",projectId}}),
  importMedia: (path: string) => runCore(["import", path]),
  deletePreflight: (projectId: string) => runCore(["project", "delete-preflight", projectId]),
  deleteProject: (projectId: string, expectedVersionId: string) => runCore(["project", "delete", projectId, "--expected-version", expectedVersionId]),
};
