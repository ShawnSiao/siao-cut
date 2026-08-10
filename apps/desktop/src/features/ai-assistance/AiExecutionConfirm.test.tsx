import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockAiRequest, resetMockAiSettingsForTest } from "../environment-settings/mock-ai-settings";
import AiExecutionConfirm from "./AiExecutionConfirm";

describe("AiExecutionConfirm", () => {
  beforeEach(() => resetMockAiSettingsForTest());
  afterEach(() => cleanup());

  it("prefers the configured API default and binds its revisions", async () => {
    const saved = await mockAiRequest({ kind: "save", input: { providerId: "openai", displayName: "OpenAI", modelId: "gpt-test", apiKey: "secret" } });
    const service = (saved.aiServices as { services: Array<{ id: string }> }).services[0];
    await mockAiRequest({ kind: "set_default", input: { id: service.id } });
    const onConfirm = vi.fn();
    render(<AiExecutionConfirm returnFocusRef={createRef()} codexReady={false} taskLabel="AI 辅助 · 润色" segmentCount={12} characterCount={345} startTime={0} endTime={72} contextLabel={null} onClose={vi.fn()} onConfirm={onConfirm}/>);
    await waitFor(() => expect((screen.getByRole("radio", { name: /AI 服务/ }) as HTMLInputElement).checked).toBe(true));
    expect(screen.getByText(/12 段 · 345 字符/)).not.toBeNull();
    expect(screen.getByText(/不包含视频、音频、本机媒体路径、数据库或凭据/)).not.toBeNull();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "确认并执行" }));
    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ kind: "api", serviceConfigId: service.id, modelId: "gpt-test" }));
  });
});
