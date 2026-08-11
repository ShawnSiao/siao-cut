import { runAiRequest } from "../../core";
import type { AiEnvironment, AiModelList, AiServiceDraft, AiServiceSettings, AiServiceTestResult, AiNetworkSettings } from "./ai-service-types";

function required<T>(value: unknown, field: string): T {
  if (value === undefined || value === null) throw new Error(`AI 服务响应缺少 ${field}`);
  return value as T;
}

const probeInput = (draft: AiServiceDraft) => ({
  serviceConfigId: draft.id ?? null,
  providerId: draft.providerId,
  protocol: draft.protocol,
  baseUrl: draft.baseUrl || null,
  modelId: draft.modelId || null,
  apiKey: draft.apiKey || null,
});

export const aiServicesGateway = {
  async snapshot() {
    const envelope = await runAiRequest({ kind: "snapshot" });
    return required<AiEnvironment>(envelope.aiEnvironment, "aiEnvironment");
  },
  async save(revision: number, draft: AiServiceDraft) {
    const envelope = await runAiRequest({
      kind: "save",
      input: {
        expectedRevision: revision,
        id: draft.id ?? null,
        providerId: draft.providerId,
        displayName: draft.displayName,
        protocol: draft.protocol,
        baseUrl: draft.baseUrl || null,
        modelId: draft.modelId || null,
        apiKey: draft.apiKey || null,
      },
    });
    return required<AiServiceSettings>(envelope.aiServices, "aiServices");
  },
  async remove(revision: number, id: string) {
    const envelope = await runAiRequest({ kind: "delete", input: { expectedRevision: revision, id } });
    return required<AiServiceSettings>(envelope.aiServices, "aiServices");
  },
  async removeCredential(revision: number, id: string) {
    const envelope = await runAiRequest({ kind: "delete_credential", input: { expectedRevision: revision, id } });
    return required<AiServiceSettings>(envelope.aiServices, "aiServices");
  },
  async setDefault(revision: number, id: string | null) {
    const envelope = await runAiRequest({ kind: "set_default", input: { expectedRevision: revision, id } });
    return required<AiServiceSettings>(envelope.aiServices, "aiServices");
  },
  async setNetwork(revision: number, customProxyUrl: string | null) {
    const envelope = await runAiRequest({ kind: "set_network", input: { expectedRevision: revision, customProxyUrl } });
    return required<AiNetworkSettings>(envelope.network, "network");
  },
  async listModels(draft: AiServiceDraft) {
    const envelope = await runAiRequest({ kind: "list_models", input: probeInput(draft) });
    return required<AiModelList>(envelope.aiModels, "aiModels");
  },
  async test(draft: AiServiceDraft) {
    const envelope = await runAiRequest({ kind: "test_service", input: probeInput(draft) });
    return required<AiServiceTestResult>(envelope.testResult, "testResult");
  },
};
