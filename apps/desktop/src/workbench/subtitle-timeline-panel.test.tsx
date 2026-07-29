import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { sampleProject } from "../mock";
import {
  DEFAULT_TIMELINE_PREFERENCES,
  deriveTimelineReviewMarkers,
  parseTimelinePreferences,
  SubtitleTimelinePanel,
  timelinePercent,
  timelineTickInterval,
  TIMELINE_PREFERENCES_STORAGE_KEY,
} from "./subtitle-timeline-panel";

afterEach(() => {
  cleanup();
  localStorage.removeItem(TIMELINE_PREFERENCES_STORAGE_KEY);
});

describe("subtitle timeline model", () => {
  it("loads versioned preferences and falls back safely", () => {
    expect(parseTimelinePreferences(null)).toEqual(DEFAULT_TIMELINE_PREFERENCES);
    expect(parseTimelinePreferences("{broken")).toEqual(DEFAULT_TIMELINE_PREFERENCES);
    expect(parseTimelinePreferences(JSON.stringify({
      version: 1,
      expanded: false,
      mode: "review",
      zoom: 99,
      followPlayhead: false,
    }))).toEqual({
      version: 1,
      expanded: false,
      mode: "review",
      zoom: 3,
      followPlayhead: false,
    });
  });

  it("maps time without inflating segment geometry and chooses readable ticks", () => {
    expect(timelinePercent(5, 20)).toBe(25);
    expect(timelinePercent(-1, 20)).toBe(0);
    expect(timelinePercent(30, 20)).toBe(100);
    expect(timelineTickInterval(320, 2458)).toBe(15);
    expect(timelineTickInterval(0, 0)).toBe(10);
  });

  it("derives de-duplicated quality, Agent, transcription, and edit markers", () => {
    const project = structuredClone(sampleProject);
    project.subtitleQuality.issues.push({ ...project.subtitleQuality.issues[0], id: "duplicate-gap" });
    const markers = deriveTimelineReviewMarkers(project, [{
      id: "review-1",
      projectId: project.id,
      runId: "run-1",
      segmentId: "s3",
      severity: "warning",
      kind: "short_fragment",
      message: "片段过短",
      status: "open",
      createdAt: "2026-07-29T00:00:00Z",
      resolvedAt: null,
    }]);

    expect(markers.filter((marker) => marker.source === "quality")).toHaveLength(3);
    expect(markers.some((marker) => marker.source === "agent" && marker.segmentId === "s2")).toBe(true);
    expect(markers.some((marker) => marker.source === "transcription" && marker.segmentId === "s3")).toBe(true);
    expect(markers.some((marker) => marker.source === "edit" && marker.segmentId === "s1")).toBe(true);
  });
});

describe("SubtitleTimelinePanel", () => {
  it("defaults to B, persists C, collapses to A, and restores the expanded mode", async () => {
    const onOpenReviewDetail = vi.fn();
    render(<SubtitleTimelinePanel
      project={structuredClone(sampleProject)}
      speakerTrack={null}
      transcriptionReviews={[]}
      waveformUrl={null}
      playback={{ playing: false, currentTime: 13.4, duration: 278 }}
      selectedId="s2"
      selectedSegmentIds={["s2"]}
      busy={false}
      onSelectSegment={vi.fn()}
      onSeek={vi.fn()}
      onTogglePlayback={vi.fn()}
      onNudgeSelected={vi.fn()}
      onOpenTiming={vi.fn()}
      onOpenReviewDetail={onOpenReviewDetail}
      onRestoreCut={vi.fn()}
    />);

    expect(screen.getByRole("button", { name: "精细编辑" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("slider", { name: "缩放比例" })).toHaveValue("160");
    expect(screen.getByRole("region", { name: "字幕时间轴" })).toHaveClass("expanded", "edit");

    fireEvent.click(screen.getByRole("button", { name: /高级审校/ }));
    expect(screen.getByRole("region", { name: "字幕时间轴" })).toHaveClass("review");
    fireEvent.click(screen.getByRole("button", { name: "收起时间线" }));
    expect(screen.getByRole("region", { name: "字幕时间轴" })).toHaveClass("collapsed", "overview");
    fireEvent.click(screen.getByRole("button", { name: "展开时间线" }));
    expect(screen.getByRole("region", { name: "字幕时间轴" })).toHaveClass("expanded", "review");

    await waitFor(() => expect(JSON.parse(localStorage.getItem(TIMELINE_PREFERENCES_STORAGE_KEY) ?? "{}")).toMatchObject({
      expanded: true,
      mode: "review",
      zoom: 1.6,
    }));

    fireEvent.click(screen.getAllByRole("button", { name: /质量问题/ })[0]);
    expect(onOpenReviewDetail).toHaveBeenCalledWith(expect.objectContaining({ source: "quality", detailTarget: "quality" }));
  });

  it("uses whole-segment nudge actions and keeps exact timing separate", () => {
    const onNudgeSelected = vi.fn();
    const onOpenTiming = vi.fn();
    const project = structuredClone(sampleProject);
    render(<SubtitleTimelinePanel
      project={project}
      speakerTrack={null}
      transcriptionReviews={[]}
      waveformUrl={null}
      playback={{ playing: false, currentTime: 13.4, duration: 278 }}
      selectedId="s2"
      selectedSegmentIds={["s2"]}
      busy={false}
      onSelectSegment={vi.fn()}
      onSeek={vi.fn()}
      onTogglePlayback={vi.fn()}
      onNudgeSelected={onNudgeSelected}
      onOpenTiming={onOpenTiming}
      onOpenReviewDetail={vi.fn()}
      onRestoreCut={vi.fn()}
    />);

    fireEvent.click(screen.getByRole("button", { name: "前移 0.1 秒" }));
    fireEvent.click(screen.getByRole("button", { name: "后移 0.1 秒" }));
    expect(onNudgeSelected).toHaveBeenNthCalledWith(1, "s2", -0.1);
    expect(onNudgeSelected).toHaveBeenNthCalledWith(2, "s2", 0.1);

    fireEvent.click(screen.getByRole("button", { name: "精确时间" }));
    expect(onOpenTiming).toHaveBeenCalledWith(expect.objectContaining({ id: "s2" }));
  });
});
