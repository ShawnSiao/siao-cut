import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockAiRequest, resetMockAiSettingsForTest } from "../environment-settings/mock-ai-settings";
import { aiApprovalClient } from "../../domains/ai-approval-client";
import AiExecutionConfirm from "./AiExecutionConfirm";

describe("AiExecutionConfirm", () => {
  beforeEach(() => resetMockAiSettingsForTest());
  afterEach(() => { cleanup(); vi.restoreAllMocks(); });

  it("binds actual payload and invalidates consent immediately when the model changes", async () => {
    const saved = await mockAiRequest({ kind: "save", input: { providerId: "openai", displayName: "OpenAI", modelId: "gpt-test", apiKey: "secret" } });
    const service = (saved.aiServices as { services: Array<{ id: string }> }).services[0];
    await mockAiRequest({ kind: "set_default", input: { id: service.id } });
    vi.spyOn(aiApprovalClient, "preview").mockImplementation(async (spec) => ({
      approvalId: `approval-${JSON.stringify(spec.target)}`, spec, payloadHash: "hash", receiver: "OpenAI", endpoint: "https://api.openai.com", model: "gpt-test", receiverVerified: true,
      configurationRevision: "1", segmentCount: 12, characterCount: 345, startTime: 0, endTime: 72, payloadJson: '{"segments":[{"text":"actual"}]}',
    }));
    const onConfirm = vi.fn();
    render(<AiExecutionConfirm scope={{ projectId: "p", expectedVersionId: "v", kind: "polish", language: null, taskId: null, instructionLocale: "zh-CN" }} returnFocusRef={createRef()} codexReady={false} taskLabel="AI 辅助 · 润色" segmentCount={12} characterCount={345} startTime={0} endTime={72} contextLabel={null} onClose={vi.fn()} onConfirm={onConfirm}/>);
    await waitFor(() => expect((screen.getByRole("radio", { name: /AI 服务/ }) as HTMLInputElement).checked).toBe(true));
    expect(screen.getByRole("combobox", { name: "服务" }).closest(".ai-service-control")).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "本次模型" }).closest(".ai-service-model-control")).not.toBeNull();
    expect(screen.getByText("仅影响本次运行")).not.toBeNull();
    expect(screen.getByText(/12 段 · 345 字符/)).not.toBeNull();
    expect(screen.getByText(/不包含视频、音频、本机媒体路径、数据库或凭据/)).not.toBeNull();
    await waitFor(() => expect(screen.getByText("查看实际发送文本、术语和结构约束")).not.toBeNull());
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.change(screen.getByRole("textbox", { name: "本次模型" }), { target: { value: "changed-model" } });
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByRole("button", { name: "确认并执行" }) as HTMLButtonElement).disabled).toBe(true);
    await waitFor(() => expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(false));
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "确认并执行" }));
    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ kind: "api", serviceConfigId: service.id, modelId: "changed-model" }), expect.stringContaining("approval-"));
  });
  it.each(["ai_approval_stale", "ai_dispatch_uncertain"])("requires a fresh unchecked preview after %s", async (code) => {
    let count = 0;
    const preview = vi.spyOn(aiApprovalClient, "preview").mockImplementation(async spec => ({
      approvalId: `approval-${++count}`, spec, payloadHash: "hash", receiver: "Codex", endpoint: null, model: null, receiverVerified: false,
      configurationRevision: "revision", segmentCount: 1, characterCount: 4, startTime: 0, endTime: 2, payloadJson: '{"segments":[{"text":"text"}]}',
    }));
    const onConfirm = vi.fn().mockRejectedValueOnce(Object.assign(new Error(`${code}: changed`), { code }));
    render(<AiExecutionConfirm scope={{ projectId: "p", expectedVersionId: "v", kind: "polish", language: null, taskId: null, instructionLocale: "zh-CN" }} returnFocusRef={createRef()} codexReady taskLabel="AI 辅助" segmentCount={1} characterCount={4} startTime={0} endTime={2} contextLabel={null} onClose={vi.fn()} onConfirm={onConfirm}/>);
    fireEvent.click(screen.getByRole("radio", { name: /本机 Codex/ }));
    await waitFor(() => expect(screen.getByRole("checkbox")).not.toBeDisabled());
    fireEvent.click(screen.getByRole("checkbox")); fireEvent.click(screen.getByRole("button", { name: "确认并执行" }));
    await waitFor(() => expect(preview).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "确认并执行" })).toBeDisabled();
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("alert")).toHaveTextContent("再次执行可能消耗额度");
  });

});
