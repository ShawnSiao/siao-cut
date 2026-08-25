import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { sampleProject } from "../mock";
import FocusReviewPanel from "./focus-review-panel";

afterEach(cleanup);

describe("FocusReviewPanel", () => {
  it("navigates safely and leaves decisions behind explicit buttons", () => {
    const onLocate = vi.fn();
    const onCutReview = vi.fn();
    const onTogglePlayback = vi.fn();
    const onSeekDelta = vi.fn();
    const onExit = vi.fn();
    render(<FocusReviewPanel
      project={structuredClone(sampleProject)}
      transcriptionReviews={[]}
      audioRisks={[]}
      busy={false}
      error={null}
      onLocate={onLocate}
      onAgentReview={vi.fn()}
      onCutReview={onCutReview}
      onTranscriptionReview={vi.fn()}
      onOpenEditor={vi.fn()}
      onTogglePlayback={onTogglePlayback}
      onSeekDelta={onSeekDelta}
      onExit={onExit}
    />);

    expect(screen.getByText("粗剪建议")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保留原片" }));
    expect(onCutReview).toHaveBeenCalledWith(expect.objectContaining({ id: "cut:e1" }), "dismiss");

    fireEvent.keyDown(window, { key: "n" });
    expect(onLocate).toHaveBeenCalledWith(expect.objectContaining({ kind: "agent" }));
    fireEvent.keyDown(window, { code: "Space", key: " " });
    fireEvent.keyDown(window, { key: "ArrowRight", shiftKey: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onTogglePlayback).toHaveBeenCalledTimes(1);
    expect(onSeekDelta).toHaveBeenCalledWith(0.1);
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it("keeps conflicts visible and disables applying them", () => {
    const project = structuredClone(sampleProject);
    project.edits = [];
    project.subtitleQuality.issues = [];
    project.patchSets[0].items[0].status = "conflict";
    render(<FocusReviewPanel
      project={project}
      transcriptionReviews={[]}
      audioRisks={[]}
      busy={false}
      error="project_version_conflict"
      onLocate={vi.fn()}
      onAgentReview={vi.fn()}
      onCutReview={vi.fn()}
      onTranscriptionReview={vi.fn()}
      onOpenEditor={vi.fn()}
      onTogglePlayback={vi.fn()}
      onSeekDelta={vi.fn()}
      onExit={vi.fn()}
    />);

    expect(screen.getByText(/项目版本已经变化/)).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent("project_version_conflict");
    expect(screen.getByRole("button", { name: "应用建议" })).toBeDisabled();
  });
});
