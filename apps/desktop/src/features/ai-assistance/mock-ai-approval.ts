// Browser demonstration only. Native authorization and dispatch live in Core.
import type { AiApprovalRequest, AiSendPreview } from "../../generated/core-contract";
import type { CoreEnvelope } from "../../types";
import { aiServicesGateway } from "../environment-settings/ai-services-gateway";
import { mockRun, mockAuthorizeAutoTask } from "../../core.mock";
const approvals = new Map<string, AiSendPreview>();
const launches = new Map<string, Promise<CoreEnvelope>>();
export async function mockAiApproval(request: AiApprovalRequest): Promise<CoreEnvelope> {
  if (request.action === "execute") {
    const existing = launches.get(request.approvalId);
    if (existing) return existing;
    const preview = approvals.get(request.approvalId);
    if (!preview) throw new Error("发送授权不存在，请重新预检。");
    const launch = (async () => {
      const current = (await mockRun(["project", "show", preview.spec.projectId])).project;
      if (current?.history.currentVersionId !== preview.spec.expectedVersionId) throw new Error("项目版本已变化，请重新预检。");
      const workflow = preview.spec.taskId ? null : (await mockRun(["workflow", "create", preview.spec.projectId, "--kind", preview.spec.kind, "--locale", preview.spec.instructionLocale, ...(preview.spec.language ? ["--lang", preview.spec.language] : [])]));
      const taskId = preview.spec.taskId ?? workflow?.taskId ?? "";
      mockAuthorizeAutoTask(taskId);
      const target = preview.spec.target;
      return mockRun(["agent", "start", taskId, "--execution", target.kind,
        ...(target.kind === "api" ? ["--service-config-id", target.service_config_id, "--service-revision", String(target.service_revision), "--network-revision", String(target.network_revision), "--model-id", target.model_id] : [])]);
    })();
    launches.set(request.approvalId, launch);
    return launch;
  }
  const { spec } = request;
  const project = (await mockRun(["project", "show", spec.projectId])).project;
  if (!project || project.history.currentVersionId !== spec.expectedVersionId) throw new Error("项目版本已变化，请重新预检。");
  const environment = await aiServicesGateway.snapshot();
  const target = spec.target;
  const service = target.kind === "api" ? environment.aiServices.services.find((item) => item.id === target.service_config_id) : null;
  const segments = project.transcript.segments;
  const preview: AiSendPreview = {
    approvalId: crypto.randomUUID(), payloadHash: "browser-simulation", spec,
    receiver: spec.target.kind === "codex" ? "Codex：接收方未核实，可能使用远程模型" : service?.displayName ?? "浏览器模拟服务",
    endpoint: service ? new URL(service.baseUrl).origin : null, model: spec.target.kind === "api" ? spec.target.model_id : null, receiverVerified: false,
    configurationRevision: "browser-simulation", segmentCount: segments.length,
    characterCount: segments.reduce((count, segment) => count + Array.from(segment.text).length, 0),
    startTime: segments[0]?.start ?? 0, endTime: segments.at(-1)?.end ?? 0,
    payloadJson: JSON.stringify({ kind: spec.kind, language: spec.language, baseVersionId: spec.expectedVersionId,
      segments: segments.map(({ id, text, start, end }) => ({ id, text, start, end })), glossary: spec.kind === "translate" ? project.glossary : null }, null, 2),
  };
  approvals.set(preview.approvalId, preview);
  return { apiVersion: "0.1", status: "ok", aiSendPreview: preview };
}
