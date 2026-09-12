import type { TranscriptionCommand } from "../generated/core-contract";
import type { CoreEnvelope, Project, TranscriptionJob } from "../types";
const mockTranscriptionCommands = new Map<string, { json: string; id: string }>();
const mockWhisperPolls = new Map<string, number>();
export function resetMockTranscriptionCommands() { mockTranscriptionCommands.clear(); mockWhisperPolls.clear(); }

export async function runMockTranscription(request: TranscriptionCommand, dependencies: { mockTranscriptionJobs: Map<string, TranscriptionJob>; mockProject: Project; mockProjects: Project[]; mockRun: (args: string[]) => Promise<CoreEnvelope> }): Promise<CoreEnvelope> {
  const { mockTranscriptionJobs, mockProject, mockProjects, mockRun } = dependencies;
  const response = (id: string): CoreEnvelope => ({ apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(mockTranscriptionJobs.get(id) ?? null) });
  const mutation = "mutationId" in request ? request.mutationId : null;
  const previous = mutation ? mockTranscriptionCommands.get(mutation) : null;
  if (previous) { if (previous.json !== JSON.stringify(request)) throw new Error("mutation_id_reused"); return response(previous.id); }
  if (request.action === "list") return { apiVersion: "0.1", status: "ok", transcriptionJobs: [...mockTranscriptionJobs.values()].filter((job) => !request.projectId || job.projectId === request.projectId).map((job) => structuredClone(job)) };
  if (request.action === "preview") return { apiVersion: "0.1", status: "ok", candidatePreview: { jobId: request.jobId, versionId: mockProject.history.currentVersionId ?? "", overwrittenSegments: mockProject.transcript.segments.length, total: 1, offset: request.offset, segments: [{ start: 0, end: 8.4, text: "这是经过明确确认后应用的多人转写候选结果。" }] } };
  if (request.action === "start") {
    const active = [...mockTranscriptionJobs.values()].find((job) => job.projectId === request.projectId && ["queued", "running", "awaiting_apply"].includes(job.status));
    if (active) return response(active.id);
    if (request.expectedVersionId !== mockProject.history.currentVersionId) throw new Error("project_version_conflict");
    const now = new Date().toISOString(), id = `whisper-${crypto.randomUUID()}`;
    mockTranscriptionJobs.set(id, { id, projectId: request.projectId, providerId: "whisper_local", endpoint: "", modelId: request.modelPath, language: request.language, prompt: null, hotwords: [], status: "queued", stage: "queued", resultRunId: null, baseVersionId: request.expectedVersionId, sourceSha256: "mock-source", inputAudioSha256: null, cancelRequestedAt: null, errorMessage: null, createdAt: now, updatedAt: now, completedAt: null, attemptCount: 1, candidate: null });
    mockTranscriptionCommands.set(request.mutationId, { json: JSON.stringify(request), id });
    return response(id);
  }
  const job = mockTranscriptionJobs.get(request.jobId);
  if (request.action === "apply" && request.expectedVersionId !== mockProject.history.currentVersionId) throw new Error("transcription_apply_version_mismatch");
  if (job?.providerId === "whisper_local" && request.action === "retry") {
    job.attemptCount += 1; job.status = "queued"; job.stage = "queued"; job.cancelRequestedAt = null; mockWhisperPolls.delete(job.id);
    mockTranscriptionCommands.set(request.mutationId, { json: JSON.stringify(request), id: job.id });
    return response(job.id);
  }
  if (job && request.action === "cancel") { job.status = "cancelled"; job.stage = "cancelled"; job.cancelRequestedAt = new Date().toISOString(); return response(job.id); }

  if (job?.providerId === "whisper_local" && request.action === "get" && ["queued", "running"].includes(job.status)) {
    const count = (mockWhisperPolls.get(job.id) ?? 0) + 1; mockWhisperPolls.set(job.id, count);
    job.status = "running"; job.stage = "requesting_model";
    if (count >= 2) {
      const project = mockProjects.find((item) => item.id === job.projectId) ?? mockProject;
      if (project.transcript.segments.length || project.history.currentVersionId !== job.baseVersionId) {
        job.status = "awaiting_apply"; job.stage = "awaiting_apply"; job.resultRunId = `run-${job.id}`;
        job.candidate = { runId: job.resultRunId, segmentCount: 1, speakerCount: 0, warningCount: 0, durationSeconds: 8, baseVersionId: job.baseVersionId, currentVersionId: project.history.currentVersionId, canApply: true };
      } else { job.status = "completed"; job.stage = "completed"; job.completedAt = new Date().toISOString(); }
    }
    return response(job.id);
  }
  const result = await mockRun(["transcription", request.action === "get" ? "status" : request.action === "retry" ? "resume" : request.action, request.jobId, ...(request.action === "apply" ? ["--expected-version", request.expectedVersionId, "--confirm-replace"] : [])]);
  if (mutation) mockTranscriptionCommands.set(mutation, { json: JSON.stringify(request), id: request.jobId });
  return result;
}
