import type { CoreEnvelope } from "../../types";

const providers = [
  ["openai", "OpenAI", "openai_responses", "https://api.openai.com/v1"],
  ["anthropic", "Anthropic", "anthropic_messages", "https://api.anthropic.com"],
  ["gemini", "Gemini", "gemini_generate_content", "https://generativelanguage.googleapis.com/v1beta"],
  ["deepseek", "DeepSeek", "openai_chat_completions", "https://api.deepseek.com"],
  ["kimi", "Kimi", "openai_chat_completions", "https://api.moonshot.ai/v1"],
  ["glm", "GLM", "openai_chat_completions", "https://open.bigmodel.cn/api/paas/v4"],
  ["custom", "其他兼容服务", "openai_chat_completions", null],
] as const;

export async function mockAiRequest(_request: object): Promise<CoreEnvelope> {
  return {
    apiVersion: "0.1",
    status: "ok",
    aiEnvironment: {
      aiServices: {
        schemaVersion: 1,
        revision: 0,
        defaultServiceId: null,
        services: [],
        providerCatalog: {
          schemaVersion: 1,
          providers: providers.map(([id, displayName, protocol, officialBaseUrl]) => ({
            id,
            displayName,
            protocol,
            officialBaseUrl,
            modelsPath: "/models",
            documentationUrl: null,
            supportsModelDiscovery: true,
          })),
        },
      },
      network: {
        schemaVersion: 1,
        revision: 0,
        customProxyUrl: null,
        effectiveMode: "direct",
        effectiveSource: "direct",
        effectiveProxyAddress: null,
      },
    },
  };
}
