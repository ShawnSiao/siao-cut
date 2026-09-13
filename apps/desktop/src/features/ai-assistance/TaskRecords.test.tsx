import "@testing-library/jest-dom/vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { TaskRecords } from "./TaskRecords";
import type { Task } from "../../types";
afterEach(() => { cleanup(); localStorage.clear(); vi.restoreAllMocks(); });
const task = { id: "failed1", kind: "polish", status: "failed", attemptCount: 1, createdAt: "2026-01-01", baseVersionId: "v1" } as Task;
function open() { fireEvent.click(document.querySelector(".task-records > summary")!); }
it("archives failures persistently, restores them and resurfaces new attempts", () => {
  const props = { projectId: "p1", tasks: [task], onRetry: vi.fn(), pending: {} };
  const view = render(<TaskRecords {...props}/>); open();
  fireEvent.click(screen.getByRole("button", { name: "归档" }));
  expect(screen.queryByRole("button", { name: "重新排队" })).not.toBeInTheDocument();
  view.unmount();
  const next = render(<TaskRecords {...props}/>); open();
  expect(screen.getByText("暂无记录")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("记录筛选"), { target: { value: "archived" } });
  fireEvent.click(screen.getByRole("button", { name: "恢复显示" }));
  fireEvent.change(screen.getByLabelText("记录筛选"), { target: { value: "attention" } });
  fireEvent.click(screen.getByRole("button", { name: "归档全部旧失败" }));
  next.rerender(<TaskRecords {...props} tasks={[{ ...task, attemptCount: 2 }]}/>);
  expect(screen.getByRole("button", { name: "重新排队" })).toBeInTheDocument();
});
it("does not archive running or review results and reports storage failure", () => {
  render(<TaskRecords projectId="p2" tasks={[task, { ...task, id: "active", status: "running" }]} onRetry={vi.fn()} pending={{}}/>); open();
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("full"); });
  fireEvent.click(screen.getByRole("button", { name: "归档全部旧失败" }));
  expect(screen.getByRole("alert")).toHaveTextContent("保存失败");
  expect(screen.getByRole("button", { name: "重新排队" })).toBeInTheDocument();
});
