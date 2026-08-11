import type { CoreEnvelope } from "../../types";
import type { AiEnvironment, AiProviderCatalogEntry, AiServiceSummary } from "./ai-service-types";

const providers: AiProviderCatalogEntry[] = [
  ["openai", "OpenAI", "openai_responses", "https://api.openai.com/v1"],
  ["anthropic", "Anthropic", "anthropic_messages", "https://api.anthropic.com"],
  ["gemini", "Gemini", "gemini_generate_content", "https://generativelanguage.googleapis.com/v1beta"],
  ["deepseek", "DeepSeek", "openai_chat_completions", "https://api.deepseek.com"],
  ["kimi", "Kimi", "openai_chat_completions", "https://api.moonshot.ai/v1"],
  ["glm", "GLM", "openai_chat_completions", "https://open.bigmodel.cn/api/paas/v4"],
  ["custom", "其他兼容服务", "openai_chat_completions", null],
].map(([id, displayName, protocol, officialBaseUrl]) => ({
  id, displayName, protocol, officialBaseUrl, modelsPath: "/models", documentationUrl: null, supportsModelDiscovery: true,
})) as AiProviderCatalogEntry[];

let environment: AiEnvironment = initialEnvironment();

function initialEnvironment(): AiEnvironment {
  return {
    aiServices: { schemaVersion: 1, revision: 0, defaultServiceId: null, services: [], providerCatalog: { schemaVersion: 1, providers } },
    network: { schemaVersion: 1, revision: 0, customProxyUrl: null, effectiveMode: "direct", effectiveSource: "direct", effectiveProxyAddress: null },
  };
}

const ok = (values: Record<string, unknown>): CoreEnvelope => ({ apiVersion: "0.1", status: "ok", ...values });
const inputOf = (request: object) => (request as { input?: Record<string, unknown> }).input ?? {};

export function resetMockAiSettingsForTest() { environment = initialEnvironment(); }

export async function mockAiRequest(request: object): Promise<CoreEnvelope> {
  const kind = (request as { kind?: string }).kind;
  const input = inputOf(request);
  if (kind === "snapshot") return ok({ aiEnvironment: structuredClone(environment) });
  if (kind === "save") {
    const existing = environment.aiServices.services.find((service) => service.id === input.id);
    const provider = providers.find((item) => item.id === input.providerId) ?? providers[0];
    const service: AiServiceSummary = {
      id: existing?.id ?? `preview-${environment.aiServices.services.length + 1}`,
      providerId: provider.id,
      displayName: String(input.displayName ?? provider.displayName),
      protocol: provider.protocol,
      baseUrl: String(input.baseUrl || provider.officialBaseUrl || ""),
      modelId: input.modelId ? String(input.modelId) : null,
      credentialState: input.apiKey || existing?.credentialState === "stored" ? "stored" : "missing",
      connectionState: existing?.connectionState ?? "untested",
      lastTestedAt: existing?.lastTestedAt ?? null,
      lastErrorCode: null,
      isDefault: existing?.isDefault ?? false,
      revision: (existing?.revision ?? 0) + 1,
    };
    environment.aiServices.services = existing ? environment.aiServices.services.map((item) => item.id === service.id ? service : item) : [...environment.aiServices.services, service];
    environment.aiServices.revision += 1;
    return ok({ aiServices: structuredClone(environment.aiServices) });
  }
  if (kind === "delete" || kind === "delete_credential") {
    const id = String(input.id ?? "");
    if (kind === "delete") {
      environment.aiServices.services = environment.aiServices.services.filter((service) => service.id !== id);
      if (environment.aiServices.defaultServiceId === id) environment.aiServices.defaultServiceId = null;
    } else {
      environment.aiServices.services = environment.aiServices.services.map((service) => service.id === id ? { ...service, credentialState: "missing" } : service);
    }
    environment.aiServices.revision += 1;
    return ok({ aiServices: structuredClone(environment.aiServices) });
  }
  if (kind === "set_default") {
    const id = input.id ? String(input.id) : null;
    environment.aiServices.defaultServiceId = id;
    environment.aiServices.services = environment.aiServices.services.map((service) => ({ ...service, isDefault: service.id === id }));
    environment.aiServices.revision += 1;
    return ok({ aiServices: structuredClone(environment.aiServices) });
  }
  if (kind === "set_network") {
    const proxy = input.customProxyUrl ? String(input.customProxyUrl) : null;
    environment.network = { ...environment.network, revision: environment.network.revision + 1, customProxyUrl: proxy, effectiveMode: proxy ? "proxy" : "direct", effectiveSource: proxy ? "custom" : "direct", effectiveProxyAddress: proxy };
    return ok({ network: structuredClone(environment.network) });
  }
  const model = String(input.modelId || "preview-model");
  const models = [{ id: model, displayName: model }, { id: "preview-fast", displayName: "preview-fast" }];
  if (kind === "list_models") return ok({ aiModels: { models, manualEntryAllowed: true } });
  if (kind === "test_service") {
    const id = input.serviceConfigId ? String(input.serviceConfigId) : null;
    if (id) environment.aiServices.services = environment.aiServices.services.map((service) => service.id === id ? { ...service, connectionState: "ready", lastTestedAt: new Date().toISOString() } : service);
    return ok({ testResult: { state: "ready", models, selectedModelId: model, minimalRequestUsed: false, mayIncurUsage: false, providerRequestId: "preview-request" } });
  }
  return ok({});
}
