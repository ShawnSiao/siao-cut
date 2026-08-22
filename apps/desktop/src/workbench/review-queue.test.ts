import { describe, expect, it } from "vitest";
import { sampleProject } from "../mock";
import { deriveReviewQueue } from "./review-queue";

describe("deriveReviewQueue", () => {
  it("combines all actionable review sources in timeline order", () => {
    const project = structuredClone(sampleProject);
    project.patchSets[0].items.push({
      ...project.patchSets[0].items[0],
      id: "pi-conflict",
      segmentId: "s3",
      status: "conflict",
    });
    const reviews = [{
      id: "review-1",
      projectId: project.id,
      runId: "run-1",
      segmentId: "s4",
      severity: "warning" as const,
      kind: "short_fragment",
      message: "片段过短",
      status: "open" as const,
      createdAt: "2026-07-29T00:00:00Z",
      resolvedAt: null,
    }];
    const risk = { kind: "silence" as const, start: 20, end: 22, measuredValue: 2, threshold: 1.5, unit: "seconds", toolVersion: "test" };
    const queue = deriveReviewQueue(project, reviews, [risk, risk]);

    expect(queue.map((item) => item.kind)).toEqual(["cut", "agent", "quality", "agent", "quality", "audio", "quality", "transcription"]);
    expect(queue.find((item) => item.id === "agent:pi-conflict")?.conflict).toBe(true);
    expect(queue.filter((item) => item.kind === "audio")).toHaveLength(1);
  });

  it("excludes decisions that are already complete", () => {
    const project = structuredClone(sampleProject);
    project.patchSets[0].items[0].status = "kept";
    project.edits[0].status = "dismissed";
    const queue = deriveReviewQueue(project, [{
      id: "review-1",
      projectId: project.id,
      runId: "run-1",
      segmentId: "s1",
      severity: "warning",
      kind: "short_fragment",
      message: "handled",
      status: "resolved",
      createdAt: "2026-07-29T00:00:00Z",
      resolvedAt: "2026-07-29T00:01:00Z",
    }], []);

    expect(queue.some((item) => ["agent", "cut", "transcription"].includes(item.kind))).toBe(false);
  });
});
