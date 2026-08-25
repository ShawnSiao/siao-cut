import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import type { AutoWorkflow, ExportJob, SourceImportJob, TranscriptionJob } from "../types";
import { deriveWorkbenchActivities, transcriptionStageLabel } from "./workbench-activity";
import WorkbenchActivityCenter from "./workbench-activity-center";

const now = "2026-08-22T10:00:00.000Z";

function transcription(status: TranscriptionJob["status"], stage = status): TranscriptionJob {
  return { id: "transcription-1", projectId: "p1", providerId: "moss_openai", endpoint: "http://127.0.0.1:8000", modelId: "moss", language: null, prompt: null, hotwords: [], status, stage, resultRunId: null, baseVersionId: null, sourceSha256: null, inputAudioSha256: null, cancelRequestedAt: null, errorMessage: null, createdAt: now, updatedAt: now, completedAt: null, attemptCount: 1, candidate: null };
}

function workflow(status: AutoWorkflow["status"], updatedAt = now): AutoWorkflow {
  return { id: `auto-${status}`, inputKind: "local", inputValue: "talk.mp4", title: "Talk", confirmedMediaId: null, projectId: "p1", sourceImportId: "source-child", modelPath: "model.bin", transcribeLanguage: null, translationLanguage: null, outputPath: "out.mp4", burnSubtitles: true, subtitleMode: "source", profile: "balanced", status, currentStage: status === "needs_review" ? "review" : "transcribe", progress: 0.5, transcriptVersionId: null, agentTaskId: "agent-child", audioAnalysisJobId: null, aiExecutionKind: null, aiServiceConfigId: null, aiServiceRevision: null, aiNetworkRevision: null, aiModelId: null, aiAuthorized: false, exportJobId: "export-child", audit: null, cancelRequestedAt: null, errorMessage: null, createdAt: now, updatedAt, completedAt: null, attemptCount: 1, instructionLocale: "zh-CN" };
}

it("prioritizes required action, then failures, then active work", () => {
  const source = { id: "source-1", status: "running", updatedAt: now, title: "URL", progress: 0.2 } as SourceImportJob;
  const activities = deriveWorkbenchActivities({ sourceJob: source, transcriptionJob: transcription("awaiting_apply"), autoWorkflows: [workflow("failed", "2026-08-22T11:00:00.000Z")] });
  expect(activities.map((activity) => activity.state)).toEqual(["action_required", "failed", "running"]);
});

it("sorts equal priority by the most recent update", () => {
  const activities = deriveWorkbenchActivities({ autoWorkflows: [workflow("failed", "2026-08-22T09:00:00.000Z"), { ...workflow("interrupted", "2026-08-22T12:00:00.000Z"), id: "newer" }] });
  expect(activities[0].id).toBe("auto:newer");
});

it("does not duplicate child jobs already represented by an auto workflow", () => {
  const source = { id: "source-child", status: "running", updatedAt: now, title: "URL", progress: 0.2 } as SourceImportJob;
  const exportJob = { id: "export-child", status: "running", updatedAt: now, outputPath: "out.mp4", progress: 0.3 } as ExportJob;
  const activities = deriveWorkbenchActivities({ sourceJob: source, exportJob, autoWorkflows: [workflow("running")] });
  expect(activities.map((activity) => activity.kind)).toEqual(["auto"]);
});

describe("transcriptionStageLabel", () => {
  it("keeps an unknown stage visible instead of failing", () => {
    expect(transcriptionStageLabel("future_stage")).toBe("future_stage");
  });
});

it("renders one primary status region and keeps remaining activities in details", () => {
  render(<WorkbenchActivityCenter inputs={{ transcriptionJob: transcription("awaiting_apply"), autoWorkflows: [workflow("failed")] }} actionsFor={() => []}/>);
  expect(screen.getAllByRole("region", { name: "项目任务状态" })).toHaveLength(1);
  expect(screen.getByText("候选结果等待确认")).toBeTruthy();
  fireEvent.click(screen.getByText("查看其余 1 项任务"));
  expect(screen.getByText("流程失败 · 本地转录")).toBeTruthy();
});
