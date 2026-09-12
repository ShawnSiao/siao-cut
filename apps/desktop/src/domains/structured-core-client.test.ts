import { beforeEach, describe, expect, it, vi } from "vitest";

const coreMocks = vi.hoisted(() => ({
  listProjects: vi.fn(),
  loadProject: vi.fn(),
  runCore: vi.fn(),
  runCoreStructured: vi.fn(),
}));

vi.mock("../core", () => coreMocks);

import { backgroundTaskClient } from "./background-task-client";
import { editingClient } from "./editing-client";
import { projectSessionClient } from "./project-session-client";

describe("structured Desktop Core requests", () => {
  beforeEach(() => {
    coreMocks.runCore.mockReset();
    coreMocks.runCoreStructured.mockReset();
  });

  it("keeps a 100-segment offset request in one structured payload", async () => {
    const segmentIds = Array.from({ length: 100 }, (_, index) => `字幕-${index + 1}`);

    await editingClient.mutate!({projectId:"项目-一",mutationId:"offset-once",expectedVersionId:"v-before",operation:{kind:"offset",segmentIds,delta:-0.125}});

    expect(coreMocks.runCoreStructured).toHaveBeenCalledOnce();
    expect(coreMocks.runCoreStructured).toHaveBeenCalledWith({
      kind: "editing",
      request: {action:"mutate",mutation:{projectId:"项目-一",mutationId:"offset-once",expectedVersionId:"v-before",operation:{kind:"offset",segmentIds,delta:-0.125}}},
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
    const mutation = {projectId:"p1",mutationId:"import-once",expectedVersionId:"v-before",operation:{kind:"import_subtitle" as const,path:"D:/字幕/final.srt",sha256:"sha-256",previewVersionId:"v-before"}};
    await editingClient.mutate!(mutation);
    await projectSessionClient.deleteProject("p1", "v-before");
    expect(coreMocks.runCoreStructured).toHaveBeenCalledWith({kind:"editing",request:{action:"mutate",mutation}});
    expect(coreMocks.runCore).toHaveBeenCalledExactlyOnceWith([
      "project", "delete", "p1", "--expected-version", "v-before",
    ]);
  });
});
