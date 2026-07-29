import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import QuickRetranscriptionDialog from "./quick-retranscription-dialog";

afterEach(cleanup);

describe("QuickRetranscriptionDialog", () => {
  it("requires explicit confirmation after a successful preflight", () => {
    const onConfirmedChange = vi.fn();
    const onConfirm = vi.fn();
    render(
      <QuickRetranscriptionDialog
        preflight={{ canReplace: true, currentVersionId: "v-current", blockers: { edits: 0, patchItems: 0, taskSegments: 0 } }}
        checking={false}
        busy={false}
        confirmed={false}
        blockerMessage={null}
        error={null}
        onConfirmedChange={onConfirmedChange}
        onConfirm={onConfirm}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText(/只有通过原始媒体时间轴验收/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认并重新转写" })).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /确认替换当前字幕/ }));
    expect(onConfirmedChange).toHaveBeenCalledWith(true);
  });

  it("shows dependency blockers and prevents replacement", () => {
    render(
      <QuickRetranscriptionDialog
        preflight={{ canReplace: false, currentVersionId: "v-current", blockers: { edits: 1, patchItems: 2, taskSegments: 3 } }}
        checking={false}
        busy={false}
        confirmed={false}
        blockerMessage="当前字幕仍有关联内容：剪辑 1 项、Agent 建议 2 项、任务基线 3 项。"
        error={null}
        onConfirmedChange={vi.fn()}
        onConfirm={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("任务基线 3 项");
    expect(screen.getByRole("checkbox", { name: /确认替换当前字幕/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "确认并重新转写" })).toBeDisabled();
  });
});
