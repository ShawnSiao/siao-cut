import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import AiServicesPanel from "./AiServicesPanel";
import { resetMockAiSettingsForTest } from "./mock-ai-settings";

describe("AiServicesPanel", () => {
  beforeEach(() => resetMockAiSettingsForTest());
  afterEach(() => cleanup());

  it("saves a provider without echoing its API key", async () => {
    render(<AiServicesPanel codexHealth={{ available: true, authenticated: true, version: "codex-cli 0.145.0", authMode: "chatgpt" }} onRefreshCodex={vi.fn()}/>);
    expect(await screen.findByRole("button", { name: /本机 Codex/ })).not.toBeNull();
    expect(screen.getByRole("button", { name: /Anthropic/ })).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /OpenAI/ }));
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "test-secret-value" } });
    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "gpt-test" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并使用" }));
    await waitFor(() => expect(screen.getByText("已配置，未测试")).not.toBeNull());
    const keyInput = screen.getByLabelText("API Key") as HTMLInputElement;
    expect(keyInput.value).toContain("••••");
    expect(screen.queryByDisplayValue("test-secret-value")).toBeNull();
  });

  it("keeps custom endpoints editable and model entry available", async () => {
    render(<AiServicesPanel codexHealth={null} onRefreshCodex={vi.fn()}/>);
    await screen.findByRole("button", { name: /添加其他服务/ });
    fireEvent.click(screen.getByRole("button", { name: /添加其他服务/ }));
    const endpoint = screen.getByRole("textbox", { name: /服务地址/ }) as HTMLInputElement;
    fireEvent.click(screen.getByText("服务地址与网络设置"));
    expect(endpoint.readOnly).toBe(false);
    fireEvent.change(endpoint, { target: { value: "http://127.0.0.1:8040/v1" } });
    fireEvent.change(screen.getByLabelText("显示名称"), { target: { value: "本机兼容服务" } });
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "local-test-key" } });
    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "local-model" } });
    expect((screen.getByRole("button", { name: "保存并使用" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
