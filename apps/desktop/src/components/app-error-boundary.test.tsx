import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppErrorBoundary } from "./app-error-boundary";

function BrokenView(): never {
  throw new Error("renderer regression");
}

describe("AppErrorBoundary", () => {
  it("shows a recoverable diagnostic instead of leaving an empty root", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<AppErrorBoundary><BrokenView /></AppErrorBoundary>);

    expect(screen.getByRole("alert")).toHaveTextContent("界面未能正常显示");
    expect(screen.getByText("renderer regression")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新加载" })).toBeInTheDocument();
    vi.restoreAllMocks();
  });
});
