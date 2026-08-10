import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import AiServicesPanel from "./AiServicesPanel";
import { resetMockAiSettingsForTest } from "./mock-ai-settings";

describe("AiServicesPanel", () => {
  beforeEach(() => resetMockAiSettingsForTest());
  afterEach(() => cleanup());

  it("saves a provider without echoing its API key", async () => {
    render(<AiServicesPanel/>);
    expect(await screen.findByText(/尚未配置 API 服务/)).not.toBeNull();
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "test-secret-value" } });
    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "gpt-test" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(screen.getByText(/已保存 Key/)).not.toBeNull());
    const keyInput = screen.getByLabelText("API Key") as HTMLInputElement;
    expect(keyInput.value).toBe("");
    expect(keyInput.placeholder).toContain("已保存");
    expect(screen.queryByDisplayValue("test-secret-value")).toBeNull();
  });

  it("keeps custom endpoints editable and model entry available", async () => {
    render(<AiServicesPanel/>);
    await screen.findByText(/尚未配置 API 服务/);
    fireEvent.change(screen.getByLabelText("服务类型"), { target: { value: "custom" } });
    const endpoint = screen.getByRole("textbox", { name: /服务地址/ }) as HTMLInputElement;
    expect(endpoint.readOnly).toBe(false);
    fireEvent.change(endpoint, { target: { value: "http://127.0.0.1:8040/v1" } });
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "local-test-key" } });
    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "local-model" } });
    expect((screen.getByRole("button", { name: "保存" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
