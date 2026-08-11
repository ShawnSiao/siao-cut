import type {
  AiProviderCatalogEntry,
  AiProviderId,
  AiServiceDraft,
  AiServiceSettings,
  AiServiceSummary,
} from "./ai-service-types";

export const localCodexSelectionId = "local:codex";

export function providerSelectionId(providerId: AiProviderId) {
  return `provider:${providerId}`;
}

export function serviceForSelection(settings: AiServiceSettings, selectionId: string) {
  return settings.services.find((service) => service.id === selectionId) ?? null;
}

export function providerForSelection(
  settings: AiServiceSettings,
  selectionId: string,
): AiProviderCatalogEntry | null {
  const service = serviceForSelection(settings, selectionId);
  const providerId = service?.providerId ?? (
    selectionId.startsWith("provider:")
      ? selectionId.slice("provider:".length) as AiProviderId
      : null
  );
  return settings.providerCatalog.providers.find((provider) => provider.id === providerId) ?? null;
}

export function initialServiceSelection(settings: AiServiceSettings) {
  return settings.defaultServiceId ?? localCodexSelectionId;
}

export function draftForSelection(
  service: AiServiceSummary | null,
  provider: AiProviderCatalogEntry,
): AiServiceDraft {
  return {
    id: service?.id,
    providerId: provider.id,
    displayName: service?.displayName ?? provider.displayName,
    protocol: provider.protocol,
    baseUrl: service?.baseUrl ?? provider.officialBaseUrl ?? "",
    modelId: service?.modelId ?? "",
    apiKey: "",
  };
}
