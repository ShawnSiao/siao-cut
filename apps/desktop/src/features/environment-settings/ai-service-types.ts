import type * as Wire from "../../generated/core-contract";
export type AiProviderId = Wire.AiProviderId;
export type AiProtocol = Wire.AiProtocol;
export type CredentialState = Wire.CredentialState;
export type ConnectionState = Wire.ConnectionState;
export type AiProviderCatalogEntry = Wire.AiProviderCatalogEntry;
export type AiServiceSummary = Wire.AiServiceSummary;
export type AiServiceSettings = Wire.AiServiceSettings;
export type AiModelInfo = Wire.AiModelInfo;
export type AiModelList = Wire.AiModelList;
export type AiServiceTestResult = Wire.AiServiceTestResult;
export type AiNetworkSettings = Wire.NetworkSettings;
export type AiEnvironment = Wire.AiEnvironmentSettings;

export type AiServiceDraft = {
  id?: string;
  providerId: AiProviderId;
  displayName: string;
  protocol: AiProtocol;
  baseUrl: string;
  modelId: string;
  apiKey: string;
};
