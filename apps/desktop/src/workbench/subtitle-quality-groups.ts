import type { SubtitleQualityIssue } from "../types";

export type SubtitleQualityIssueGroup = Readonly<{
  id: string;
  kind: SubtitleQualityIssue["kind"];
  severity: SubtitleQualityIssue["severity"];
  count: number;
  start: number;
  end: number;
  first: SubtitleQualityIssue;
}>;

export function groupSubtitleQualityIssues(issues: SubtitleQualityIssue[]): SubtitleQualityIssueGroup[] {
  const groups = new Map<string, SubtitleQualityIssueGroup>();
  for (const issue of issues) {
    const key = issue.severity === "error" ? `error:${issue.id}` : `warning:${issue.kind}`;
    const existing = groups.get(key);
    groups.set(key, existing ? {
      ...existing,
      count: existing.count + 1,
      start: Math.min(existing.start, issue.start),
      end: Math.max(existing.end, issue.end),
    } : {
      id: key,
      kind: issue.kind,
      severity: issue.severity,
      count: 1,
      start: issue.start,
      end: issue.end,
      first: issue,
    });
  }
  return [...groups.values()].sort((left, right) => left.start - right.start || left.id.localeCompare(right.id));
}
