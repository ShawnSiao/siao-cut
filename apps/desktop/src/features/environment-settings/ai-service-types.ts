export type AiProviderId = "openai" | "anthropic" | "gemini" | "deepseek" | "kimi" | "glm" | "custom";
export type AiProtocol = "openai_responses" | "anthropic_messages" | "gemini_generate_content" | "openai_chat_completions";
export type CredentialState = "missing" | "stored";
export type ConnectionState = "untested" | "ready" | "error";

export type AiProviderCatalogEntry = {
  id: AiProviderId;
  displayName: string;
  protocol: AiProtocol;
  officialBaseUrl: string | null;
  modelsPath: string;
  documentationUrl: string | null;
  supportsModelDiscovery: boolean;
};

export type AiServiceSummary = {
  id: string;
  providerId: AiProviderId;
  displayName: string;
  protocol: AiProtocol;
  baseUrl: string;
  modelId: string | null;
  credentialState: CredentialState;
  connectionState: ConnectionState;
  lastTestedAt: string | null;
  lastErrorCode: string | null;
  isDefault: boolean;
  revision: number;
};

export type AiServiceSettings = {
  schemaVersion: number;
  revision: number;
  providerCatalog: { schemaVersion: number; providers: AiProviderCatalogEntry[] };
  services: AiServiceSummary[];
  defaultServiceId: string | null;
};

export type AiNetworkSettings = {
  schemaVersion: number;
  revision: number;
  customProxyUrl: string | null;
  effectiveMode: string;
  effectiveSource: string;
  effectiveProxyAddress: string | null;
};

export type AiEnvironment = { aiServices: AiServiceSettings; network: AiNetworkSettings };

export type AiServiceDraft = {
  id?: string;
  providerId: AiProviderId;
  displayName: string;
  protocol: AiProtocol;
  baseUrl: string;
  modelId: string;
  apiKey: string;
};

export type AiModelInfo = { id: string; displayName: string };
export type AiModelList = { models: AiModelInfo[]; manualEntryAllowed: boolean };
export type AiServiceTestResult = {
  state: ConnectionState;
  models: AiModelInfo[];
  selectedModelId: string | null;
  minimalRequestUsed: boolean;
  mayIncurUsage: boolean;
  providerRequestId: string | null;
};
