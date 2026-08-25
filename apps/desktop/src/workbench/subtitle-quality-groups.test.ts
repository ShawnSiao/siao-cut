import { describe, expect, it } from "vitest";
import type { SubtitleQualityIssue } from "../types";
import { groupSubtitleQualityIssues } from "./subtitle-quality-groups";

const issue = (overrides: Partial<SubtitleQualityIssue>): SubtitleQualityIssue => ({
  id: "warning-1",
  kind: "reading_speed_high",
  severity: "warning",
  segmentId: "s1",
  relatedSegmentId: null,
  start: 2,
  end: 3,
  message: "字幕阅读速度过快",
  measuredValue: 24,
  threshold: 20,
  ...overrides,
});

describe("groupSubtitleQualityIssues", () => {
  it("summarizes repetitive warnings but keeps blocking errors individually addressable", () => {
    const groups = groupSubtitleQualityIssues([
      issue({ id: "speed-1", start: 8, end: 9 }),
      issue({ id: "speed-2", segmentId: "s2", start: 12, end: 14 }),
      issue({ id: "line-1", kind: "line_too_long", start: 4, end: 6 }),
      issue({ id: "error-1", kind: "overlap", severity: "error", start: 1, end: 2 }),
      issue({ id: "error-2", kind: "overlap", severity: "error", start: 3, end: 4 }),
    ]);

    expect(groups).toHaveLength(4);
    expect(groups.find((group) => group.id === "warning:reading_speed_high")).toMatchObject({ count: 2, start: 8, end: 14 });
    expect(groups.filter((group) => group.severity === "error")).toHaveLength(2);
  });
});
