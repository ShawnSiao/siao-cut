import type { Dispatch,SetStateAction } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { Project } from "../../types";
/** A command can await review data without reloading playback or other job domains. */
export async function refreshReview(id: string, setProject: Dispatch<SetStateAction<Project | null>>) {
  const response = await projectSessionClient.review(id);
  setProject(current => current?.id === id && current.history.currentVersionId === response.versionId
    ? { ...current, tasks: response.tasks ?? current.tasks, patchSets: response.patchSets ?? current.patchSets, workflows: response.projectWorkflows ?? current.workflows, readModels: {...current.readModels, review: response.versionId} }
    : current);
}
