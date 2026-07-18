import { newId, mutateProject } from "./store.mjs";

export function validateSegment(segment) {
  if (!segment || !segment.id || typeof segment.text !== "string") throw new Error("字幕段必须包含 id 和 text");
  if (!Number.isFinite(segment.start) || !Number.isFinite(segment.end) || segment.start < 0 || segment.end <= segment.start) {
    throw new Error(`无效时间范围：${segment.id}`);
  }
}

export async function addSegment(projectId, { start, end, text, confidence = null }) {
  return mutateProject(projectId, "新增字幕段", (project) => {
    const segment = { id: newId("s"), start: Number(start), end: Number(end), text, confidence };
    validateSegment(segment);
    project.transcript.segments.push(segment);
    project.transcript.segments.sort((a, b) => a.start - b.start);
    return segment;
  });
}

export async function editSegment(projectId, segmentId, text) {
  return mutateProject(projectId, "编辑原文", (project) => {
    const segment = project.transcript.segments.find((item) => item.id === segmentId);
    if (!segment) throw new Error(`字幕段不存在：${segmentId}`);
    segment.text = text;
    for (const translation of Object.values(project.translations)) translation.status = "stale";
    return segment;
  });
}

const filler = /^(嗯+|呃+|额+|啊+|uh+|um+|erm+)[，。,.!?！？”“\s]*$/iu;

export async function detectCuts(projectId) {
  return mutateProject(projectId, "检测口癖", (project) => {
    const existing = new Set(project.edits.filter((edit) => edit.kind === "cut").map((edit) => edit.segmentId));
    const suggestions = project.transcript.segments
      .filter((segment) => filler.test(segment.text.trim()) && !existing.has(segment.id))
      .map((segment) => ({
        id: newId("cut"), kind: "cut", status: "proposed", segmentId: segment.id,
        start: segment.start, end: segment.end, reason: "疑似口癖", createdAt: new Date().toISOString()
      }));
    project.edits.push(...suggestions);
    return suggestions;
  });
}

export async function setCutStatus(projectId, editId, status) {
  if (!new Set(["applied", "restored"]).has(status)) throw new Error("软剪辑状态只能是 applied 或 restored");
  return mutateProject(projectId, status === "applied" ? "应用软剪辑" : "恢复软剪辑", (project) => {
    const edit = project.edits.find((item) => item.id === editId);
    if (!edit || edit.kind !== "cut") throw new Error(`软剪辑不存在：${editId}`);
    edit.status = status;
    return edit;
  });
}

export async function restoreAllCuts(projectId) {
  return mutateProject(projectId, "恢复全部软剪辑", (project) => {
    const restored = project.edits.filter((item) => item.kind === "cut" && item.status === "applied");
    restored.forEach((item) => { item.status = "restored"; });
    return restored;
  });
}

export function activeCuts(project) {
  return project.edits.filter((edit) => edit.kind === "cut" && edit.status === "applied");
}
