import { beforeEach, describe, expect, it, vi } from "vitest";

const coreMocks = vi.hoisted(() => ({
  listProjects: vi.fn(),
  loadProject: vi.fn(),
  runCore: vi.fn(),
  runCoreStructured: vi.fn(),
}));

vi.mock("../core", () => coreMocks);

import { backgroundTaskClient } from "./background-task-client";
import { projectSessionClient } from "./project-session-client";
import { transcriptEditingClient } from "./transcript-editing-client";

describe("structured Desktop Core requests", () => {
  beforeEach(() => {
    coreMocks.runCore.mockReset();
    coreMocks.runCoreStructured.mockReset();
  });

  it("keeps a 100-segment offset request in one structured payload", async () => {
    const segmentIds = Array.from({ length: 100 }, (_, index) => `字幕-${index + 1}`);

    await transcriptEditingClient.offsetSegments("项目-一", segmentIds, -0.125);

    expect(coreMocks.runCoreStructured).toHaveBeenCalledOnce();
    expect(coreMocks.runCoreStructured).toHaveBeenCalledWith({
      kind: "transcript_offset",
      projectId: "项目-一",
      segmentIds,
      delta: -0.125,
    });
    expect(coreMocks.runCore).not.toHaveBeenCalled();
  });

  it("preserves Unicode prompt and hotwords in one transcription payload", async () => {
    await backgroundTaskClient.startTranscription({
      projectId: "项目-一",
      language: "zh",
      prompt: "区分「小爱」和「小艾」🎙️",
      hotwords: ["SiaoCut", "李雷", "韩梅梅"],
    });

    expect(coreMocks.runCoreStructured).toHaveBeenCalledOnce();
    expect(coreMocks.runCoreStructured).toHaveBeenCalledWith({
      kind: "transcription_start",
      projectId: "项目-一",
      language: "zh",
      prompt: "区分「小爱」和「小艾」🎙️",
      hotwords: ["SiaoCut", "李雷", "韩梅梅"],
    });
  });

  it("binds destructive confirmations to the preflight project version", async () => {
    await transcriptEditingClient.importSubtitleFile("p1", "D:\\字幕\\final.srt", "sha-256", "v-before");
    await projectSessionClient.deleteProject("p1", "v-before");

    expect(coreMocks.runCore).toHaveBeenNthCalledWith(1, [
      "transcript", "import-file", "p1", "D:\\字幕\\final.srt",
      "--confirm-replace", "--expected-sha256", "sha-256",
      "--expected-version", "v-before",
    ]);
    expect(coreMocks.runCore).toHaveBeenNthCalledWith(2, [
      "project", "delete", "p1", "--expected-version", "v-before",
    ]);
  });
});
