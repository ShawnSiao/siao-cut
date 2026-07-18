import { newId, listProjects, mutateProject } from "./store.mjs";

function taskPayload(project, task) {
  return {
    taskId: task.id,
    projectId: project.id,
    kind: task.kind,
    language: task.language || null,
    baseVersionId: project.versions.at(-1)?.id || null,
    instructions: task.kind === "translate" ? "逐段翻译，保留原意与术语。" : "逐段润色，纠正明显转写错误，不删改事实。",
    segments: project.transcript.segments.map(({ id, text, start, end }) => ({ id, text, start, end }))
  };
}

export async function createTask(projectId, kind, language) {
  if (!new Set(["polish", "translate", "summary"]).has(kind)) throw new Error("任务类型必须为 polish、translate 或 summary");
  return mutateProject(projectId, `创建 ${kind} 任务`, (project) => {
    const task = { id: newId("t"), kind, language, status: "queued", createdAt: new Date().toISOString(), lease: null };
    project.tasks.push(task);
    return task;
  });
}

export async function claimTask(worker) {
  const projects = await listProjects();
  for (const project of projects) {
    const queued = project.tasks.find((task) => task.status === "queued");
    if (!queued) continue;
    return mutateProject(project.id, `Agent ${worker} 领取任务`, (fresh) => {
      const task = fresh.tasks.find((item) => item.id === queued.id);
      task.status = "claimed";
      task.lease = { worker, id: newId("lease"), expiresAt: new Date(Date.now() + 10 * 60 * 1000).toISOString() };
      return { task, payload: taskPayload(fresh, task) };
    });
  }
  return null;
}

function validateResponse(project, task, response) {
  if (task.kind === "summary") {
    if (typeof response.summary !== "string" || !response.summary.trim()) throw new Error("摘要任务需要非空 summary");
    return;
  }
  if (!Array.isArray(response.segments) || response.segments.length === 0) throw new Error("任务响应需要 segments 数组");
  const validIds = new Set(project.transcript.segments.map((segment) => segment.id));
  for (const item of response.segments) {
    if (!validIds.has(item.segmentId) || typeof item.text !== "string" || !item.text.trim()) throw new Error("任务响应包含无效字幕段");
  }
}

export async function submitTask(taskId, worker, response) {
  const projects = await listProjects();
  const project = projects.find((item) => item.tasks.some((task) => task.id === taskId));
  if (!project) throw new Error(`任务不存在：${taskId}`);
  return mutateProject(project.id, `Agent ${worker} 提交任务`, (fresh) => {
    const task = fresh.tasks.find((item) => item.id === taskId);
    if (task.status !== "claimed" || task.lease?.worker !== worker) throw new Error("任务未由当前 Agent 领取");
    if (Date.parse(task.lease.expiresAt) < Date.now()) throw new Error("任务租约已过期，请重新领取");
    validateResponse(fresh, task, response);
    if (task.kind === "polish") {
      for (const item of response.segments) fresh.transcript.segments.find((segment) => segment.id === item.segmentId).text = item.text;
      for (const translation of Object.values(fresh.translations)) translation.status = "stale";
    } else if (task.kind === "translate") {
      if (!task.language) throw new Error("翻译任务缺少目标语言");
      fresh.translations[task.language] = { status: "current", updatedAt: new Date().toISOString(), segments: response.segments };
    } else {
      fresh.summary = { text: response.summary, updatedAt: new Date().toISOString() };
    }
    task.status = "done";
    task.completedAt = new Date().toISOString();
    task.lease = null;
    return task;
  });
}
