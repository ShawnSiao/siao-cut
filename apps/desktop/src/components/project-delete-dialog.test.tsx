import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProjectDeleteDialog from "./project-delete-dialog";
import { sampleProject } from "../mock";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("ProjectDeleteDialog", () => {
  it("does not flash the checking message for a fast preflight", () => {
    vi.useFakeTimers();
    const { rerender } = render(
      <ProjectDeleteDialog
        project={sampleProject}
        checking
        deleting={false}
        deletable={false}
        blockerMessage={null}
        error={null}
        onClose={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.queryByText("正在向 Core 检查项目是否可删除…")).not.toBeInTheDocument();
    act(() => vi.advanceTimersByTime(120));
    rerender(
      <ProjectDeleteDialog
        project={sampleProject}
        checking={false}
        deleting={false}
        deletable
        blockerMessage={null}
        error={null}
        onClose={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.queryByText("正在向 Core 检查项目是否可删除…")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认删除" })).toBeEnabled();
  });

  it("shows the checking message when the preflight is actually slow", () => {
    vi.useFakeTimers();
    render(
      <ProjectDeleteDialog
        project={sampleProject}
        checking
        deleting={false}
        deletable={false}
        blockerMessage={null}
        error={null}
        onClose={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    act(() => vi.advanceTimersByTime(180));

    expect(screen.getByRole("status")).toHaveTextContent("正在向 Core 检查项目是否可删除…");
  });
});
