import { useEffect, useState } from "react";
import { aiApprovalClient } from "../../domains/ai-approval-client";
import type { AiSendPreview, AiSendSpec } from "../../generated/core-contract";

export function useAiSendPreview(spec: AiSendSpec | null) {
  const key = spec ? JSON.stringify(spec) : null;
  const [result, setResult] = useState<{ key: string; preview?: AiSendPreview; error?: string } | null>(null);
  const [attempt, retry] = useState(0);
  useEffect(() => {
    if (!key) return;
    let live = true;
    const timer = setTimeout(() => {
      void aiApprovalClient.preview(JSON.parse(key) as AiSendSpec).then(
        (preview) => { if (live) setResult({ key, preview }); },
        (cause) => { if (live) setResult({ key, error: cause instanceof Error ? cause.message : String(cause) }); },
      );
    }, 200);
    return () => { live = false; clearTimeout(timer); };
  }, [key, attempt]);
  return {
    preview: result?.key === key ? result.preview ?? null : null,
    error: result?.key === key ? result.error ?? null : null,
    retry: () => { setResult(null); retry((value) => value + 1); },
  };
}
