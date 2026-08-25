import type { AudioRisk, Project, TranscriptionReviewItem } from "../types";

export type ReviewQueueItemKind = "quality" | "agent" | "cut" | "transcription" | "audio";

export type ReviewQueueItem = Readonly<{
  id: string;
  kind: ReviewQueueItemKind;
  sourceId: string;
  segmentId: string | null;
  start: number;
  end: number;
  detail: string;
  conflict: boolean;
}>;

export function deriveReviewQueue(
  project: Project,
  transcriptionReviews: TranscriptionReviewItem[],
  audioRisks: AudioRisk[],
): ReviewQueueItem[] {
  const segmentById = new Map(project.transcript.segments.map((segment) => [segment.id, segment]));
  const items: ReviewQueueItem[] = [];

  project.subtitleQuality.issues.forEach((issue) => items.push({
    id: `quality:${issue.id}`,
    kind: "quality",
    sourceId: issue.id,
    segmentId: issue.segmentId,
    start: issue.start,
    end: issue.end,
    detail: issue.message,
    conflict: false,
  }));

  project.patchSets.forEach((set) => set.items
    .filter((item) => ["pending", "conflict"].includes(item.status))
    .forEach((item) => {
      const segment = item.segmentId ? segmentById.get(item.segmentId) : null;
      items.push({
        id: `agent:${item.id}`,
        kind: "agent",
        sourceId: item.id,
        segmentId: item.segmentId,
        start: segment?.start ?? 0,
        end: segment?.end ?? 0,
        detail: item.reason,
        conflict: item.status === "conflict",
      });
    }));

  project.edits
    .filter((edit) => ["suggested", "proposed"].includes(edit.status))
    .forEach((edit) => items.push({
      id: `cut:${edit.id}`,
      kind: "cut",
      sourceId: edit.id,
      segmentId: edit.segmentId,
      start: edit.start,
      end: edit.end,
      detail: edit.reason,
      conflict: false,
    }));

  transcriptionReviews
    .filter((item) => item.status === "open")
    .forEach((item) => {
      const segment = item.segmentId ? segmentById.get(item.segmentId) : null;
      items.push({
        id: `transcription:${item.id}`,
        kind: "transcription",
        sourceId: item.id,
        segmentId: item.segmentId,
        start: segment?.start ?? 0,
        end: segment?.end ?? 0,
        detail: item.message,
        conflict: false,
      });
    });

  audioRisks.forEach((risk) => items.push({
    id: `audio:${risk.kind}:${risk.start}:${risk.end}`,
    kind: "audio",
    sourceId: `${risk.kind}:${risk.start}:${risk.end}`,
    segmentId: project.transcript.segments.find((segment) => risk.start >= segment.start && risk.start < segment.end)?.id ?? null,
    start: risk.start,
    end: risk.end,
    detail: risk.kind,
    conflict: false,
  }));

  const unique = new Map<string, ReviewQueueItem>();
  items
    .sort((left, right) => left.start - right.start || left.kind.localeCompare(right.kind) || left.id.localeCompare(right.id))
    .forEach((item) => unique.set(item.id, item));
  return [...unique.values()];
}
