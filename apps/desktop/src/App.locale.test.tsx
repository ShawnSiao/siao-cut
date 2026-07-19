import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App";
import { changeUiLocale } from "./i18n";

afterEach(() => {
  cleanup();
  changeUiLocale("zh-CN");
});

describe("App locale switching", () => {
  it("switches the application chrome to English without changing project content", async () => {
    changeUiLocale("zh-CN");
    render(<App />);

    expect(await screen.findByRole("button", { name: "新建项目" })).toBeInTheDocument();
    const projectHeading = screen.getByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.change(screen.getByRole("combobox", { name: "界面语言" }), {
      target: { value: "en-US" },
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "New project" })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Transcribe" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send to Agent" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Export video" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
    expect(screen.getByText("4 subtitles")).toBeInTheDocument();
    expect(screen.getByText("1 subtitle")).toBeInTheDocument();
    expect(screen.getByText("ZH · 4 segments · 5 words")).toBeInTheDocument();
    expect(projectHeading).toHaveTextContent("发布口播 · 草稿");
    expect(document.documentElement.lang).toBe("en-US");
  });
});
