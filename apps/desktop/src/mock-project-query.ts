import type { ProjectQuery } from "./generated/core-contract";
import type { CoreEnvelope,Project } from "./types";

// Structured read models use the same preview database as legacy commands.
export async function queryMockProject(request: ProjectQuery, state: {project: () => Project; projects: () => Project[]; run: (args: string[]) => Promise<CoreEnvelope>}): Promise<CoreEnvelope> {
  const ok = (body: Partial<CoreEnvelope>): CoreEnvelope => ({apiVersion:"0.1",status:"ok",...body});
  if (request.action === "list") {
    if (!state.projects().length) await state.run(["project", "list"]);
    const items = state.projects().slice(request.offset, request.offset + (request.limit ?? 50)).map((p) => ({
      id:p.id,title:p.title,createdAt:p.createdAt,updatedAt:p.updatedAt,segmentCount:p.transcript.segments.length,
      durationSeconds:p.media.durationSeconds,versionId:p.history.currentVersionId,
    }));
    const next = request.offset + items.length;
    return ok({projectPage:{items,total:state.projects().length,nextOffset:next < state.projects().length ? next : null}});
  }
  if (request.action === "show") return state.run(["project","show",request.projectId]);
  const stored = state.project().id === request.projectId ? state.project() : state.projects().find(item => item.id === request.projectId);
  if (!stored) throw new Error("Project not found");
  const p = structuredClone(stored);
  return request.action === "review" ? ok({projectId:p.id,versionId:p.history.currentVersionId ?? undefined,tasks:p.tasks,patchSets:p.patchSets,projectWorkflows:p.workflows}) : request.action === "history" ? ok({projectId:p.id,history:p.history,versions:p.versions})
    : ok({projectId:p.id,versionId:p.history.currentVersionId ?? undefined,subtitleQuality:p.subtitleQuality,speechInsights:p.speechInsights});
}
