import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { backgroundTaskClient } from "../domains/background-task-client";
import { useTranscriptionTasks } from "./use-transcription-tasks";
import type { CoreEnvelope, TranscriptionJob } from "../types";

vi.mock("../domains/background-task-client", () => ({ backgroundTaskClient: { listTranscriptions: vi.fn(), getTranscriptionJob: vi.fn() } }));
afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); });

it("serializes slow polling, keeps jobs across project navigation, and stops after completion", async () => {
  vi.useFakeTimers();
  const job = { id: "job", projectId: "other-project", status: "queued", stage: "queued", attemptCount: 1, updatedAt: "2026-09-12T00:00:00Z" } as TranscriptionJob;
  vi.mocked(backgroundTaskClient.listTranscriptions).mockResolvedValue({ status: "ok", transcriptionJobs: [job] } as CoreEnvelope);
  let resolve!: (value: CoreEnvelope) => void;
  vi.mocked(backgroundTaskClient.getTranscriptionJob).mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  const applied = vi.fn().mockResolvedValue(undefined);
  const { result, unmount } = renderHook(() => useTranscriptionTasks(applied));
  await act(async () => {});
  expect(result.current.jobs).toHaveLength(1);
  await act(() => vi.advanceTimersByTimeAsync(800));
  await act(() => vi.advanceTimersByTimeAsync(5000));
  expect(backgroundTaskClient.getTranscriptionJob).toHaveBeenCalledTimes(1);
  await act(async () => resolve({ status: "ok", transcriptionJob: { ...job, status: "completed", stage: "completed", updatedAt: "2026-09-12T00:00:02Z" } } as CoreEnvelope));
  await act(() => vi.advanceTimersByTimeAsync(5000));
  expect(backgroundTaskClient.getTranscriptionJob).toHaveBeenCalledTimes(1);
  expect(applied).toHaveBeenCalledTimes(1);
  expect(result.current.jobs[0].status).toBe("completed");
  unmount();
});
