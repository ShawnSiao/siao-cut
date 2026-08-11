import { useCallback, useEffect, useState } from "react";
import { aiServicesGateway } from "./ai-services-gateway";
import type { AiEnvironment, AiModelList, AiServiceDraft, AiServiceTestResult } from "./ai-service-types";

export function useAiServices() {
  const [environment, setEnvironment] = useState<AiEnvironment | null>(null);
  const [busy, setBusy] = useState<string | null>("正在读取 AI 服务…");
  const [error, setError] = useState<string | null>(null);

  const perform = useCallback(async <T,>(label: string, action: () => Promise<T>) => {
    setBusy(label);
    setError(null);
    try {
      return await action();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    } finally {
      setBusy(null);
    }
  }, []);

  const refresh = useCallback(() => perform("正在读取 AI 服务…", async () => {
    const next = await aiServicesGateway.snapshot();
    setEnvironment(next);
    return next;
  }), [perform]);

  useEffect(() => { void refresh().catch(() => undefined); }, [refresh]);

  const updateServices = useCallback((aiServices: AiEnvironment["aiServices"]) => {
    setEnvironment((current) => current ? { ...current, aiServices } : current);
  }, []);

  return {
    environment,
    busy,
    error,
    refresh,
    save: (draft: AiServiceDraft) => perform("正在保存 AI 服务…", async () => {
      if (!environment) throw new Error("AI 服务尚未载入");
      const settings = await aiServicesGateway.save(environment.aiServices.revision, draft);
      updateServices(settings);
      return settings;
    }),
    remove: (id: string) => perform("正在删除 AI 服务…", async () => {
      if (!environment) throw new Error("AI 服务尚未载入");
      updateServices(await aiServicesGateway.remove(environment.aiServices.revision, id));
    }),
    removeCredential: (id: string) => perform("正在删除 API Key…", async () => {
      if (!environment) throw new Error("AI 服务尚未载入");
      updateServices(await aiServicesGateway.removeCredential(environment.aiServices.revision, id));
    }),
    setDefault: (id: string | null) => perform("正在更新默认 AI 服务…", async () => {
      if (!environment) throw new Error("AI 服务尚未载入");
      updateServices(await aiServicesGateway.setDefault(environment.aiServices.revision, id));
    }),
    setProxy: (url: string | null) => perform("正在更新代理…", async () => {
      if (!environment) throw new Error("AI 服务尚未载入");
      const network = await aiServicesGateway.setNetwork(environment.network.revision, url);
      setEnvironment((current) => current ? { ...current, network } : current);
    }),
    listModels: (draft: AiServiceDraft): Promise<AiModelList> => perform("正在获取模型…", () => aiServicesGateway.listModels(draft)),
    test: async (draft: AiServiceDraft): Promise<AiServiceTestResult> => {
      const result = await perform("正在测试连接…", () => aiServicesGateway.test(draft));
      if (draft.id) await refresh();
      return result;
    },
  };
}
